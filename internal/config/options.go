package config

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"strings"
)

// Options are the few switches in the player's options menu, each a plain
// flag. The zero value is the default, so a missing file or field means
// "as shipped". The daemon reads them too (autoplay).
type Options struct {
	Cover         string `json:"cover,omitempty"`           // pixel (default), smooth or original
	NoCoverColors bool   `json:"no_cover_colors,omitempty"` // the theme's accents, not the cover's
	NoAutoplay    bool   `json:"no_autoplay,omitempty"`
	ReduceMotion  bool   `json:"reduce_motion,omitempty"` // names stay put, no cover cards
	AlwaysTips    bool   `json:"always_tips,omitempty"`   // keys on their buttons
	NoBarScroll   bool   `json:"no_bar_scroll,omitempty"` // ReduceMotion, as the bar widget reads it (BarWidget.qml)
	NoVizCycle    bool   `json:"no_viz_cycle,omitempty"`  // the visualizer keeps its style instead of moving on each minute
	VizFPS        int    `json:"viz_fps,omitempty"`       // fullscreen frames per second: 30, 60 (default) or 120
	Bar           string `json:"bar,omitempty"`           // the bar widget as last set or seen: on or off; empty: never known
	BarOffered    bool   `json:"bar_offered,omitempty"`   // the first start asked about the bar widget
	Start         string `json:"start,omitempty"`         // what plain brumm opens: tui (default) or gui
}

// legacy are switches reduce motion took over: any of them set starts it on.
type legacy struct {
	NoScroll bool `json:"no_scroll"`
	NoCards  bool `json:"no_cards"`
}

func optionsPath() string { return filepath.Join(Dir(), "options.json") }

// LoadOptions reads the options; anything unreadable is the defaults.
func LoadOptions() Options {
	var o Options
	if b, err := os.ReadFile(optionsPath()); err == nil {
		var l legacy
		_ = json.Unmarshal(b, &o)
		_ = json.Unmarshal(b, &l)
		o.ReduceMotion = o.ReduceMotion || o.NoBarScroll || l.NoScroll || l.NoCards
	}
	return o
}

// Fields are the options by their JSON names, every one, zero or not:
// what a player sends the daemon to set them all.
func (o Options) Fields() map[string]any {
	out := map[string]any{}
	v, t := reflect.ValueOf(o), reflect.TypeOf(o)
	for i := range t.NumField() {
		name, _, _ := strings.Cut(t.Field(i).Tag.Get("json"), ",")
		out[name] = v.Field(i).Interface()
	}
	return out
}

func (o Options) Save() error {
	o.NoBarScroll = o.ReduceMotion
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
