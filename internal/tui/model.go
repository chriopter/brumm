// Package tui is brumm's terminal interface: a drill-in browser on the left
// (library sections, then playlists, albums, artists, songs and search
// results) and a stage on the right with the cover, a live spectrum, the
// playhead and the transport controls.
package tui

import (
	"context"
	"fmt"
	"image"
	"math"
	"net/http"
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/art"
	"github.com/chriopter/brumm/internal/ipc"
	"github.com/chriopter/brumm/internal/login"
)

// bands is how many spectrum bands the TUI asks the daemon for.
const bands = 48

// A play queues at most this many songs around the chosen one.
const (
	queueBefore = 100
	queueAfter  = 400
)

type section int

const (
	secPlaylists section = iota
	secAlbums
	secArtists
	secSongs
	secSearch
)

var (
	sectionNames = []string{"Playlists", "Albums", "Artists", "Songs", "Search"}
	sectionLists = []string{ipc.ListPlaylists, ipc.ListAlbums, ipc.ListArtists, ipc.ListSongs}
)

// row is one line of a list: a song, or something that opens.
type row struct {
	track *apple.Track
	item  *apple.Item
}

// view is one level of the browser.
type view struct {
	title   string
	key     string      // what a play started here is attributed to
	item    *apple.Item // the item this view shows the inside of
	rows    []row
	loaded  bool
	loading bool
	err     error
	sel     int
	off     int
	selAt   int // frame the selection last changed, for the marquee
}

// songs returns the view's song ids and maps a row index to its position
// among them.
func (v *view) songs() (ids []string, at map[int]int) {
	at = map[int]int{}
	for i, r := range v.rows {
		if r.track != nil {
			at[i] = len(ids)
			ids = append(ids, r.track.ID)
		}
	}
	return ids, at
}

type (
	connectedMsg struct {
		client *ipc.Client
		state  ipc.State
		err    error
	}
	eventMsg  ipc.Message
	closedMsg struct{}
	loadedMsg struct {
		v     *view
		reply ipc.Message
		err   error
	}
	coverMsg struct {
		url string
		img image.Image
	}
	loginMsg struct{ err error }
	errMsg   struct{ err error }
	tickMsg  struct{}
)

type Model struct {
	client *ipc.Client
	http   *http.Client

	width, height int
	cellAspect    float64 // cell height ÷ width, measured in pixels
	frame         int

	state   ipc.State
	stateAt time.Time
	spec    []float64

	section section
	stacks  [5][]*view // one navigation stack per section

	searching bool // the search box has focus
	query     string

	coverURL string
	cover    image.Image
	rendered map[art.Size][]string

	flash   string
	flashAt time.Time
	lastErr string
	help    bool
	resumed bool
	lastVol float64
	geo     geometry
}

func newModel(client *ipc.Client, initial ipc.State) *Model {
	m := &Model{
		client:     client,
		http:       &http.Client{Timeout: 10 * time.Second},
		state:      initial,
		stateAt:    time.Now(),
		rendered:   map[art.Size][]string{},
		cellAspect: cellAspect(),
		lastVol:    1,
	}
	for s := secPlaylists; s <= secSongs; s++ {
		m.stacks[s] = []*view{{title: sectionNames[s], key: "list:" + sectionLists[s]}}
	}
	m.stacks[secSearch] = []*view{{title: "Search", key: "search:", loaded: true}}
	return m
}

func (m *Model) Init() tea.Cmd {
	return tea.Batch(m.listen(), tick(), m.load(m.cur()), m.maybeFetchCover())
}

func tick() tea.Cmd {
	return tea.Tick(100*time.Millisecond, func(time.Time) tea.Msg { return tickMsg{} })
}

func (m *Model) listen() tea.Cmd {
	events := m.client.Events()
	return func() tea.Msg {
		msg, ok := <-events
		if !ok {
			return closedMsg{}
		}
		return eventMsg(msg)
	}
}

// reconnect retries the daemon once a second, starting it if needed.
func reconnect() tea.Cmd {
	return tea.Tick(time.Second, func(time.Time) tea.Msg {
		c, err := connect()
		if err != nil {
			return connectedMsg{err: err}
		}
		r, err := c.Do(ipc.Request{Cmd: ipc.CmdSubscribe, Bands: bands})
		if err != nil {
			c.Close()
			return connectedMsg{err: err}
		}
		return connectedMsg{client: c, state: *r.State}
	})
}

// send fires a command at the daemon; failures surface as a flash.
func (m *Model) send(r ipc.Request) tea.Cmd {
	client := m.client
	return func() tea.Msg {
		if _, err := client.Do(r); err != nil {
			return errMsg{err}
		}
		return nil
	}
}

func (m *Model) stack() []*view { return m.stacks[m.section] }
func (m *Model) cur() *view     { s := m.stack(); return s[len(s)-1] }

// load fills a view from the daemon, whose cache usually answers at once.
func (m *Model) load(v *view) tea.Cmd {
	if v.loading || m.state.Status == ipc.StatusLoggedOut {
		return nil
	}
	var req ipc.Request
	switch {
	case v.item != nil:
		req = ipc.Request{Cmd: ipc.CmdOpen, Item: v.item}
	case strings.HasPrefix(v.key, "list:"):
		req = ipc.Request{Cmd: ipc.CmdList, List: strings.TrimPrefix(v.key, "list:")}
	case strings.HasPrefix(v.key, "search:") && v.key != "search:":
		req = ipc.Request{Cmd: ipc.CmdSearch, Query: strings.TrimPrefix(v.key, "search:")}
	default:
		return nil
	}
	v.loading = true
	client := m.client
	return func() tea.Msg {
		reply, err := client.Do(req)
		return loadedMsg{v, reply, err}
	}
}

func fill(v *view, reply ipc.Message) {
	var rows []row
	if res := reply.Results; res != nil {
		for i := range res.Songs {
			rows = append(rows, row{track: &res.Songs[i]})
		}
		for _, list := range [][]apple.Item{res.Albums, res.Artists, res.Playlists} {
			for i := range list {
				rows = append(rows, row{item: &list[i]})
			}
		}
	}
	for i := range reply.Items {
		rows = append(rows, row{item: &reply.Items[i]})
	}
	for i := range reply.Tracks {
		rows = append(rows, row{track: &reply.Tracks[i]})
	}
	v.rows, v.loaded, v.err = rows, true, nil
	v.sel = min(v.sel, max(0, len(rows)-1))
}

func (m *Model) maybeFetchCover() tea.Cmd {
	url := m.state.Artwork
	if url == m.coverURL {
		return nil
	}
	m.coverURL, m.cover, m.rendered = url, nil, map[art.Size][]string{}
	if url == "" {
		return nil
	}
	client := m.http
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		img, _ := art.Fetch(ctx, client, url)
		return coverMsg{url, img}
	}
}

func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		m.cellAspect = cellAspect()
	case tickMsg:
		m.frame++
		if m.flash != "" && time.Since(m.flashAt) > 4*time.Second {
			m.flash = ""
		}
		return m, tick()
	case eventMsg:
		return m, m.event(ipc.Message(msg))
	case closedMsg:
		m.state.Status, m.state.Message = ipc.StatusStarting, "reconnecting"
		return m, reconnect()
	case connectedMsg:
		if msg.err != nil {
			return m, reconnect()
		}
		m.client, m.state, m.stateAt = msg.client, msg.state, time.Now()
		return m, tea.Batch(m.listen(), m.load(m.cur()))
	case loadedMsg:
		msg.v.loading = false
		if msg.err != nil {
			msg.v.err = msg.err
			return m, nil
		}
		fill(msg.v, msg.reply)
		return m, m.maybeResume()
	case coverMsg:
		if msg.url == m.coverURL {
			m.cover = msg.img
		}
	case loginMsg:
		if msg.err != nil {
			m.setFlash("sign-in failed: " + msg.err.Error())
			return m, nil
		}
		m.setFlash("signed in")
		return m, m.send(ipc.Request{Cmd: ipc.CmdReload})
	case errMsg:
		m.setFlash(msg.err.Error())
	case tea.KeyPressMsg:
		if m.searching {
			return m, m.searchKey(msg)
		}
		return m, m.key(msg.String())
	case tea.MouseClickMsg:
		return m, m.click(msg.Mouse())
	case tea.MouseWheelMsg:
		if msg.Button == tea.MouseWheelUp {
			return m, m.move("up")
		}
		return m, m.move("down")
	}
	return m, nil
}

func (m *Model) event(msg ipc.Message) tea.Cmd {
	cmds := []tea.Cmd{m.listen()}
	if msg.State != nil {
		wasLoggedOut := m.state.Status == ipc.StatusLoggedOut
		m.state, m.stateAt = *msg.State, time.Now()
		if e := m.state.Err; e != "" && e != m.lastErr {
			m.setFlash(e)
		}
		m.lastErr = m.state.Err
		if wasLoggedOut || !m.cur().loaded {
			cmds = append(cmds, m.load(m.cur()))
		}
		cmds = append(cmds, m.maybeFetchCover(), m.maybeResume())
	}
	if msg.Spectrum != nil {
		m.feedSpectrum(msg.Spectrum)
	}
	if msg.Library {
		// Refreshed in the background: reload what is on screen, keeping
		// the old rows visible until the new ones arrive.
		for s := secPlaylists; s <= secSongs; s++ {
			for _, v := range m.stacks[s] {
				if s == m.section {
					cmds = append(cmds, m.load(v))
				} else {
					v.loaded = false
				}
			}
		}
	}
	return tea.Batch(cmds...)
}

func (m *Model) setFlash(s string) { m.flash, m.flashAt = s, time.Now() }

// feedSpectrum eases the bars: they jump up at once and fall back slowly,
// which reads as motion rather than flicker.
func (m *Model) feedSpectrum(in []int) {
	if len(m.spec) != len(in) {
		m.spec = make([]float64, len(in))
	}
	for i, v := range in {
		m.spec[i] = max(float64(v)/255, m.spec[i]*0.86)
	}
}

// position extrapolates the playhead between state updates.
func (m *Model) position() float64 {
	p := m.state.Pos
	if m.state.Playing {
		p += time.Since(m.stateAt).Seconds()
	}
	if m.state.Dur > 0 {
		p = min(p, m.state.Dur)
	}
	return p
}

// ── navigation ─────────────────────────────────────────────────────────

func (m *Model) switchTo(s section) tea.Cmd {
	m.section, m.help = s, false
	if s == secSearch && m.cur().key == "search:" {
		m.searching = true
	}
	if v := m.cur(); !v.loaded {
		return m.load(v)
	}
	return nil
}

func (m *Model) push(v *view) tea.Cmd {
	m.stacks[m.section] = append(m.stacks[m.section], v)
	v.selAt = m.frame
	return m.load(v)
}

func (m *Model) back() {
	if s := m.stack(); len(s) > 1 {
		m.stacks[m.section] = s[:len(s)-1]
	}
}

// activate opens the selected item or plays the selected song.
func (m *Model) activate() tea.Cmd {
	v := m.cur()
	if v.sel >= len(v.rows) {
		return nil
	}
	r := v.rows[v.sel]
	if r.item != nil {
		it := *r.item
		return m.push(&view{title: it.Name, key: it.Key(), item: &it})
	}
	ids, at := v.songs()
	i := at[v.sel]
	lo, hi := max(0, i-queueBefore), min(len(ids), i+queueAfter)
	return m.send(ipc.Request{Cmd: ipc.CmdPlay, IDs: ids[lo:hi], Start: r.track.ID, Source: v.key})
}

// jumpToPlaying shows the list the current song was started from with the
// song selected, reopening that list if needed.
func (m *Model) jumpToPlaying() tea.Cmd {
	src, id := m.state.Source, m.state.ID
	if src == "" {
		return nil
	}
	selectID := func(v *view) {
		for i, r := range v.rows {
			if r.track != nil && r.track.ID == id {
				v.sel, v.selAt = i, m.frame
				return
			}
		}
	}
	for s := secPlaylists; s <= secSearch; s++ {
		for _, v := range m.stacks[s] {
			if v.key == src {
				m.section = s
				for m.cur() != v {
					m.back()
				}
				selectID(v)
				return nil
			}
		}
	}
	for s := secPlaylists; s <= secArtists; s++ {
		for _, r := range m.stacks[s][0].rows {
			if r.item != nil && r.item.Key() == src {
				it := *r.item
				v := &view{title: it.Name, key: src, item: &it, loading: true}
				m.section, m.stacks[s] = s, append(m.stacks[s][:1], v)
				client := m.client
				return func() tea.Msg {
					reply, err := client.Do(ipc.Request{Cmd: ipc.CmdOpen, Item: v.item})
					for i, t := range reply.Tracks {
						if t.ID == id {
							v.sel = i
						}
					}
					return loadedMsg{v, reply, err}
				}
			}
		}
	}
	return nil
}

// maybeResume lands on what is playing the first time brumm opens mid-song.
func (m *Model) maybeResume() tea.Cmd {
	if m.resumed || m.state.Source == "" {
		return nil
	}
	root := m.stacks[secPlaylists][0]
	if strings.Contains(m.state.Source, ":album:") {
		root = m.stacks[secAlbums][0]
	}
	if !root.loaded {
		return m.load(root)
	}
	m.resumed = true
	return m.jumpToPlaying()
}

func (m *Model) key(k string) tea.Cmd {
	switch k {
	case "q", "ctrl+c":
		return tea.Quit
	case "Q":
		return tea.Sequence(m.send(ipc.Request{Cmd: ipc.CmdQuit}), tea.Quit)
	case "?":
		m.help = !m.help
	case "L":
		return func() tea.Msg { return loginMsg{login.Run(context.Background(), func(string) {})} }
	case "1", "2", "3", "4", "5":
		return m.switchTo(section(k[0] - '1'))
	case "tab":
		return m.switchTo((m.section + 1) % 5)
	case "shift+tab":
		return m.switchTo((m.section + 4) % 5)
	case "/":
		m.section, m.searching, m.help = secSearch, true, false
	case "space", " ":
		return m.send(ipc.Request{Cmd: ipc.CmdToggle})
	case "n":
		return m.send(ipc.Request{Cmd: ipc.CmdNext})
	case "p", "b":
		return m.send(ipc.Request{Cmd: ipc.CmdPrev})
	case "left", "right":
		delta := 10.0
		if k == "left" {
			delta = -10
		}
		return m.send(ipc.Request{Cmd: ipc.CmdSeek, Value: max(0, m.position()+delta)})
	case "+", "=", "-":
		delta := 0.05
		if k == "-" {
			delta = -0.05
		}
		return m.setVolume(m.state.Volume + delta)
	case "m":
		if m.state.Volume > 0 {
			m.lastVol = m.state.Volume
			return m.setVolume(0)
		}
		return m.setVolume(m.lastVol)
	case "s":
		on := !m.state.Shuffle
		m.state.Shuffle = on
		m.setFlash(map[bool]string{true: "shuffle on", false: "shuffle off"}[on])
		return m.send(ipc.Request{Cmd: ipc.CmdShuffle, Value: map[bool]float64{true: 1}[on]})
	case "r":
		// off → all → one → off, the order most players use.
		next := [...]int{2, 0, 1}[max(0, min(m.state.Repeat, 2))]
		m.state.Repeat = next
		m.setFlash([...]string{"repeat off", "repeat one", "repeat all"}[next])
		return m.send(ipc.Request{Cmd: ipc.CmdRepeat, Value: float64(next)})
	case "c":
		return m.jumpToPlaying()
	case "esc", "h", "backspace":
		if m.help {
			m.help = false
			return nil
		}
		m.back()
	case "enter", "l":
		return m.activate()
	default:
		return m.move(k)
	}
	return nil
}

func (m *Model) setVolume(v float64) tea.Cmd {
	v = min(1, max(0, v))
	m.state.Volume = v // feel immediate; the daemon confirms
	m.setFlash(fmt.Sprintf("volume %d%%", int(math.Round(v*100))))
	return m.send(ipc.Request{Cmd: ipc.CmdVolume, Value: v})
}

// searchKey edits the search box.
func (m *Model) searchKey(msg tea.KeyPressMsg) tea.Cmd {
	switch msg.String() {
	case "esc":
		m.searching = false
	case "enter":
		m.searching = false
		q := strings.TrimSpace(m.query)
		if q == "" {
			return nil
		}
		m.stacks[secSearch] = []*view{{title: "Search", key: "search:" + q}}
		return m.load(m.cur())
	case "backspace":
		if r := []rune(m.query); len(r) > 0 {
			m.query = string(r[:len(r)-1])
		}
	case "ctrl+u":
		m.query = ""
	case "ctrl+c":
		return tea.Quit
	default:
		m.query += msg.Text
	}
	return nil
}

func (m *Model) move(k string) tea.Cmd {
	v := m.cur()
	n := len(v.rows)
	if n == 0 {
		return nil
	}
	page := max(1, m.listRows()-1)
	prev := v.sel
	switch k {
	case "up", "k":
		v.sel--
	case "down", "j":
		v.sel++
	case "pgup", "ctrl+u":
		v.sel -= page
	case "pgdown", "ctrl+d":
		v.sel += page
	case "g", "home":
		v.sel = 0
	case "G", "end":
		v.sel = n - 1
	default:
		return nil
	}
	v.sel = max(0, min(v.sel, n-1))
	if v.sel != prev {
		v.selAt = m.frame
	}
	return nil
}

// click maps a mouse click onto the layout recorded by the last render.
// A click plays a song or opens an item; right click goes back.
func (m *Model) click(ms tea.Mouse) tea.Cmd {
	g := m.geo
	if ms.Button == tea.MouseRight {
		m.back()
		return nil
	}
	if ms.Button != tea.MouseLeft {
		return nil
	}
	for i, r := range g.tabs {
		if r.has(ms.X, ms.Y) {
			return m.switchTo(section(i))
		}
	}
	switch {
	case g.search.has(ms.X, ms.Y):
		m.searching = true
	case g.crumb.has(ms.X, ms.Y):
		m.back()
	case g.play.has(ms.X, ms.Y):
		return m.send(ipc.Request{Cmd: ipc.CmdToggle})
	case g.prev.has(ms.X, ms.Y):
		return m.send(ipc.Request{Cmd: ipc.CmdPrev})
	case g.next.has(ms.X, ms.Y):
		return m.send(ipc.Request{Cmd: ipc.CmdNext})
	case g.shuffle.has(ms.X, ms.Y):
		return m.key("s")
	case g.repeat.has(ms.X, ms.Y):
		return m.key("r")
	case g.volume.has(ms.X, ms.Y):
		return m.key("m")
	case g.bar.has(ms.X, ms.Y) && m.state.Dur > 0:
		f := float64(ms.X-g.bar.x0) / float64(max(1, g.bar.x1-g.bar.x0-1))
		return m.send(ipc.Request{Cmd: ipc.CmdSeek, Value: f * m.state.Dur})
	case g.list.has(ms.X, ms.Y):
		v := m.cur()
		idx := v.off + ms.Y - g.list.y0
		if idx >= len(v.rows) {
			return nil
		}
		v.sel, v.selAt = idx, m.frame
		return m.activate()
	}
	return nil
}
