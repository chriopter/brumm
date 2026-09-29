package tui

import (
	"strings"
	"testing"
	"time"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/art"
	"github.com/chriopter/brumm/internal/ipc"
)

func TestNextTick(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.cur().loaded = true
	if _, ok := m.nextTick(); ok {
		t.Fatal("an idle, paused player still ticks")
	}
	m.state.Playing = true
	if d, ok := m.nextTick(); !ok || d < 250*time.Millisecond {
		t.Fatalf("playing ticks every %v; the spectrum supplies the frames", d)
	}
	m.marquee = true
	if d, _ := m.nextTick(); d != frameEvery {
		t.Fatalf("a scrolling row ticks every %v, want %v", d, frameEvery)
	}
	m.marquee, m.blurred = false, true
	if d, ok := m.nextTick(); !ok || d != time.Second {
		t.Fatalf("playing in an unfocused terminal ticks every %v, want 1s", d)
	}
	m.state.Playing = false
	if _, ok := m.nextTick(); ok {
		t.Fatal("a paused, unfocused player still ticks")
	}
	m.blurred, m.full, m.state.Playing = false, true, true
	if d, ok := m.nextTick(); !ok || d != time.Second/60 {
		t.Fatalf("the fullscreen visualizer draws every %v, want 60 fps", d)
	}
	m.opts.VizFPS = 120
	if d, _ := m.nextTick(); d != time.Second/120 {
		t.Fatalf("at 120 fps it draws every %v", d)
	}
	m.state.Playing = false
	if d, _ := m.nextTick(); d != 2*frameEvery {
		t.Fatalf("paused, it draws every %v, want only a slow breath", d)
	}
}

// Kitty placeholder cells must measure one cell each, or every line that
// carries a cover would be cut and padded wrong.
func TestKittyPlaceholderWidth(t *testing.T) {
	lines := art.KittyLines(20, art.Size{Width: 7, Height: 3})
	for _, l := range lines {
		if w := lipgloss.Width(l); w != 7 {
			t.Fatalf("placeholder line is %d cells wide, want 7", w)
		}
		if w := lipgloss.Width(ansi.Cut(l, 2, 5)); w != 3 {
			t.Fatalf("cut placeholder line is %d cells wide, want 3", w)
		}
	}
}

func TestOptionsOverlay(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir()) // options are saved on change
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 100, 30
	m.cur().loaded = true
	m.optOpen = true
	out := m.View().Content
	lines := strings.Split(out, "\n")
	if len(lines) != 30 {
		t.Fatalf("%d lines, want 30", len(lines))
	}
	for i, l := range lines {
		if w := lipgloss.Width(l); w > 100 {
			t.Fatalf("line %d is %d wide", i, w)
		}
	}
	if !strings.Contains(out, "options") || !strings.Contains(out, "level meter") {
		t.Fatal("the menu is missing")
	}
	// A click on the second row flips the meter.
	cmd := m.optionsClick(m.geo.options.x0+3, m.geo.optRow0+m.optLines[optMeter])
	_ = cmd
	if !m.opts.NoMeter {
		t.Fatal("clicking the meter row did not switch it off")
	}
	m.optionsKey("esc")
	if m.optOpen {
		t.Fatal("esc did not close the menu")
	}
}

// A switch in the options shows on screen at once, behind the menu.
func TestOptionsApplyAtOnce(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	m.cur().loaded = true
	m.optOpen, m.optSel = true, optKeys
	m.optionsKey("space")
	if !m.opts.AlwaysTips || !strings.Contains(ansi.Strip(m.View().Content), "1 Home") {
		t.Fatal("keys on buttons do not show while the menu is open")
	}
}
