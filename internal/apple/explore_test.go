package apple

import "testing"

func TestExploreRouteValidation(t *testing.T) {
	for _, route := range []string{"https://example.com/v1/me/library/songs", "//example.com/v1/me/library/songs", "/v1/me/../tokens", "/v1/me/%2e%2e/tokens", "/v1/catalog/de/songs#fragment", "/other"} {
		if validExploreRoute(route) == nil {
			t.Errorf("accepted %q", route)
		}
	}
	for _, route := range []string{"/v1/me/library/playlist-folders?filter[identity]=playlistsroot", "/v1/catalog/de/genres", "/v1/storefronts"} {
		if err := validExploreRoute(route); err != nil {
			t.Errorf("rejected %q: %v", route, err)
		}
	}
}

func TestExplorePreservesVideosMetadataAndPagination(t *testing.T) {
	api, _ := fakeAPI(t, map[string]string{"/v1/catalog/de/music-videos": `{"data":[{"id":"v1","type":"music-videos","href":"/v1/catalog/de/music-videos/v1","attributes":{"name":"Video","hasLyrics":true,"composerName":"Composer","audioVariants":["lossless"]}}],"next":"/v1/catalog/de/music-videos?offset=1"}`})
	page, err := api.Explore("/v1/catalog/de/music-videos")
	if err != nil {
		t.Fatal(err)
	}
	if len(page.Items) != 1 || page.Items[0].Kind != "music-videos" || page.Items[0].Route != "/v1/catalog/de/music-videos/v1" {
		t.Fatalf("video lost: %+v", page)
	}
	if len(page.Items[0].Details) != 4 || page.Next != "/v1/catalog/de/music-videos?offset=1" {
		t.Fatalf("metadata/pagination lost: %+v", page)
	}
}
