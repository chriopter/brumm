// Package apple is a small Apple Music REST client: the user's library,
// the catalog, and what Apple lets a third-party app do with them.
package apple

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"sync"
	"time"
)

const apiBase = "https://api.music.apple.com"

// ErrUnauthorized means Apple rejected the session; the user must log in again.
var ErrUnauthorized = errors.New("apple music: not signed in")

// ErrPartial means a listing stopped after some pages: what came is usable,
// but it is not the whole list.
var ErrPartial = errors.New("apple music: listing incomplete")

// Kinds of containers.
const (
	KindPlaylist = "playlist"
	KindAlbum    = "album"
	KindArtist   = "artist"
	KindStation  = "station" // plays, never opens
	KindShelf    = "shelf"   // a page of shelves: charts, a recommendation
	KindTerm     = "term"    // a search suggestion
	KindVideo    = "music-videos"
	KindFolder   = "playlist-folders"
	KindGenre    = "genres"
)

// Item is a container the user can open: a playlist, an album or an artist.
// Catalog items (from search) carry Catalog so their children come from the
// catalog instead of the library.
type Item struct {
	Kind    string `json:"kind"`
	ID      string `json:"id"`
	Name    string `json:"name"`
	Artist  string `json:"artist,omitempty"`
	Catalog bool   `json:"catalog,omitempty"`
	Artwork string `json:"artwork,omitempty"`
	// Info is a one-line summary: year, label, song count, audio quality.
	Info string `json:"info,omitempty"`
	// Note is Apple's editorial blurb or a playlist's description.
	Note       string   `json:"note,omitempty"`
	Editable   bool     `json:"editable,omitempty"` // a playlist songs can be added to
	Route      string   `json:"route,omitempty"`    // a public API resource or explorer route
	Details    []Detail `json:"details,omitempty"`
	Favorite   bool     `json:"favorite,omitempty"`
	URL        string   `json:"url,omitempty"`
	PreviewURL string   `json:"previewUrl,omitempty"`
}

// Key identifies an item across kinds and sources.
func (it Item) Key() string {
	src := "lib"
	if it.Catalog {
		src = "cat"
	}
	return src + ":" + it.Kind + ":" + it.ID
}

// Track is one playable song. ID is what MusicKit plays: a library id
// ("i.…") or a catalog id.
type Track struct {
	ID       string   `json:"id"`
	Title    string   `json:"title"`
	Artist   string   `json:"artist"`
	Album    string   `json:"album"`
	Duration float64  `json:"duration"` // seconds
	Artwork  string   `json:"artwork,omitempty"`
	Number   int      `json:"number,omitempty"` // position on its album
	Details  []Detail `json:"details,omitempty"`
	Favorite bool     `json:"favorite,omitempty"`
}

// artworkSize is the cover size brumm asks for everywhere, so the same
// cover always has the same address and caches once.
const artworkSize = "600"

func artworkURL(a *struct {
	URL string `json:"url"`
}) string {
	if a == nil {
		return ""
	}
	return strings.NewReplacer("{w}", artworkSize, "{h}", artworkSize).Replace(a.URL)
}

type Client struct {
	base      string // the API's address; a test server's in tests
	dev, user string
	http      *http.Client

	sfOnce     sync.Once
	storefront string
	lang       string // catalog text language, when the storefront has the user's
}

func New(developerToken, userToken string) *Client {
	return &Client{base: apiBase, dev: developerToken, user: userToken, http: &http.Client{Timeout: 15 * time.Second}}
}

type resource struct {
	ID         string     `json:"id"`
	Type       string     `json:"type"`
	Href       string     `json:"href,omitempty"`
	Attributes attributes `json:"attributes"`
}

type notes struct {
	Short    string `json:"short"`
	Standard string `json:"standard"`
}

type attributes struct {
	Extra       map[string]any `json:"-"`
	Name        string         `json:"name"`
	ArtistName  string         `json:"artistName"`
	AlbumName   string         `json:"albumName"`
	CuratorName string         `json:"curatorName"`
	DurationMS  int64          `json:"durationInMillis"`
	TrackNumber int            `json:"trackNumber"`
	PlayParams  *struct {
		ID string `json:"id"`
	} `json:"playParams"`
	Artwork *struct {
		URL string `json:"url"`
	} `json:"artwork"`
	CanEdit        bool     `json:"canEdit"`
	ReleaseDate    string   `json:"releaseDate"`
	RecordLabel    string   `json:"recordLabel"`
	TrackCount     int      `json:"trackCount"`
	AudioTraits    []string `json:"audioTraits"`
	EditorialNotes *notes   `json:"editorialNotes"`
	Description    *notes   `json:"description"`
	IsLive         bool     `json:"isLive"`
}

func (a *attributes) UnmarshalJSON(data []byte) error {
	type plain attributes
	var v plain
	if err := json.Unmarshal(data, &v); err != nil {
		return err
	}
	*a = attributes(v)
	return json.Unmarshal(data, &a.Extra)
}

type page struct {
	Next string     `json:"next"`
	Data []resource `json:"data"`
}

// send makes a request with an optional JSON body and decodes the answer
// into out when it is non-nil.
func (c *Client) send(method, path string, body, out any) error {
	var rd io.Reader
	if body != nil {
		b, err := json.Marshal(body)
		if err != nil {
			return err
		}
		rd = bytes.NewReader(b)
	}
	req, err := http.NewRequest(method, c.base+path, rd)
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", "Bearer "+c.dev)
	req.Header.Set("Music-User-Token", c.user)
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	resp, err := c.http.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	switch {
	case resp.StatusCode == http.StatusUnauthorized || resp.StatusCode == http.StatusForbidden:
		return ErrUnauthorized
	case resp.StatusCode == http.StatusNotFound && method == http.MethodDelete:
		return nil // nothing to remove
	case resp.StatusCode >= 300:
		return fmt.Errorf("apple music: %s", resp.Status)
	}
	if out != nil && resp.StatusCode != http.StatusNoContent {
		return json.NewDecoder(resp.Body).Decode(out)
	}
	return nil
}

// get fetches one document into out. Apple answers 404 for an empty
// library collection, which is reported as found=false, not an error.
func (c *Client) get(path string, out any) (found bool, err error) {
	req, err := http.NewRequest(http.MethodGet, c.base+c.localize(path), nil)
	if err != nil {
		return false, err
	}
	req.Header.Set("Authorization", "Bearer "+c.dev)
	req.Header.Set("Music-User-Token", c.user)
	resp, err := c.http.Do(req)
	if err != nil {
		return false, err
	}
	defer resp.Body.Close()
	switch {
	case resp.StatusCode == http.StatusNotFound:
		return false, nil
	case resp.StatusCode == http.StatusUnauthorized || resp.StatusCode == http.StatusForbidden:
		return false, ErrUnauthorized
	case resp.StatusCode != http.StatusOK:
		return false, fmt.Errorf("apple music: %s", resp.Status)
	}
	return true, json.NewDecoder(resp.Body).Decode(out)
}

// all follows `next` links. They come back as "/v1/me/…", already carrying
// the version prefix; joining them onto a base that has it too silently
// stopped listings at the first 100 items in vibez.
func (c *Client) all(path string, maxPages int) ([]resource, error) {
	var out []resource
	for i := 0; path != "" && i < maxPages; i++ {
		var p page
		found, err := c.get(path, &p)
		if err != nil {
			if len(out) > 0 && !errors.Is(err, ErrUnauthorized) {
				return out, ErrPartial // keep what we have, but say so
			}
			return out, err
		}
		if !found {
			break
		}
		out = append(out, p.Data...)
		if p.Next != "" {
			path = inheritPageQuery(path, p.Next)
		} else {
			path = ""
		}
		if path != "" && !strings.HasPrefix(path, "/v1/") {
			path = "/v1/" + strings.TrimPrefix(path, "/")
		}
	}
	if path != "" {
		return out, ErrPartial
	}
	return out, nil
}

// Storefront is the user's country, needed for catalog requests. It also
// picks the language for catalog texts: the system's ($LANG), when the
// storefront offers it and it is not the storefront's default anyway.
func (c *Client) Storefront() string {
	c.sfOnce.Do(func() {
		var doc struct {
			Data []struct {
				ID         string `json:"id"`
				Attributes struct {
					Default   string   `json:"defaultLanguageTag"`
					Supported []string `json:"supportedLanguageTags"`
				} `json:"attributes"`
			} `json:"data"`
		}
		if found, err := c.get("/v1/me/storefront", &doc); err == nil && found && len(doc.Data) > 0 {
			sf := doc.Data[0]
			c.storefront = sf.ID
			c.lang = pickLang(os.Getenv("LANG"), sf.Attributes.Default, sf.Attributes.Supported)
		}
		if c.storefront == "" {
			c.storefront = "us"
		}
	})
	return c.storefront
}

// pickLang turns a locale like de_DE.UTF-8 into a language tag the
// storefront supports, matching the language alone if the region differs;
// "" means the storefront's default.
func pickLang(locale, def string, supported []string) string {
	locale, _, _ = strings.Cut(locale, ".")
	locale, _, _ = strings.Cut(locale, "@")
	tag := strings.ReplaceAll(locale, "_", "-")
	if tag == "" || tag == "C" || tag == "POSIX" {
		return ""
	}
	lang, _, _ := strings.Cut(tag, "-")
	best := ""
	for _, s := range supported {
		switch {
		case strings.EqualFold(s, tag):
			best = s
		case best == "" && strings.EqualFold(strings.SplitN(s, "-", 2)[0], lang):
			best = s
		}
	}
	if strings.EqualFold(best, def) {
		return ""
	}
	return best
}

// localize adds the language to catalog requests.
func (c *Client) localize(path string) string {
	if !strings.HasPrefix(path, "/v1/catalog/") || c.lang == "" || strings.Contains(path, "l=") {
		return path
	}
	sep := "?"
	if strings.Contains(path, "?") {
		sep = "&"
	}
	return path + sep + "l=" + url.QueryEscape(c.lang)
}

func items(rs []resource, kind string, catalog bool) []Item {
	out := make([]Item, 0, len(rs))
	for _, r := range rs {
		out = append(out, item(r, kind, catalog))
	}
	return out
}

func item(r resource, kind string, catalog bool) Item {
	a := r.Attributes
	artist := a.ArtistName
	if kind == KindPlaylist {
		artist = a.CuratorName
	}
	it := Item{Kind: kind, ID: r.ID, Name: a.Name, Artist: artist, Catalog: catalog, Artwork: artworkURL(a.Artwork),
		Editable: a.CanEdit}
	it.Details = attributeDetails(a.Extra)
	it.Favorite, _ = a.Extra["inFavorites"].(bool)
	it.URL = attributeString(a.Extra, "url")
	if previews, ok := a.Extra["previews"].([]any); ok && len(previews) > 0 {
		if p, ok := previews[0].(map[string]any); ok {
			it.PreviewURL = attributeString(p, "url")
		}
	}
	if kind != KindAlbum && kind != KindPlaylist && kind != KindArtist && kind != KindStation {
		it.Route = r.Href
	}
	var info []string
	if len(a.ReleaseDate) >= 4 {
		info = append(info, a.ReleaseDate[:4])
	}
	if a.RecordLabel != "" {
		info = append(info, a.RecordLabel)
	}
	if a.TrackCount > 0 {
		info = append(info, fmt.Sprintf("%d songs", a.TrackCount))
	}
	for _, t := range a.AudioTraits {
		switch t {
		case "lossless", "hi-res-lossless":
			info = append(info, "Lossless")
		case "atmos", "spatial":
			info = append(info, "Dolby Atmos")
		}
	}
	if a.IsLive {
		info = append(info, "live")
	}
	it.Info = strings.Join(dedupe(info), " · ")
	for _, n := range []*notes{a.EditorialNotes, a.Description} {
		if n == nil {
			continue
		}
		if it.Note = plain(n.Short); it.Note == "" {
			it.Note = plain(n.Standard)
		}
		if it.Note != "" {
			break
		}
	}
	return it
}

// kindOf maps an API resource type to an Item kind and whether it is from
// the catalog; ok is false for what brumm does not show (music videos…).
func kindOf(typ string) (kind string, catalog, ok bool) {
	catalog = !strings.HasPrefix(typ, "library-")
	switch strings.TrimPrefix(typ, "library-") {
	case "albums":
		return KindAlbum, catalog, true
	case "playlists":
		return KindPlaylist, catalog, true
	case "artists":
		return KindArtist, catalog, true
	case "stations":
		return KindStation, true, true
	case "music-videos", "playlist-folders", "genres", "station-genres", "activities", "curators", "apple-curators", "record-labels":
		return strings.TrimPrefix(typ, "library-"), catalog, true
	}
	return "", false, false
}

// mixed splits a list of any resources into things to open and songs.
func mixed(rs []resource) ([]Item, []Track) {
	var its []Item
	for _, r := range rs {
		if kind, catalog, ok := kindOf(r.Type); ok {
			its = append(its, item(r, kind, catalog))
		}
	}
	return its, tracks(rs)
}

func tracks(rs []resource) []Track {
	out := make([]Track, 0, len(rs))
	for _, r := range rs {
		if r.Type != "" && r.Type != "songs" && r.Type != "library-songs" {
			continue // music videos and the like do not play as audio
		}
		a := r.Attributes
		id := r.ID
		if a.PlayParams != nil && a.PlayParams.ID != "" {
			id = a.PlayParams.ID
		}
		out = append(out, Track{ID: id, Title: a.Name, Artist: a.ArtistName, Album: a.AlbumName,
			Duration: float64(a.DurationMS) / 1000, Artwork: artworkURL(a.Artwork), Number: a.TrackNumber, Details: attributeDetails(a.Extra), Favorite: a.Extra["inFavorites"] == true})
	}
	return out
}

// Library listings.

func (c *Client) Playlists() ([]Item, error) {
	rs, err := c.all("/v1/me/library/playlists?limit=100&extend=inFavorites", 20)
	return items(rs, KindPlaylist, false), err
}

func (c *Client) Albums() ([]Item, error) {
	rs, err := c.all("/v1/me/library/albums?limit=100&extend=inFavorites", 100)
	return items(rs, KindAlbum, false), err
}

func (c *Client) Artists() ([]Item, error) {
	rs, err := c.all("/v1/me/library/artists?limit=100&extend=inFavorites", 100)
	return items(rs, KindArtist, false), err
}

func (c *Client) Songs() ([]Track, error) {
	rs, err := c.all("/v1/me/library/songs?limit=100&extend=inFavorites", 300)
	return tracks(rs), err
}

// Tracks lists a playlist's or an album's songs.
func (c *Client) Tracks(it Item) ([]Track, error) {
	id := url.PathEscape(it.ID)
	storefront := ""
	if it.Catalog && it.Route != "" && validExploreRoute(it.Route) == nil {
		u, _ := url.Parse(it.Route)
		parts := strings.Split(strings.Trim(u.Path, "/"), "/")
		if len(parts) == 5 && parts[0] == "v1" && parts[1] == "catalog" && parts[3] == it.Kind+"s" && parts[4] == it.ID {
			storefront = parts[2]
		}
	}
	if it.Catalog && storefront == "" {
		storefront = c.Storefront()
	}
	var path string
	switch {
	case it.Catalog && it.Kind == KindPlaylist:
		path = "/v1/catalog/" + storefront + "/playlists/" + id + "/tracks?limit=100"
	case it.Catalog:
		path = "/v1/catalog/" + storefront + "/albums/" + id + "/tracks?limit=100"
	case it.Kind == KindPlaylist:
		path = "/v1/me/library/playlists/" + id + "/tracks?limit=100"
	default:
		path = "/v1/me/library/albums/" + id + "/tracks?limit=100"
	}
	rs, err := c.all(path, 50)
	return tracks(rs), err
}

// ArtistAlbums lists an artist's albums.
func (c *Client) ArtistAlbums(it Item) ([]Item, error) {
	id := url.PathEscape(it.ID)
	path := "/v1/me/library/artists/" + id + "/albums?limit=100"
	if it.Catalog {
		path = "/v1/catalog/" + c.Storefront() + "/artists/" + id + "/albums?limit=100"
	}
	rs, err := c.all(path, 10)
	return items(rs, KindAlbum, it.Catalog), err
}

// AlbumOf finds the album a song belongs to: in the library for library
// songs, in the catalog otherwise.
func (c *Client) AlbumOf(songID string) (Item, error) {
	id := url.PathEscape(songID)
	catalog := !strings.HasPrefix(songID, "i.")
	path := "/v1/me/library/songs/" + id + "/albums"
	if catalog {
		path = "/v1/catalog/" + c.Storefront() + "/songs/" + id + "/albums"
	}
	var p page
	found, err := c.get(path, &p)
	if err != nil {
		return Item{}, err
	}
	if !found || len(p.Data) == 0 {
		return Item{}, errors.New("no album for this song")
	}
	return items(p.Data[:1], KindAlbum, catalog)[0], nil
}

// AddToLibrary adds catalog songs, albums or playlists (kind is "songs",
// "albums" or "playlists") to the library. Apple adds them in the
// background: they show up in library listings a few seconds later. There
// is no way back through the API — removing is done in the Music app.
func (c *Client) AddToLibrary(kind string, ids []string) error {
	switch kind {
	case "songs", "albums", "playlists":
	default:
		return fmt.Errorf("cannot add %s to the library", kind)
	}
	if len(ids) == 0 {
		return nil
	}
	q := url.Values{"ids[" + kind + "]": {strings.Join(ids, ",")}}
	return c.send(http.MethodPost, "/v1/me/library?"+q.Encode(), nil, nil)
}

// ArtistOf finds the artist of a song, in the library or the catalog.
func (c *Client) ArtistOf(songID string) (Item, error) {
	id := url.PathEscape(songID)
	catalog := !strings.HasPrefix(songID, "i.")
	path := "/v1/me/library/songs/" + id + "/artists"
	if catalog {
		path = "/v1/catalog/" + c.Storefront() + "/songs/" + id + "/artists"
	}
	var p page
	found, err := c.get(path, &p)
	if err != nil {
		return Item{}, err
	}
	if !found || len(p.Data) == 0 {
		return Item{}, errors.New("no artist for this song")
	}
	return items(p.Data[:1], KindArtist, catalog)[0], nil
}

// Link is a song's public music.apple.com address.
func (c *Client) Link(songID string) (string, error) {
	id := url.PathEscape(songID)
	path := "/v1/catalog/" + c.Storefront() + "/songs/" + id
	if strings.HasPrefix(songID, "i.") {
		path = "/v1/me/library/songs/" + id + "/catalog"
	}
	var doc struct {
		Data []struct {
			Attributes struct {
				URL string `json:"url"`
			} `json:"attributes"`
		} `json:"data"`
	}
	found, err := c.get(path, &doc)
	if err != nil {
		return "", err
	}
	if !found || len(doc.Data) == 0 || doc.Data[0].Attributes.URL == "" {
		return "", errors.New("this song has no public link")
	}
	return doc.Data[0].Attributes.URL, nil
}

// ParseLink resolves a music.apple.com link to what it points at: an album
// or playlist to open, and the song within it, if the link names one.
func ParseLink(link string) (it Item, songID string, ok bool) {
	u, err := url.Parse(strings.TrimSpace(link))
	if err != nil || !strings.HasSuffix(u.Host, "music.apple.com") {
		return Item{}, "", false
	}
	parts := strings.Split(strings.Trim(u.Path, "/"), "/")
	if len(parts) < 3 {
		return Item{}, "", false
	}
	id := parts[len(parts)-1]
	switch parts[1] {
	case "album":
		return Item{Kind: KindAlbum, ID: id, Name: "Album", Catalog: true}, u.Query().Get("i"), true
	case "playlist":
		return Item{Kind: KindPlaylist, ID: id, Name: "Playlist", Catalog: true}, "", true
	case "artist":
		return Item{Kind: KindArtist, ID: id, Name: "Artist", Catalog: true}, "", true
	case "song":
		return Item{}, id, true
	}
	return Item{}, "", false
}
