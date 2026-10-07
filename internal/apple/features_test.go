package apple

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestFeatureLookupUsesEncodedDocumentedFilters(t *testing.T) {
	c, asked := fakeAPI(t, map[string]string{"/v1/catalog/de/songs": `{"data":[]}`})
	for _, route := range []string{"app:lookup:isrc:US123&ids=bad", "app:lookup:equivalent:123,456"} {
		if _, err := c.Feature(route); err != nil {
			t.Fatal(err)
		}
	}
	if !strings.Contains((*asked)[1], "filter%5Bisrc%5D=US123%26ids%3Dbad") {
		t.Fatal(*asked)
	}
	if !strings.Contains((*asked)[2], "filter%5Bequivalents%5D=123%2C456") {
		t.Fatal(*asked)
	}
}
func TestFeatureFoldersResolvesMetaRootAndOffersCreates(t *testing.T) {
	c, _ := fakeAPI(t, map[string]string{
		"/v1/me/library/playlist-folders#playlistsroot":   `{"data":[],"meta":{"filters":{"identity":{"playlistsroot":[{"id":"p.root","type":"library-playlist-folders","href":"/v1/me/library/playlist-folders/p.root"}]}}}}`,
		"/v1/me/library/playlist-folders/p.root/children": `{"data":[{"id":"p.child","type":"library-playlist-folders","href":"/v1/me/library/playlist-folders/p.child","attributes":{"name":"Child"}}]}`,
	})
	p, err := c.Feature("app:folders")
	if err != nil {
		t.Fatal(err)
	}
	if len(p.Items) != 3 || p.Items[0].Route != "/v1/me/library/playlist-folders/p.child/children" || p.Items[1].Route != "action:create:folder" {
		t.Fatalf("%+v", p)
	}
}
func TestExploreReplayUnwrapsPlayablePeriodSong(t *testing.T) {
	c, _ := fakeAPI(t, map[string]string{"/v1/me/music-summaries": `{"data":[{"id":"replay","type":"music-summaries","attributes":{"year":2025},"views":{"top-songs":{"data":[{"id":"summary","type":"song-period-summaries","relationships":{"song":{"data":[{"id":"123","type":"songs","attributes":{"name":"Song"}}]}}}]}}}]}`})
	p, err := c.Feature("app:replay")
	if err != nil {
		t.Fatal(err)
	}
	if len(p.Items) != 0 || len(p.Shelves) != 1 || len(p.Shelves[0].Tracks) != 1 || p.Shelves[0].Tracks[0].ID != "123" {
		t.Fatalf("%+v", p)
	}
}
func TestCreateFolderHasTypedParentAndFavoriteIsQueryOnly(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Music-User-Token") != "user" || r.Method != "POST" {
			t.Error("wrong authentication or method")
		}
		switch r.URL.Path {
		case "/v1/me/library/playlist-folders":
			var b struct {
				Relationships struct {
					Parent struct{ Data []struct{ ID, Type string } }
				}
				Attributes struct{ Name string }
			}
			if err := json.NewDecoder(r.Body).Decode(&b); err != nil {
				t.Error(err)
			}
			if b.Attributes.Name != "Folder" || len(b.Relationships.Parent.Data) != 1 || b.Relationships.Parent.Data[0].Type != "library-playlist-folders" || b.Relationships.Parent.Data[0].ID != "p.root" {
				t.Errorf("bad body: %+v", b)
			}
			w.WriteHeader(201)
			w.Write([]byte(`{"data":[{"id":"p.new","type":"library-playlist-folders","attributes":{"name":"Folder"}}]}`))
		case "/v1/me/favorites":
			if r.URL.Query().Get("ids[songs]") != "123" || r.URL.Query().Get("ids[library-songs]") != "i.456" {
				t.Error(r.URL.String())
			}
			w.WriteHeader(202)
		default:
			t.Error(r.URL.Path)
			http.NotFound(w, r)
		}
	}))
	defer server.Close()
	c := New("dev", "user")
	c.base = server.URL
	if _, err := c.CreateContainer("folder", "Folder", "p.root"); err != nil {
		t.Fatal(err)
	}
	if err := c.Favorite([]Ref{{Kind: "song", ID: "123"}, {Kind: "song", ID: "i.456"}}); err != nil {
		t.Fatal(err)
	}
}

func TestLibraryPaginationRetainsFavoriteExtensionAndLimit(t *testing.T) {
	calls := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls++
		if r.URL.Query().Get("limit") != "100" || r.URL.Query().Get("extend") != "inFavorites" {
			t.Error("pagination lost requested attributes or page size")
		}
		if r.URL.Query().Get("offset") == "1" {
			w.Write([]byte(`{"data":[{"id":"i.two","type":"library-songs","attributes":{"name":"Two","inFavorites":true}}]}`))
			return
		}
		w.Write([]byte(`{"next":"/v1/me/library/songs?offset=1","data":[{"id":"i.one","type":"library-songs","attributes":{"name":"One","inFavorites":false}}]}`))
	}))
	defer server.Close()
	c := New("dev", "user")
	c.base = server.URL
	tracks, err := c.Songs()
	if err != nil {
		t.Fatal(err)
	}
	if calls != 2 || len(tracks) != 2 || tracks[0].Favorite || !tracks[1].Favorite {
		t.Fatalf("incomplete favorite snapshot: calls=%d tracks=%+v", calls, tracks)
	}
}
