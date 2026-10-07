package apple

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestRoutedContainerTracksFollowsAllPages(t *testing.T) {
	for _, kind := range []string{KindAlbum, KindPlaylist} {
		t.Run(kind, func(t *testing.T) {
			// A US chart card must retain its storefront even when the account
			// normally browses Germany.
			path := "/v1/catalog/us/" + kind + "s/123/tracks"
			srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				switch r.URL.Path {
				case "/v1/me/storefront":
					w.Write([]byte(`{"data":[{"id":"de"}]}`))
				case path:
					if r.URL.Query().Get("offset") == "1" {
						w.Write([]byte(`{"data":[{"id":"second","type":"songs","attributes":{"name":"Second"}}]}`))
					} else {
						w.Write([]byte(`{"data":[{"id":"first","type":"songs","attributes":{"name":"First"}}],"next":"` + path + `?offset=1"}`))
					}
				default:
					http.NotFound(w, r)
				}
			}))
			defer srv.Close()
			c := New("dev", "user")
			c.base = srv.URL
			tracks, err := c.Tracks(Item{Kind: kind, ID: "123", Catalog: true, Route: "/v1/catalog/us/" + kind + "s/123"})
			if err != nil || len(tracks) != 2 || tracks[1].ID != "second" {
				t.Fatalf("all songs must be available to bulk actions: %+v, %v", tracks, err)
			}
		})
	}
}
