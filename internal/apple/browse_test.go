package apple

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
)

// fakeAPI answers paths (without query) with canned JSON; the storefront
// is "de", offering German and English.
func fakeAPI(t *testing.T, docs map[string]string) (*Client, *[]string) {
	t.Helper()
	var asked []string
	var mu sync.Mutex
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		mu.Lock()
		asked = append(asked, r.URL.String())
		mu.Unlock()
		if r.URL.Path == "/v1/me/storefront" {
			w.Write([]byte(`{"data":[{"id":"de","attributes":{"defaultLanguageTag":"de-DE","supportedLanguageTags":["de-DE","en-GB"]}}]}`))
			return
		}
		key := r.URL.Path
		if f := r.URL.Query().Get("filter[identity]"); f != "" {
			key += "#" + f
		}
		if f := r.URL.Query().Get("filter[featured]"); f != "" {
			key += "#" + f
		}
		doc, ok := docs[key]
		if !ok {
			http.NotFound(w, r)
			return
		}
		w.Write([]byte(doc))
	}))
	t.Cleanup(srv.Close)
	c := New("dev", "user")
	c.base = srv.URL
	return c, &asked
}

func TestHome(t *testing.T) {
	c, _ := fakeAPI(t, map[string]string{
		"/v1/me/recent/played": `{"data":[{"id":"l.1","type":"library-albums","attributes":{"name":"Blue","artistName":"Joni"}},
			{"id":"ra.9","type":"stations","attributes":{"name":"Joni Radio"}},{"id":"mv","type":"music-videos","attributes":{"name":"x"}}]}`,
		"/v1/me/recent/played/tracks": `{"data":[{"id":"1","type":"songs","attributes":{"name":"River","artistName":"Joni","durationInMillis":240000}}]}`,
		"/v1/me/recommendations": `{"data":[{"attributes":{"title":{"stringForDisplay":"Made for You"}},
			"relationships":{"contents":{"data":[{"id":"pl.1","type":"playlists","attributes":{"name":"Favourites Mix","curatorName":"Apple Music","description":{"short":"<b>Your</b> favourites &amp; more"}}}]}}}]}`,
		"/v1/catalog/de/stations#personal":               `{"data":[{"id":"ra.me","type":"stations","attributes":{"name":"My Station"}}]}`,
		"/v1/catalog/de/stations#apple-music-live-radio": `{"data":[{"id":"ra.1","type":"stations","attributes":{"name":"Apple Music 1","isLive":true}}]}`,
		"/v1/me/recent/radio-stations":                   `{"data":[{"id":"ra.me","type":"stations","attributes":{"name":"My Station"}}]}`,
	})
	shelves, err := c.Home()
	if err != nil {
		t.Fatal(err)
	}
	var titles []string
	for _, s := range shelves {
		titles = append(titles, s.Title)
	}
	if got := strings.Join(titles, "|"); got != "Your music|Recently played|Recently played songs|Made for You|Browse" {
		t.Fatalf("shelves: %s", got)
	}
	if its := shelves[1].Items; len(its) != 4 || its[0].Kind != KindAlbum || its[0].Catalog || its[1].Kind != KindStation || its[2].Kind != KindVideo {
		t.Fatalf("recently played: %+v (music videos must be preserved)", its)
	}
	if n := shelves[3].Items[0].Note; n != "Your favourites & more" {
		t.Fatalf("note: %q", n)
	}
}

func TestRadio(t *testing.T) {
	c, _ := fakeAPI(t, map[string]string{
		"/v1/catalog/de/stations#personal":               `{"data":[{"id":"ra.me","type":"stations","attributes":{"name":"My Station"}}]}`,
		"/v1/catalog/de/stations#apple-music-live-radio": `{"data":[{"id":"ra.1","type":"stations","attributes":{"name":"Apple Music 1","isLive":true}}]}`,
	})
	shelves, err := c.Open(Item{Kind: KindShelf, ID: "radio"})
	if err != nil {
		t.Fatal(err)
	}
	if len(shelves) != 2 || shelves[0].Items[0].ID != "ra.me" || shelves[1].Items[0].Info != "live" {
		t.Fatalf("radio: %+v", shelves)
	}
}

func TestArtist(t *testing.T) {
	c, asked := fakeAPI(t, map[string]string{
		"/v1/me/library/artists/r.5/albums":  `{"data":[{"id":"l.2","type":"library-albums","attributes":{"name":"Court and Spark"}}]}`,
		"/v1/me/library/artists/r.5/catalog": `{"data":[{"id":"55","type":"artists","attributes":{"name":"Joni"}}]}`,
		"/v1/catalog/de/artists/55": `{"data":[{"id":"55","views":{
			"top-songs":{"attributes":{"title":"Top-Songs"},"data":[{"id":"7","type":"songs","attributes":{"name":"Both Sides Now"}}]},
			"similar-artists":{"attributes":{"title":""},"data":[{"id":"66","type":"artists","attributes":{"name":"Carole"}}]}},
			"relationships":{"station":{"data":[{"id":"ra.55","type":"stations","attributes":{"name":"Joni Station"}}]}}}]}`,
	})
	shelves, err := c.Artist(Item{Kind: KindArtist, ID: "r.5", Name: "Joni"})
	if err != nil {
		t.Fatal(err)
	}
	var titles []string
	for _, s := range shelves {
		titles = append(titles, s.Title)
	}
	if got := strings.Join(titles, "|"); got != "Radio|In your library|Top-Songs|Similar artists" {
		t.Fatalf("shelves: %s", got)
	}
	for _, u := range *asked {
		if strings.HasPrefix(u, "/v1/catalog/") && strings.Contains(u, "l=") {
			t.Fatalf("asked for a language the storefront has by default: %s", u)
		}
	}
}

func TestPickLang(t *testing.T) {
	sup := []string{"en-US", "es-MX", "fr-FR"}
	for _, c := range []struct{ locale, want string }{
		{"en_US.UTF-8", ""}, // the default
		{"es_ES.UTF-8", "es-MX"},
		{"fr_FR@euro", "fr-FR"},
		{"de_DE.UTF-8", ""},
		{"C", ""},
		{"", ""},
	} {
		if got := pickLang(c.locale, "en-US", sup); got != c.want {
			t.Errorf("pickLang(%q) = %q, want %q", c.locale, got, c.want)
		}
	}
}

func TestRatingPath(t *testing.T) {
	for _, c := range []struct{ kind, id, want string }{
		{"song", "i.abc", "/v1/me/ratings/library-songs"},
		{"", "123", "/v1/me/ratings/songs"},
		{KindAlbum, "l.x", "/v1/me/ratings/library-albums"},
		{KindAlbum, "144", "/v1/me/ratings/albums"},
		{KindPlaylist, "p.x", "/v1/me/ratings/library-playlists"},
		{KindPlaylist, "pl.x", "/v1/me/ratings/playlists"},
		{KindStation, "ra.x", "/v1/me/ratings/stations"},
	} {
		if got := ratingPath(c.kind, c.id); got != c.want {
			t.Errorf("ratingPath(%q, %q) = %q, want %q", c.kind, c.id, got, c.want)
		}
	}
}
