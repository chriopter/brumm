// Package tui is brumm's terminal interface: a drill-in browser on the left
// (library sections, then playlists, albums, artists, songs and search
// results) and a stage on the right with the cover, a live spectrum, the
// playhead and the transport controls.
package tui

import (
	"context"
	"fmt"
	"image"
	"image/color"
	"math"
	"math/rand/v2"
	"net/http"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
	"unicode"
	"unicode/utf8"

	tea "charm.land/bubbletea/v2"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/art"
	"github.com/chriopter/brumm/internal/config"
	"github.com/chriopter/brumm/internal/ipc"
	"github.com/chriopter/brumm/internal/launch"
	"github.com/chriopter/brumm/internal/login"
)

// bands is how many spectrum bands the TUI asks the daemon for, and at
// what rate: the small meters need less than the fullscreen visualizer.
const (
	bands    = 48
	meterFPS = 10 // the playing row's equalizer, drawn at most this often (eqEvery)
	vizFPS   = 30 // spectrum frames for the fullscreen visualizer; it draws faster, see drawFPS
	fallRef  = 25 // the rate the meters' fall was tuned at
)

// A play queues the chosen song and at most this many in all.
const queueAfter = 500

type section int

const (
	secHome section = iota
	secPlaylists
	secAlbums
	secArtists
	secSongs
	secSearch
	secQueue
	secRadio
	numSections
)

var (
	sectionNames = []string{"Home", "Playlists", "Albums", "Artists", "Songs", "Search", "Queue", "Radio"}
	sectionIcons = []string{icHome, icPlaylist, icAlbum, icArtist, icSong, icSearch, icQueue, icStation}
	sectionLists = []string{ipc.ListPlaylists, ipc.ListAlbums, ipc.ListArtists, ipc.ListSongs}
)

// row is one line of a list: a song, or something that opens.
type row struct {
	track *apple.Track
	item  *apple.Item
	head  string // a shelf's title
	note  string // a line of text about the view
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
	filter  string
	all     []row  // every row while a filter shows some (filter.go)
	want    string // a song to select once the rows come
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
	thumbMsg struct {
		key   string
		lines []string
	}
	albumMsg struct {
		item apple.Item
		song string // the song looked up, selected when the item lists it
		err  error
	}
	loginMsg   struct{ err error }
	errMsg     struct{ err error }
	tickMsg    struct{ seq int }
	wheelMsg   struct{}          // the spinning wheel's next frame
	refreshMsg int               // the screen's refresh rate, found at start
	holdMsg    struct{ seq int } // space is still down after holdDelay
	seekMsg    struct{ seq int } // the last seek key was a moment ago
	typedMsg   struct{ seq int } // the search query stopped changing
	flashMsg   string
	openMsg    struct { // open an item found by a lookup, select a song in it
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
	frame         int     // animation clock in 100 ms steps, read fresh on every update
	start         time.Time
	ticking       bool      // a tick is on its way; none is while nothing animates
	tickAt        time.Time // when it comes
	tickSeq       int       // the number of the tick that counts
	blurred       bool      // the terminal lost focus: stop what only a viewer would see
	eqShown       bool      // the last render drew the playing row's equalizer
	cardShown     bool      // the last render laid a card on the cover (card.go)
	marquee       bool      // the last render scrolled a selected row
	specFPS       int       // the rate the spectrum arrives at
	specOn        bool      // the stream carries the spectrum

	state   ipc.State
	stateAt time.Time
	spec    []float64

	section section
	stacks  [numSections][]*view // one navigation stack per section

	searching bool // the search box has focus
	query     string
	typedSeq  int

	rating map[string]int // loves (1) and dislikes (-1) seen so far, by id

	pending  *pendingPlay // a song started and shown, not yet reported playing
	wantPlay *wantPlaying // a pause or play asked for, shown before the player reports it

	cardSize art.Size // the size cards were last drawn at, for drawing ahead
	refresh  int      // the screen's refresh rate: the visualizer's auto rate

	upd *updatePopup // the update check's answer, while it shows

	seekTo  float64 // where pending seek keys point
	seekAt  time.Time
	seekSeq int

	coverURL string
	cover    image.Image
	coverDim image.Image // darker, behind a card (card.go)
	rendered map[art.Size][]string
	covers   map[string]image.Image // recently seen covers by address
	coverLRU []string
	fetching map[string]bool
	thumbs   map[string][]string // rendered preview-card covers by address and size
	// The preview card the last render wanted but did not have. Dithering
	// takes ~20 ms, too slow for every row a held arrow key passes, so it
	// runs in the background at most once per tick.
	thumbWant thumbReq
	thumbBusy bool

	flash   string
	flashAt time.Time
	lastErr string
	help    bool
	resumed bool
	placing bool // asking for a place the window left (place.go): no resuming till it answers
	lastVol float64
	geo     geometry

	releases   bool // the terminal reports key releases (kitty protocol)
	spaceDown  bool
	spaceSeq   int
	previewing bool

	split    float64 // the list's share of the width
	dragging bool    // the divider is being dragged
	dragOff  int     // where on the divider it was grabbed
	reexec   bool    // the binary was updated: restart into it on quit
	pointer  string  // the mouse pointer's shape as last sent (pointer.go)

	opts     options
	sentOpts map[string]any      // the options as last sent to the daemon
	optQueue chan map[string]any // changes on their way to it, in order

	hasOmarchy bool // omarchy is here: the bar widget can be switched (bar.go)
	barOn      bool // the bar widget is on, as far as is known
	barAsk     bool // the first start's question about it is due

	termBg color.Color // the terminal's background, once it says: accents keep clear of it
	acc    accent      // the colors of what plays (accent.go)

	next      []nextUp      // the queue's next covers (upnext.go)
	upTracks  []apple.Track // the queue from the song playing on, as last asked
	upPos     int           // the queue position of its first song
	nextAsked nextKey       // the state they were last asked for in
	nextSeq   int
	nextGen   int        // counts changes to next, for the row's cache
	nextDraw  nextStrip  // the row as last drawn
	songs     []nextSong // the queue's next songs, when no other cover comes
	ahead     int        // songs in the queue after the playing one
	songDraw  nextList   // their list as last drawn

	filtering bool    // typing into the list's filter (filter.go)
	optOpen   bool    // the options menu shows
	pick      *picker // the add-to-playlist menu, while it shows
	optLines  []int   // the options menu line of each option
	optSel    int

	// Covers sent to the terminal as kitty graphics, by address and size.
	kitty      map[string]*kittyImage
	kittyWant  []kittyReq // what the last render lacked
	kittyBusy  bool
	kittyNext  int
	kittyClock int

	loginTried bool // the sign-in page opened on its own once

	// A spinning mouse wheel (see wheel).
	wheelAt    time.Time // the last notch
	wheelDrawn time.Time // the last notch drawn at once
	wheelMoved bool      // notches since the screen was drawn
	wheelTick  bool      // a wheelMsg is on its way
	still      bool      // this update changed nothing on screen: View reuses drawn
	drawn      tea.View
	motion     bool      // this update only moved time on: a spectrum frame, a tick (frame.go)
	fr         frame     // the last full screen, for redrawing only what moves
	eqBars     string    // the playing row's equalizer bars as last measured
	eqAt       time.Time // when
	vizKey     vizStatus // the visualizer's status line as last drawn
	vizLine    string
	vizFoot    []footHit

	full     bool // fullscreen visualizer
	vizList  bool // the list of styles shows over it
	vizSel   int
	vizFrom  int // the style before the list opened: esc goes back to it
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
		start:      time.Now(),
		specFPS:    meterFPS,
		specOn:     true, // as Run subscribed
		rendered:   map[art.Size][]string{},
		cellAspect: cellAspect(),
		lastVol:    1,
		split:      loadSplit(),
		opts:       loadOptions(),
		acc:        themeAccent(),
		hasOmarchy: hasOmarchy(),
		refresh:    60,
		kitty:      map[string]*kittyImage{},
		vizStyle:   vizOpening,
		vizPrev:    -1,
	}
	for s := secPlaylists; s <= secSongs; s++ {
		m.stacks[s] = []*view{{title: sectionNames[s], key: "list:" + sectionLists[s-secPlaylists]}}
	}
	m.stacks[secHome] = []*view{{title: "Home", key: "home:"}}
	m.stacks[secRadio] = []*view{{title: "Radio", key: "radio:", item: &apple.Item{Kind: apple.KindShelf, ID: "radio", Name: "Radio", Catalog: true}}}
	m.stacks[secSearch] = []*view{{title: "Search", key: "search:", loaded: true}}
	m.stacks[secQueue] = []*view{{title: "Queue", key: "queue:"}}
	m.rating = map[string]int{}
	m.covers, m.fetching, m.thumbs = map[string]image.Image{}, map[string]bool{}, map[string][]string{}
	m.barOn = m.hasOmarchy && barKnown(m.opts)
	return m
}

func (m *Model) Init() tea.Cmd {
	m.placing = true
	return tea.Batch(m.askPlace(), m.listen(), m.schedule(frameEvery), func() tea.Msg { return refreshMsg(screenRefresh()) }, m.load(m.cur()), m.maybeFetchCover(), m.subscribe(), m.autoLogin(),
		m.send(ipc.Request{Cmd: ipc.CmdUpdate}), // look for an update on every start
		m.readBar(), tea.RequestBackgroundColor, m.fetchNext(false))
}

const frameEvery = 100 * time.Millisecond

// schedule sets the next frame of the screen's own, replacing any later
// one: only the tick with the newest number counts, so there is always at
// most one chain of ticks however often they are rescheduled.
func (m *Model) schedule(every time.Duration) tea.Cmd {
	m.tickSeq++
	m.ticking, m.tickAt = true, time.Now().Add(every)
	seq := m.tickSeq
	return tea.Tick(every, func(time.Time) tea.Msg { return tickMsg{seq} })
}

// nextTick says when the screen next needs a frame of its own, if ever.
// Nothing ticks while nothing moves, so an idle brumm costs no wakeups;
// every message restarts the clock for as long as something animates.
func (m *Model) nextTick() (time.Duration, bool) {
	switch {
	case m.blurred && !m.full:
		return time.Second, m.state.Playing // the clock and the meter, for a glance
	case m.full && !m.state.Playing:
		return 2 * frameEvery, true // the paused visualizer only breathes
	case m.full:
		// The visualizer moves by the clock, so it draws at the display's
		// pace, not the spectrum's: its motion is as smooth as the rate.
		return time.Second / time.Duration(m.drawFPS()), true
	case m.full || m.marquee || m.flash != "" || m.thumbBusy || m.kittyBusy || len(m.kittyWant) > 0 || m.state.Preview != nil ||
		(m.upd != nil && (m.upd.checking || m.upd.installing)) ||
		m.state.Status == ipc.StatusStarting || m.cur().loading:
		return frameEvery, true
	case m.cardShown:
		return max(frameEvery, m.cardLeft()), true // take the card away on time
	case m.state.Playing && m.eqShown && len(m.spec) == 0:
		return 125 * time.Millisecond, true // no spectrum: the row's equalizer wobbles by itself
	case m.state.Playing:
		return m.nextChange(), true // the spectrum's own frames move the rest
	}
	return 0, false
}

// nextChange is how long until the playing screen changes by itself: the
// bear's next step, or the clock's next second, whichever comes first.
// Ticks land just after it, so the time is on time and nothing is drawn
// in between that would look the same.
func (m *Model) nextChange() time.Duration {
	bear := 5 * frameEvery
	d := bear - time.Since(m.start)%bear
	pos := m.position()
	_, played := math.Modf(pos)
	_, left := math.Modf(max(0, m.state.Dur-pos)) // the time left turns over on its own when the length is not whole
	for _, s := range []float64{1 - played, left} {
		if s > 0 {
			d = min(d, time.Duration(s*float64(time.Second)))
		}
	}
	return max(20*time.Millisecond, d+5*time.Millisecond)
}

// wantSpec: the spectrum can be seen — the visualizer, or the playing
// row's equalizer in a focused window.
func (m *Model) wantSpec() bool { return m.full || (!m.blurred && m.eqShown) }

// subscribe tells the daemon what to stream: the spectrum only while it
// can be seen, and faster only for the fullscreen visualizer.
func (m *Model) subscribe() tea.Cmd {
	r := ipc.Request{Cmd: ipc.CmdSubscribe, Bands: bands, FPS: meterFPS}
	switch {
	case m.full:
		r.Wave, r.FPS = max(96, m.width*3), vizFPS // room for the scope to find its trigger
	case !m.wantSpec():
		r.Bands = 0
	}
	m.specFPS, m.specOn = r.FPS, r.Bands > 0
	if !m.specOn {
		m.spec = nil // stale bars would stand still: the wobble until it streams again
	}
	return m.send(r)
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
		c, err := launch.Connect()
		if err != nil {
			return connectedMsg{err: err}
		}
		r, err := c.Do(ipc.Request{Cmd: ipc.CmdSubscribe, Bands: bands, FPS: meterFPS})
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
	case v.key == "home:":
		req = ipc.Request{Cmd: ipc.CmdHome}
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
	m.coverURL, m.cover, m.coverDim, m.rendered = url, nil, nil, map[art.Size][]string{}
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

// coverKeep is how many decoded covers stay: at least all prefetchCovers
// reaches, or each move would evict and fetch the same covers again.
func (m *Model) coverKeep() int { return max(80, 3*m.listRows()+8) }

// keepCover remembers a decoded cover, dropping the oldest past coverKeep.
func (m *Model) keepCover(url string, img image.Image) {
	if _, ok := m.covers[url]; !ok {
		m.coverLRU = append(m.coverLRU, url)
	}
	m.covers[url] = img
	for len(m.coverLRU) > m.coverKeep() {
		delete(m.covers, m.coverLRU[0])
		m.coverLRU = m.coverLRU[1:]
	}
}

type thumbReq struct {
	key, url string
	size     art.Size
}

// renderThumb draws a card off the event loop, one at a time: the one the
// screen wants, else one for a row next to the selection, so moving on
// finds its card drawn.
func (m *Model) renderThumb() tea.Cmd {
	if m.thumbBusy {
		return nil
	}
	w := m.thumbWant
	if _, done := m.thumbs[w.key]; done || w.key == "" || m.covers[w.url] == nil {
		w = m.nextThumb()
	}
	img := m.covers[w.url]
	if w.key == "" || img == nil {
		return nil
	}
	m.thumbBusy = true
	style := m.opts.cover()
	return func() tea.Msg { return thumbMsg{w.key, drawCover(style, img, w.size)} }
}

// prefetchCovers loads the covers of the rows on screen and a page either
// side, so a card or a song's cover is ready the moment it is wanted. Most
// come from the disk cache; the rest download side by side.
func (m *Model) prefetchCovers(v *view) tea.Cmd {
	var cmds []tea.Cmd
	seen := map[string]bool{}
	page := m.listRows()
	lo, hi := max(0, v.off-page), min(len(v.rows), v.off+2*page)
	// The selected row first: its preview card shows it.
	order := []int{v.sel}
	for i := lo; i < hi; i++ {
		order = append(order, i)
	}
	for _, i := range order {
		if i < 0 || i >= len(v.rows) || len(cmds) >= 32 {
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
	m.frame = int(time.Since(m.start) / frameEvery)
	m.still = false
	switch msg := msg.(type) {
	case tickMsg:
		m.motion = msg.seq == m.tickSeq
	case eventMsg:
		m.motion = msg.State == nil && msg.Options == nil && !msg.Library
	default:
		m.motion = false
	}
	if m.hovering(msg) {
		m.still = true
		return m, m.hover(msg)
	}
	model, cmd := m.update(msg)
	if _, ok := msg.(eventMsg); ok && m.motion && m.full && m.ticking && !m.still {
		m.still = true // the visualizer draws on its own clock: the spectrum waits for the next frame
	}
	if m.client != nil && m.drawn.Content != "" && m.wantSpec() != m.specOn {
		cmd = tea.Batch(cmd, m.subscribe()) // the playing row came into view, or left it
	}
	if _, isTick := msg.(tickMsg); !isTick {
		// Something happened: give the screen a frame to settle, and more
		// for as long as it animates. A tick far off (a card waiting to
		// go) is brought forward when something now needs frames sooner.
		if !m.ticking {
			cmd = tea.Batch(cmd, m.schedule(frameEvery))
		} else if every, ok := m.nextTick(); ok && every+10*time.Millisecond < time.Until(m.tickAt) {
			cmd = tea.Batch(cmd, m.schedule(every))
		}
	}
	// A cover that just arrived, or a card the last frame lacked, is drawn
	// at once rather than on the next tick.
	return model, tea.Batch(cmd, m.renderThumb())
}

func (m *Model) update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.FocusMsg:
		m.blurred = false
		return m, tea.Batch(m.subscribe(), tea.RequestBackgroundColor) // the theme may have changed
	case tea.BackgroundColorMsg:
		m.termBg = msg.Color
	case tea.BlurMsg:
		m.blurred = true
		return m, tea.Batch(m.subscribe(), m.setPointer(shapeDefault))
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		m.cellAspect = cellAspect()
	case tickMsg:
		if msg.seq != m.tickSeq {
			m.still = true
			return m, nil // replaced by a sooner one
		}
		m.ticking = false
		if m.full && !m.opts.NoVizCycle && time.Since(m.vizAt) > vizEvery {
			m.showViz((m.vizStyle + 1) % len(vizNames))
		}
		if m.flash != "" && time.Since(m.flashAt) > 4*time.Second {
			m.flash, m.motion = "", false
		}
		cmds := []tea.Cmd{m.renderThumb(), m.kittySend()}
		if every, ok := m.nextTick(); ok {
			cmds = append(cmds, m.schedule(every))
		}
		return m, tea.Batch(cmds...)
	case kittyMsg:
		return m, m.kittySent(msg)
	case updateMsg:
		m.updateChecked(msg)
	case installedMsg:
		m.updateInstalled(msg)
	case refreshMsg:
		m.refresh = int(msg)
	case barMsg:
		m.barRead(msg)
	case barSetMsg:
		m.barSet(msg)
	case thumbMsg:
		m.thumbBusy = false
		if len(m.thumbs) > 150 {
			m.thumbs = map[string][]string{}
		}
		m.thumbs[msg.key] = msg.lines
		return m, m.renderThumb()
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
		return m, tea.Batch(m.listen(), m.load(m.cur()), m.fetchNext(true))
	case loadedMsg:
		msg.v.loading = false
		if msg.err != nil {
			msg.v.err = msg.err
			return m, nil
		}
		fill(msg.v, msg.reply)
		if msg.selectID != "" {
			msg.v.want = msg.selectID
		}
		m.selectSong(msg.v, msg.v.want)
		msg.v.want = ""
		return m, tea.Batch(m.maybeResume(), m.fetchRatings(msg.v), m.prefetchCovers(msg.v), m.maybeFetchCover(), m.warm(msg.v))
	case ratingsMsg:
		for id, v := range msg {
			m.rating[id] = v
		}
	case pickerListsMsg:
		if p := m.pick; p != nil {
			p.loading, p.lists = false, msg.lists
			if msg.err != nil {
				m.setFlash(msg.err.Error())
			}
		}
	case seekMsg:
		if msg.seq == m.seekSeq {
			if d := m.state.Dur; d > 0 && m.seekTo >= d-1 {
				return m, m.skip()
			}
			return m, m.send(ipc.Request{Cmd: ipc.CmdSeek, Value: m.seekTo})
		}
	case typedMsg:
		if msg.seq == m.typedSeq && m.section == secSearch { // not after leaving it
			return m, m.runSearch(false)
		}
	case flashMsg:
		m.setFlash(string(msg))
	case placeMsg:
		return m, m.gotPlace(msg)
	case openMsg:
		it := msg.item
		return m, m.push(&view{title: it.Name, key: it.Key(), item: &it}, msg.song)
	case tea.PasteMsg:
		if m.filtering {
			v := m.cur()
			v.filter += strings.TrimSpace(msg.Content)
			applyFilter(v)
			return m, m.prefetchCovers(v)
		}
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
	case nextMsg:
		return m, m.gotNext(msg)
	case albumMsg:
		if msg.err != nil {
			m.setFlash(msg.err.Error())
			return m, nil
		}
		it := msg.item
		return m, m.push(&view{title: it.Name, key: it.Key(), item: &it}, msg.song)
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
		if m.upd != nil {
			return m, m.updateKey(msg.String())
		}
		if m.barAsking() {
			return m, m.barKey(msg.String())
		}
		if m.pick != nil {
			return m, m.pickerKey(msg)
		}
		if m.optOpen {
			return m, m.optionsKey(msg.String())
		}
		if m.searching {
			return m, m.searchKey(msg)
		}
		if m.filtering {
			return m, m.filterKey(msg)
		}
		if k := msg.String(); k == "space" || k == " " {
			return m, m.spaceDownKey(msg.IsRepeat)
		}
		if msg.String() == "?" && !m.full {
			m.help = !m.help
			return m, nil
		}
		if m.full {
			if cmd, ok := m.fullKey(msg.String()); ok {
				return m, cmd
			}
		}
		return m, m.key(msg.String())
	case tea.MouseClickMsg:
		if m.upd != nil {
			return m, m.updateClick(msg.Mouse().X, msg.Mouse().Y)
		}
		if m.barAsking() {
			return m, m.barClick(msg.Mouse().X, msg.Mouse().Y)
		}
		if m.pick != nil {
			return m, m.pickerClick(msg.Mouse().X, msg.Mouse().Y)
		}
		if m.optOpen {
			return m, m.optionsClick(msg.Mouse().X, msg.Mouse().Y)
		}
		if ms := msg.Mouse(); ms.Button == tea.MouseLeft && m.geo.divider.has(ms.X, ms.Y) {
			m.dragging, m.dragOff = true, ms.X-m.geo.divider.x0
			return m, nil
		}
		return m, m.click(msg.Mouse())
	case tea.MouseMotionMsg:
		if m.full && m.vizList {
			if i := msg.Mouse().Y - m.geo.optRow0; m.geo.options.has(msg.Mouse().X, msg.Mouse().Y) && i >= 0 && i < len(vizNames) {
				m.vizSel = i
				m.previewViz(i)
			}
			return m, nil
		}
		if m.dragging { // the divider stays under the mouse where it grabbed it
			m.setSplit(float64(msg.X-m.dragOff+1-margin) / float64(max(1, m.width-2*margin)))
		}
	case tea.MouseReleaseMsg:
		if m.dragging {
			m.dragging = false
			m.setSplit(m.shownSplit()) // dragged past where the cover stops growing: where it stopped
			saveSplit(m.split)
		}
	case tea.MouseWheelMsg:
		return m, m.wheel(msg.Button)
	case wheelMsg:
		return m, m.wheelFrame()
	}
	return m, nil
}

func (m *Model) event(msg ipc.Message) tea.Cmd {
	cmds := []tea.Cmd{m.listen()}
	if msg.State != nil {
		wasLoggedOut := m.state.Status == ipc.StatusLoggedOut
		m.keepShowing(msg.State)
		m.keepPlaying(msg.State)
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
		cmds = append(cmds, m.maybeFetchCover(), m.maybeResume(), m.autoLogin(), m.fetchNext(false))
	}
	if msg.Options != nil {
		m.optionsChanged(*msg.Options)
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
	fall := math.Pow(0.86, float64(fallRef)/float64(m.specFPS)) // the same fall per second at any rate
	for i, v := range in {
		m.spec[i] = max(float64(v)/255, m.spec[i]*fall)
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
	m.section, m.help, m.filtering = s, false, false // a filter stays with its list, the typing does not
	if s == secQueue {
		m.stacks[secQueue] = m.stacks[secQueue][:1]
		m.cur().loaded = false // always fresh
	}
	m.searching = false // the box stays behind; at Search, typing a letter goes into it (typesSearch)
	if v := m.cur(); !v.loaded {
		return m.load(v)
	}
	return nil
}

// push opens v over the current view. A view the stack already holds is
// gone back to instead, rows and selection kept, so going between an
// artist and an album never piles up. song, when given, is selected.
func (m *Model) push(v *view, song string) tea.Cmd {
	s := m.stack()
	for i, o := range s {
		if o.key == v.key {
			m.stacks[m.section], v = s[:i+1], o
			break
		}
	}
	if m.cur() != v {
		m.stacks[m.section] = append(s, v)
		v.selAt = m.frame
	}
	if v.loaded {
		m.selectSong(v, song)
		return nil
	}
	if song != "" {
		v.want = song
	}
	return m.load(v)
}

// selectSong selects the row playing id — or the item id — if the view
// lists it.
func (m *Model) selectSong(v *view, id string) {
	if id == "" {
		return
	}
	for i, r := range v.rows {
		if (r.track != nil && (r.track.ID == id || strings.HasSuffix(r.track.ID, id))) || (r.item != nil && r.item.ID == id) {
			v.sel, v.selAt = i, m.frame
			return
		}
	}
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
	if !r.selectable() {
		return nil
	}
	if r.item != nil {
		it := *r.item
		switch it.Kind {
		case apple.KindStation:
			return m.startStation(ipc.Request{Cmd: ipc.CmdStation, Item: &it})
		case apple.KindTerm:
			m.query = it.Name
			return m.runSearch(true)
		}
		return m.push(&view{title: it.Name, key: it.Key(), item: &it}, "")
	}
	if v.key == "queue:" {
		return tea.Batch(m.showPlayingAt(*r.track, m.state.Source, v.qpos+v.sel),
			m.send(ipc.Request{Cmd: ipc.CmdJump, Value: float64(v.qpos + v.sel)}))
	}
	ids, at := v.songs()
	i := at[v.sel]
	window := ids[i:min(len(ids), i+queueAfter)]
	if m.state.Shuffle && len(ids) > len(window) {
		// Shuffle draws from the whole list, not just the songs around
		// the chosen one: it first, then a random pick of the rest.
		window = append([]string{r.track.ID}, sample(ids, r.track.ID, queueAfter-1)...)
	}
	return tea.Batch(m.showPlaying(*r.track, v.key),
		m.send(ipc.Request{Cmd: ipc.CmdPlay, IDs: window, Start: r.track.ID, Source: v.key}))
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
	return m.lookupID(cmd, id)
}

// lookupID opens what the song id belongs to.
func (m *Model) lookupID(cmd, id string) tea.Cmd {
	client := m.client
	return func() tea.Msg {
		reply, err := client.Do(ipc.Request{Cmd: cmd, Start: id})
		if err != nil || len(reply.Items) == 0 {
			return errMsg{fmt.Errorf("nothing found for this song")}
		}
		return albumMsg{item: reply.Items[0], song: id}
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
		ids, err := songsOf(client, r)
		if err != nil {
			return errMsg{err}
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
		return m.toggle()
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
	return m.toggle()
}

// toggleFull switches the fullscreen visualizer, asking the daemon for
// waveform data only while it shows.
func (m *Model) toggleFull() tea.Cmd {
	m.full, m.vizList = !m.full, false
	if m.full {
		m.vizStyle, m.vizPrev, m.vizAt = vizOpening, -1, time.Now()
	}
	return m.subscribe()
}

// fullKey handles the keys that mean something else in fullscreen.
// In fullscreen, tab steps through the styles, v lists them, a switches
// the minutely change on and off; other keys (n, space…) work as ever.
func (m *Model) fullKey(k string) (tea.Cmd, bool) {
	n := len(vizNames)
	if m.vizList {
		switch k {
		case "up", "k", "shift+tab":
			m.vizSel = (m.vizSel + n - 1) % n
			m.previewViz(m.vizSel)
		case "down", "j", "tab":
			m.vizSel = (m.vizSel + 1) % n
			m.previewViz(m.vizSel)
		case "enter", "space", " ", "l", "v":
			m.vizList = false // keep what shows
		case "esc", "q", "h":
			m.previewViz(m.vizFrom) // back to the style before the list
			m.vizList = false
		case "a":
			m.toggleVizCycle()
		default:
			if i, ok := vizDigit(k); ok {
				m.pickViz(i)
			}
		}
		return nil, true
	}
	switch k {
	case "esc", "f", "q":
		return m.toggleFull(), true
	case "tab", "right", "down", "j", "l":
		m.showViz((m.vizStyle + 1) % n)
	case "shift+tab", "left", "up", "k", "h":
		m.showViz((m.vizStyle + n - 1) % n)
	case "v":
		m.vizList, m.vizSel, m.vizFrom = true, m.vizStyle, m.vizStyle
	case "a":
		m.toggleVizCycle()
	case "F":
		m.opts.VizFPS = step(drawRates, m.opts.VizFPS, 1)
		m.saveOptions()
		m.setFlash("visualizer: " + m.fpsLabel())
	default:
		i, ok := vizDigit(k)
		if !ok {
			return nil, false
		}
		if i != m.vizStyle {
			m.showViz(i)
		}
	}
	return nil, true
}

// vizDigit maps 1–9 and 0 to the first ten styles.
func vizDigit(k string) (int, bool) {
	if len(k) != 1 || k[0] < '0' || k[0] > '9' {
		return 0, false
	}
	i := int(k[0]-'0') - 1
	if k == "0" {
		i = 9
	}
	return i, i < len(vizNames)
}

// previewViz shows style i at once, without the dissolve: moving through
// the list shows each style as the cursor lands on it.
func (m *Model) previewViz(i int) {
	if i != m.vizStyle {
		m.vizPrev, m.vizStyle, m.vizAt = -1, i, time.Now()
	}
}

// pickViz shows style i and closes the list.
func (m *Model) pickViz(i int) {
	m.vizList = false
	if i != m.vizStyle {
		m.showViz(i)
	}
}

// toggleVizCycle switches the minutely change of style, and remembers it.
func (m *Model) toggleVizCycle() {
	m.opts.NoVizCycle = !m.opts.NoVizCycle
	m.saveOptions()
	m.vizAt = time.Now() // switched back on: the next change is a minute away
	m.setFlash(map[bool]string{false: "styles change every minute", true: "style stays"}[m.opts.NoVizCycle])
}

// Fullscreen opens on the showpiece and moves on every minute, dissolving
// from one style into the next.
const (
	vizOpening = 0 // milkdrop
	vizEvery   = time.Minute
	vizFade    = 1500 * time.Millisecond
)

func (m *Model) showViz(style int) {
	m.vizPrev, m.vizStyle, m.vizAt = m.vizStyle, style, time.Now()
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
	for s := secHome; s <= secSearch; s++ {
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
	if m.resumed || m.placing || m.state.Source == "" {
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

// typesSearch is whether k, pressed on the Search section's results with
// the box not focused, starts typing into it: any character but a digit
// (the sections) and the few keys that mean something everywhere.
func (m *Model) typesSearch(k string) bool {
	if m.section != secSearch || len(m.stack()) != 1 || m.full || utf8.RuneCountInString(k) != 1 {
		return false
	}
	r, _ := utf8.DecodeRuneInString(k)
	return unicode.IsPrint(r) && r != ' ' && !unicode.IsDigit(r) && !strings.ContainsRune("/?ofgqQ", r)
}

func (m *Model) key(k string) tea.Cmd {
	if m.typesSearch(k) {
		m.searching, m.help = true, false
		m.query += k
		return m.typed()
	}
	switch k {
	case "q":
		// Done listening: the music stops and brumm with it.
		return tea.Sequence(m.send(ipc.Request{Cmd: ipc.CmdQuit}), tea.Quit)
	case "Q", "ctrl+c", "ctrl+q":
		return tea.Quit // close the window; the music plays on
	case "L":
		return signIn()
	case "1", "2", "3", "4", "5", "6", "7", "8":
		return m.switchTo(section(k[0] - '1'))
	case "tab", "right":
		return m.switchTo((m.section + 1) % numSections)
	case "shift+tab", "left":
		return m.switchTo((m.section + numSections - 1) % numSections)
	case "/":
		if m.filterable() {
			m.startFilter() // here first; tab goes on to all of Apple Music
			return nil
		}
		m.section, m.searching, m.help = secSearch, true, false
	case "f":
		return m.toggleFull()
	case "n":
		return m.skip()
	case "p", "b":
		return m.goBack()
	case "shift+left", "shift+right":
		delta := 10.0
		if k == "shift+left" {
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
		m.setSplit(m.shownSplit() + delta)
		saveSplit(m.split)
	case "c":
		return m.jumpToPlaying()
	case "o":
		m.optOpen, m.help = true, false
	case "O":
		return m.togglePreview()
	case "i":
		return m.addToLibrary()
	case "a":
		return m.lookup(ipc.CmdAlbum) // the selected song's, else the playing one's
	case "A":
		return m.lookup(ipc.CmdArtist)
	case "z", "Z":
		return m.enqueue(k == "Z")
	case "*":
		return m.rate(apple.Love)
	case "d":
		return m.rate(apple.Dislike)
	case "R":
		return m.playStation()
	case "P":
		return m.openPicker()
	case "y":
		return m.copyLink()
	case "!":
		m.setFlash("opening a new issue on GitHub in your browser…")
		return m.send(ipc.Request{Cmd: ipc.CmdFeedback, Query: "tui"})
	case "U":
		return m.checkUpdate()
	case "g":
		return m.toWindow()
	case "esc", "h", "backspace":
		if m.help {
			m.help = false
			return nil
		}
		if v := m.cur(); v.all != nil && k == "esc" {
			m.clearFilter(v) // the filter goes before the view does
			return nil
		}
		if m.previewing {
			return m.togglePreview()
		}
		m.back()
	case "enter", "l":
		if v := m.cur(); m.section == secSearch && len(m.stack()) == 1 && (v.sel >= len(v.rows) || !v.rows[v.sel].selectable()) {
			m.searching = true // no results to open: into the box
			return nil
		}
		return m.activate()
	default:
		return m.move(k)
	}
	return nil
}

// setSplit asks for the list's share; render keeps both sides their
// minimum, and a cover held back by the height gives the list the rest.
func (m *Model) setSplit(f float64) { m.split = min(0.95, max(0.1, f)) }

// shownSplit is the list's share as drawn.
func (m *Model) shownSplit() float64 {
	inner := m.width - 2*margin
	if navW, colW, _ := m.layout(inner, m.height-bodyTop-2); colW > 0 {
		return float64(navW) / float64(inner)
	}
	return m.split
}

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
	case "ctrl+c", "ctrl+q":
		return tea.Quit
	case "down", "up":
		m.searching = false // move into the results
		return m.move(msg.String())
	case "left", "right", "tab", "shift+tab":
		m.searching = false // on to the next section: the box has no cursor to move
		return m.key(msg.String())
	case "1", "2", "3", "4", "5", "6", "7", "8":
		if m.query == "" { // nothing typed yet: a digit is a section, as outside the box
			m.searching = false
			return m.key(msg.String())
		}
		m.query += msg.Text
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
	if root := m.stacks[secSearch][0]; root.key == "search:"+q {
		m.stacks[secSearch] = m.stacks[secSearch][:1] // already found: back to the results
		return nil
	}
	m.stacks[secSearch] = []*view{{title: "Search", key: "search:" + q}}
	return m.load(m.cur())
}

func (m *Model) move(k string) tea.Cmd {
	v := m.cur()
	prev := v.sel
	ok := m.step(k)
	if v.sel == prev && (k == "up" || k == "k") && m.section == secSearch && len(m.stack()) == 1 {
		m.searching = true // up from the first result: back into the box
		return nil
	}
	if !ok {
		return nil
	}
	return m.prefetchCovers(v)
}

// step moves the selection by a key, false if k is no move.
func (m *Model) step(k string) bool {
	v := m.cur()
	n := len(v.rows)
	if n == 0 {
		return false
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
	case "home":
		v.sel = 0
	case "G", "end":
		v.sel = n - 1
	default:
		return false
	}
	v.sel = max(0, min(v.sel, n-1))
	dir := 1
	if v.sel < prev || k == "G" || k == "end" {
		dir = -1
	}
	if k == "home" {
		dir = 1
	}
	v.sel = nearest(v.rows, v.sel, dir)
	if v.sel != prev {
		v.selAt = m.frame
	}
	return true
}

// A free-spinning wheel sends hundreds of notches a second. Drawn one by
// one — bubbletea draws after every message — they queued up and the list
// scrolled on long after the wheel stopped. So a notch only moves the
// selection; the screen catches up at most once per wheelEvery, and covers
// load once the wheel rests for wheelRest.
const (
	wheelEvery = 25 * time.Millisecond
	wheelRest  = 150 * time.Millisecond
)

// wheel moves the selection a row per notch. It stays in the list: up
// from the first search result does not fall into the search box.
func (m *Model) wheel(b tea.MouseButton) tea.Cmd {
	k := "down"
	switch b {
	case tea.MouseWheelUp:
		k = "up"
	case tea.MouseWheelDown:
	default:
		return nil
	}
	if !m.step(k) {
		return nil
	}
	now := time.Now()
	m.wheelAt = now
	if now.Sub(m.wheelDrawn) >= wheelEvery {
		m.wheelDrawn = now // a lone notch shows at once
	} else {
		m.still, m.wheelMoved = true, true
	}
	if m.wheelTick {
		return nil
	}
	m.wheelTick = true
	return tea.Tick(wheelEvery, func(time.Time) tea.Msg { return wheelMsg{} })
}

// wheelFrame draws what the notches since the last frame moved, and once
// the wheel rests, loads the covers around where it stopped.
func (m *Model) wheelFrame() tea.Cmd {
	m.wheelTick = false
	if m.wheelMoved {
		m.wheelMoved, m.wheelDrawn = false, time.Now()
	} else {
		m.still = true
	}
	if time.Since(m.wheelAt) >= wheelRest {
		return m.prefetchCovers(m.cur())
	}
	m.wheelTick = true
	return tea.Tick(wheelEvery, func(time.Time) tea.Msg { return wheelMsg{} })
}

// click maps a mouse click onto the layout recorded by the last render.
// A click plays a song or opens an item; right click goes back.
func (m *Model) click(ms tea.Mouse) tea.Cmd {
	if m.full {
		return m.fullClick(ms)
	}
	g := m.geo
	if ms.Button == tea.MouseRight {
		m.back()
		return nil
	}
	if ms.Button != tea.MouseLeft {
		return nil
	}
	searching := m.searching
	m.searching = false // a click anywhere but the box leaves it
	for i, r := range g.tabs {
		if r.has(ms.X, ms.Y) {
			return m.switchTo(section(i))
		}
	}
	for _, f := range g.foot {
		if f.r.has(ms.X, ms.Y) {
			switch {
			case searching && f.action == "esc": // done: the click left the box
				return nil
			case searching && f.action == "enter":
				return m.runSearch(true)
			}
			if f.action == "?" {
				m.help = !m.help
				return nil
			}
			return m.key(f.action)
		}
	}
	if cmd, ok := m.nextClick(ms.X, ms.Y); ok {
		return cmd
	}
	switch {
	case g.search.has(ms.X, ms.Y):
		m.searching = true
	case g.crumb.has(ms.X, ms.Y):
		m.back()
	case g.play.has(ms.X, ms.Y):
		return m.toggle()
	case g.prev.has(ms.X, ms.Y):
		return m.goBack()
	case g.next.has(ms.X, ms.Y):
		return m.skip()
	case g.shuffle.has(ms.X, ms.Y):
		return m.key("s")
	case g.repeat.has(ms.X, ms.Y):
		return m.key("r")
	case g.volume.has(ms.X, ms.Y):
		return m.key("m")
	case g.artist.has(ms.X, ms.Y) && m.state.ID != "":
		return m.lookupID(ipc.CmdArtist, m.state.ID)
	case g.album.has(ms.X, ms.Y) && m.state.ID != "":
		return m.lookupID(ipc.CmdAlbum, m.state.ID)
	case g.bar.has(ms.X, ms.Y) && m.state.Dur > 0:
		f := float64(ms.X-g.bar.x0) / float64(max(1, g.bar.x1-g.bar.x0-1))
		return m.seek(f * m.state.Dur)
	case g.list.has(ms.X, ms.Y):
		v := m.cur()
		idx := v.off + ms.Y - g.list.y0
		if idx >= len(v.rows) {
			return nil
		}
		if !v.rows[idx].selectable() {
			return nil
		}
		v.sel, v.selAt = idx, m.frame
		return m.activate()
	}
	return nil
}

// signIn opens the Apple Music sign-in page in the browser.
func signIn() tea.Cmd {
	return func() tea.Msg { return loginMsg{login.Run(context.Background(), func(string) {})} }
}

// autoLogin opens the sign-in page by itself the first time brumm finds
// it is signed out, so nobody has to hunt for the key.
func (m *Model) autoLogin() tea.Cmd {
	if m.loginTried || m.state.Status != ipc.StatusLoggedOut {
		return nil
	}
	m.loginTried = true
	m.setFlash("opening the Apple Music sign-in in your browser…")
	return signIn()
}

// addToLibrary adds the selected catalog song, album or playlist — or the
// song playing — to the library.
func (m *Model) addToLibrary() tea.Cmd {
	kind, id, name := "", "", ""
	v := m.cur()
	switch {
	case v.sel < len(v.rows) && v.rows[v.sel].track != nil:
		t := v.rows[v.sel].track
		kind, id, name = "songs", t.ID, t.Title
	case v.sel < len(v.rows) && v.rows[v.sel].item != nil:
		it := v.rows[v.sel].item
		switch {
		case it.Kind == apple.KindArtist:
			m.setFlash("artists cannot be added; add one of their albums")
			return nil
		case !it.Catalog:
			m.setFlash(it.Name + " is already in your library")
			return nil
		}
		kind, id, name = it.Kind+"s", it.ID, it.Name
	case m.state.ID != "":
		kind, id, name = "songs", m.state.ID, m.state.Title
	default:
		return nil
	}
	if strings.HasPrefix(id, "i.") {
		m.setFlash(name + " is already in your library")
		return nil
	}
	client := m.client
	return func() tea.Msg {
		if _, err := client.Do(ipc.Request{Cmd: ipc.CmdAdd, List: kind, IDs: []string{id}}); err != nil {
			return errMsg{err}
		}
		return flashMsg("added " + name + " to your library")
	}
}

// ── playing at once ─────────────────────────────────────────────────────

// A play takes the player a moment — a license, the first bytes — so the
// screen does not wait for it: the song shows as playing at once, cover and
// all, and the player's reports keep showing it until they catch up.
type pendingPlay struct {
	track  apple.Track
	source string
	index  int // its place in the queue; -1 when not known
	at     time.Time
}

// showPlaying puts track on the stage now, as if it already played.
func (m *Model) showPlaying(t apple.Track, source string) tea.Cmd {
	return m.showPlayingAt(t, source, -1)
}

// showPlayingAt is showPlaying for a song at a known place in the queue.
func (m *Model) showPlayingAt(t apple.Track, source string, index int) tea.Cmd {
	m.pending = &pendingPlay{t, source, index, time.Now()}
	m.wantPlay = &wantPlaying{true, time.Now()} // no flash of "paused" while it loads
	st := m.state
	m.keepShowing(&st)
	m.state, m.stateAt = st, time.Now()
	return m.maybeFetchCover()
}

// wantPlaying is a pause or play asked for: the button shows it at once,
// and the player's reports keep showing it until they catch up.
type wantPlaying struct {
	playing bool
	at      time.Time
}

// keepPlaying lays the pause or play asked for over a report that does
// not have it yet, for a few seconds at most.
func (m *Model) keepPlaying(st *ipc.State) {
	w := m.wantPlay
	if w == nil {
		return
	}
	if st.Playing == w.playing || time.Since(w.at) > 6*time.Second {
		m.wantPlay = nil
		return
	}
	st.Playing = w.playing
}

// toggle pauses or plays, shown at once.
func (m *Model) toggle() tea.Cmd {
	if m.state.ID != "" {
		playing := !m.state.Playing
		m.state.Pos, m.stateAt = m.position(), time.Now()
		m.state.Playing = playing
		m.wantPlay = &wantPlaying{playing, time.Now()}
	}
	return m.send(ipc.Request{Cmd: ipc.CmdToggle})
}

// stayPlaying keeps the button on pause through a change of song: the
// player reports "not playing" while it loads the next one.
func (m *Model) stayPlaying() {
	if m.state.Playing {
		m.wantPlay = &wantPlaying{true, time.Now()}
	}
}

// skip goes to the next song, which shows at once when the queue ahead
// is known.
func (m *Model) skip() tea.Cmd {
	m.stayPlaying()
	var cmd tea.Cmd
	if pos, ok := m.afterPlaying(); ok && m.state.Preview == nil {
		if t, ok := m.upTrack(pos); ok {
			cmd = m.showPlayingAt(t, m.state.Source, pos)
		}
	}
	return tea.Batch(cmd, m.send(ipc.Request{Cmd: ipc.CmdNext}))
}

// goBack goes to the previous song, or after 3 s back to the start of
// this one, as the player does; the playhead shows that at once.
func (m *Model) goBack() tea.Cmd {
	m.stayPlaying()
	if m.state.Dur > 0 && m.position() > 3 {
		m.seekTo, m.seekAt = 0, time.Now()
	}
	return m.send(ipc.Request{Cmd: ipc.CmdPrev})
}

// jump plays the song at queue position pos, shown at once when known.
func (m *Model) jump(pos int) tea.Cmd {
	var cmd tea.Cmd
	if t, ok := m.upTrack(pos); ok {
		cmd = m.showPlayingAt(t, m.state.Source, pos)
	}
	return tea.Batch(cmd, m.send(ipc.Request{Cmd: ipc.CmdJump, Value: float64(pos)}))
}

// keepShowing lays the song being started over a report from the player
// that does not have it yet; once the player plays it, fails, or takes
// longer than a few seconds, its reports are the truth again.
func (m *Model) keepShowing(st *ipc.State) {
	p := m.pending
	if p == nil {
		return
	}
	if st.ID == p.track.ID || (st.Err != "" && st.Err != m.state.Err) || time.Since(p.at) > 6*time.Second {
		m.pending = nil
		return
	}
	t := p.track
	st.ID, st.Title, st.Artist, st.Album, st.Dur, st.Pos = t.ID, t.Title, t.Artist, t.Album, t.Duration, 0
	st.Source, st.Playing, st.Preview = p.source, true, nil
	if p.index >= 0 {
		st.Index = p.index
	}
	if t.Artwork != "" {
		st.Artwork = t.Artwork
	}
}

// warm tells the player which songs a list holds, so starting one skips
// the lookup at Apple; the first few hundred, which is what gets played.
func (m *Model) warm(v *view) tea.Cmd {
	ids, _ := v.songs()
	if len(ids) == 0 || v.key == "queue:" {
		return nil
	}
	ids = ids[:min(len(ids), 300)]
	client := m.client
	return func() tea.Msg {
		_, _ = client.Do(ipc.Request{Cmd: ipc.CmdWarm, IDs: ids})
		return nil
	}
}

// fullClick handles the mouse over the fullscreen visualizer: its buttons,
// and the list of styles while it shows.
func (m *Model) fullClick(ms tea.Mouse) tea.Cmd {
	if ms.Button != tea.MouseLeft {
		return nil
	}
	if m.vizList {
		if !m.geo.options.has(ms.X, ms.Y) {
			m.vizList = false
			return nil
		}
		if i := ms.Y - m.geo.optRow0; i >= 0 && i < len(vizNames) {
			m.pickViz(i)
		}
		return nil
	}
	for _, f := range m.geo.foot {
		if f.r.has(ms.X, ms.Y) {
			cmd, _ := m.fullKey(f.action)
			return cmd
		}
	}
	return nil
}
