package tui

import (
	"image"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/art"
	"github.com/chriopter/brumm/internal/engine"
	"github.com/chriopter/brumm/internal/ipc"
)

// Headings and notes are read, never selected: the cursor steps over them.
func TestShelvesSkipHeadings(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 40
	v := m.cur()
	if v.key != "home:" {
		t.Fatalf("brumm opens on %q, want home", v.key)
	}
	fill(v, ipc.Message{Shelves: []apple.Shelf{
		{Title: "Recently played", Items: []apple.Item{{Kind: apple.KindAlbum, ID: "l.1", Name: "Blue"}}},
		{Title: "Radio", Items: []apple.Item{{Kind: apple.KindStation, ID: "ra.1", Name: "Station"}}},
	}})
	if !v.rows[v.sel].selectable() || v.rows[v.sel].item.Name != "Blue" {
		t.Fatalf("selected row %d: %+v", v.sel, v.rows[v.sel])
	}
	m.move("down")
	if v.rows[v.sel].item == nil || v.rows[v.sel].item.Name != "Station" {
		t.Fatalf("down landed on %+v", v.rows[v.sel])
	}
	m.move("up")
	if v.rows[v.sel].item.Name != "Blue" {
		t.Fatalf("up landed on %+v", v.rows[v.sel])
	}
	m.move("home")
	if !v.rows[v.sel].selectable() {
		t.Fatal("g landed on a heading")
	}
	out := m.View().Content
	if !strings.Contains(out, "Recently played") || !strings.Contains(out, "Radio") {
		t.Fatal("shelf headings are not drawn")
	}
}

// Seven tabs fit even the narrowest list: inactive ones turn into icons.
func TestTabsFit(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	for _, w := range []int{60, 90, 140} {
		m.width, m.height = w, 30
		for i, l := range strings.Split(m.View().Content, "\n") {
			if lw := lipgloss.Width(l); lw > w {
				t.Fatalf("width %d: line %d is %d wide", w, i, lw)
			}
		}
		if g := m.geo.tabs[secQueue]; g.x1 == 0 || g.x1 > w {
			t.Fatalf("width %d: the queue tab is at %+v", w, g)
		}
	}
}

// Every key the help names does something.
func TestHelpKeysWork(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 40
	tr := apple.Track{ID: "1", Title: "Song"}
	fill(m.cur(), ipc.Message{Tracks: []apple.Track{tr, tr}})
	m.state.ID, m.state.Title, m.state.Dur = "1", "Song", 100
	for _, k := range []string{"o", "O", "i", "*", "d", "R", "P", "y", "z", "Z", "a", "A", "s", "r", "n", "p", "+", "-", "m", "f", "q", "Q"} {
		m.optOpen, m.pick, m.full = false, nil, false
		before := *m
		cmd := m.key(k)
		changed := m.optOpen || m.pick != nil || m.full || m.flash != before.flash || m.state != before.state
		if cmd == nil && !changed {
			t.Errorf("key %q does nothing", k)
		}
	}
}

// ? opens and closes the key list; it types into the search box there.
// Keys on the buttons come from the options alone.
func TestHelpKey(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	press := func() { m.Update(tea.KeyPressMsg{Code: '?', Text: "?"}) }
	press()
	if !m.help || m.showTips() {
		t.Fatal("? does not open the key list")
	}
	press()
	if m.help {
		t.Fatal("? again does not close it")
	}
	m.opts.AlwaysTips = true
	if !m.showTips() {
		t.Fatal("the option does not put keys on the buttons")
	}
	m.searching = true
	press()
	if m.query != "?" {
		t.Fatalf("? while searching types into the query, got %q", m.query)
	}
}

// ← and → leave the search box for the next section.
func TestArrowsLeaveSearch(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	m.switchTo(secSearch)
	if m.searching {
		t.Fatal("arriving at search focused its box")
	}
	m.key("/")
	m.Update(tea.KeyPressMsg{Code: tea.KeyRight})
	if m.section != secQueue {
		t.Fatalf("→ from search went to section %d", m.section)
	}
	m.switchTo(secSearch)
	m.key("/")
	m.Update(tea.KeyPressMsg{Code: tea.KeyLeft})
	if m.section != secSongs {
		t.Fatalf("← from search went to section %d", m.section)
	}
}

// A click on another tab leaves the search box: the next key is a key
// again, not a letter of the query.
func TestClickLeavesSearch(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	m.switchTo(secSearch)
	m.key("/")
	if !strings.Contains(ansi.Strip(m.View().Content), "esc done") {
		t.Fatal("the focused box does not say esc leaves it")
	}
	r := m.geo.tabs[secHome]
	m.Update(tea.MouseClickMsg{X: r.x0, Y: r.y0, Button: tea.MouseLeft})
	if m.section != secHome || m.searching {
		t.Fatalf("click on Home: section %d, searching %v", m.section, m.searching)
	}
	m.Update(tea.KeyPressMsg{Code: 'o', Text: "o"})
	if !m.optOpen || m.query != "" {
		t.Fatalf("o after the click: options %v, query %q", m.optOpen, m.query)
	}
	m.Update(typedMsg{m.typedSeq})
	if m.section != secHome {
		t.Fatalf("the search pulled back to section %d", m.section)
	}
}

// Keytips keep every line exactly as wide as the screen, and put space
// under the play button.
func TestTipsLayout(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	m.state.Title, m.state.Artist, m.state.Album, m.state.ID, m.state.Dur = "Midnight City", "M83", "Hurry Up", "1", 240
	fill(m.cur(), ipc.Message{Tracks: []apple.Track{{ID: "1", Title: "Midnight City", Artist: "M83"}}})
	m.opts.AlwaysTips = true
	lines := strings.Split(m.View().Content, "\n")
	found := false
	for i, l := range lines {
		if w := lipgloss.Width(l); w > 120 {
			t.Fatalf("line %d is %d wide", i, w)
		}
		plain := ansi.Strip(l)
		if c := strings.Index(plain, " space "); c >= 0 && i == m.geo.play.y0-1 {
			if col := lipgloss.Width(plain[:c+1]); col != m.geo.play.x0 {
				t.Fatalf("space is at column %d, the play button at %d", col, m.geo.play.x0)
			}
			found = true
		}
	}
	if !found {
		t.Fatal("no space badge above the play button")
	}
	if out := ansi.Strip(strings.Join(lines, "\n")); !strings.Contains(out, "z Z queue") || !strings.Contains(out, "1 Home") {
		t.Fatal("row keys or tab digits are missing")
	}
}

// Up from the first search result goes back into the search box.
func TestUpIntoSearch(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	m.section = secSearch
	v := m.cur()
	fill(v, ipc.Message{Shelves: []apple.Shelf{{Title: "Songs", Tracks: []apple.Track{{ID: "1", Title: "a"}, {ID: "2", Title: "b"}}}}})
	m.move("down")
	m.move("up")
	if m.searching {
		t.Fatal("up to the first result already left the list")
	}
	m.move("up")
	if !m.searching {
		t.Fatal("up from the first result does not reach the search box")
	}
}

// A selected album lies on the playing cover as a card; every line keeps
// the cover's width. The playing song's own row brings no card.
func TestCardOnCover(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 140, 40
	m.state.Title, m.state.ID, m.state.Playing = "Now", "1", true
	m.coverURL, m.cover = "now.jpg", image.NewRGBA(image.Rect(0, 0, 60, 60))
	fill(m.cur(), ipc.Message{Items: []apple.Item{{Kind: apple.KindAlbum, ID: "l.1", Name: "Blue", Artwork: "blue.jpg", Info: "1971"},
		{Kind: apple.KindAlbum, ID: "l.2", Name: "Hejira", Artwork: "hejira.jpg"}}})
	size := art.Size{Width: 40, Height: 20}
	lines := m.stageCover(size)
	if len(lines) != 20 {
		t.Fatalf("%d lines", len(lines))
	}
	for i, l := range lines {
		if w := lipgloss.Width(l); w != 40 {
			t.Fatalf("line %d is %d wide", i, w)
		}
	}
	if !strings.Contains(ansi.Strip(lines[19]), "Blue") {
		t.Fatalf("no caption under the card: %q", ansi.Strip(lines[19]))
	}
	m.frame = m.cur().selAt + int(cardFor/frameEvery) + 1
	if m.selectedCard() != nil {
		t.Fatal("the card stays after the list rested")
	}
	m.move("down")
	if m.selectedCard() == nil {
		t.Fatal("moving the selection does not bring a card back")
	}
	fill(m.cur(), ipc.Message{Tracks: []apple.Track{{ID: "1", Title: "Now", Artwork: "x.jpg"}}})
	if m.selectedCard() != nil {
		t.Fatal("the playing song shows a card of itself")
	}
}

// Enter shows the song as playing at once; the player's older reports do
// not take it back, its first report of the song ends the stand-in.
func TestPlayShowsAtOnce(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	m.state.ID, m.state.Title = "old", "Old Song"
	fill(m.cur(), ipc.Message{Tracks: []apple.Track{{ID: "new", Title: "New Song", Artist: "A", Duration: 200}}})
	m.showPlaying(*m.cur().rows[0].track, "list:songs")
	if m.state.Title != "New Song" || !m.state.Playing || m.state.Pos != 0 {
		t.Fatalf("stage shows %+v", m.state.State)
	}
	report := func(st ipc.State) { m.keepShowing(&st); m.state = st } // as event does
	report(ipc.State{Status: ipc.StatusReady, State: engine.State{ID: "old", Title: "Old Song", Volume: 0.5}})
	if m.state.Title != "New Song" || m.state.Volume != 0.5 {
		t.Fatalf("an older report took the song back: %+v", m.state.State)
	}
	report(ipc.State{Status: ipc.StatusReady, State: engine.State{ID: "new", Title: "New Song", Pos: 1.2, Playing: true}})
	if m.pending != nil || m.state.Pos != 1.2 {
		t.Fatal("the player's own report did not take over")
	}
}

// Fullscreen: tab steps, v lists the styles, a stops the minutely change;
// the buttons on the bottom line do the same when clicked.
func TestVizControls(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height, m.full = 120, 35, true
	start := m.vizStyle
	m.fullKey("tab")
	if m.vizStyle == start {
		t.Fatal("tab does not change the style")
	}
	m.fullKey("v")
	if !m.vizList || !strings.Contains(ansi.Strip(m.View().Content), "styles") {
		t.Fatal("v does not list the styles")
	}
	from := m.vizStyle
	m.fullKey("down")
	if m.vizStyle == from {
		t.Fatal("moving in the list does not show the style")
	}
	m.fullKey("esc")
	if m.vizList || m.vizStyle != from {
		t.Fatal("esc does not go back to the style before the list")
	}
	m.fullKey("v")
	m.fullKey("3")
	if m.vizList || m.vizStyle != 2 {
		t.Fatalf("3 in the list shows style %d", m.vizStyle)
	}
	m.fullKey("a")
	if !m.opts.NoVizCycle {
		t.Fatal("a does not stop the change")
	}
	m.View()
	for _, f := range m.geo.foot {
		if f.action == "v" {
			m.fullClick(tea.Mouse{X: f.r.x0, Y: f.r.y0, Button: tea.MouseLeft})
		}
	}
	if !m.vizList {
		t.Fatal("clicking the styles button does not list them")
	}
}

// A cover that arrives is drawn as a card at once, even while the next
// tick is seconds away (a card waiting to go); and the rows next to the
// selection are drawn ahead.
func TestCardDrawnAtOnce(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	m.schedule(4 * time.Second)
	m.thumbWant = thumbReq{"pixel:a.jpg@20x10", "a.jpg", art.Size{Width: 20, Height: 10}}
	m.Update(coverMsg{"a.jpg", image.NewRGBA(image.Rect(0, 0, 10, 10))})
	if !m.thumbBusy {
		t.Fatal("the card waits for the next tick")
	}
	m.Update(thumbMsg{"pixel:a.jpg@20x10", []string{"x"}})
	fill(m.cur(), ipc.Message{Tracks: []apple.Track{{ID: "1", Artwork: "a.jpg"}, {ID: "2", Artwork: "b.jpg"}}})
	m.cardSize = art.Size{Width: 20, Height: 10}
	m.covers["b.jpg"] = image.NewRGBA(image.Rect(0, 0, 10, 10))
	if w := m.nextThumb(); w.url != "b.jpg" {
		t.Fatalf("the next row's card is not drawn ahead: %+v", w)
	}
}

// U always answers: checking, then the newest already or one to install.
func TestUpdatePopup(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	m.upd = &updatePopup{checking: true}
	if !strings.Contains(ansi.Strip(m.View().Content), "looking for a new release") {
		t.Fatal("no word while checking")
	}
	m.updateChecked(updateMsg{reply: ipc.Message{Version: "v0.5.2", Link: "v0.5.2"}})
	if !strings.Contains(ansi.Strip(m.View().Content), "v0.5.2 is the newest version") {
		t.Fatal("no word that this is the newest")
	}
	m.updateKey("esc")
	if m.upd != nil {
		t.Fatal("esc does not close")
	}
	m.upd = &updatePopup{}
	m.updateChecked(updateMsg{reply: ipc.Message{Version: "v0.5.2", Link: "v0.5.3", Pos: 1}})
	if out := ansi.Strip(m.View().Content); !strings.Contains(out, "v0.5.3 is available") || !strings.Contains(out, "install") {
		t.Fatal("a newer release is not offered")
	}
}

// The visualizer draws at 30 fps unless told otherwise; F steps through
// 60, 120 and the screen's own rate.
func TestAutoFPS(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.refresh = 144
	m.full = true
	for _, want := range []string{"30 fps", "60 fps", "120 fps", "auto 144 fps", "30 fps"} {
		if m.fpsLabel() != want {
			t.Fatalf("the rate is %s, want %s", m.fpsLabel(), want)
		}
		m.fullKey("F")
	}
	if m.opts.VizFPS = 30; m.drawFPS() != 30 { // set to 30 before 30 was the default
		t.Fatalf("30 draws at %d", m.drawFPS())
	}
}

// / filters the list where you are; tab takes the words to all of Apple
// Music; esc brings the whole list back.
func TestFilterHere(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	m.section = secPlaylists
	v := &view{title: "Mix", key: "lib:playlist:p.1", item: &apple.Item{Kind: apple.KindPlaylist, ID: "p.1"}}
	m.stacks[secPlaylists] = append(m.stacks[secPlaylists], v)
	fill(v, ipc.Message{Tracks: []apple.Track{
		{ID: "1", Title: "Around the World", Artist: "Daft Punk"},
		{ID: "2", Title: "Get Lucky", Artist: "Daft Punk"},
		{ID: "3", Title: "Blue Monday", Artist: "New Order"},
	}})
	m.key("/")
	if !m.filtering || m.section != secPlaylists {
		t.Fatal("/ does not filter the list where you are")
	}
	for _, r := range "daft luck" {
		m.Update(tea.KeyPressMsg{Code: r, Text: string(r)})
	}
	if len(v.rows) != 1 || v.rows[0].track.ID != "2" {
		t.Fatalf("filter left %d rows", len(v.rows))
	}
	if !strings.Contains(ansi.Strip(m.View().Content), "daft luck") {
		t.Fatal("the filter box does not show")
	}
	m.Update(tea.KeyPressMsg{Code: tea.KeyEscape})
	if len(v.rows) != 3 || m.filtering {
		t.Fatal("esc does not bring the list back")
	}
	m.key("/")
	for _, r := range "kraftwerk" {
		m.Update(tea.KeyPressMsg{Code: r, Text: string(r)})
	}
	m.filterKey(tea.KeyPressMsg{Code: tea.KeyTab})
	if m.section != secSearch || m.query != "kraftwerk" {
		t.Fatalf("tab does not search all of Apple Music (section %d, %q)", m.section, m.query)
	}
}

// Once an update is installed, the popup offers to restart into it; esc
// leaves that to the next pause.
func TestUpdateOffersRestart(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.width, m.height = 120, 35
	m.upd = &updatePopup{latest: "v9.9.9", newer: true, installing: true}
	m.updateInstalled(installedMsg{})
	if m.upd == nil || !m.upd.installed {
		t.Fatal("no restart offer after installing")
	}
	if box := ansi.Strip(strings.Join(m.updateBox(), "\n")); !strings.Contains(box, "v9.9.9 is installed") || !strings.Contains(box, "restart now") {
		t.Fatalf("offer reads %q", box)
	}
	if cmd := m.updateKey("x"); cmd != nil || m.upd == nil {
		t.Fatal("another key closed the offer")
	}
	m.updateKey("esc")
	if m.upd != nil {
		t.Fatal("esc keeps the offer open")
	}
	m.upd = &updatePopup{installed: true}
	if m.updateKey("enter") == nil || m.upd != nil {
		t.Fatal("enter does not restart")
	}
}

// Pause and play show at once; reports the player sends before it has
// caught up do not take it back, its own does.
func TestPauseShowsAtOnce(t *testing.T) {
	m := playingModel(120, 35)
	m.state.Playing = true
	m.key("n") // no queue known: nothing to show yet, the button stays
	if !m.state.Playing || m.state.ID != "1" {
		t.Fatalf("n without a queue shows %q playing %v", m.state.ID, m.state.Playing)
	}
	m.wantPlay = nil
	m.toggle()
	if m.state.Playing {
		t.Fatal("pause did not show at once")
	}
	report := func(playing bool) { st := m.state; st.Playing = playing; m.keepPlaying(&st); m.state = st } // as event does
	report(true)
	if m.state.Playing {
		t.Fatal("an older report took the pause back")
	}
	report(false)
	if m.wantPlay != nil {
		t.Fatal("the player's own report did not take over")
	}
	report(true)
	if !m.state.Playing {
		t.Fatal("a later play was held back")
	}
}

// n shows the next song of the queue at once, and again for a second n
// before the player answers; a click on a song up next shows it too.
func TestNextShowsAtOnce(t *testing.T) {
	m := playingModel(120, 35)
	m.state.Playing, m.state.Index, m.state.Length = true, 4, 20
	m.nextSeq = 1
	q := albumQueue(m, 5, -1)
	m.gotNext(nextMsg{seq: 1, tracks: q, pos: 4, ok: true})
	m.key("n")
	if m.state.ID != q[1].ID || m.state.Index != 5 || !m.state.Playing {
		t.Fatalf("n shows %q at %d", m.state.ID, m.state.Index)
	}
	m.key("n")
	if m.state.ID != q[2].ID || m.state.Index != 6 {
		t.Fatalf("n again shows %q at %d", m.state.ID, m.state.Index)
	}
	// The queue asked for while the player is still a song behind counts
	// from the song on the stage.
	m.nextSeq = 2
	m.gotNext(nextMsg{seq: 2, tracks: q, pos: 4, ok: true})
	if m.upPos != 6 || m.upTracks[0].ID != q[2].ID {
		t.Fatalf("queue counted from %d", m.upPos)
	}
	m.jump(8)
	if m.state.ID != q[4].ID || m.state.Index != 8 {
		t.Fatalf("a jump shows %q at %d", m.state.ID, m.state.Index)
	}
}

// p after 3 s goes back to the start, and the playhead shows it at once.
func TestPrevRestartsAtOnce(t *testing.T) {
	m := playingModel(120, 35)
	m.state.Pos = 42
	m.key("p")
	if p := m.position(); p != 0 {
		t.Fatalf("p shows the playhead at %v", p)
	}
}

// Seeking to the end goes on to the next song, shown at once.
func TestSeekToEndSkips(t *testing.T) {
	m := playingModel(120, 35)
	m.nextSeq = 1
	q := albumQueue(m, 2, -1)
	m.gotNext(nextMsg{seq: 1, tracks: q, pos: 0, ok: true})
	m.seek(1000)
	m.Update(seekMsg{m.seekSeq})
	if m.state.ID != q[1].ID {
		t.Fatalf("seeking to the end shows %q", m.state.ID)
	}
}

// Another section leaves the filter's box; its list keeps the filter.
func TestSectionLeavesFilter(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.section = secSongs
	fill(m.cur(), ipc.Message{Tracks: []apple.Track{{ID: "1", Title: "A"}, {ID: "2", Title: "B"}}})
	m.key("/")
	m.switchTo(secAlbums)
	if m.filtering {
		t.Fatal("the filter box followed to another section")
	}
}

// Searching again for what the results show goes back to them.
func TestSearchAgainShowsResults(t *testing.T) {
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.query = "daft punk"
	m.runSearch(true)
	m.stacks[secSearch][0].loading = false
	m.stacks[secSearch] = append(m.stacks[secSearch], &view{title: "Discovery", key: "cat:album:1"})
	m.runSearch(true)
	if len(m.stack()) != 1 || m.cur().key != "search:daft punk" {
		t.Fatalf("search again shows %q", m.cur().key)
	}
}

// Arriving at Search leaves the box alone: a digit goes on to its
// section, a letter starts a search with it typed.
func TestSearchNotATrap(t *testing.T) {
	press := func(m *Model, s string) {
		for _, r := range s {
			m.Update(tea.KeyPressMsg{Code: r, Text: string(r)})
		}
	}
	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	press(m, "61")
	if m.section != secHome || m.query != "" || m.searching {
		t.Fatalf("6 then 1: section %d, query %q", m.section, m.query)
	}
	press(m, "6d")
	if m.section != secSearch || !m.searching || m.query != "d" {
		t.Fatalf("6 then d: searching %v for %q", m.searching, m.query)
	}
	press(m, "2")
	if m.query != "d2" {
		t.Fatalf("a digit after words types: %q", m.query)
	}
	m.query = ""
	press(m, "3")
	if m.section != secAlbums || m.searching {
		t.Fatal("a digit into an empty box is not its section")
	}
	press(m, "6")
	m.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	if !m.searching {
		t.Fatal("enter with no results does not go into the box")
	}
}
