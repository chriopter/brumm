package tui

import (
	"fmt"
	"image"
	"image/color"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/art"
)

// lastCover is the size the stage's cover is drawn at.
func (m *Model) lastCover() (s art.Size) {
	m.rendered = map[art.Size][]string{}
	m.View()
	for k := range m.rendered {
		if k.Height > s.Height {
			s = k
		}
	}
	return s
}

// stageW is the stage column's width as last drawn: from the divider to
// the right margin.
func (m *Model) stageW() int { return m.width - margin - m.geo.divider.x1 }

// Whatever the window and the divider, no line is wider than the screen
// and the stage ends at the right margin: there is no empty inset.
func TestLayoutWidths(t *testing.T) {
	for _, size := range [][2]int{{200, 55}, {120, 35}, {80, 24}, {300, 40}, {100, 80}, {60, 30}} {
		for _, split := range []float64{0.1, 0.3, 0.5, 0.7, 0.95} {
			m := playingModel(size[0], size[1])
			m.split = split
			widths(t, m)
			if m.geo.divider == (rect{}) {
				continue // the mini player
			}
			if b := m.geo.bar; b.x1 != m.width-margin || b.x1-b.x0 != m.stageW() {
				t.Fatalf("%v at %.2f: meter %v, stage %d wide", size, split, b, m.stageW())
			}
		}
	}
}

// A cover held back by the height leaves no room beside it: the list
// takes the width, and the stage is the cover's column.
func TestHeightLimitedGivesListTheWidth(t *testing.T) {
	m := playingModel(300, 40)
	m.split = 0.3
	widths(t, m)
	c := m.lastCover()
	if m.stageW() != max(c.Width, ctlMin) {
		t.Fatalf("stage %d wide for a cover %d wide", m.stageW(), c.Width)
	}
	if nav := m.geo.divider.x0 + 1 - margin; nav <= int(0.3*float64(m.width-2*margin)) {
		t.Fatalf("the list is %d wide, no more than asked", nav)
	}
}

// Under a small cover the controls keep their width, the cover in the
// middle of them, and every button is where it is drawn.
func TestControlsMinWidth(t *testing.T) {
	m := playingModel(200, 24)
	m.cellAspect = 2
	m.state.Volume = 0.8
	widths(t, m)
	c := m.lastCover()
	if c.Width >= ctlMin || c.Width == 0 {
		t.Fatalf("cover %v, want one narrower than %d", c, ctlMin)
	}
	b := m.geo.bar
	if b.x1-b.x0 != ctlMin {
		t.Fatalf("meter %d wide, want %d", b.x1-b.x0, ctlMin)
	}
	lines := strings.Split(ansi.Strip(m.View().Content), "\n")
	at := func(r rect) string { return ansi.Cut(lines[r.y0], r.x0, r.x1) }
	for name, want := range map[string]struct {
		r    rect
		icon string
	}{"play": {m.geo.play, icPlay}, "prev": {m.geo.prev, icPrev}, "next": {m.geo.next, icNext},
		"shuffle": {m.geo.shuffle, icShuffle}, "repeat": {m.geo.repeat, icRepeat}, "volume": {m.geo.volume, icVolume}} {
		r := want.r
		if r.x0 < b.x0 || r.x1 > b.x1 || !strings.Contains(at(r), want.icon) {
			t.Fatalf("%s at %v reads %q", name, r, at(r))
		}
		if m.shapeAt(r.x0, r.y0) != shapePointer {
			t.Fatalf("no hand over %s", name)
		}
	}
	if at(m.geo.artist) != "M83" || at(m.geo.album) != "Hurry Up" {
		t.Fatalf("artist %q, album %q", at(m.geo.artist), at(m.geo.album))
	}
}

// The divider sets the cover's size and follows the mouse: left makes it
// bigger until the height holds it, right smaller down to the controls'.
func TestDividerSizesCover(t *testing.T) {
	m := playingModel(240, 50)
	m.cellAspect = 2
	m.split = 0.75
	widths(t, m)
	mid := m.lastCover()
	d := m.geo.divider
	m.Update(tea.MouseClickMsg{X: d.x0 + 2, Y: d.y0 + 2, Button: tea.MouseLeft})
	m.Update(tea.MouseMotionMsg{X: d.x0 + 2 + 10, Y: d.y0 + 2, Button: tea.MouseLeft})
	widths(t, m)
	if m.geo.divider.x0 != d.x0+10 && m.geo.divider.x0 != d.x0+9 {
		t.Fatalf("divider at %d, dragged to %d", m.geo.divider.x0, d.x0+10)
	}
	if small := m.lastCover(); small.Width >= mid.Width {
		t.Fatalf("dragged right: cover %v, was %v", small, mid)
	}
	m.Update(tea.MouseMotionMsg{X: 3, Y: d.y0 + 2, Button: tea.MouseLeft})
	m.Update(tea.MouseReleaseMsg{X: 3, Y: d.y0 + 2, Button: tea.MouseLeft})
	widths(t, m)
	big := m.lastCover()
	if big.Height != m.height-bodyTop-2-1-stageBelow {
		t.Fatalf("dragged left: cover %v, not the full height", big)
	}
	if m.split != m.shownSplit() {
		t.Fatalf("split %.3f saved, %.3f shown", m.split, m.shownSplit())
	}
	m.Update(tea.MouseClickMsg{X: m.geo.divider.x0, Y: d.y0 + 2, Button: tea.MouseLeft})
	m.Update(tea.MouseMotionMsg{X: m.width, Y: d.y0 + 2, Button: tea.MouseLeft})
	widths(t, m)
	if m.stageW() != ctlMin {
		t.Fatalf("dragged right to the end: stage %d wide", m.stageW())
	}
}

// solid is a w×w image: c, with a gray border a quarter wide.
func solid(c color.RGBA) *image.RGBA {
	img := image.NewRGBA(image.Rect(0, 0, 80, 80))
	for y := range 80 {
		for x := range 80 {
			p := c
			if x < 20 || y < 20 {
				p = color.RGBA{128, 128, 128, 255}
			}
			img.Set(x, y, p)
		}
	}
	return img
}

func fg(s lipgloss.Style) color.RGBA { return rgba(s.GetForeground()) }

// The accent is the cover's color, readable on the background; a gray
// cover, nothing playing or the option off give the theme's.
func TestCoverAccent(t *testing.T) {
	m := playingModel(120, 35)
	m.cover = solid(color.RGBA{220, 40, 30, 255})
	for _, bg := range []color.RGBA{{0, 0, 0, 255}, {250, 250, 245, 255}, {30, 30, 46, 255}} {
		m.termBg = bg
		m.View()
		c := fg(m.acc.here)
		if h, _, _ := hsl(float64(c.R)/255, float64(c.G)/255, float64(c.B)/255); h > 0.05 && h < 0.95 {
			t.Fatalf("on %v: accent %v is not red", bg, c)
		}
		if contrast(c, bg) < 4.5 {
			t.Fatalf("accent %v on %v: contrast %.1f", c, bg, contrast(c, bg))
		}
		if fg(m.acc.pill) != c || fg(m.acc.plays) != c || !strings.Contains(m.View().Content, fmt.Sprintf("38;2;%d;%d;%d", c.R, c.G, c.B)) {
			t.Fatal("the accent is not drawn")
		}
	}
	// Once per cover: the same address is not looked at again.
	before := m.acc
	m.cover = solid(color.RGBA{30, 200, 40, 255})
	m.View()
	if fg(m.acc.here) != fg(before.here) {
		t.Fatal("picked again for the same cover")
	}

	theme := fg(sHere)
	m.coverURL, m.cover = "gray.jpg", solid(color.RGBA{128, 128, 128, 255})
	m.View()
	if fg(m.acc.here) != theme || fg(m.acc.plays) != fg(sPlays) {
		t.Fatal("a gray cover got an accent")
	}
	m.coverURL, m.cover = "red.jpg", solid(color.RGBA{220, 40, 30, 255})
	m.opts.NoCoverColors = true
	m.View()
	if fg(m.acc.here) != theme {
		t.Fatal("the option is off, the accent on")
	}
	m.opts.NoCoverColors = false
	m.state.Title, m.state.ID = "", ""
	m.View()
	if fg(m.acc.here) != theme {
		t.Fatal("an accent with nothing playing")
	}
	if c, ok := vivid(solid(color.RGBA{240, 240, 235, 255})); ok {
		t.Fatalf("near white is vivid: %v", c)
	}
}

// track is a queue song of album al.
func track(id, al string) apple.Track {
	return apple.Track{ID: id, Title: "song " + id, Artist: "someone", Album: al, Artwork: al + ".jpg"}
}

// Songs of one album show one cover, the playing album's none, and a
// click on one plays from its first song.
func TestUpcomingDedupes(t *testing.T) {
	q := []apple.Track{track("1", "A"), track("2", "A"), track("3", "A"), track("4", "B"), track("5", "B"),
		track("6", "C"), track("7", "A"), track("8", "B"), track("9", "D"), track("10", "E"), track("11", "F")}
	got := upcoming(q, 10, "A.jpg")
	var names []string
	for _, n := range got {
		names = append(names, fmt.Sprintf("%s@%d", n.name, n.pos))
	}
	if s := strings.Join(names, " "); s != "B@13 C@15 D@18 E@19 F@20" {
		t.Fatalf("up next: %s", s)
	}
	m := playingModel(120, 35)
	m.coverURL = "A.jpg"
	m.nextSeq = 2
	if m.gotNext(nextMsg{seq: 1, tracks: q, pos: 10, ok: true}); m.next != nil {
		t.Fatal("an old answer taken")
	}
	gen := m.nextGen
	if m.gotNext(nextMsg{seq: 2, tracks: q, pos: 10, ok: true}) == nil || len(m.next) != 5 || m.nextGen == gen {
		t.Fatalf("up next %v, no covers asked for", m.next)
	}
	if m.gotNext(nextMsg{seq: 2, tracks: q, pos: 10, ok: true}); m.nextGen != gen+1 {
		t.Fatal("the same covers count as new")
	}
}

// withNext gives m n covers coming up, their images loaded.
func withNext(m *Model, n int) {
	var q []apple.Track
	q = append(q, apple.Track{ID: m.state.ID, Album: m.state.Album, Artist: m.state.Artist, Artwork: m.coverURL})
	for i := range n {
		al := string(rune('A' + i))
		q = append(q, track(fmt.Sprint(i+2), al), track(fmt.Sprint(i+100), al))
		m.covers[al+".jpg"] = solid(color.RGBA{uint8(40 * i), 90, 200, 255})
	}
	m.setNext(upcoming(q, 0, m.coverURL))
	m.View()
	for w := m.nextThumbWant(); w.key != ""; w = m.nextThumbWant() {
		m.thumbs[w.key] = drawCover(m.opts.cover(), m.covers[w.url], w.size)
		m.View()
	}
}

// The covers coming up show only in room to spare, never wider than the
// screen, and each click plays its own.
func TestUpNextStrip(t *testing.T) {
	m := playingModel(200, 55)
	m.cellAspect = 2
	withNext(m, 3)
	widths(t, m)
	if m.geo.upnext[0] != (rect{}) {
		t.Fatal("covers up next with the cover filling the height")
	}
	m.split = 0.72
	withNext(m, 3)
	widths(t, m)
	g := m.geo.upnext
	if g[0] == (rect{}) || g[2] == (rect{}) || g[3] != (rect{}) {
		t.Fatalf("up next at %v", g)
	}
	if g[0].x0 != m.geo.bar.x0 || g[2].x1 > m.geo.bar.x1 || g[0].y0 <= m.geo.play.y0 || g[0].y1 > m.height-2 {
		t.Fatalf("up next at %v, meter %v", g, m.geo.bar)
	}
	for i, want := range []int{1, 3, 5} {
		r := g[i]
		if pos := m.nextAt((r.x0+r.x1)/2, r.y1-1); pos != want {
			t.Fatalf("cover %d plays %d, want %d", i, pos, want)
		}
		if m.shapeAt(r.x0, r.y0) != shapePointer {
			t.Fatal("no hand over a cover up next")
		}
	}
	if m.nextAt(g[0].x1, g[0].y0) != -1 {
		t.Fatal("the gap between covers plays")
	}
	if !strings.Contains(ansi.Strip(m.View().Content), "up next  A") {
		t.Fatal("no label")
	}

	// Drawn once: the next frame splices the same rows.
	if !m.nextDraw.done {
		t.Fatal("covers still missing")
	}
	rows := &m.nextDraw.rows[0]
	m.state.Pos = 100
	m.View()
	if &m.nextDraw.rows[0] != rows {
		t.Fatal("the row was drawn again")
	}

	// Too little room for a cover of nextMinH rows: nothing.
	m.split = 0.5
	for _, h := range []int{30, 35, 40} {
		m.height = h
		widths(t, m)
		_, _, th := m.nextFit(m.stageW(), 0)
		if m.geo.upnext[0] != (rect{}) && m.geo.upnext[0].y1-m.geo.upnext[0].y0 < nextMinH {
			t.Fatalf("height %d: a cover %d rows high", h, th)
		}
	}

	m.height, m.split = 55, 0.72
	m.setNext(upcoming(nil, 0, m.coverURL))
	widths(t, m)
	if m.geo.upnext[0] != (rect{}) {
		t.Fatal("up next with nothing coming")
	}
	withNext(m, 3)
	m.state.Title, m.state.ID = "", ""
	widths(t, m)
	if m.geo.upnext[0] != (rect{}) {
		t.Fatal("up next with nothing playing")
	}
}
