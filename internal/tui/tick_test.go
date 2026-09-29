package tui

import (
	"fmt"
	"image"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/apple"
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
	if !strings.Contains(out, "options") || !strings.Contains(out, "reduce motion") {
		t.Fatal("the menu is missing")
	}
	// A click on a row flips it.
	m.optionsClick(m.geo.options.x0+3, m.geo.optRow0+m.optLines[optMotion])
	if !m.opts.ReduceMotion || m.optSel != optMotion {
		t.Fatal("clicking the reduce motion row did not switch it on")
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

// songList is a list of n songs, each with a cover of its own.
func songList(n int) *Model {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 200, 55
	m.section = secSongs
	var tracks []apple.Track
	for i := range n {
		tracks = append(tracks, apple.Track{ID: fmt.Sprint(i), Title: fmt.Sprint("song ", i), Artwork: fmt.Sprint("cover", i, ".jpg")})
	}
	fill(m.cur(), ipc.Message{Tracks: tracks})
	return m
}

// A free-spinning wheel's notches move the selection at once but are drawn
// a frame at a time, and covers load only once the wheel rests: nothing is
// left queued behind the wheel when it stops.
func TestWheelSpin(t *testing.T) {
	m := songList(600)
	m.View()
	cmds, draws := 0, 0
	for range 500 {
		if _, cmd := m.Update(tea.MouseWheelMsg{Button: tea.MouseWheelDown}); cmd != nil {
			cmds++
		}
		if !m.still {
			draws++
		}
		m.View()
	}
	if got := m.cur().sel; got != 500 {
		t.Fatalf("500 notches down selected row %d", got)
	}
	if cmds > 3 || draws > 5 {
		t.Fatalf("500 notches returned %d commands and drew %d times", cmds, draws)
	}
	if len(m.fetching) > 0 {
		t.Fatalf("a spinning wheel fetches %d covers", len(m.fetching))
	}
	if m.Update(wheelMsg{}); m.still || !strings.Contains(ansi.Strip(m.View().Content), "song 500") {
		t.Fatal("the wheel's frame does not draw where it got to")
	}
	m.wheelAt = time.Now().Add(-wheelRest)
	if _, cmd := m.Update(wheelMsg{}); cmd == nil || len(m.fetching) == 0 || len(m.fetching) > 32 {
		t.Fatalf("the rested wheel fetches %d covers", len(m.fetching))
	}
	if m.wheelTick {
		t.Fatal("the rested wheel keeps ticking")
	}
	// A notch after a pause is drawn at once, one row on.
	m.wheelDrawn = time.Time{}
	m.Update(tea.MouseWheelMsg{Button: tea.MouseWheelUp})
	if m.still || m.cur().sel != 499 {
		t.Fatalf("a lone notch up: still %v, row %d", m.still, m.cur().sel)
	}
}

// Covers stay for every row prefetchCovers reaches: moving within them
// fetches nothing again.
func TestCoversKept(t *testing.T) {
	m := songList(400)
	img := image.NewRGBA(image.Rect(0, 0, 8, 8))
	deliver := func() int {
		n := 0
		for url := range m.fetching {
			m.Update(coverMsg{url, img})
			n++
		}
		return n
	}
	for range 8 {
		m.prefetchCovers(m.cur())
		deliver()
	}
	for i := range 10 {
		m.move("down")
		if n := deliver(); n > 0 {
			t.Fatalf("move %d fetched %d covers again", i, n)
		}
	}
}
