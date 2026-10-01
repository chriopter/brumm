package tui

import (
	"slices"
	"strings"
	"time"

	"github.com/chriopter/brumm/internal/ipc"
)

// While a song plays, almost every update only moves time on: a spectrum
// frame, a tick. What that changes on screen is a handful of lines — the
// playing row's equalizer, the selected row's scrolling title, the meter,
// the times, the bear in the footer. So the last full screen is kept as
// lines, as the visualizer keeps its rows (omacom/ttfx does the same):
// such an update rebuilds only those lines and hands every other one back
// as the same string, and when none of them changed, no frame is drawn at
// all. Anything else draws the whole screen again.

// eqEvery is how often the playing row's equalizer moves: ten times a
// second is plenty for four little bars.
const eqEvery = 90 * time.Millisecond

// frame is the last full screen and where on it the moving parts sit.
type frame struct {
	ok          bool     // a patch may build on it
	lines       []string // the screen, a line each: a blank, the body rows, a blank, the footer
	left, right []string // the body rows' list part and stage part
	pad, gap    string   // before the list, between list and stage
	w, h        int      // the screen's size it was drawn at
	v           *view    // the list drawn, where it was scrolled and selected
	off, sel    int
	listW       int // its rows' width, and the width between its borders
	inner       int
	live        []liveRow // list rows that move by themselves
	colW        int       // the stage's width
	meter       int       // body rows of the meter and the times, -1 when not shown
	times       int
	bear        string // the bear in the footer as drawn
	frameAt     int    // the animation clock as drawn
	clock       clock3 // the playhead as drawn
	draws       int    // whole screens drawn, for tests
}

// liveRow is a list row that moves: the playing song's (its equalizer) or
// the selected one while its title scrolls.
type liveRow struct {
	body, i    int
	eq, scroll bool
}

// clock3 is what the meter and the times show of the playhead: the
// seconds played and left, and the meter's half cells.
type clock3 [3]int

// clockAt is the clock now, for a meter w wide.
func (m *Model) clockAt(w int) clock3 {
	pos, dur := m.position(), m.state.Dur
	c := clock3{int(pos), int(max(0, dur-pos))}
	if dur > 0 {
		c[2] = int(float64(2*w) * pos / dur)
	}
	return c
}

// calm says nothing on the screen moves but what a patch redraws.
func (m *Model) calm() bool {
	v := m.cur()
	return !m.full && !m.cardShown && m.state.Preview == nil && m.state.Status != ipc.StatusStarting &&
		v.loaded && !v.loading && !m.overlaid()
}

// overlaid: a box lies over the screen.
func (m *Model) overlaid() bool {
	return m.fb != nil || m.upd != nil || m.barAsking() || m.pick != nil || m.optOpen
}

// patch redraws the moving lines of the last frame after an update that
// only moved time on. It reports false when the whole screen must be drawn.
func (m *Model) patch() bool {
	f := &m.fr
	v := m.cur()
	if !f.ok || m.drawn.Content == "" || m.width != f.w || m.height != f.h || !m.calm() ||
		v != f.v || v.off != f.off || v.sel != f.sel {
		return false
	}
	var dirty []int // body rows whose list or stage part changed
	mark := func(r int) {
		if !slices.Contains(dirty, r) {
			dirty = append(dirty, r)
		}
	}
	// Only what can have moved is built again: the equalizer when it is
	// due, a scrolling title a tick on, the meter and times when the
	// clock moved a second or the meter half a cell.
	eqDue := time.Since(m.eqAt) >= eqEvery
	for _, r := range f.live {
		if !(r.eq && eqDue) && !(r.scroll && m.frame != f.frameAt) {
			continue
		}
		if r.i >= len(v.rows) {
			return false
		}
		if l := m.listLine(v, r.i, f.listW, f.inner); l != f.left[r.body] {
			f.left[r.body] = l
			mark(r.body)
		}
	}
	f.frameAt = m.frame
	if c := m.clockAt(f.colW); c != f.clock {
		f.clock = c
		for _, r := range []struct {
			row  int
			text func(int) string
		}{{f.meter, m.meter}, {f.times, m.times}} {
			if r.row < 0 {
				continue
			}
			if l := placeLine(stageLine{text: r.text(f.colW)}, f.colW); l != f.right[r.row] {
				f.right[r.row] = l
				mark(r.row)
			}
		}
	}
	changed := false
	for _, r := range dirty {
		if l := f.pad + f.left[r] + f.gap + f.right[r]; l != f.lines[1+r] {
			f.lines[1+r], changed = l, true
		}
	}
	if b := m.bear(); b != f.bear {
		f.bear = b
		f.lines[len(f.lines)-1], changed = m.footer(), true
	}
	if changed {
		m.drawn.Content = strings.Join(f.lines, "\n")
	}
	return true
}

// listLine is list row i as the list's box draws it, borders and all.
func (m *Model) listLine(v *view, i, listW, inner int) string {
	return boxSide + fit(m.row(v, i, listW), inner) + " " + boxSide
}
