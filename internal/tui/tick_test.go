package tui

import (
	"fmt"
	"image"
	"slices"
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
	if d, ok := m.nextTick(); !ok || d < 20*time.Millisecond || d > 505*time.Millisecond {
		t.Fatalf("playing ticks every %v; want the bear's next step or the clock's next second", d)
	}
	m.state.Pos, m.state.Dur, m.stateAt = 10.8, 100, time.Now()
	if d, _ := m.nextTick(); d > 210*time.Millisecond {
		t.Fatalf("0.2 s before the clock's next second, the tick is %v away", d)
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
	if d, ok := m.nextTick(); !ok || d != time.Second/30 {
		t.Fatalf("the fullscreen visualizer draws every %v, want 30 fps", d)
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
	if !strings.Contains(out, "options") || !strings.Contains(out, "Reduce Motion") {
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

// playingList is a list of songs, the fourth one playing and its
// equalizer showing, the spectrum streaming.
func playingList(t *testing.T) *Model {
	m := songList(40)
	m.client, _ = fakeDaemon(t, &[]apple.Track{})
	m.width, m.height = 160, 45
	m.state.Title, m.state.ID, m.state.Dur, m.state.Playing = "song 3", "3", 240, true
	m.state.Pos, m.stateAt = 30.5, time.Now()
	m.cur().sel, m.cur().selAt = 10, -100 // long enough ago that its card is gone
	m.feedSpectrum(make([]int, bands))
	return m
}

// spectrum is one frame of it, different each time.
func spectrum(i int) eventMsg {
	s := make([]int, bands)
	for b := range s {
		s[b] = (i*37 + b*11) % 256
	}
	return eventMsg{Spectrum: s}
}

// While a song plays, spectrum frames and ticks draw only the lines that
// move: never the whole screen, the equalizer at most every eqEvery, and
// what they draw is what a whole screen would show.
func TestPlayingPatches(t *testing.T) {
	m := playingList(t)
	first := m.View().Content
	if !m.eqShown || !m.fr.ok || len(m.fr.live) != 1 || m.fr.meter < 0 || m.fr.times < 0 {
		t.Fatalf("the playing screen is not patchable: ok %v, live %v, meter %d, times %d", m.fr.ok, m.fr.live, m.fr.meter, m.fr.times)
	}
	full := m.fr.draws
	// A burst of spectrum frames within eqEvery: the bars wait.
	frames := 0
	last := first
	for i := range 50 {
		m.Update(spectrum(i))
		if c := m.View().Content; c != last {
			frames, last = frames+1, c
		}
	}
	if m.fr.draws != full || frames > 2 {
		t.Fatalf("50 spectrum frames drew %d whole screens and %d frames", m.fr.draws-full, frames)
	}
	// Once due, the bars move: one line changes, every other one is kept.
	before := slices.Clone(m.fr.lines)
	m.eqAt = time.Now().Add(-eqEvery)
	m.Update(spectrum(99))
	patched := m.View().Content
	changed := 0
	for i, l := range m.fr.lines {
		if l != before[i] {
			changed++
		}
	}
	if m.fr.draws != full || changed != 1 {
		t.Fatalf("a due equalizer drew %d whole screens and changed %d lines", m.fr.draws-full, changed)
	}
	m.motion = false // the same moment drawn whole
	if whole := m.View().Content; whole != patched {
		t.Fatal("the patched screen differs from the whole one")
	}
	// The clock moving on redraws the times, and the patch still matches.
	m.stateAt = m.stateAt.Add(-time.Second)
	m.Update(tickMsg{m.tickSeq})
	patched = m.View().Content
	m.motion = false
	if whole := m.View().Content; whole != patched || !strings.Contains(ansi.Strip(whole), "0:31 / 4:00") {
		t.Fatal("the clock's second is not drawn as a whole screen would")
	}
	// A key draws the whole screen.
	full = m.fr.draws
	m.Update(tea.KeyPressMsg{Code: tea.KeyDown})
	m.View()
	if m.fr.draws != full+1 {
		t.Fatal("a key press did not draw the screen")
	}
}

// A stale tick, replaced by a sooner one, draws nothing.
func TestStaleTickStill(t *testing.T) {
	m := playingList(t)
	m.View()
	m.Update(tickMsg{m.tickSeq - 1})
	if !m.still {
		t.Fatal("a stale tick draws")
	}
}

// The fullscreen visualizer draws on its own clock: spectrum frames in
// between draw nothing, its ticks draw a frame each.
func TestVisualizerDrawsOnTicks(t *testing.T) {
	m := playingList(t)
	m.full = true
	m.Update(tickMsg{m.tickSeq})
	m.View()
	if !m.ticking {
		t.Fatal("the visualizer does not tick")
	}
	for i := range 10 {
		if m.Update(spectrum(i)); !m.still {
			t.Fatal("a spectrum frame draws the visualizer")
		}
	}
	if m.Update(tickMsg{m.tickSeq}); m.still {
		t.Fatal("the visualizer's tick draws nothing")
	}
	if m.fr.draws != 0 {
		t.Fatal("the browser is drawn under the visualizer")
	}
}

// A selected title too long for its row scrolls by patches too, and the
// bear in the footer steps: both as a whole screen would draw them.
func TestScrollPatched(t *testing.T) {
	m := playingList(t)
	m.cur().rows[10].track.Title = strings.Repeat("a very long name ", 20)
	m.View()
	if !m.marquee || len(m.fr.live) != 2 {
		t.Fatalf("the long title does not scroll: live rows %v", m.fr.live)
	}
	full := m.fr.draws
	for range 3 {
		m.start = m.start.Add(-1500 * time.Millisecond) // fifteen frames on: the bear steps too
		m.Update(tickMsg{m.tickSeq})
		patched := m.View().Content
		if m.fr.draws != full {
			t.Fatal("a tick drew the whole screen")
		}
		m.motion = false
		if whole := m.View().Content; whole != patched {
			t.Fatal("the scrolled title differs from a whole screen")
		}
		full = m.fr.draws
	}
}
