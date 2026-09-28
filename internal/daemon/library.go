package daemon

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sync"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/config"
)

// library caches everything the TUI browses — the library lists, each
// playlist's and album's tracks, each artist's albums — in memory and on
// disk, so lists open at once and the network only refreshes them.
//
// Keys are a list name (ipc.ListPlaylists, …) or an apple.Item's Key().
type library struct {
	mu     sync.Mutex
	Items  map[string][]apple.Item  `json:"items"`
	Tracks map[string][]apple.Track `json:"tracks"`
}

func libraryPath() string { return filepath.Join(config.CacheDir(), "library.json") }

func loadLibrary() *library {
	l := &library{}
	if b, err := os.ReadFile(libraryPath()); err == nil {
		_ = json.Unmarshal(b, l)
	}
	if l.Items == nil {
		l.Items = map[string][]apple.Item{}
	}
	if l.Tracks == nil {
		l.Tracks = map[string][]apple.Track{}
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

func (l *library) items(key string) ([]apple.Item, bool) {
	l.mu.Lock()
	defer l.mu.Unlock()
	v, ok := l.Items[key]
	return v, ok
}

func (l *library) tracks(key string) ([]apple.Track, bool) {
	l.mu.Lock()
	defer l.mu.Unlock()
	v, ok := l.Tracks[key]
	return v, ok
}

func (l *library) setItems(key string, v []apple.Item) {
	l.mu.Lock()
	defer l.mu.Unlock()
	l.Items[key] = v
}

func (l *library) setTracks(key string, v []apple.Track) {
	l.mu.Lock()
	defer l.mu.Unlock()
	l.Tracks[key] = v
}
