package tui

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/ipc"
)

func titles(s []*view) string {
	var t []string
	for _, v := range s {
		t = append(t, v.title)
	}
	return strings.Join(t, " / ")
}

// Going between an artist and an album — the names over the cover, a
// link — goes back to the one already open instead of piling up.
func TestCrumbNoRepeats(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.state.ID = "s2"
	artist := apple.Item{Kind: apple.KindArtist, ID: "a.1", Name: "Deftones", Catalog: true}
	album := apple.Item{Kind: apple.KindAlbum, ID: "al.1", Name: "Koi No Yokan", Catalog: true}
	songs := []apple.Track{{ID: "s1", Title: "Swerve City"}, {ID: "s2", Title: "Romantic Dreams"}}
	for range 3 {
		m.Update(albumMsg{item: artist, song: "s2"})
		if v := m.cur(); !v.loaded {
			fill(v, ipc.Message{Items: []apple.Item{album}})
		}
		m.Update(albumMsg{item: album, song: "s2"})
		if v := m.cur(); !v.loaded {
			fill(v, ipc.Message{Tracks: songs})
			m.selectSong(v, v.want)
		}
	}
	if got := titles(m.stack()); got != "Home / Deftones / Koi No Yokan" {
		t.Fatalf("stack is %q", got)
	}
	// Opening what shows already changes nothing but the song selected.
	v := m.cur()
	v.sel = 0
	m.Update(openMsg{album, "s2"})
	if got := titles(m.stack()); got != "Home / Deftones / Koi No Yokan" || m.cur() != v || v.sel != 1 {
		t.Fatalf("stack is %q, row %d", got, v.sel)
	}
	// Back on the artist, its rows and selection stay.
	a := m.stack()[1]
	a.sel = 0
	m.Update(albumMsg{item: artist})
	if m.cur() != a || len(a.rows) != 1 || a.sel != 0 || len(m.stack()) != 2 {
		t.Fatalf("the artist came back as %q, row %d", titles(m.stack()), a.sel)
	}
}

// A song asked for before its view has rows is selected when they come.
func TestPushSelectsOnceLoaded(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	album := apple.Item{Kind: apple.KindAlbum, ID: "al.1", Name: "Koi No Yokan", Catalog: true}
	m.Update(openMsg{album, "s2"})
	v := m.cur()
	m.Update(loadedMsg{v: v, reply: ipc.Message{Tracks: []apple.Track{{ID: "s1"}, {ID: "s2"}}}})
	if v.sel != 1 {
		t.Fatalf("row %d selected, want 1", v.sel)
	}
}

// A crumb wider than its box gives up its first parts, not its last.
func TestCrumbCollapses(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height, m.split = 80, 30, 0.5 // not a width another test left behind
	for _, n := range []string{"Deftones", "Koi No Yokan", "Chino Moreno", "Crosses", "Goodnight, Sweetheart"} {
		it := apple.Item{Kind: apple.KindAlbum, ID: n, Name: n}
		m.push(&view{title: n, key: it.Key(), item: &it}, "")
	}
	out := m.View().Content
	top := ansi.Strip(strings.Split(out, "\n")[bodyTop])
	if !strings.Contains(top, "… / ") || !strings.Contains(top, "Goodnight, Sweetheart") {
		t.Fatalf("crumb reads %q", top)
	}
	for i, l := range strings.Split(out, "\n") {
		if lipgloss.Width(l) > m.width {
			t.Fatalf("line %d is %d wide", i, lipgloss.Width(l))
		}
	}
}

// Buttons and names get a hand, the divider a resize arrow, the list and
// empty space the default.
func TestPointerShapes(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.geo = geometry{
		list:    rect{0, 5, 40, 20},
		divider: rect{40, 5, 41, 20},
		play:    rect{50, 20, 52, 21},
		artist:  rect{50, 10, 60, 11},
		foot:    []footHit{{rect{2, 29, 8, 30}, "?"}},
	}
	m.geo.tabs[0] = rect{2, 3, 8, 4}
	for _, c := range []struct {
		x, y int
		want string
	}{
		{3, 3, shapePointer},   // a tab
		{51, 20, shapePointer}, // play
		{3, 29, shapePointer},  // the footer
		{40, 9, shapeResize},   // the divider
		{10, 9, shapeDefault},  // a list row
		{55, 10, shapeDefault}, // the artist, with nothing playing
		{70, 25, shapeDefault},
	} {
		if got := m.shapeAt(c.x, c.y); got != c.want {
			t.Errorf("at %d,%d: %q, want %q", c.x, c.y, got, c.want)
		}
	}
	m.state.ID = "s1"
	if got := m.shapeAt(55, 10); got != shapePointer {
		t.Errorf("the artist playing: %q", got)
	}
	m.optOpen = true
	if got := m.shapeAt(51, 20); got != shapeDefault {
		t.Errorf("under the options menu: %q", got)
	}
}

// A move sends the shape only when it changes, and draws nothing.
func TestHoverSendsOnChange(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	m.View()
	box := m.geo.tabs[1]
	if box == (rect{}) {
		t.Fatal("no tab drawn")
	}
	move := func(x, y int) string {
		_, cmd := m.Update(tea.MouseMotionMsg{X: x, Y: y})
		if cmd == nil {
			return ""
		}
		raw, ok := cmd().(tea.RawMsg)
		if !ok {
			t.Fatalf("a move sent %T", cmd())
		}
		m.Update(raw) // its echo, as the program hands it back
		return raw.Msg.(pointerSeq).String()
	}
	if s := move(box.x0, box.y0); s != ansi.SetPointerShape("pointer") {
		t.Fatalf("over a tab: %q", s)
	}
	// Many moves over it: nothing sent, nothing drawn, no frames asked
	// for. (A render records the layout afresh, so a changed record
	// that survives shows none ran.)
	m.geo.tabs[1] = rect{0, 0, 2, 1}
	before := m.drawn.Content
	for i := range 50 {
		if s := move(i%2, 0); s != "" {
			t.Fatalf("still over it: %q", s)
		}
		if m.View().Content != before || m.geo.tabs[1] != (rect{0, 0, 2, 1}) {
			t.Fatal("a move drew the screen")
		}
	}
	if m.ticking {
		t.Fatal("a move asked for frames")
	}
	if s := move(10, 20); s != ansi.SetPointerShape("default") {
		t.Fatalf("off it: %q", s)
	}
	m.Update(tea.WindowSizeMsg{Width: 120, Height: 35})
	m.View()
	if m.geo.tabs[1] == (rect{0, 0, 2, 1}) {
		t.Fatal("a resize did not draw the screen")
	}
	// Losing focus puts the default back; quitting does too.
	move(m.geo.tabs[1].x0, m.geo.tabs[1].y0)
	if m.pointerReset() == "" {
		t.Fatal("quitting leaves the hand")
	}
	m.Update(tea.BlurMsg{})
	if m.pointer != shapeDefault || m.pointerReset() != "" {
		t.Fatalf("blurred, the pointer is %q", m.pointer)
	}
}

// Dragging the divider still works with every move reported.
func TestDragDivider(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	m.View()
	d := m.geo.divider
	m.Update(tea.MouseClickMsg{X: d.x0, Y: d.y0 + 2, Button: tea.MouseLeft})
	if !m.dragging {
		t.Fatal("the divider did not catch")
	}
	m.Update(tea.MouseMotionMsg{X: 80, Y: d.y0 + 2, Button: tea.MouseLeft})
	if m.split < 0.6 {
		t.Fatalf("split %.2f after dragging right", m.split)
	}
}

func (s pointerSeq) String() string { return string(s) }
