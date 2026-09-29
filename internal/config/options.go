package config

import (
	"encoding/json"
	"os"
	"path/filepath"
)

// Options are the few switches in the player's options menu, each a plain
// flag. The zero value is the default, so a missing file or field means
// "as shipped". The daemon reads them too (autoplay).
type Options struct {
	Cover       string `json:"cover,omitempty"`       // pixel (default), smooth or original
	CoverSize   string `json:"cover_size,omitempty"`  // large (default), medium or small
	NoBackdrop  bool   `json:"no_backdrop,omitempty"` // no glow of the cover's colors behind it
	NoMeter     bool   `json:"no_meter,omitempty"`
	NoScroll    bool   `json:"no_scroll,omitempty"`
	NoCards     bool   `json:"no_cards,omitempty"`
	NoAutoplay  bool   `json:"no_autoplay,omitempty"`
	AlwaysTips  bool   `json:"always_tips,omitempty"`   // keys on their buttons
	NoBarScroll bool   `json:"no_bar_scroll,omitempty"` // the bar widget's title stays put (read by BarWidget.qml)
	NoVizCycle  bool   `json:"no_viz_cycle,omitempty"`  // the visualizer keeps its style instead of moving on each minute
	VizFPS      int    `json:"viz_fps,omitempty"`       // fullscreen frames per second: 30, 60 (default) or 120
	Bar         string `json:"bar,omitempty"`           // the bar widget as last set or seen: on or off; empty: never known
	BarOffered  bool   `json:"bar_offered,omitempty"`   // the first start asked about the bar widget
}

func optionsPath() string { return filepath.Join(Dir(), "options.json") }

// LoadOptions reads the options; anything unreadable is the defaults.
func LoadOptions() Options {
	var o Options
	if b, err := os.ReadFile(optionsPath()); err == nil {
		_ = json.Unmarshal(b, &o)
	}
	return o
}

func (o Options) Save() error {
	b, err := json.MarshalIndent(o, "", "  ")
	if err != nil {
		return err
	}
	if err := os.MkdirAll(Dir(), 0o700); err != nil {
		return err
	}
	tmp := optionsPath() + ".tmp"
	if err := os.WriteFile(tmp, append(b, '\n'), 0o600); err != nil {
		return err
	}
	return os.Rename(tmp, optionsPath())
}
