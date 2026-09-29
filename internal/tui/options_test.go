package tui

import (
	"errors"
	"image"
	"image/color"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/ipc"
	"github.com/chriopter/brumm/internal/update"
)

// playingModel is a w×h screen playing a song with a two-color cover.
func playingModel(w, h int) *Model {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height, m.split = w, h, 0.5
	m.state.Title, m.state.Artist, m.state.Album, m.state.ID, m.state.Dur = "Midnight City", "M83", "Hurry Up", "1", 240
	img := image.NewRGBA(image.Rect(0, 0, 60, 60))
	for y := range 60 {
		for x := range 60 {
			img.Set(x, y, color.RGBA{uint8(200 * x / 60), 40, uint8(200 * y / 60), 255})
		}
	}
	m.coverURL, m.cover = "now.jpg", img
	return m
}

// widths is each line's width, and fails on one wider than the screen.
func widths(t *testing.T, m *Model) []int {
	t.Helper()
	lines := strings.Split(m.View().Content, "\n")
	if len(lines) != m.height {
		t.Fatalf("%d lines, want %d", len(lines), m.height)
	}
	out := make([]int, len(lines))
	for i, l := range lines {
		if out[i] = lipgloss.Width(l); out[i] > m.width {
			t.Fatalf("line %d is %d wide", i, out[i])
		}
	}
	return out
}

// fakeOmarchy stands in for the omarchy command, answering list with
// enabled and recording the rest.
func fakeOmarchy(t *testing.T, enabled bool, fail error) *[]string {
	t.Helper()
	var calls []string
	old, oldHas := update.Omarchy, hasOmarchy
	update.Omarchy = func(args ...string) ([]byte, error) {
		cmd := strings.Join(args, " ")
		if cmd == "plugin list --json" {
			return []byte(`[{"id":"chriopter.brumm","enabled":` + map[bool]string{true: "true", false: "false"}[enabled] + `}]`), nil
		}
		calls = append(calls, cmd)
		if fail != nil {
			return []byte("omarchy-plugin-enable: " + fail.Error() + "\n"), fail
		}
		return nil, nil
	}
	hasOmarchy = func() bool { return true }
	t.Cleanup(func() { update.Omarchy, hasOmarchy = old, oldHas })
	return &calls
}

// The bar widget's row shows only with omarchy, reads the shell, and
// switches the widget; a refusal takes the tick back and says why.
func TestBarOption(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	m.optOpen = true
	if strings.Contains(m.View().Content, "top bar player") || m.optLines[optBar] != -1 {
		t.Fatal("the bar widget's row shows without omarchy")
	}
	for range numOptions {
		if m.optionsKey("down"); m.optSel == optBar {
			t.Fatal("the hidden row can be selected")
		}
	}

	calls := fakeOmarchy(t, true, nil)
	m = newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	m.opts.BarOffered = true
	m.barRead(m.readBar()().(barMsg))
	if !m.barOn || m.barAsk {
		t.Fatalf("on %v, asking %v", m.barOn, m.barAsk)
	}
	m.optOpen = true
	if !strings.Contains(ansi.Strip(m.View().Content), "top bar player") {
		t.Fatal("no top bar player row")
	}
	m.optSel = optBar
	msg := m.optionsKey("space")().(barSetMsg)
	m.barSet(msg)
	if m.barOn || m.opts.Bar != "off" || strings.Join(*calls, ",") != "plugin disable chriopter.brumm" {
		t.Fatalf("on %v, saved %q, ran %q", m.barOn, m.opts.Bar, *calls)
	}
	if loadOptions().Bar != "off" {
		t.Fatal("not saved")
	}

	fakeOmarchy(t, false, errors.New("plugin 'chriopter.brumm' is not known"))
	m.barSet(m.optionsKey("space")().(barSetMsg))
	if m.barOn || !strings.Contains(m.flash, "is not known") || strings.Contains(m.flash, "omarchy-plugin") {
		t.Fatalf("on %v, flash %q", m.barOn, m.flash)
	}
}

// The first start asks once to turn the widget on, after signing in; yes
// turns it on, and neither answer is asked for again.
func TestBarOffer(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	calls := fakeOmarchy(t, false, nil)
	m := newModel(nil, ipc.State{Status: ipc.StatusLoggedOut})
	m.width, m.height = 120, 35
	m.barRead(m.readBar()().(barMsg))
	if strings.Contains(m.View().Content, "Omarchy bar") {
		t.Fatal("asked while signed out")
	}
	m.state.Status = ipc.StatusReady
	if !strings.Contains(ansi.Strip(m.View().Content), "Show brumm in the Omarchy bar?") {
		t.Fatal("not asked")
	}
	if m.barKey("x") != nil || !m.barAsking() {
		t.Fatal("another key answered")
	}
	m.barSet(m.barKey("enter")().(barSetMsg))
	if !m.barOn || m.barAsking() || strings.Join(*calls, ",") != "plugin enable chriopter.brumm" {
		t.Fatalf("on %v, asking %v, ran %q", m.barOn, m.barAsking(), *calls)
	}

	m = newModel(nil, ipc.State{Status: ipc.StatusReady}) // the next start
	m.barRead(barMsg{on: false, ok: true})
	if m.barAsking() {
		t.Fatal("asked again")
	}
}

// The menu is these rows in this order, each with its sentence under
// them for the selected one, and fits an 80-column window.
func TestOptionsMenu(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	fakeOmarchy(t, false, nil)
	m := playingModel(80, 24)
	m.optOpen = true
	m.View()
	want := []string{"cover", "cover colors", "autoplay", "reduce motion", "show shortcuts", "top bar player"}
	for i := range numOptions {
		m.optSel = i
		lines := strings.Split(ansi.Strip(m.View().Content), "\n")
		for j, l := range lines {
			if lipgloss.Width(l) > 80 {
				t.Fatalf("line %d is %d wide", j, lipgloss.Width(l))
			}
		}
		box := strings.Join(lines[m.geo.options.y0:m.geo.options.y1], "\n")
		if m.geo.options.x1 > 80 {
			t.Fatalf("the menu ends at %d", m.geo.options.x1)
		}
		row := lines[m.geo.optRow0+m.optLines[i]]
		if !strings.Contains(row, "▌  "+want[i]) {
			t.Fatalf("row %d reads %q", i, row)
		}
		h := optHints[i]
		if !strings.Contains(box, h) || h[0] < 'A' || h[0] > 'Z' || !strings.HasSuffix(h, ".") {
			t.Fatalf("hint %q for %s", h, want[i])
		}
		if !strings.Contains(box, "↑↓ move   space change   esc close") {
			t.Fatal("no keys")
		}
		for j := range numOptions {
			if j != i && strings.Contains(box, optHints[j]) {
				t.Fatalf("%s shows the hint of %s", want[i], want[j])
			}
		}
	}
	for i, r := range m.optLines {
		if i > 0 && r != m.optLines[i-1]+1 {
			t.Fatalf("rows not in order, one under the other: %v", m.optLines)
		}
	}
}

// Without kitty graphics original is no choice, and a saved original
// draws as smooth; with them it is the third.
func TestCoverChoices(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	old := hasKitty
	t.Cleanup(func() { hasKitty = old })
	m := playingModel(100, 30)
	m.optOpen = true
	hasKitty = false
	m.opts.Cover = coverOriginal
	if m.opts.cover() != coverSmooth || strings.Contains(m.View().Content, "original") {
		t.Fatalf("original offered without kitty, drawn as %s", m.opts.cover())
	}
	m.optSel = optCover
	for _, want := range []string{coverPixel, coverSmooth, coverPixel} {
		if m.optionsKey("right"); m.opts.Cover != want {
			t.Fatalf("stepped to %s, want %s", m.opts.Cover, want)
		}
	}
	hasKitty = true
	m.optionsKey("left")
	if m.opts.Cover != coverOriginal || !strings.Contains(m.View().Content, "original") {
		t.Fatalf("left from pixel: %s", m.opts.Cover)
	}
}

// Any switch reduce motion took over starts it on, and the bar widget
// reads it as no_bar_scroll; off, that goes too.
func TestReduceMotionMigrates(t *testing.T) {
	for _, old := range []string{`{"no_scroll":true}`, `{"no_cards":true}`, `{"no_bar_scroll":true}`, `{"cover":"smooth"}`} {
		dir := t.TempDir()
		t.Setenv("XDG_CONFIG_HOME", dir)
		path := filepath.Join(dir, "brumm", "options.json")
		os.MkdirAll(filepath.Dir(path), 0o700)
		os.WriteFile(path, []byte(old), 0o600)
		m := newModel(nil, ipc.State{Status: ipc.StatusReady})
		on := old != `{"cover":"smooth"}`
		if m.opts.ReduceMotion != on {
			t.Fatalf("%s: reduce motion %v", old, m.opts.ReduceMotion)
		}
		b, _ := os.ReadFile(path)
		if on != strings.Contains(strings.ReplaceAll(string(b), " ", ""), `"no_bar_scroll":true`) {
			t.Fatalf("%s: saved %s", old, b)
		}
		m.optSel = optMotion
		m.optionsKey("space")
		b, _ = os.ReadFile(path)
		if loadOptions().ReduceMotion == on || strings.Contains(string(b), "no_bar_scroll") == on || strings.Contains(string(b), "no_scroll") {
			t.Fatalf("%s: switched, saved %s", old, b)
		}
	}
}

// Reduce motion keeps a long selected name still and lays no card on
// the cover.
func TestReduceMotion(t *testing.T) {
	m := songList(20)
	m.cur().rows[0].track.Title = strings.Repeat("a very long name ", 20)
	m.View()
	if !m.marquee {
		t.Fatal("a long name does not scroll")
	}
	m.opts.ReduceMotion = true
	m.View()
	if m.marquee || m.selectedCard() != nil {
		t.Fatal("moving with reduce motion on")
	}
}

// The spectrum streams only while the playing row's bars show in a
// focused window; without it they wobble on their own.
func TestSpectrumOnlyWhenSeen(t *testing.T) {
	m := playingModel(120, 35)
	m.state.Playing = true
	m.eqShown = true
	m.subscribe()
	if !m.specOn {
		t.Fatal("no spectrum for the bars in view")
	}
	m.feedSpectrum(make([]int, bands))
	if d, _ := m.nextTick(); d != 500*time.Millisecond {
		t.Fatalf("ticks every %v with the spectrum streaming", d)
	}
	m.eqShown = false
	if m.subscribe(); m.specOn || m.spec != nil {
		t.Fatal("the spectrum streams with the bars out of view")
	}
	m.eqShown, m.blurred = true, true
	if m.subscribe(); m.specOn {
		t.Fatal("the spectrum streams in the background")
	}
	m.blurred = false
	if d, _ := m.nextTick(); d != 125*time.Millisecond {
		t.Fatalf("no spectrum yet: ticks every %v, want the wobble", d)
	}
}

// BenchmarkView is a whole frame of the playing screen at 200×55, the
// text changing as the clock runs: the cover filling the height, and a
// smaller one with the covers coming up under it.
func BenchmarkView(b *testing.B) {
	for _, name := range []string{"plain", "upnext"} {
		b.Run(name, func(b *testing.B) {
			m := playingModel(200, 55)
			m.termBg = color.Black
			if name == "upnext" {
				m.split = 0.7
				withNext(m, 4)
			}
			m.View()
			b.ResetTimer()
			for i := range b.N {
				m.state.Pos = float64(i % 240)
				m.View()
			}
		})
	}
}
