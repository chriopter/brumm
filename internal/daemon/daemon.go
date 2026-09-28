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
	stateEvery = 5                     // poll state every 5th frame (200 ms)
	staleAfter = 10 * time.Second      // no answer from the page this long: restart Chrome
	sendWithin = 200 * time.Millisecond
)

// conn is one client. Writes carry a deadline so a stalled client (say, a
// suspended TUI) is dropped instead of blocking every other one.
type conn struct {
	mu    sync.Mutex
	nc    net.Conn
	enc   *json.Encoder
	bands int // spectrum bands wanted; 0 = none
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

	mu         sync.Mutex
	eng        *engine.Engine
	api        *apple.Client
	state      ipc.State
	conns      map[*conn]bool
	booting    bool
	refreshing bool

	quit     chan struct{}
	quitOnce sync.Once
}

// Run serves until SIGTERM, SIGINT or a quit request.
func Run(version string) error {
	ln, err := listen()
	if err != nil {
		return err
	}
	defer os.Remove(config.Socket())

	d := &Daemon{
		version: version,
		state:   ipc.State{Status: ipc.StatusStarting, Message: "starting"},
		lib:     loadLibrary(),
		conns:   map[*conn]bool{},
		quit:    make(chan struct{}),
	}
	if d.mpris, err = mpris.Start(d); err != nil {
		log.Printf("mpris disabled: %v", err)
	}

	go d.accept(ln)
	go d.boot()
	go d.loop()

	sig := make(chan os.Signal, 1)
	signal.Notify(sig, syscall.SIGINT, syscall.SIGTERM)
	select {
	case <-sig:
	case <-d.quit:
	}
	_ = ln.Close()
	d.mu.Lock()
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
	path := config.Socket()
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
	d.mu.Unlock()
	go d.refresh()

	if err := chrome.Ensure(func(msg string) { d.setStatus(ipc.StatusStarting, msg) }); err != nil {
		d.setStatus(ipc.StatusError, err.Error())
		return
	}
	d.setStatus(ipc.StatusStarting, "starting player")
	eng, err := engine.Start(cfg.Developer(), cfg.UserToken, d.version)
	if err != nil {
		d.setStatus(ipc.StatusError, err.Error())
		return
	}
	d.mu.Lock()
	if d.eng != nil {
		d.eng.Close()
	}
	d.eng = eng
	d.mu.Unlock()
}

func (d *Daemon) engine() *engine.Engine {
	d.mu.Lock()
	defer d.mu.Unlock()
	return d.eng
}

// loop polls the page: the spectrum every frame while music plays and
// someone watches, the player state every stateEvery frames, pushing state
// only when it changed. A dead or unresponsive page is restarted.
func (d *Daemon) loop() {
	t := time.NewTicker(frame)
	defer t.Stop()
	lastOK := time.Now()
	for n := 0; ; n++ {
		select {
		case <-d.quit:
			return
		case <-t.C:
		}
		eng := d.engine()
		if eng == nil {
			lastOK = time.Now()
			continue
		}
		if !eng.Alive() || time.Since(lastOK) > staleAfter {
			lastOK = time.Now()
			d.setStatus(ipc.StatusStarting, "restarting player")
			go d.boot()
			continue
		}
		if bands := d.maxBands(); bands > 0 && d.playing() {
			if spec, err := eng.Spectrum(bands); err == nil {
				d.broadcast(ipc.Message{Spectrum: spec})
			}
		}
		if n%stateEvery != 0 {
			continue
		}
		es, err := eng.State()
		if err != nil {
			continue
		}
		lastOK = time.Now()
		d.apply(es)
	}
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
	next := ipc.State{Status: status, Message: msg, State: es}
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

func (d *Daemon) maxBands() int {
	d.mu.Lock()
	defer d.mu.Unlock()
	n := 0
	for c := range d.conns {
		n = max(n, c.bands)
	}
	return n
}

// broadcast pushes to subscribers; spectrum frames only to those that want
// them. A client that cannot keep up is disconnected.
func (d *Daemon) broadcast(m ipc.Message) {
	d.mu.Lock()
	var targets []*conn
	for c := range d.conns {
		if c.sub && (m.Spectrum == nil || c.bands > 0) {
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
	for sc.Scan() {
		var r ipc.Request
		if json.Unmarshal(sc.Bytes(), &r) != nil {
			continue
		}
		// Requests may be slow (network); answer them concurrently so a
		// search does not hold up a pause.
		go func() {
			reply := ipc.Message{ID: r.ID}
			if err := d.handle(c, r, &reply); err != nil {
				reply.Error = err.Error()
			}
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
		c.sub, c.bands = true, min(r.Bands, 256)
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
		return eng.PlayIDs(r.IDs, r.Start, r.Source)
	case ipc.CmdToggle:
		return eng.Toggle()
	case ipc.CmdNext:
		return eng.Next()
	case ipc.CmdPrev:
		return eng.Prev()
	case ipc.CmdSeek:
		return eng.Seek(r.Value)
	case ipc.CmdVolume:
		return eng.SetVolume(r.Value)
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
	if reply.Tracks, err = api.Tracks(it); err != nil {
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
	d.boot()
}

// refresh re-reads the library from Apple Music — the lists first, then
// every playlist's tracks — saving and telling clients after each step.
func (d *Daemon) refresh() {
	d.mu.Lock()
	api, busy := d.api, d.refreshing
	d.refreshing = true
	d.mu.Unlock()
	if api == nil || busy {
		return
	}
	defer func() {
		d.mu.Lock()
		d.refreshing = false
		d.mu.Unlock()
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

// mpris.Controller

func (d *Daemon) Play()   { d.do((*engine.Engine).Play) }
func (d *Daemon) Pause()  { d.do((*engine.Engine).Pause) }
func (d *Daemon) Toggle() { d.do((*engine.Engine).Toggle) }
func (d *Daemon) Next()   { d.do((*engine.Engine).Next) }
func (d *Daemon) Prev()   { d.do((*engine.Engine).Prev) }
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
	}
}
