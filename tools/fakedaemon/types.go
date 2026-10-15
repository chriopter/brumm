// Types for the development fixture only. Production models live in Rust.
package main

import (
	"encoding/json"
	"os"
	"path/filepath"
)

type Status string

const (
	CmdExplore    = "explore"
	CmdHome       = "home"
	CmdList       = "list"
	CmdOpen       = "open"
	CmdOptions    = "options"
	CmdPlace      = "place"
	CmdPlay       = "play"
	CmdQueue      = "queue"
	CmdRatings    = "ratings"
	CmdSearch     = "search"
	CmdSeek       = "seek"
	CmdShow       = "show"
	CmdStation    = "station"
	CmdSubscribe  = "subscribe"
	CmdToggle     = "toggle"
	CmdUpdate     = "update"
	KindAlbum     = "album"
	KindArtist    = "artist"
	KindPlaylist  = "playlist"
	KindShelf     = "shelf"
	KindStation   = "station"
	ListAlbums    = "albums"
	ListArtists   = "artists"
	ListPlaylists = "playlists"
	StatusReady   = "ready"
)

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

type Detail struct {
	Label string `json:"label"`
	Value string `json:"value"`
}

type ExplorePage struct {
	Visuals []Item   `json:"visuals,omitempty"`
	Title   string   `json:"title,omitempty"`
	Info    string   `json:"info,omitempty"`
	Details []Detail `json:"details,omitempty"`
	Shelves []Shelf  `json:"shelves,omitempty"`
	Items   []Item   `json:"items,omitempty"`
	Tracks  []Track  `json:"tracks,omitempty"`
	Next    string   `json:"next,omitempty"`
}

type Shelf struct {
	Route  string  `json:"route,omitempty"`
	Title  string  `json:"title"`
	Items  []Item  `json:"items,omitempty"`
	Tracks []Track `json:"tracks,omitempty"`
}

type Ref struct {
	Kind string `json:"kind,omitempty"`
	ID   string `json:"id"`
}

type Player struct {
	Ready     bool    `json:"ready"`
	Err       string  `json:"err"`
	NeedsAuth bool    `json:"needsAuth"`
	Playing   bool    `json:"playing"`
	Title     string  `json:"title"`
	Artist    string  `json:"artist"`
	Album     string  `json:"album"`
	Artwork   string  `json:"artwork"`
	Pos       float64 `json:"pos"`
	Dur       float64 `json:"dur"`
	Index     int     `json:"index"`
	Length    int     `json:"length"`
	ID        string  `json:"id"`     // the playing song
	Source    string  `json:"source"` // key of the list it was started from
	Volume    float64 `json:"volume"`
	Shuffle   bool    `json:"shuffle"`
	Repeat    int     `json:"repeat"` // 0 off, 1 one, 2 all
	Preview   *Clip   `json:"preview,omitempty"`
}

type Clip struct {
	Title   string `json:"title"`
	Artist  string `json:"artist"`
	Artwork string `json:"artwork"`
}

type Options struct {
	Cover         string `json:"cover,omitempty"`           // pixel (default), smooth or original
	NoCoverColors bool   `json:"no_cover_colors,omitempty"` // the theme's accents, not the cover's
	NoAutoplay    bool   `json:"no_autoplay,omitempty"`
	ReduceMotion  bool   `json:"reduce_motion,omitempty"` // names stay put, no cover cards
	AlwaysTips    bool   `json:"always_tips,omitempty"`   // keys on their buttons
	NoBarScroll   bool   `json:"no_bar_scroll,omitempty"` // ReduceMotion, as the bar widget reads it (BarWidget.qml)
	NoVizCycle    bool   `json:"no_viz_cycle,omitempty"`  // the visualizer keeps its style instead of moving on each minute
	VizFPS        int    `json:"viz_fps,omitempty"`       // fullscreen frames per second: 30 (default), 60, 120, or -1 for the screen's rate
	Bar           string `json:"bar,omitempty"`           // the bar widget as last set or seen: on or off; empty: never known
	BarOffered    bool   `json:"bar_offered,omitempty"`   // the first start asked about the bar widget
	Start         string `json:"start,omitempty"`         // what plain brumm opens: tui (default) or gui
	MusicKeys     bool   `json:"music_keys,omitempty"`    // SUPER+SHIFT+M (and +ALT) open brumm, not Omarchy's music apps
}

type Request struct {
	Generation int64    `json:"generation,omitempty"`
	Saved      *bool    `json:"saved,omitempty"`
	ID         int      `json:"id"`
	Cmd        string   `json:"cmd"`
	List       string   `json:"list,omitempty"`
	Item       *Item    `json:"item,omitempty"`
	Query      string   `json:"query,omitempty"`
	IDs        []string `json:"ids,omitempty"`
	Start      string   `json:"start,omitempty"`
	Source     string   `json:"source,omitempty"`
	Value      float64  `json:"value,omitempty"`
	Refs       []Ref    `json:"refs,omitempty"`
	Bands      int      `json:"bands,omitempty"`
	Wave       int      `json:"wave,omitempty"` // waveform samples wanted per frame
	FPS        int      `json:"fps,omitempty"`  // spectrum frames per second wanted; 25 by default
	// Options changes the named options, by their JSON names; the rest
	// stay as they are.
	Options map[string]any `json:"options,omitempty"`
	// Place is where a player is, for the other one to open there: kept
	// by the daemon as it comes, never read by it.
	Place json.RawMessage `json:"place,omitempty"`
}

type State struct {
	Status  Status `json:"status"`
	Message string `json:"message,omitempty"` // progress or error text
	// ExpiresIn is set when this build's Apple Music access runs out soon
	// (days left); an update brings a fresh one.
	ExpiresIn int `json:"expiresIn,omitempty"`
	// Update names a newer release when one is available.
	Update string `json:"update,omitempty"`
	Player
}

type Message struct {
	Generation int64           `json:"generation,omitempty"`
	Favorites  map[string]bool `json:"favorites,omitempty"`
	Provider   json.RawMessage `json:"provider,omitempty"`
	ID         int             `json:"id,omitempty"`
	Error      string          `json:"error,omitempty"`
	Items      []Item          `json:"items,omitempty"`
	Tracks     []Track         `json:"tracks,omitempty"`
	Shelves    []Shelf         `json:"shelves,omitempty"`
	Explorer   *ExplorePage    `json:"explorer,omitempty"`
	Ratings    map[string]int  `json:"ratings,omitempty"`
	State      *State          `json:"state,omitempty"`
	Spectrum   []int           `json:"spectrum,omitempty"`
	Wave       []int           `json:"wave,omitempty"`
	Library    bool            `json:"library,omitempty"` // the cached library changed
	Pos        int             `json:"pos,omitempty"`
	IDs        []string        `json:"ids,omitempty"`
	Link       string          `json:"link,omitempty"`
	Version    string          `json:"version,omitempty"` // the daemon's own, with an update check
	Notes      []string        `json:"notes,omitempty"`   // with an update check: what the newest release says is new
	Options    *Options        `json:"options,omitempty"` // the saved options, answering or announcing a change
	Omarchy    bool            `json:"omarchy,omitempty"` // with the options: omarchy is here, the bar widget can be switched
	Agent      bool            `json:"agent,omitempty"`   // with the options: Omarchy has a coding agent to hand feedback to
	Place      json.RawMessage `json:"place,omitempty"`   // where a player last was (CmdPlace)
}

func socketPath() string {
	if dir := os.Getenv("XDG_RUNTIME_DIR"); dir != "" {
		return filepath.Join(dir, "brumm.sock")
	}
	dir := os.Getenv("XDG_CACHE_HOME")
	if dir == "" {
		home, _ := os.UserHomeDir()
		dir = filepath.Join(home, ".cache")
	}
	return filepath.Join(dir, "brumm", "brumm.sock")
}
