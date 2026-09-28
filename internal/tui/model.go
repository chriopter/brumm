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
	"math/rand/v2"
	"net/http"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/art"
	"github.com/chriopter/brumm/internal/config"
	"github.com/chriopter/brumm/internal/ipc"
	"github.com/chriopter/brumm/internal/login"
)

// bands is how many spectrum bands the TUI asks the daemon for.
const bands = 48

// A play queues the chosen song and at most this many in all.
const queueAfter = 500

type section int

const (
	secPlaylists section = iota
	secAlbums
	secArtists
	secSongs
	secSearch
	secQueue
	numSections
)

var (
	sectionNames = []string{"Playlists", "Albums", "Artists", "Songs", "Search", "Queue"}
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
	qpos    int // for the queue view: the queue index of the first row
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
		v        *view
		reply    ipc.Message
		err      error
		selectID string // select this song once loaded
	}
	coverMsg struct {
		url string
		img image.Image
	}
	albumMsg struct {
		item apple.Item
		err  error
	}
	loginMsg struct{ err error }
	errMsg   struct{ err error }
	tickMsg  struct{}
	holdMsg  struct{ seq int } // space is still down after holdDelay
	lovedMsg struct{ ids []string }
	seekMsg  struct{ seq int } // the last seek key was a moment ago
	typedMsg struct{ seq int } // the search query stopped changing
	flashMsg string
	openMsg  struct { // open an item found by a lookup, select a song in it
		item apple.Item
		song string
	}
)

// Holding space this long previews the selected song instead of pausing.
const holdDelay = 280 * time.Millisecond

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
	stacks  [numSections][]*view // one navigation stack per section

	searching bool // the search box has focus
	query     string
	typedSeq  int

	loved map[string]bool // favorite song ids seen so far

	seekTo  float64 // where pending seek keys point
	seekAt  time.Time
	seekSeq int

	coverURL string
	cover    image.Image
	rendered map[art.Size][]string
	covers   map[string]image.Image // recently seen covers by address
	coverLRU []string
	fetching map[string]bool
	thumbs   map[string][]string // rendered preview-card covers by address and size

	flash   string
	flashAt time.Time
	lastErr string
	help    bool
	resumed bool
	lastVol float64
	geo     geometry

	releases   bool // the terminal reports key releases (kitty protocol)
	spaceDown  bool
	spaceSeq   int
	previewing bool

	split    float64 // the list's share of the width
	dragging bool    // the divider is being dragged
	reexec   bool    // the binary was updated: restart into it on quit

	full     bool // fullscreen visualizer
	vizStyle int
	vizPrev  int       // style fading out, -1 when none
	vizAt    time.Time // when the current style came in
	viz      visualizer
	vizSpec  []float64
	wave     []float64
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
		split:      loadSplit(),
		vizStyle:   vizOpening,
		vizPrev:    -1,
	}
	for s := secPlaylists; s <= secSongs; s++ {
		m.stacks[s] = []*view{{title: sectionNames[s], key: "list:" + sectionLists[s]}}
	}
	m.stacks[secSearch] = []*view{{title: "Search", key: "search:", loaded: true}}
	m.stacks[secQueue] = []*view{{title: "Queue", key: "queue:"}}
	m.loved = map[string]bool{}
	m.covers, m.fetching, m.thumbs = map[string]image.Image{}, map[string]bool{}, map[string][]string{}
	return m
}

func (m *Model) Init() tea.Cmd {
	return tea.Batch(m.listen(), tick(), m.load(m.cur()), m.maybeFetchCover(),
		m.send(ipc.Request{Cmd: ipc.CmdUpdate})) // look for an update on every start
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
	case v.key == "queue:":
		req = ipc.Request{Cmd: ipc.CmdQueue, Value: 500}
	default:
		return nil
	}
	v.loading = true
	client := m.client
	return func() tea.Msg {
		reply, err := client.Do(req)
		return loadedMsg{v: v, reply: reply, err: err}
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
	v.qpos = max(0, reply.Pos)
	// An album opened from a link has no name yet; its songs carry it.
	if v.item != nil && v.item.Kind == apple.KindAlbum && v.title == "Album" && len(reply.Tracks) > 0 {
		v.title = reply.Tracks[0].Album
	}
	v.sel = min(v.sel, max(0, len(rows)-1))
}

// fetchLoved asks which songs of a view are favorites.
func (m *Model) fetchLoved(v *view) tea.Cmd {
	var ids []string
	for _, r := range v.rows {
		if r.track != nil && len(ids) < 1000 {
			ids = append(ids, r.track.ID)
		}
	}
	if len(ids) == 0 {
		return nil
	}
	client := m.client
	return func() tea.Msg {
		reply, err := client.Do(ipc.Request{Cmd: ipc.CmdLoved, IDs: ids})
		if err != nil {
			return nil
		}
		return lovedMsg{reply.IDs}
	}
}

// stageArtwork is the cover to show: the preview's, else the playing
// song's as the library lists it (the same address the list prefetched),
// else what the player reports.
func (m *Model) stageArtwork() string {
	if p := m.state.Preview; p != nil && p.Artwork != "" {
		return p.Artwork
	}
	if id := m.state.ID; id != "" {
		for _, stack := range m.stacks {
			for _, v := range stack {
				for _, r := range v.rows {
					if r.track != nil && r.track.ID == id && r.track.Artwork != "" {
						return r.track.Artwork
					}
				}
			}
		}
	}
	return m.state.Artwork
}

func (m *Model) maybeFetchCover() tea.Cmd {
	url := m.stageArtwork()
	if url == m.coverURL {
		return nil
	}
	m.coverURL, m.cover, m.rendered = url, nil, map[art.Size][]string{}
	if url == "" {
		return nil
	}
	if img, ok := m.covers[url]; ok {
		m.cover = img
		return nil
	}
	return m.fetchCover(url)
}

// fetchCover loads a cover through the disk cache, once at a time per address.
func (m *Model) fetchCover(url string) tea.Cmd {
	if m.fetching[url] {
		return nil
	}
	m.fetching[url] = true
	client := m.http
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
		defer cancel()
		img, _ := art.Cached(ctx, client, filepath.Join(config.CacheDir(), "covers"), url)
		return coverMsg{url, img}
	}
}

// keepCover remembers a decoded cover, dropping the oldest past 80.
func (m *Model) keepCover(url string, img image.Image) {
	if _, ok := m.covers[url]; !ok {
		m.coverLRU = append(m.coverLRU, url)
	}
	m.covers[url] = img
	for len(m.coverLRU) > 80 {
		delete(m.covers, m.coverLRU[0])
		m.coverLRU = m.coverLRU[1:]
	}
	if len(m.thumbs) > 40 {
		m.thumbs = map[string][]string{}
	}
}

// prefetchCovers loads the covers of the rows on screen and just beyond,
// so a song's cover is ready the moment it plays.
func (m *Model) prefetchCovers(v *view) tea.Cmd {
	var cmds []tea.Cmd
	seen := map[string]bool{}
	lo, hi := max(0, v.off-10), min(len(v.rows), v.off+m.listRows()+10)
	// The selected row first: its preview card shows it.
	order := []int{v.sel}
	for i := lo; i < hi; i++ {
		order = append(order, i)
	}
	for _, i := range order {
		if i < 0 || i >= len(v.rows) || len(cmds) >= 8 {
			continue
		}
		url := ""
		if t := v.rows[i].track; t != nil {
			url = t.Artwork
		} else if it := v.rows[i].item; it != nil {
			url = it.Artwork
		}
		if url == "" || seen[url] {
			continue
		}
		seen[url] = true
		if _, ok := m.covers[url]; ok {
			continue
		}
		if cmd := m.fetchCover(url); cmd != nil {
			cmds = append(cmds, cmd)
		}
	}
	return tea.Batch(cmds...)
}

func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		m.cellAspect = cellAspect()
	case tickMsg:
		m.frame++
		if m.full && time.Since(m.vizAt) > vizEvery {
			m.showViz((m.vizStyle + 1) % len(vizNames))
		}
		if m.flash != "" && time.Since(m.flashAt) > 4*time.Second {
			m.flash = ""
		}
		return m, tick()
	case eventMsg:
		return m, m.event(ipc.Message(msg))
	case closedMsg:
		// The daemon went away. After an update it restarts with a new
		// binary; restart the TUI into it too.
		if binaryUpdated() {
			m.reexec = true
			return m, tea.Quit
		}
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
		if id := msg.selectID; id != "" {
			for i, r := range msg.v.rows {
				if r.track != nil && (r.track.ID == id || strings.HasSuffix(r.track.ID, id)) {
					msg.v.sel, msg.v.selAt = i, m.frame
					break
				}
			}
		}
		return m, tea.Batch(m.maybeResume(), m.fetchLoved(msg.v), m.prefetchCovers(msg.v), m.maybeFetchCover())
	case lovedMsg:
		for _, id := range msg.ids {
			m.loved[id] = true
		}
	case seekMsg:
		if msg.seq == m.seekSeq {
			if d := m.state.Dur; d > 0 && m.seekTo >= d-1 {
				return m, m.send(ipc.Request{Cmd: ipc.CmdNext})
			}
			return m, m.send(ipc.Request{Cmd: ipc.CmdSeek, Value: m.seekTo})
		}
	case typedMsg:
		if msg.seq == m.typedSeq {
			return m, m.runSearch(false)
		}
	case flashMsg:
		m.setFlash(string(msg))
	case openMsg:
		it := msg.item
		v := &view{title: it.Name, key: it.Key(), item: &it, loading: true}
		m.stacks[m.section] = append(m.stack(), v)
		client, song := m.client, msg.song
		return m, func() tea.Msg {
			reply, err := client.Do(ipc.Request{Cmd: ipc.CmdOpen, Item: v.item})
			return loadedMsg{v: v, reply: reply, err: err, selectID: song}
		}
	case tea.PasteMsg:
		if m.searching {
			m.query += strings.TrimSpace(msg.Content)
			return m, m.typed()
		}
		// A pasted music.apple.com link opens right away.
		m.query = strings.TrimSpace(msg.Content)
		return m, m.runSearch(true)
	case coverMsg:
		delete(m.fetching, msg.url)
		if msg.img != nil {
			m.keepCover(msg.url, msg.img)
		}
		if msg.url == m.coverURL {
			m.cover = msg.img
		}
	case albumMsg:
		if msg.err != nil {
			m.setFlash(msg.err.Error())
			return m, nil
		}
		it := msg.item
		return m, m.push(&view{title: it.Name, key: it.Key(), item: &it})
	case loginMsg:
		if msg.err != nil {
			m.setFlash("sign-in failed: " + msg.err.Error())
			return m, nil
		}
		m.setFlash("signed in")
		return m, m.send(ipc.Request{Cmd: ipc.CmdReload})
	case errMsg:
		m.setFlash(msg.err.Error())
	case tea.KeyboardEnhancementsMsg:
		m.releases = m.releases || msg.SupportsEventTypes()
	case holdMsg:
		return m, m.hold(msg.seq)
	case tea.KeyReleaseMsg:
		// Any release proves the terminal reports them — the startup
		// answer is not always right — so hold-to-preview can work.
		m.releases = true
		if msg.String() == "space" || msg.String() == " " {
			return m, m.spaceUp()
		}
	case tea.KeyPressMsg:
		if m.searching {
			return m, m.searchKey(msg)
		}
		if k := msg.String(); k == "space" || k == " " {
			return m, m.spaceDownKey(msg.IsRepeat)
		}
		if m.full {
			if cmd, ok := m.fullKey(msg.String()); ok {
				return m, cmd
			}
		}
		return m, m.key(msg.String())
	case tea.MouseClickMsg:
		if ms := msg.Mouse(); ms.Button == tea.MouseLeft && m.geo.divider.has(ms.X, ms.Y) {
			m.dragging = true
			return m, nil
		}
		return m, m.click(msg.Mouse())
	case tea.MouseMotionMsg:
		if m.dragging {
			m.setSplit(float64(msg.X-margin) / float64(max(1, m.width-2*margin)))
		}
	case tea.MouseReleaseMsg:
		if m.dragging {
			m.dragging = false
			saveSplit(m.split)
		}
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
		songChanged := msg.State.ID != m.state.ID
		m.state, m.stateAt = *msg.State, time.Now()
		if songChanged && m.section == secQueue && len(m.stack()) == 1 {
			cmds = append(cmds, m.load(m.cur()))
		}
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
	if msg.Wave != nil {
		if len(m.wave) != len(msg.Wave) {
			m.wave = make([]float64, len(msg.Wave))
		}
		for i, v := range msg.Wave {
			m.wave[i] = float64(v) / 100
		}
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

// position extrapolates the playhead between state updates; right after a
// seek it shows where the seek goes, so the bar never jumps back.
func (m *Model) position() float64 {
	if since := time.Since(m.seekAt); since < 1500*time.Millisecond {
		p := m.seekTo
		if m.state.Playing && since > 250*time.Millisecond {
			p += (since - 250*time.Millisecond).Seconds()
		}
		return min(p, max(m.state.Dur, p))
	}
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
	if s == secQueue {
		m.stacks[secQueue] = m.stacks[secQueue][:1]
		m.cur().loaded = false // always fresh
	}
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
	if v.key == "queue:" {
		return m.send(ipc.Request{Cmd: ipc.CmdJump, Value: float64(v.qpos + v.sel)})
	}
	ids, at := v.songs()
	i := at[v.sel]
	window := ids[i:min(len(ids), i+queueAfter)]
	if m.state.Shuffle && len(ids) > len(window) {
		// Shuffle draws from the whole list, not just the songs around
		// the chosen one: it first, then a random pick of the rest.
		window = append([]string{r.track.ID}, sample(ids, r.track.ID, queueAfter-1)...)
	}
	m.setFlash("▶ " + r.track.Title) // at once: the song itself starts a moment later
	return m.send(ipc.Request{Cmd: ipc.CmdPlay, IDs: window, Start: r.track.ID, Source: v.key})
}

// sample returns up to n ids in random order, leaving out skip.
func sample(ids []string, skip string, n int) []string {
	out := make([]string, 0, len(ids))
	for _, id := range ids {
		if id != skip {
			out = append(out, id)
		}
	}
	rand.Shuffle(len(out), func(i, j int) { out[i], out[j] = out[j], out[i] })
	return out[:min(n, len(out))]
}

// seek moves the playhead to sec. Repeated seek keys collect into one
// request, sent once they stop; the bar shows the target at once.
func (m *Model) seek(sec float64) tea.Cmd {
	if m.state.Dur <= 0 {
		return nil
	}
	m.seekTo = max(0, min(sec, m.state.Dur))
	m.seekAt = time.Now()
	m.seekSeq++
	seq := m.seekSeq
	return tea.Tick(250*time.Millisecond, func(time.Time) tea.Msg { return seekMsg{seq} })
}

// selectedTrack is the song under the cursor, or else the one playing.
func (m *Model) selectedTrack() (id string, ok bool) {
	v := m.cur()
	if v.sel < len(v.rows) && v.rows[v.sel].track != nil {
		return v.rows[v.sel].track.ID, true
	}
	return m.state.ID, m.state.ID != ""
}

// lookup opens what a song belongs to (CmdArtist, CmdAlbum).
func (m *Model) lookup(cmd string) tea.Cmd {
	id, ok := m.selectedTrack()
	if !ok {
		return nil
	}
	client := m.client
	return func() tea.Msg {
		reply, err := client.Do(ipc.Request{Cmd: cmd, Start: id})
		if err != nil || len(reply.Items) == 0 {
			return errMsg{fmt.Errorf("nothing found for this song")}
		}
		return albumMsg{item: reply.Items[0]}
	}
}

// enqueue adds the selected song — or everything in the selected album or
// playlist — to play next or at the end of the queue.
func (m *Model) enqueue(next bool) tea.Cmd {
	v := m.cur()
	if v.sel >= len(v.rows) {
		return nil
	}
	r, client := v.rows[v.sel], m.client
	done := map[bool]string{true: "plays next", false: "added to the queue"}[next]
	return func() tea.Msg {
		var ids []string
		if r.track != nil {
			ids = []string{r.track.ID}
		} else {
			reply, err := client.Do(ipc.Request{Cmd: ipc.CmdOpen, Item: r.item})
			if err != nil {
				return errMsg{err}
			}
			for _, t := range reply.Tracks {
				ids = append(ids, t.ID)
			}
		}
		if len(ids) == 0 {
			return nil
		}
		value := 0.0
		if next {
			value = 1
		}
		if _, err := client.Do(ipc.Request{Cmd: ipc.CmdEnqueue, IDs: ids, Value: value}); err != nil {
			return errMsg{err}
		}
		return flashMsg(done)
	}
}

// toggleLoved marks the selected (or playing) song as a favorite or not.
func (m *Model) toggleLoved() tea.Cmd {
	id, ok := m.selectedTrack()
	if !ok {
		return nil
	}
	on := !m.loved[id]
	if on {
		m.loved[id] = true
	} else {
		delete(m.loved, id)
	}
	m.setFlash(map[bool]string{true: "♥ favorite", false: "no longer a favorite"}[on])
	value := 0.0
	if on {
		value = 1
	}
	return m.send(ipc.Request{Cmd: ipc.CmdLove, Start: id, Value: value})
}

// copyLink puts the song's music.apple.com link on the clipboard.
func (m *Model) copyLink() tea.Cmd {
	id, ok := m.selectedTrack()
	if !ok {
		return nil
	}
	client := m.client
	return func() tea.Msg {
		reply, err := client.Do(ipc.Request{Cmd: ipc.CmdLink, Start: id})
		if err != nil {
			return errMsg{err}
		}
		cmd := exec.Command("wl-copy")
		cmd.Stdin = strings.NewReader(reply.Link)
		if err := cmd.Run(); err != nil {
			return errMsg{fmt.Errorf("copy: %w", err)}
		}
		return flashMsg("link copied")
	}
}

// spaceDownKey starts a tap-or-hold: a tap toggles playback on release, a
// hold previews the selected song until release. Terminals that do not
// report releases just toggle.
func (m *Model) spaceDownKey(repeat bool) tea.Cmd {
	if !m.releases {
		return m.send(ipc.Request{Cmd: ipc.CmdToggle})
	}
	if repeat || m.spaceDown {
		return nil
	}
	m.spaceDown = true
	m.spaceSeq++
	seq := m.spaceSeq
	return tea.Tick(holdDelay, func(time.Time) tea.Msg { return holdMsg{seq} })
}

func (m *Model) hold(seq int) tea.Cmd {
	if !m.spaceDown || seq != m.spaceSeq {
		return nil
	}
	v := m.cur()
	if v.sel >= len(v.rows) || v.rows[v.sel].track == nil {
		return nil // nothing to preview: release will toggle
	}
	m.previewing = true
	return m.send(ipc.Request{Cmd: ipc.CmdPreview, Start: v.rows[v.sel].track.ID, Value: 1})
}

// togglePreview starts previewing the selected song, or ends a preview —
// the same as holding space, for terminals that cannot report releases.
func (m *Model) togglePreview() tea.Cmd {
	if m.previewing {
		m.previewing = false
		return m.send(ipc.Request{Cmd: ipc.CmdPreview})
	}
	v := m.cur()
	if v.sel >= len(v.rows) || v.rows[v.sel].track == nil {
		return nil
	}
	m.previewing = true
	return m.send(ipc.Request{Cmd: ipc.CmdPreview, Start: v.rows[v.sel].track.ID, Value: 1})
}

func (m *Model) spaceUp() tea.Cmd {
	if !m.spaceDown {
		return nil
	}
	m.spaceDown = false
	if m.previewing {
		m.previewing = false
		return m.send(ipc.Request{Cmd: ipc.CmdPreview})
	}
	return m.send(ipc.Request{Cmd: ipc.CmdToggle})
}

// toggleFull switches the fullscreen visualizer, asking the daemon for
// waveform data only while it shows.
func (m *Model) toggleFull() tea.Cmd {
	m.full = !m.full
	wave := 0
	if m.full {
		wave = max(64, m.width*2)
		m.vizStyle, m.vizPrev, m.vizAt = vizOpening, -1, time.Now()
	}
	return m.send(ipc.Request{Cmd: ipc.CmdSubscribe, Bands: bands, Wave: wave})
}

// fullKey handles the keys that mean something else in fullscreen.
func (m *Model) fullKey(k string) (tea.Cmd, bool) {
	n := len(vizNames)
	switch k {
	case "esc", "f", "q":
		return m.toggleFull(), true
	case "v", "tab", "down", "j":
		m.showViz((m.vizStyle + 1) % n)
	case "V", "shift+tab", "up", "k":
		m.showViz((m.vizStyle + n - 1) % n)
	case "1", "2", "3", "4", "5", "6", "7", "8", "9", "0":
		i := int(k[0]-'0') - 1
		if k == "0" {
			i = 9
		}
		if i < n && i != m.vizStyle {
			m.showViz(i)
		}
	default:
		return nil, false
	}
	return nil, true
}

// Fullscreen opens on the showpiece and moves on every minute, dissolving
// from one style into the next.
const (
	vizOpening = 9 // milkdrop
	vizEvery   = time.Minute
	vizFade    = 1500 * time.Millisecond
)

func (m *Model) showViz(style int) {
	m.vizPrev, m.vizStyle, m.vizAt = m.vizStyle, style, time.Now()
}

// openAlbum opens the album of the selected song.
func (m *Model) openAlbum() tea.Cmd {
	v := m.cur()
	if v.sel >= len(v.rows) || v.rows[v.sel].track == nil {
		return nil
	}
	id, client := v.rows[v.sel].track.ID, m.client
	return func() tea.Msg {
		reply, err := client.Do(ipc.Request{Cmd: ipc.CmdAlbum, Start: id})
		if err != nil || len(reply.Items) == 0 {
			return albumMsg{err: fmt.Errorf("no album found for this song")}
		}
		return albumMsg{item: reply.Items[0]}
	}
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
					return loadedMsg{v: v, reply: reply, err: err, selectID: id}
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
	case "1", "2", "3", "4", "5", "6":
		return m.switchTo(section(k[0] - '1'))
	case "tab":
		return m.switchTo((m.section + 1) % numSections)
	case "shift+tab":
		return m.switchTo((m.section + numSections - 1) % numSections)
	case "/":
		m.section, m.searching, m.help = secSearch, true, false
	case "f":
		return m.toggleFull()
	case "n":
		return m.send(ipc.Request{Cmd: ipc.CmdNext})
	case "p", "b":
		return m.send(ipc.Request{Cmd: ipc.CmdPrev})
	case "left", "right":
		delta := 10.0
		if k == "left" {
			delta = -10
		}
		return m.seek(m.position() + delta)
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
	case "[", "]":
		delta := 0.05
		if k == "[" {
			delta = -0.05
		}
		m.setSplit(m.split + delta)
		saveSplit(m.split)
	case "c":
		return m.jumpToPlaying()
	case "o":
		return m.togglePreview()
	case "a":
		return m.openAlbum()
	case "A":
		return m.lookup(ipc.CmdArtist)
	case "z", "Z":
		return m.enqueue(k == "Z")
	case "*":
		return m.toggleLoved()
	case "y":
		return m.copyLink()
	case "U":
		if m.state.Update == "" {
			return nil
		}
		m.setFlash("installing " + m.state.Update + "…")
		client := m.client
		return func() tea.Msg {
			if _, err := client.Do(ipc.Request{Cmd: ipc.CmdUpdate, Value: 1}); err != nil {
				return errMsg{err}
			}
			return flashMsg("updated — brumm restarts into it at the next pause or song change")
		}
	case "esc", "h", "backspace":
		if m.help {
			m.help = false
			return nil
		}
		if m.previewing {
			return m.togglePreview()
		}
		m.back()
	case "enter", "l":
		return m.activate()
	default:
		return m.move(k)
	}
	return nil
}

func (m *Model) setSplit(f float64) { m.split = min(0.75, max(0.2, f)) }

func (m *Model) setVolume(v float64) tea.Cmd {
	v = min(1, max(0, v))
	m.state.Volume = v // feel immediate; the daemon confirms
	m.setFlash(fmt.Sprintf("volume %d%%", int(math.Round(v*100))))
	return m.send(ipc.Request{Cmd: ipc.CmdVolume, Value: v})
}

// searchKey edits the search box; results follow the typing.
func (m *Model) searchKey(msg tea.KeyPressMsg) tea.Cmd {
	switch msg.String() {
	case "esc":
		m.searching = false
		return nil
	case "enter":
		m.searching = false
		return m.runSearch(true)
	case "backspace":
		if r := []rune(m.query); len(r) > 0 {
			m.query = string(r[:len(r)-1])
		}
	case "ctrl+w":
		q := strings.TrimRight(m.query, " ")
		if i := strings.LastIndex(q, " "); i >= 0 {
			m.query = q[:i+1]
		} else {
			m.query = ""
		}
	case "ctrl+u":
		m.query = ""
	case "ctrl+c":
		return tea.Quit
	case "down", "up":
		m.searching = false // move into the results
		return m.move(msg.String())
	default:
		if msg.Text == "" {
			return nil
		}
		m.query += msg.Text
	}
	return m.typed()
}

// typed runs the search once the query rests for a moment.
func (m *Model) typed() tea.Cmd {
	m.typedSeq++
	seq := m.typedSeq
	return tea.Tick(350*time.Millisecond, func(time.Time) tea.Msg { return typedMsg{seq} })
}

// runSearch searches for the query — or opens it, if it is a
// music.apple.com link. final is true for enter or a paste.
func (m *Model) runSearch(final bool) tea.Cmd {
	q := strings.TrimSpace(m.query)
	m.section = secSearch
	if it, song, ok := apple.ParseLink(q); ok {
		if !final {
			return nil
		}
		m.searching = false
		m.stacks[secSearch] = m.stacks[secSearch][:1]
		if it.ID == "" { // a song link: open its album
			client := m.client
			return func() tea.Msg {
				reply, err := client.Do(ipc.Request{Cmd: ipc.CmdAlbum, Start: song})
				if err != nil || len(reply.Items) == 0 {
					return errMsg{fmt.Errorf("could not open that link")}
				}
				return openMsg{reply.Items[0], song}
			}
		}
		return func() tea.Msg { return openMsg{it, song} }
	}
	if len([]rune(q)) < 2 {
		return nil
	}
	if cur := m.stacks[secSearch][0]; cur.key == "search:"+q {
		return nil // already showing it
	}
	m.stacks[secSearch] = []*view{{title: "Search", key: "search:" + q}}
	return m.load(m.cur())
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
	return m.prefetchCovers(v)
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
		return m.seek(f * m.state.Dur)
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
