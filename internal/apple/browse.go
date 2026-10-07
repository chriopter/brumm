package apple

import (
	"errors"
	"html"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"sync"
)

// Shelf is a titled row of a page: Home, an artist, the charts, a search.
type Shelf struct {
	Title  string  `json:"title"`
	Items  []Item  `json:"items,omitempty"`
	Tracks []Track `json:"tracks,omitempty"`
}

func (s Shelf) empty() bool { return len(s.Items) == 0 && len(s.Tracks) == 0 }

var tags = regexp.MustCompile(`<[^>]*>`)

// plain turns an editorial note's bit of HTML into one line of text.
func plain(s string) string {
	s = html.UnescapeString(tags.ReplaceAllString(s, " "))
	return strings.Join(strings.Fields(s), " ")
}

func dedupe(list []string) []string {
	seen := map[string]bool{}
	out := list[:0]
	for _, s := range list {
		if !seen[s] {
			seen[s] = true
			out = append(out, s)
		}
	}
	return out
}

// catalogPath is a path under the user's storefront.
func (c *Client) catalogPath(p string) string { return "/v1/catalog/" + c.Storefront() + p }

// fetch gets a plain list of resources; an empty collection is no error.
func (c *Client) fetch(path string) ([]resource, error) {
	var p page
	_, err := c.get(path, &p)
	return p.Data, err
}

// parallel runs jobs at once and returns the first unauthorized error, so
// a page with one broken shelf still shows the rest.
func parallel(jobs ...func() error) error {
	var wg sync.WaitGroup
	errs := make([]error, len(jobs))
	for i, job := range jobs {
		wg.Add(1)
		go func() {
			defer wg.Done()
			errs[i] = job()
		}()
	}
	wg.Wait()
	failed := 0
	for _, err := range errs {
		if errors.Is(err, ErrUnauthorized) {
			return err
		}
		if err != nil {
			failed++
		}
	}
	if failed == len(jobs) && failed > 0 {
		return errs[0]
	}
	return nil
}

// Home is the start page: what was played, what came in, Apple's picks
// for this listener, and a way to the charts. Radio has its own page.
func (c *Client) Home() ([]Shelf, error) {
	var (
		recent, songs, rotation, added Shelf
		recs                           []Shelf
	)
	err := parallel(
		func() error {
			rs, err := c.fetch("/v1/me/recent/played?limit=10")
			recent = Shelf{Title: "Recently played"}
			recent.Items, _ = mixed(rs)
			return err
		},
		func() error {
			rs, err := c.fetch("/v1/me/recent/played/tracks?limit=20&types=songs,library-songs")
			songs = Shelf{Title: "Recently played songs", Tracks: tracks(rs)}
			return err
		},
		func() error {
			rs, err := c.fetch("/v1/me/history/heavy-rotation?limit=10")
			rotation = Shelf{Title: "Heavy rotation"}
			rotation.Items, _ = mixed(rs)
			return err
		},
		func() error {
			rs, err := c.fetch("/v1/me/library/recently-added?limit=10")
			if err == nil {
				more, _ := c.fetch("/v1/me/library/recently-added?limit=10&offset=10")
				rs = append(rs, more...)
			}
			added = Shelf{Title: "Recently added"}
			added.Items, _ = mixed(rs)
			return err
		},
		func() error {
			var err error
			recs, err = c.recommendations()
			return err
		},
	)
	if err != nil {
		return nil, err
	}
	if !recent.empty() {
		recent.Items = append(recent.Items, featureLink("More recently played", "/v1/me/recent/played?limit=25"))
	}
	if !songs.empty() {
		songs.Items = append(songs.Items, featureLink("More recently played songs", "/v1/me/recent/played/tracks?types=songs,library-songs&limit=25"))
	}
	if !rotation.empty() {
		rotation.Items = append(rotation.Items, featureLink("More heavy rotation", "/v1/me/history/heavy-rotation?limit=25"))
	}
	if !added.empty() {
		added.Items = append(added.Items, featureLink("More recently added", "/v1/me/library/recently-added?limit=25"))
	}
	browse := Shelf{Title: "Browse", Items: []Item{{Kind: KindShelf, ID: "charts", Name: "Top charts", Catalog: true}}}
	personal := Shelf{Title: "Your music", Items: []Item{
		featureLink("Favorites", "app:favorites"), featureLink("Your Replay", "app:replay"),
		featureLink("Playlist folders", "app:folders"),
	}}
	var out []Shelf
	for _, s := range append(append([]Shelf{personal, recent, songs, rotation, added}, recs...), browse) {
		if !s.empty() {
			out = append(out, s)
		}
	}
	return out, nil
}

// recommendations are Apple's "for you" groups, each a shelf.
func (c *Client) recommendations() ([]Shelf, error) {
	var doc struct {
		Data []struct {
			Attributes struct {
				Title struct {
					Text string `json:"stringForDisplay"`
				} `json:"title"`
			} `json:"attributes"`
			Relationships struct {
				Contents struct {
					Data []resource `json:"data"`
				} `json:"contents"`
			} `json:"relationships"`
		} `json:"data"`
	}
	if _, err := c.get("/v1/me/recommendations?limit=10", &doc); err != nil {
		return nil, err
	}
	var out []Shelf
	for _, g := range doc.Data {
		s := Shelf{Title: g.Attributes.Title.Text}
		s.Items, s.Tracks = mixed(g.Relationships.Contents.Data)
		if s.Title != "" && !s.empty() {
			out = append(out, s)
		}
	}
	return out, nil
}

// Charts are the storefront's most played songs, albums and playlists.
func (c *Client) Charts() ([]Shelf, error) {
	type chart struct {
		Name string     `json:"name"`
		Data []resource `json:"data"`
	}
	var doc struct {
		Results map[string][]chart `json:"results"`
	}
	if _, err := c.get(c.catalogPath("/charts?types=songs,albums,playlists&limit=25"), &doc); err != nil {
		return nil, err
	}
	var out []Shelf
	for _, typ := range []string{"songs", "albums", "playlists"} {
		for _, ch := range doc.Results[typ] {
			s := Shelf{Title: ch.Name}
			s.Items, s.Tracks = mixed(ch.Data)
			if !s.empty() {
				out = append(out, s)
			}
		}
	}
	return out, nil
}

// Radio is the radio page: your station, Apple's live radio, and the
// stations played lately.
func (c *Client) Radio() ([]Shelf, error) {
	mine := Shelf{Title: "Your station"}
	live := Shelf{Title: "Live radio"}
	recent := Shelf{Title: "Recently played"}
	err := parallel(
		func() error {
			rs, err := c.fetch(c.catalogPath("/stations?filter[identity]=personal"))
			mine.Items, _ = mixed(rs)
			return err
		},
		func() error {
			rs, err := c.fetch(c.catalogPath("/stations?filter[featured]=apple-music-live-radio"))
			live.Items, _ = mixed(rs)
			return err
		},
		func() error {
			rs, err := c.fetch("/v1/me/recent/radio-stations?limit=25")
			recent.Items, _ = mixed(rs)
			return err
		},
	)
	if err != nil {
		return nil, err
	}
	var out []Shelf
	for _, s := range []Shelf{mine, live, recent} {
		if !s.empty() {
			out = append(out, s)
		}
	}
	return out, nil
}

// Open lists a shelf item's page.
func (c *Client) Open(it Item) ([]Shelf, error) {
	switch {
	case it.Kind == KindShelf && it.ID == "charts":
		return c.Charts()
	case it.Kind == KindShelf && it.ID == "radio":
		return c.Radio()
	}
	return nil, errors.New("nothing to open here")
}

// catalogID finds the catalog twin of a library song or artist.
func (c *Client) catalogID(kind, id string) (string, error) {
	if !strings.HasPrefix(id, "i.") && !strings.HasPrefix(id, "r.") {
		return id, nil
	}
	rs, err := c.fetch("/v1/me/library/" + kind + "/" + url.PathEscape(id) + "/catalog")
	if err != nil {
		return "", err
	}
	if len(rs) == 0 {
		return "", errors.New("not in the Apple Music catalog")
	}
	return rs[0].ID, nil
}

// artistViews are the parts of an artist page, in the order shown.
var artistViews = []struct{ view, title string }{
	{"top-songs", "Top songs"},
	{"latest-release", "Latest release"},
	{"full-albums", "Albums"},
	{"singles", "Singles & EPs"},
	{"live-albums", "Live albums"},
	{"compilation-albums", "Compilations"},
	{"appears-on-albums", "Appears on"},
	{"featured-playlists", "Playlists"},
	{"featured-albums", "Featured albums"},
	{"top-music-videos", "Music videos"},
	{"featured-music-videos", "Featured videos"},
	{"similar-artists", "Similar artists"},
}

// Artist is an artist's page: their station, top songs, releases, and
// artists like them; for a library artist, what of theirs is in the
// library comes first.
func (c *Client) Artist(it Item) ([]Shelf, error) {
	var out []Shelf
	id := it.ID
	if !it.Catalog {
		albums, err := c.ArtistAlbums(it)
		if err != nil && !errors.Is(err, ErrPartial) {
			return nil, err
		}
		if len(albums) > 0 {
			out = append(out, Shelf{Title: "In your library", Items: albums})
		}
		if id, err = c.catalogID("artists", it.ID); err != nil {
			return out, nil // only in the library: that is all there is
		}
	}
	views := make([]string, len(artistViews))
	for i, v := range artistViews {
		views[i] = v.view
	}
	var doc struct {
		Data []struct {
			Views map[string]struct {
				Attributes struct {
					Title string `json:"title"`
				} `json:"attributes"`
				Data []resource `json:"data"`
				Next string     `json:"next"`
			} `json:"views"`
			Relationships struct {
				Station struct {
					Data []resource `json:"data"`
				} `json:"station"`
			} `json:"relationships"`
		} `json:"data"`
	}
	q := "?views=" + strings.Join(views, ",") + "&include=station&extend=editorialNotes"
	if _, err := c.get(c.catalogPath("/artists/"+url.PathEscape(id)+q), &doc); err != nil {
		return out, err
	}
	if len(doc.Data) == 0 {
		return out, nil
	}
	a := doc.Data[0]
	if st, _ := mixed(a.Relationships.Station.Data); len(st) > 0 {
		out = append([]Shelf{{Title: "Radio", Items: st}}, out...)
	}
	for _, v := range artistViews {
		view, ok := a.Views[v.view]
		if !ok {
			continue
		}
		s := Shelf{Title: v.title}
		if view.Attributes.Title != "" {
			s.Title = view.Attributes.Title
		}
		s.Items, s.Tracks = mixed(view.Data)
		if view.Next != "" {
			s.Items = append(s.Items, featureLink("More "+v.title, view.Next))
		}
		if !s.empty() {
			out = append(out, s)
		}
	}
	return out, nil
}

// Station is the radio station made from a song or an artist.
func (c *Client) Station(it Item, songID string) (Item, error) {
	kind, id := "songs", songID
	if songID == "" {
		kind, id = "artists", it.ID
	}
	id, err := c.catalogID(kind, id)
	if err != nil {
		return Item{}, err
	}
	rs, err := c.fetch(c.catalogPath("/" + kind + "/" + url.PathEscape(id) + "/station"))
	if err != nil {
		return Item{}, err
	}
	if its, _ := mixed(rs); len(its) > 0 {
		return its[0], nil
	}
	return Item{}, errors.New("Apple Music has no station for this")
}

// Search looks everywhere at once: suggestions for the words typed, the
// best matches, the catalog by kind, and the user's own library.
func (c *Client) Search(term string) ([]Shelf, error) {
	var (
		suggest, top, lib Shelf
		byKind            []Shelf
	)
	err := parallel(
		func() error {
			var doc struct {
				Results struct {
					Suggestions []struct {
						Kind    string    `json:"kind"`
						Content *resource `json:"content"`
						Term    string    `json:"searchTerm"`
						Show    string    `json:"displayTerm"`
					} `json:"suggestions"`
				} `json:"results"`
			}
			q := url.Values{"term": {term}, "kinds": {"terms,topResults"}, "limit": {"5"}}
			_, err := c.get(c.catalogPath("/search/suggestions?"+q.Encode()), &doc)
			suggest = Shelf{Title: "Suggestions"}
			for _, s := range doc.Results.Suggestions {
				if s.Content != nil {
					its, ts := mixed([]resource{*s.Content})
					suggest.Items = append(suggest.Items, its...)
					suggest.Tracks = append(suggest.Tracks, ts...)
				}
				if s.Kind == "terms" && s.Term != "" && !strings.EqualFold(s.Term, term) {
					suggest.Items = append(suggest.Items, Item{Kind: KindTerm, ID: s.Term, Name: s.Term})
				}
			}
			var hints struct {
				Results struct {
					Terms []string `json:"terms"`
				} `json:"results"`
			}
			_, hintErr := c.get(c.catalogPath("/search/hints?"+url.Values{"term": {term}, "limit": {"5"}}.Encode()), &hints)
			seen := map[string]bool{strings.ToLower(term): true}
			for _, it := range suggest.Items {
				seen[strings.ToLower(it.Name)] = true
			}
			for _, hint := range hints.Results.Terms {
				if !seen[strings.ToLower(hint)] {
					suggest.Items = append(suggest.Items, Item{Kind: KindTerm, ID: hint, Name: hint})
					seen[strings.ToLower(hint)] = true
				}
			}
			if errors.Is(hintErr, ErrUnauthorized) {
				return hintErr
			}
			return err
		},
		func() error {
			var doc struct {
				Results map[string]page `json:"results"`
			}
			q := url.Values{"term": {term}, "types": {"songs,albums,artists,playlists,stations,music-videos,activities,curators,apple-curators,record-labels"},
				"limit": {"15"}, "with": {"topResults"}}
			_, err := c.get(c.catalogPath("/search?"+q.Encode()), &doc)
			top = Shelf{Title: "Top results"}
			top.Items, top.Tracks = mixed(doc.Results["top"].Data)
			for _, k := range []struct{ typ, title string }{
				{"songs", "Songs"}, {"albums", "Albums"}, {"artists", "Artists"},
				{"playlists", "Playlists"}, {"stations", "Stations"},
				{"music-videos", "Music videos"}, {"activities", "Activities"}, {"curators", "Curators"}, {"apple-curators", "Apple curators"}, {"record-labels", "Record labels"},
			} {
				s := Shelf{Title: k.title}
				s.Items, s.Tracks = mixed(doc.Results[k.typ].Data)
				if next := doc.Results[k.typ].Next; next != "" {
					s.Items = append(s.Items, featureLink("More "+k.title, next))
				}
				byKind = append(byKind, s)
			}
			return err
		},
		func() error {
			var doc struct {
				Results map[string]page `json:"results"`
			}
			q := url.Values{"term": {term}, "limit": {"10"},
				"types": {"library-songs,library-albums,library-artists,library-playlists,library-music-videos"}}
			_, err := c.get("/v1/me/library/search?"+q.Encode(), &doc)
			lib = Shelf{Title: "In your library"}
			for _, typ := range []string{"library-songs", "library-albums", "library-artists", "library-playlists", "library-music-videos"} {
				its, trs := mixed(doc.Results[typ].Data)
				if next := doc.Results[typ].Next; next != "" {
					its = append(its, featureLink("More library results", next))
				}
				lib.Items, lib.Tracks = append(lib.Items, its...), append(lib.Tracks, trs...)
			}
			return err
		},
	)
	if err != nil {
		return nil, err
	}
	var out []Shelf
	for _, s := range append([]Shelf{suggest, top, lib}, byKind...) {
		if !s.empty() {
			out = append(out, s)
		}
	}
	return out, nil
}

// Rating kinds.
const (
	Love    = 1
	Dislike = -1
)

// ratingPath is where the rating of a song, album, playlist or station
// lives: library ids ("i.", "l.", "p.") and catalog ids have separate
// collections.
func ratingPath(kind, id string) string {
	lib := strings.HasPrefix(id, "i.") || strings.HasPrefix(id, "l.") || strings.HasPrefix(id, "p.")
	if kind == "" || kind == "track" {
		kind = "song"
	}
	p := "/v1/me/ratings/"
	if lib && kind != KindStation {
		p += "library-"
	}
	return p + kind + "s"
}

// Ref names a rated thing: a kind ("song", or an Item kind) and an id.
type Ref struct {
	Kind string `json:"kind,omitempty"`
	ID   string `json:"id"`
}

// Ratings returns the ratings (Love or Dislike) of refs that have one.
func (c *Client) Ratings(refs []Ref) (map[string]int, error) {
	out := map[string]int{}
	groups := map[string][]string{}
	for _, r := range refs {
		p := ratingPath(r.Kind, r.ID)
		groups[p] = append(groups[p], r.ID)
	}
	for path, list := range groups {
		for i := 0; i < len(list); i += 100 {
			chunk := list[i:min(i+100, len(list))]
			var doc struct {
				Data []struct {
					ID         string `json:"id"`
					Attributes struct {
						Value int `json:"value"`
					} `json:"attributes"`
				} `json:"data"`
			}
			found, err := c.get(path+"?ids="+url.QueryEscape(strings.Join(chunk, ",")), &doc)
			if err != nil {
				return out, err
			}
			if !found {
				continue
			}
			for _, r := range doc.Data {
				if r.Attributes.Value != 0 {
					out[r.ID] = r.Attributes.Value
				}
			}
		}
	}
	return out, nil
}

// Rate loves (Love) or dislikes (Dislike) something, or clears that (0).
func (c *Client) Rate(r Ref, value int) error {
	path := ratingPath(r.Kind, r.ID) + "/" + url.PathEscape(r.ID)
	if value == 0 {
		return c.send(http.MethodDelete, path, nil, nil)
	}
	body := map[string]any{"type": "rating", "attributes": map[string]int{"value": value}}
	return c.send(http.MethodPut, path, body, nil)
}

// songRefs are songs as the playlist endpoints want them.
func songRefs(ids []string) []map[string]string {
	out := make([]map[string]string, 0, len(ids))
	for _, id := range ids {
		typ := "songs"
		if strings.HasPrefix(id, "i.") {
			typ = "library-songs"
		}
		out = append(out, map[string]string{"id": id, "type": typ})
	}
	return out
}

// CreatePlaylist makes a library playlist holding songs. Apple's API can
// create playlists and add to them, but not rename, reorder, remove songs
// or delete — that stays with the Music app.
func (c *Client) CreatePlaylist(name string, songs []string) (Item, error) {
	body := map[string]any{"attributes": map[string]string{"name": name}}
	if len(songs) > 0 {
		body["relationships"] = map[string]any{"tracks": map[string]any{"data": songRefs(songs)}}
	}
	var doc page
	if err := c.send(http.MethodPost, "/v1/me/library/playlists", body, &doc); err != nil {
		return Item{}, err
	}
	if len(doc.Data) == 0 {
		return Item{Kind: KindPlaylist, Name: name, Editable: true}, nil
	}
	return item(doc.Data[0], KindPlaylist, false), nil
}

// AddToPlaylist appends songs to a library playlist brumm (or another app)
// may edit.
func (c *Client) AddToPlaylist(id string, songs []string) error {
	body := map[string]any{"data": songRefs(songs)}
	return c.send(http.MethodPost, "/v1/me/library/playlists/"+url.PathEscape(id)+"/tracks", body, nil)
}
