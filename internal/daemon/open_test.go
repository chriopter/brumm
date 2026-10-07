package daemon

import (
	"testing"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/ipc"
)

// Explore and Replay cards carry a route, but bulk song actions consume
// the same complete track response as albums opened from the library.
func TestOpenRoutedContainersReturnsTracks(t *testing.T) {
	for _, kind := range []string{apple.KindAlbum, apple.KindPlaylist} {
		t.Run(kind, func(t *testing.T) {
			it := apple.Item{Kind: kind, ID: "123", Catalog: true, Route: "/v1/catalog/de/" + kind + "s/123"}
			d := &Daemon{lib: &library{Tracks: map[string][]apple.Track{it.Key(): {{ID: "song", Title: "Song"}}}}}
			var reply ipc.Message
			if err := d.open(it, &reply); err != nil {
				t.Fatal(err)
			}
			if reply.Explorer != nil || len(reply.Tracks) != 1 || reply.Tracks[0].ID != "song" {
				t.Fatalf("bulk actions need tracks, got %+v", reply)
			}
		})
	}
}
