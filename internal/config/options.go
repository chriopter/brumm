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
	Cover       string `json:"cover,omitempty"` // pixel (default), smooth or original
	NoMeter     bool   `json:"no_meter,omitempty"`
	NoScroll    bool   `json:"no_scroll,omitempty"`
	NoCards     bool   `json:"no_cards,omitempty"`
	NoAutoplay  bool   `json:"no_autoplay,omitempty"`
	AlwaysTips  bool   `json:"always_tips,omitempty"`   // keys on their buttons
	NoBarScroll bool   `json:"no_bar_scroll,omitempty"` // the bar widget's title stays put (read by BarWidget.qml)
	VizFPS      int    `json:"viz_fps,omitempty"`       // fullscreen frames per second: 30, 60 (default) or 120
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
