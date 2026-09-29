package tui

import (
	"errors"
	"image"
	"image/color"
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/art"
	"github.com/chriopter/brumm/internal/ipc"
	"github.com/chriopter/brumm/internal/update"
)

// playingModel is a w×h screen playing a song with a two-color cover.
func playingModel(w, h int) *Model {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = w, h
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

// Every cover size keeps the lines as wide as ever, and the smaller ones
// shrink the column, which stays in the middle of the stage.
func TestCoverSizes(t *testing.T) {
	m := playingModel(200, 55)
	m.opts.CoverSize = sizeLarge
	want := widths(t, m)
	large, mid := m.geo.bar, (m.geo.bar.x0+m.geo.bar.x1)/2
	var last rect
	for _, s := range coverSizes {
		m.opts.CoverSize = s
		got := widths(t, m)
		for i := range got {
			if got[i] != want[i] {
				t.Fatalf("%s: line %d is %d wide, %d at large", s, i, got[i], want[i])
			}
		}
		b := m.geo.bar
		if last != (rect{}) && b.x1-b.x0 <= last.x1-last.x0 {
			t.Fatalf("%s is no wider than the size before it: %v, %v", s, b, last)
		}
		if c := (b.x0 + b.x1) / 2; c < mid-1 || c > mid+1 {
			t.Fatalf("%s: column centered at %d, large at %d", s, c, mid)
		}
		if !m.geo.play.has(m.geo.play.x0, b.y0+3) {
			t.Fatalf("%s: play button %v not under the meter %v", s, m.geo.play, b)
		}
		last = b
	}
	if last != large {
		t.Fatalf("large is %v, was %v", last, large)
	}
	m.opts.CoverSize = "" // an options file from before: large
	widths(t, m)
	if m.geo.bar != large {
		t.Fatal("no size set is not large")
	}
}

// The backdrop changes colors only: the same text in the same places,
// every line as wide, the cover's own lines drawn untouched, a card too.
func TestBackdrop(t *testing.T) {
	for _, bg := range []color.Color{color.Black, color.RGBA{0xee, 0xee, 0xe8, 0xff}} {
		m := playingModel(200, 55)
		m.opts.NoBackdrop = true
		m.termBg = bg
		off, plain := widths(t, m), ansi.Strip(m.View().Content)
		m.opts.NoBackdrop = false
		on := widths(t, m)
		out := m.View().Content
		for i := range on {
			if on[i] != off[i] {
				t.Fatalf("line %d is %d wide, %d without the backdrop", i, on[i], off[i])
			}
		}
		if ansi.Strip(out) != plain {
			t.Fatal("the backdrop changed the text")
		}
		if !strings.Contains(out, "\x1b[48;2;") {
			t.Fatal("no backdrop drawn")
		}
		size := m.lastCover()
		for _, l := range m.stageCover(size) {
			if !strings.Contains(out, l) {
				t.Fatal("a cover line was changed")
			}
		}
		if m.backdrop == nil || m.stageBackdrop(m.backdrop.w, m.backdrop.h) != m.backdrop {
			t.Fatal("the backdrop is worked out again for the same cover")
		}
	}
	m := playingModel(200, 55)
	m.termBg = color.Black
	m.state.Title = "" // nothing plays: no glow
	if strings.Contains(m.View().Content, "\x1b[48;2;") {
		t.Fatal("a backdrop with nothing playing")
	}
}

// lastCover is the size the stage's cover was last drawn at.
func (m *Model) lastCover() (s art.Size) {
	for k := range m.rendered {
		if k.Height > s.Height {
			s = k
		}
	}
	return s
}

// Text with a background of its own keeps it; the backdrop shows through
// everywhere else, and is set again after the text resets.
func TestBackdropPaint(t *testing.T) {
	m := playingModel(80, 30)
	m.termBg = color.Black
	b := newBackdrop(m.cover, "x", color.RGBA{0, 0, 0, 255}, 10, 4, 2)
	s := b.paint(1, 3, "a\x1b[7mb\x1b[27mc\x1b[mde")
	if ansi.Strip(s) != "abcde" || lipgloss.Width(s) != 5 {
		t.Fatalf("painted %q", s)
	}
	if i := strings.Index(s, "\x1b[7m"); strings.Contains(s[i:strings.Index(s, "b")], "48;2") {
		t.Fatalf("reverse text got the backdrop: %q", s)
	}
	if !strings.Contains(s[strings.Index(s, "\x1b[m"):], "48;2") {
		t.Fatalf("no backdrop after a reset: %q", s)
	}
	if sgrOwnBg("38;2;48;1;2", false) {
		t.Fatal("a foreground's 48 read as a background")
	}
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
	if strings.Contains(m.View().Content, "bar widget") || m.optLines[optBar] != -1 {
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
	if !strings.Contains(ansi.Strip(m.View().Content), "bar widget") {
		t.Fatal("no bar widget row")
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

// BenchmarkView is a whole frame of the playing screen at 200×55, with and
// without the backdrop, the text changing as the clock runs.
func BenchmarkView(b *testing.B) {
	for _, name := range []string{"plain", "backdrop"} {
		b.Run(name, func(b *testing.B) {
			m := playingModel(200, 55)
			m.termBg = color.Black
			m.opts.NoBackdrop = name == "plain"
			m.View()
			b.ResetTimer()
			for i := range b.N {
				m.state.Pos = float64(i % 240)
				m.View()
			}
		})
	}
}
