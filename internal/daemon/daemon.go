// Package daemon runs brumm's background player: one headless Chrome doing
// the playback, an MPRIS server for media keys and the bar, and a unix
// socket the TUI talks to. Music keeps playing when the TUI is closed.
package daemon

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"reflect"
	"sync"
	"syscall"
	"time"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/chrome"
	"github.com/chriopter/brumm/internal/config"
	"github.com/chriopter/brumm/internal/engine"
	"github.com/chriopter/brumm/internal/ipc"
	"github.com/chriopter/brumm/internal/mpris"
)

const (
	frame      = 40 * time.Millisecond // spectrum cadence (25 fps)
	statePlay  = 200 * time.Millisecond
	stateIdle  = time.Second      // paused: the page changes only when told to
	staleAfter = 10 * time.Second // no answer from the page this long: restart Chrome
	retryFirst = 2 * time.Second  // after a failed player start, doubling
	retryMax   = time.Minute
	sendWithin = 200 * time.Millisecond
)

// conn is one client. Writes carry a deadline so a stalled client (say, a
// suspended TUI) is dropped instead of blocking every other one.
type conn struct {
	mu    sync.Mutex
	nc    net.Conn
	enc   *json.Encoder
	bands int // spectrum bands wanted; 0 = none
	wave  int // waveform samples wanted; 0 = none
	sub   bool
}

func (c *conn) send(m ipc.Message) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	_ = c.nc.SetWriteDeadline(time.Now().Add(sendWithin))
	return c.enc.Encode(m)
}

type Daemon struct {
	version string
	mpris   *mpris.Server
	lib     *library
	resume  *resume // last queue and song; guarded by mu

	mu          sync.Mutex
	eng         *engine.Engine
	api         *apple.Client
	state       ipc.State
	conns       map[*conn]bool
	booting     bool
	refreshing  bool
	refreshNext bool      // another refresh was asked for during one
	expiresIn   int       // days until the developer token expires, when under 45
	expires     time.Time // when the developer token expires
	update      string    // a newer release, when one is available
	installed   string    // a release installed but not yet restarted into

	quit     chan struct{}
	quitOnce sync.Once
	checkNow chan struct{} // ask checkUpdates to look now
	nudge    chan struct{} // a command ran: poll the page now

	retryAt time.Time     // when to start the player again after a failure
	retry   time.Duration // the wait after the next failure
	booted  bool          // the library was loaded once; restarts skip it
}

// Run serves until SIGTERM, SIGINT or a quit request.
func Run(version string) error {
	ln, err := listen()
	if err != nil {
		return err
	}
	defer os.Remove(config.Socket())

	engine.SweepProfiles() // left behind by crashes
	d := &Daemon{
		version:  version,
		state:    ipc.State{Status: ipc.StatusStarting, Message: "starting"},
		lib:      loadLibrary(),
		resume:   loadResume(),
		conns:    map[*conn]bool{},
		quit:     make(chan struct{}),
		checkNow: make(chan struct{}, 1),
		nudge:    make(chan struct{}, 1),
	}
	if d.mpris, err = mpris.Start(d); err != nil {
		log.Printf("mpris disabled: %v", err)
	}

	go d.accept(ln)
	go d.boot()
	go d.loop()
	go d.checkUpdates()
	go d.watchSelf()

	sig := make(chan os.Signal, 1)
	signal.Notify(sig, syscall.SIGINT, syscall.SIGTERM)
	select {
	case <-sig:
	case <-d.quit:
	}
	_ = ln.Close()
	d.mu.Lock()
	if d.resume != nil {
		d.resume.save()
	}
	if d.eng != nil {
		d.eng.Close()
	}
	d.mu.Unlock()
	if d.mpris != nil {
		d.mpris.Close()
	}
	return nil
}

// listen claims the socket, clearing a stale one left by a crash but
// refusing to start next to a live daemon.
func listen() (net.Listener, error) {
	// Older versions made the cache directory world-readable.
	_ = os.Chmod(config.CacheDir(), 0o700)
	path := config.Socket()
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, err
	}
	if c, err := net.DialTimeout("unix", path, 300*time.Millisecond); err == nil {
		c.Close()
		return nil, errors.New("brumm daemon is already running")
	}
	_ = os.Remove(path)
	ln, err := net.Listen("unix", path)
	if err != nil {
		return nil, err
	}
	_ = os.Chmod(path, 0o600)
	return ln, nil
}

func (d *Daemon) setStatus(s ipc.Status, msg string) {
	d.mu.Lock()
	d.state.Status, d.state.Message = s, msg
	st := d.state
	d.mu.Unlock()
	d.broadcast(ipc.Message{State: &st})
}

// boot connects to Apple Music first — so the library refreshes while
// Chrome is still being installed — then starts playback. Only one boot
// runs at a time.
func (d *Daemon) boot() {
	d.mu.Lock()
	if d.booting {
		d.mu.Unlock()
		return
	}
	d.booting = true
	d.mu.Unlock()
	defer func() {
		d.mu.Lock()
		d.booting = false
		d.mu.Unlock()
	}()

	cfg, err := config.Load()
	if err != nil {
		d.setStatus(ipc.StatusError, err.Error())
		return
	}
	if cfg.Developer() == "" {
		d.setStatus(ipc.StatusError, "no Apple Music developer token in this build")
		return
	}
	if cfg.UserToken == "" {
		d.setStatus(ipc.StatusLoggedOut, "not signed in")
		return
	}
	d.mu.Lock()
	d.api = apple.New(cfg.Developer(), cfg.UserToken)
	if exp := cfg.Expires(); !exp.IsZero() {
		d.expires = exp
		if days := int(time.Until(exp).Hours() / 24); days < 45 {
			d.expiresIn = max(days, 0)
		}
	}
	first := !d.booted
	d.booted = true
	d.mu.Unlock()
	if first {
		go d.refresh()
	}

	if err := chrome.Ensure(func(msg string) { d.setStatus(ipc.StatusStarting, msg) }); err != nil {
		d.failed(err)
		return
	}
	d.setStatus(ipc.StatusStarting, "starting player")
	eng, err := engine.Start(cfg.Developer(), cfg.UserToken, d.version)
	if err != nil {
		d.failed(err)
		return
	}
	d.mu.Lock()
	if d.eng != nil {
		d.eng.Close()
	}
	d.eng = eng
	d.mu.Unlock()
}

// failed shows why the player did not start and schedules another try,
// waiting longer each time so a broken Chrome is not restarted in a loop.
func (d *Daemon) failed(err error) {
	d.mu.Lock()
	d.retry = min(max(d.retry*2, retryFirst), retryMax)
	d.retryAt = time.Now().Add(d.retry)
	wait := d.retry
	d.mu.Unlock()
	d.setStatus(ipc.StatusError, fmt.Sprintf("%v (trying again in %s)", err, wait.Round(time.Second)))
}

// poke wakes the loop to read the page now, so a pause shows at once
// even while the loop polls slowly.
func (d *Daemon) poke() {
	select {
	case d.nudge <- struct{}{}:
	default:
	}
}

func (d *Daemon) engine() *engine.Engine {
	d.mu.Lock()
	defer d.mu.Unlock()
	return d.eng
}

// loop polls the page: the spectrum every frame while music plays and
// someone watches it, the player state every 200 ms while playing and
// every second while paused, pushing state only when it changed. A dead or
// unresponsive page is restarted, with growing waits if it keeps failing.
func (d *Daemon) loop() {
	t := time.NewTimer(frame)
	defer t.Stop()
	lastOK, lastState := time.Now(), time.Time{}
	for {
		select {
		case <-d.quit:
			return
		case <-d.nudge:
			lastState = time.Time{}
			if !t.Stop() {
				select {
				case <-t.C:
				default:
				}
			}
		case <-t.C:
		}
		next := d.step(&lastOK, &lastState)
		t.Reset(next)
	}
}

// step does one round of the loop and says when to run the next.
func (d *Daemon) step(lastOK, lastState *time.Time) time.Duration {
	eng := d.engine()
	if eng == nil {
		*lastOK = time.Now()
		d.mu.Lock()
		due := !d.retryAt.IsZero() && time.Now().After(d.retryAt)
		if due {
			d.retryAt = time.Time{}
		}
		d.mu.Unlock()
		if due {
			go d.boot()
		}
		return stateIdle
	}
	if !eng.Alive() || time.Since(*lastOK) > staleAfter {
		// Drop the dead player so this runs once; boot starts a new one.
		d.mu.Lock()
		if d.eng == eng {
			d.eng = nil
		}
		d.mu.Unlock()
		go eng.Close() // a hung page may take a while to close
		d.setStatus(ipc.StatusStarting, "restarting player")
		go d.boot()
		return stateIdle
	}
	playing := d.playing()
	bands, wave := d.wants()
	if playing && bands > 0 {
		if spec, err := eng.Spectrum(bands); err == nil {
			d.broadcast(ipc.Message{Spectrum: spec})
		}
	}
	if playing && wave > 0 {
		if w, err := eng.Wave(wave); err == nil {
			d.broadcast(ipc.Message{Wave: w})
		}
	}
	every := stateIdle
	if playing {
		every = statePlay
	}
	if time.Since(*lastState) >= every {
		if es, err := eng.State(); err == nil {
			*lastOK, *lastState = time.Now(), time.Now()
			d.mu.Lock()
			d.retry = 0 // the player works: the next failure waits briefly again
			d.mu.Unlock()
			d.apply(es)
			playing = es.Playing
		}
	}
	switch {
	case playing && (bands > 0 || wave > 0):
		return frame
	case playing:
		return statePlay
	}
	return stateIdle
}

func (d *Daemon) playing() bool {
	d.mu.Lock()
	defer d.mu.Unlock()
	return d.state.Playing
}

func (d *Daemon) apply(es engine.State) {
	status, msg := ipc.StatusStarting, "loading Apple Music"
	switch {
	case es.NeedsAuth:
		status, msg = ipc.StatusLoggedOut, "Apple Music session expired"
	case es.Ready:
		status, msg = ipc.StatusReady, ""
	case es.Err != "":
		status, msg = ipc.StatusError, es.Err
	}
	d.mu.Lock()
	if r := d.resume; r != nil && r.Autoplay && es.Ready {
		r.Autoplay = false
		go func() {
			if eng := d.engine(); eng != nil {
				_ = eng.PlayIDs(r.IDs, r.ID, r.Source, r.Pos)
			}
		}()
	}
	if r := d.resume; r != nil {
		if es.Title == "" && es.Preview == nil {
			es = r.overlay(es) // idle: show the saved song, paused
		} else {
			r.track(es)
		}
	}
	next := ipc.State{Status: status, Message: msg, State: es, ExpiresIn: d.expiresIn, Update: d.update}
	changed := !reflect.DeepEqual(next, d.state)
	d.state = next
	d.mu.Unlock()
	if d.mpris != nil {
		d.mpris.Update(es)
	}
	if changed {
		d.broadcast(ipc.Message{State: &next})
	}
}

// wants is the most spectrum bands and waveform samples any client asks for.
func (d *Daemon) wants() (bands, wave int) {
	d.mu.Lock()
	defer d.mu.Unlock()
	for c := range d.conns {
		bands, wave = max(bands, c.bands), max(wave, c.wave)
	}
	return bands, wave
}

// broadcast pushes to subscribers; spectrum frames only to those that want
// them. A client that cannot keep up is disconnected.
func (d *Daemon) broadcast(m ipc.Message) {
	d.mu.Lock()
	var targets []*conn
	for c := range d.conns {
		if c.sub && (m.Spectrum == nil || c.bands > 0) && (m.Wave == nil || c.wave > 0) {
			targets = append(targets, c)
		}
	}
	d.mu.Unlock()
	for _, c := range targets {
		if err := c.send(m); err != nil {
			_ = c.nc.Close()
		}
	}
}

func (d *Daemon) accept(ln net.Listener) {
	for {
		nc, err := ln.Accept()
		if err != nil {
			return
		}
		go d.serve(nc)
	}
}

func (d *Daemon) serve(nc net.Conn) {
	c := &conn{nc: nc, enc: json.NewEncoder(nc)}
	d.mu.Lock()
	d.conns[c] = true
	d.mu.Unlock()
	defer func() {
		d.mu.Lock()
		delete(d.conns, c)
		d.mu.Unlock()
		nc.Close()
	}()
	sc := bufio.NewScanner(nc)
	sc.Buffer(make([]byte, 64<<10), 4<<20)
	busy := make(chan struct{}, 8) // at most 8 requests in flight per client
	for sc.Scan() {
		var r ipc.Request
		if json.Unmarshal(sc.Bytes(), &r) != nil {
			continue
		}
		// Requests may be slow (network); answer them concurrently so a
		// search does not hold up a pause.
		busy <- struct{}{}
		go func() {
			defer func() { <-busy }()
			reply := ipc.Message{ID: r.ID}
			if err := d.handle(c, r, &reply); err != nil {
				reply.Error = err.Error()
			}
			d.poke()
			if c.send(reply) != nil {
				_ = nc.Close()
			}
		}()
	}
}

func (d *Daemon) handle(c *conn, r ipc.Request, reply *ipc.Message) error {
	switch r.Cmd {
	case ipc.CmdSubscribe:
		d.mu.Lock()
		c.sub, c.bands, c.wave = true, min(r.Bands, 256), min(r.Wave, 1024)
		st := d.state
		d.mu.Unlock()
		reply.State = &st
		return nil
	case ipc.CmdList:
		return d.list(r.List, reply)
	case ipc.CmdOpen:
		if r.Item == nil {
			return errors.New("open: no item")
		}
		return d.open(*r.Item, reply)
	case ipc.CmdSearch:
		api, err := d.client()
		if err != nil {
			return err
		}
		res, err := api.Search(r.Query)
		if err != nil {
			return d.authCheck(err)
		}
		reply.Results = &res
		return nil
	case ipc.CmdAlbum:
		api, err := d.client()
		if err != nil {
			return err
		}
		album, err := api.AlbumOf(r.Start)
		if err != nil {
			return d.authCheck(err)
		}
		reply.Items = []apple.Item{album}
		return nil
	case ipc.CmdLoved:
		api, err := d.client()
		if err != nil {
			return err
		}
		loved, err := api.Loved(r.IDs)
		for id := range loved {
			reply.IDs = append(reply.IDs, id)
		}
		return d.authCheck(err)
	case ipc.CmdLove:
		api, err := d.client()
		if err != nil {
			return err
		}
		return d.authCheck(api.SetLoved(r.Start, r.Value != 0))
	case ipc.CmdArtist:
		api, err := d.client()
		if err != nil {
			return err
		}
		artist, err := api.ArtistOf(r.Start)
		if err != nil {
			return d.authCheck(err)
		}
		reply.Items = []apple.Item{artist}
		return nil
	case ipc.CmdLink:
		api, err := d.client()
		if err != nil {
			return err
		}
		reply.Link, err = api.Link(r.Start)
		return d.authCheck(err)
	case ipc.CmdUpdate:
		if r.Value == 0 { // just look
			select {
			case d.checkNow <- struct{}{}:
			default:
			}
			return nil
		}
		return d.installUpdate()
	case ipc.CmdReload:
		go d.reload()
		return nil
	case ipc.CmdQuit:
		d.quitOnce.Do(func() { close(d.quit) })
		return nil
	}

	eng := d.engine()
	if eng == nil {
		return errors.New("the player is still starting")
	}
	switch r.Cmd {
	case ipc.CmdPlay:
		d.mu.Lock()
		if d.resume == nil {
			d.resume = &resume{}
		}
		d.resume.IDs, d.resume.Source = r.IDs, r.Source
		d.mu.Unlock()
		return eng.PlayIDs(r.IDs, r.Start, r.Source, 0)
	case ipc.CmdToggle:
		if d.resumeIfIdle(eng) {
			return nil
		}
		return eng.Toggle()
	case ipc.CmdNext:
		return eng.Next()
	case ipc.CmdPrev:
		return eng.Prev()
	case ipc.CmdSeek:
		return eng.Seek(r.Value)
	case ipc.CmdVolume:
		return eng.SetVolume(r.Value)
	case ipc.CmdPreview:
		if r.Value == 0 {
			return eng.StopPreview()
		}
		return eng.Preview(r.Start)
	case ipc.CmdQueue:
		q, err := eng.Queue(max(1, int(r.Value)))
		if err != nil {
			return err
		}
		reply.Tracks, reply.Pos = q.Items, q.Pos
		return nil
	case ipc.CmdJump:
		return eng.Jump(int(r.Value))
	case ipc.CmdEnqueue:
		return eng.Enqueue(r.IDs, r.Value != 0)
	case ipc.CmdShuffle:
		return eng.SetShuffle(r.Value != 0)
	case ipc.CmdRepeat:
		return eng.SetRepeat(int(r.Value))
	}
	return fmt.Errorf("unknown command %q", r.Cmd)
}

// list answers a library list from the cache, fetching it on a miss.
func (d *Daemon) list(name string, reply *ipc.Message) error {
	if name == ipc.ListSongs {
		if v, ok := d.lib.tracks(name); ok {
			reply.Tracks = v
			return nil
		}
	} else if v, ok := d.lib.items(name); ok {
		reply.Items = v
		return nil
	}
	api, err := d.client()
	if err != nil {
		return err
	}
	return d.authCheck(d.fetchList(api, name, reply))
}

func (d *Daemon) fetchList(api *apple.Client, name string, reply *ipc.Message) error {
	var err error
	switch name {
	case ipc.ListPlaylists:
		reply.Items, err = api.Playlists()
	case ipc.ListAlbums:
		reply.Items, err = api.Albums()
	case ipc.ListArtists:
		reply.Items, err = api.Artists()
	case ipc.ListSongs:
		reply.Tracks, err = api.Songs()
	default:
		return fmt.Errorf("unknown list %q", name)
	}
	if errors.Is(err, apple.ErrPartial) && len(reply.Items)+len(reply.Tracks) > 0 {
		return nil // show what arrived, but do not cache an incomplete list
	}
	if err != nil {
		return err
	}
	if name == ipc.ListSongs {
		d.lib.setTracks(name, reply.Tracks)
	} else {
		d.lib.setItems(name, reply.Items)
	}
	return nil
}

// open lists what is inside an item: tracks, or an artist's albums.
func (d *Daemon) open(it apple.Item, reply *ipc.Message) error {
	key := it.Key()
	if it.Kind == apple.KindArtist {
		if v, ok := d.lib.items(key); ok {
			reply.Items = v
			return nil
		}
	} else if v, ok := d.lib.tracks(key); ok {
		reply.Tracks = v
		return nil
	}
	api, err := d.client()
	if err != nil {
		return err
	}
	if it.Kind == apple.KindArtist {
		if reply.Items, err = api.ArtistAlbums(it); err != nil {
			return d.authCheck(err)
		}
		d.lib.setItems(key, reply.Items)
		return nil
	}
	reply.Tracks, err = api.Tracks(it)
	if errors.Is(err, apple.ErrPartial) {
		return nil // show, don't cache
	}
	if err != nil {
		return d.authCheck(err)
	}
	d.lib.setTracks(key, reply.Tracks)
	return nil
}

func (d *Daemon) client() (*apple.Client, error) {
	d.mu.Lock()
	defer d.mu.Unlock()
	if d.api == nil {
		return nil, errors.New("still starting")
	}
	return d.api, nil
}

func (d *Daemon) authCheck(err error) error {
	if errors.Is(err, apple.ErrUnauthorized) {
		d.setStatus(ipc.StatusLoggedOut, "Apple Music session expired")
	}
	return err
}

// reload picks up a fresh login: hot-swaps the token into a running page,
// or starts the player if it never ran.
func (d *Daemon) reload() {
	cfg, err := config.Load()
	if err != nil || cfg.UserToken == "" {
		return
	}
	d.mu.Lock()
	d.api = apple.New(cfg.Developer(), cfg.UserToken)
	eng := d.eng
	d.mu.Unlock()
	if eng != nil && eng.SetUserToken(cfg.UserToken) == nil {
		go d.refresh()
		return
	}
	d.mu.Lock()
	d.booted = false // a new sign-in: load the library again
	d.retryAt = time.Time{}
	d.mu.Unlock()
	d.boot()
}

// refresh re-reads the library from Apple Music — the lists first, then
// every playlist's tracks — saving and telling clients after each step.
func (d *Daemon) refresh() {
	d.mu.Lock()
	api, busy := d.api, d.refreshing
	if busy {
		d.refreshNext = true // e.g. a new login: refresh again with its client
		d.mu.Unlock()
		return
	}
	d.refreshing = true
	d.mu.Unlock()
	if api == nil {
		d.mu.Lock()
		d.refreshing = false
		d.mu.Unlock()
		return
	}
	defer func() {
		d.mu.Lock()
		again := d.refreshNext
		d.refreshing, d.refreshNext = false, false
		d.mu.Unlock()
		if again {
			go d.refresh()
		}
	}()

	changed := func() {
		if err := d.lib.save(); err != nil {
			log.Printf("library cache: %v", err)
		}
		d.broadcast(ipc.Message{Library: true})
	}
	for _, name := range []string{ipc.ListPlaylists, ipc.ListAlbums, ipc.ListArtists, ipc.ListSongs} {
		var reply ipc.Message
		if err := d.fetchList(api, name, &reply); errors.Is(err, apple.ErrUnauthorized) {
			_ = d.authCheck(err)
			return
		}
	}
	changed()
	playlists, _ := d.lib.items(ipc.ListPlaylists)
	for _, p := range playlists {
		tracks, err := api.Tracks(p)
		if errors.Is(err, apple.ErrPartial) {
			continue // keep the cached full list rather than a cut one
		}
		if errors.Is(err, apple.ErrUnauthorized) {
			_ = d.authCheck(err)
			return
		}
		if err == nil {
			d.lib.setTracks(p.Key(), tracks)
		}
	}
	changed()
}

// resumeIfIdle starts the saved queue where it stopped when nothing is
// loaded yet, as after a restart. It reports whether it did.
func (d *Daemon) resumeIfIdle(eng *engine.Engine) bool {
	d.mu.Lock()
	r, idle := d.resume, d.state.Status == ipc.StatusReady && !d.state.Playing
	d.mu.Unlock()
	if r == nil || !idle || len(r.IDs) == 0 {
		return false
	}
	es, err := eng.State()
	if err != nil || es.Title != "" {
		return false // something is loaded: a plain toggle resumes it
	}
	return eng.PlayIDs(r.IDs, r.ID, r.Source, r.Pos) == nil
}

// mpris.Controller

func (d *Daemon) Play() {
	if eng := d.engine(); eng != nil && !d.resumeIfIdle(eng) {
		_ = eng.Play()
	}
	d.poke()
}
func (d *Daemon) Pause() { d.do((*engine.Engine).Pause) }
func (d *Daemon) Toggle() {
	if eng := d.engine(); eng != nil && !d.resumeIfIdle(eng) {
		_ = eng.Toggle()
	}
	d.poke()
}
func (d *Daemon) Next() { d.do((*engine.Engine).Next) }
func (d *Daemon) Prev() { d.do((*engine.Engine).Prev) }
func (d *Daemon) Seek(sec float64) {
	d.do(func(e *engine.Engine) error { return e.Seek(sec) })
}
func (d *Daemon) SetVolume(v float64) {
	d.do(func(e *engine.Engine) error { return e.SetVolume(v) })
}
func (d *Daemon) SetShuffle(on bool) {
	d.do(func(e *engine.Engine) error { return e.SetShuffle(on) })
}
func (d *Daemon) SetRepeat(mode int) {
	d.do(func(e *engine.Engine) error { return e.SetRepeat(mode) })
}

// Raise focuses the brumm TUI, opening one if none is running.
func (d *Daemon) Raise() {
	cmd := exec.Command("omarchy-launch-or-focus-tui", "brumm")
	if cmd.Start() == nil {
		go func() { _ = cmd.Wait() }()
	}
}

func (d *Daemon) do(f func(*engine.Engine) error) {
	if eng := d.engine(); eng != nil {
		_ = f(eng)
		d.poke()
	}
}
