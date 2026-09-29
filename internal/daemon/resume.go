package daemon

import (
	"encoding/json"
	"os"
	"path/filepath"
	"time"

	"github.com/chriopter/brumm/internal/config"
	"github.com/chriopter/brumm/internal/engine"
)

// resume is the last queue and song, kept across restarts so brumm opens
// on it — paused, one press of play away.
type resume struct {
	IDs     []string `json:"ids"`
	Station string   `json:"station,omitempty"` // a station plays instead of IDs
	Source  string   `json:"source"`
	ID      string   `json:"id"`
	Pos     float64  `json:"pos"`
	Dur     float64  `json:"dur"`
	Title   string   `json:"title"`
	Artist  string   `json:"artist"`
	Album   string   `json:"album"`
	Artwork string   `json:"artwork"`
	// Autoplay continues playback after a restart for an update, which
	// happens between songs while music plays.
	Autoplay bool `json:"autoplay,omitempty"`

	savedAt time.Time
}

func resumePath() string { return filepath.Join(config.CacheDir(), "resume.json") }

func loadResume() *resume {
	b, err := os.ReadFile(resumePath())
	if err != nil {
		return nil
	}
	var r resume
	if json.Unmarshal(b, &r) != nil || !r.playable() {
		return nil
	}
	return &r
}

func (r *resume) save() {
	r.savedAt = time.Now()
	b, err := json.Marshal(r)
	if err != nil {
		return
	}
	if os.MkdirAll(config.CacheDir(), 0o700) != nil {
		return
	}
	tmp := resumePath() + ".tmp"
	if os.WriteFile(tmp, b, 0o600) == nil {
		_ = os.Rename(tmp, resumePath())
	}
}

// track follows the player: the song, where it is, what it looks like.
// It writes to disk when the song changes and every few seconds.
func (r *resume) track(es engine.State) {
	if es.ID == "" || es.Preview != nil {
		return
	}
	changed := es.ID != r.ID
	moved := es.Pos != r.Pos
	r.ID, r.Pos, r.Dur = es.ID, es.Pos, es.Dur
	r.Title, r.Artist, r.Album, r.Artwork = es.Title, es.Artist, es.Album, es.Artwork
	if es.Source != "" {
		r.Source = es.Source
	}
	if changed || (moved && time.Since(r.savedAt) > 5*time.Second) { // paused: nothing to write
		r.save()
	}
}

func (r *resume) playable() bool { return r.Station != "" || (r.ID != "" && len(r.IDs) > 0) }

// play starts what was playing: the station, or the queue at the song and
// second it stopped.
func (r *resume) play(eng *engine.Engine) error {
	if r.Station != "" {
		return eng.PlayStation(r.Station, r.Source)
	}
	return eng.PlayIDs(r.IDs, r.ID, r.Source, r.Pos)
}

// overlay shows the saved song on an idle player.
func (r *resume) overlay(es engine.State) engine.State {
	es.ID, es.Source, es.Pos, es.Dur = r.ID, r.Source, r.Pos, r.Dur
	es.Title, es.Artist, es.Album, es.Artwork = r.Title, r.Artist, r.Album, r.Artwork
	return es
}
