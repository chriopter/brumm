package tui

import (
	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"
)

// ── the mouse pointer ───────────────────────────────────────────────────

// The pointer turns into a hand over what a click does something to, and
// into a resize arrow over the divider (OSC 22: kitty, ghostty, foot,
// wezterm; other terminals ignore it). The mouse is seen moving all the
// time for it, so a bare move is cheap: it sets the pointer when the shape
// changes and does nothing else, not even draw.

const (
	shapeDefault = ""
	shapePointer = "pointer"
	shapeResize  = "ew-resize"
)

// pointerSeq is a pointer shape on its way to the terminal, told apart
// from other raw sequences (kitty's) so its echo draws nothing either.
type pointerSeq string

// hovering reports whether msg is the mouse moving with no button down.
func (m *Model) hovering(msg tea.Msg) bool {
	switch msg := msg.(type) {
	case tea.MouseMotionMsg:
		return msg.Button == tea.MouseNone && !m.dragging
	case tea.RawMsg:
		_, ok := msg.Msg.(pointerSeq)
		return ok
	}
	return false
}

// hover sets the pointer for where the mouse is.
func (m *Model) hover(msg tea.Msg) tea.Cmd {
	if ms, ok := msg.(tea.MouseMotionMsg); ok {
		return m.setPointer(m.shapeAt(ms.X, ms.Y))
	}
	return nil
}

// shapeAt is the pointer shape over x, y in the layout the last render
// recorded. List rows keep the default: all of the list is clickable.
func (m *Model) shapeAt(x, y int) string {
	g := m.geo
	if m.upd != nil || m.pick != nil || m.optOpen || (m.full && m.vizList) {
		if g.options.has(x, y) {
			return shapePointer
		}
		return shapeDefault
	}
	for _, f := range g.foot {
		if f.r.has(x, y) {
			return shapePointer
		}
	}
	if m.full {
		return shapeDefault
	}
	if g.divider.has(x, y) {
		return shapeResize
	}
	for _, r := range g.tabs {
		if r.has(x, y) {
			return shapePointer
		}
	}
	for _, r := range []rect{g.search, g.crumb, g.play, g.prev, g.next, g.shuffle, g.repeat, g.volume, g.bar} {
		if r.has(x, y) {
			return shapePointer
		}
	}
	for _, r := range g.upnext {
		if r.has(x, y) {
			return shapePointer
		}
	}
	if m.state.ID != "" && (g.artist.has(x, y) || g.album.has(x, y)) {
		return shapePointer
	}
	return shapeDefault
}

// setPointer sends shape to the terminal, or nothing when it shows already.
func (m *Model) setPointer(shape string) tea.Cmd {
	if shape == m.pointer {
		return nil
	}
	m.pointer = shape
	if shape == shapeDefault {
		shape = "default"
	}
	return tea.Raw(pointerSeq(ansi.SetPointerShape(shape)))
}

// pointerReset is the default pointer back, for when brumm quits.
func (m *Model) pointerReset() string {
	if m.pointer == shapeDefault {
		return ""
	}
	return ansi.SetPointerShape("default")
}
