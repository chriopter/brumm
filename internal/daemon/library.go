package daemon

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sync"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/config"
)

// library caches the user's playlists and their tracks in memory and on
// disk, so the TUI shows everything at once and the network only refreshes.
type library struct {
	mu        sync.Mutex
	Playlists []apple.Playlist         `json:"playlists"`
	Tracks    map[string][]apple.Track `json:"tracks"`
}

func libraryPath() string { return filepath.Join(config.CacheDir(), "library.json") }

func loadLibrary() *library {
	l := &library{Tracks: map[string][]apple.Track{}}
	if b, err := os.ReadFile(libraryPath()); err == nil {
		_ = json.Unmarshal(b, l)
		if l.Tracks == nil {
			l.Tracks = map[string][]apple.Track{}
		}
	}
	return l
}

func (l *library) save() error {
	l.mu.Lock()
	b, err := json.Marshal(l)
	l.mu.Unlock()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(config.CacheDir(), 0o700); err != nil {
		return err
	}
	tmp := libraryPath() + ".tmp"
	if err := os.WriteFile(tmp, b, 0o600); err != nil {
		return err
	}
	return os.Rename(tmp, libraryPath())
}

func (l *library) playlists() ([]apple.Playlist, bool) {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.Playlists, l.Playlists != nil
}

func (l *library) tracks(id string) ([]apple.Track, bool) {
	l.mu.Lock()
	defer l.mu.Unlock()
	t, ok := l.Tracks[id]
	return t, ok
}

func (l *library) setPlaylists(p []apple.Playlist) {
	l.mu.Lock()
	defer l.mu.Unlock()
	l.Playlists = p
	// Forget playlists that no longer exist.
	keep := make(map[string]bool, len(p))
	for _, pl := range p {
		keep[pl.ID] = true
	}
	for id := range l.Tracks {
		if !keep[id] {
			delete(l.Tracks, id)
		}
	}
}

func (l *library) setTracks(id string, t []apple.Track) {
	l.mu.Lock()
	defer l.mu.Unlock()
	l.Tracks[id] = t
}
