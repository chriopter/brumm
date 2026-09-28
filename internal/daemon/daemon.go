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
	maxFails   = 25                    // consecutive failed polls before restarting Chrome
)

type subscriber struct {
	conn  *conn
	bands int
}

type conn struct {
	mu  sync.Mutex
	enc *json.Encoder
}

func (c *conn) send(m ipc.Message) {
	c.mu.Lock()
	defer c.mu.Unlock()
	_ = c.enc.Encode(m)
}

type Daemon struct {
	version string
	mpris   *mpris.Server

	lib *library

	mu         sync.Mutex
	eng        *engine.Engine
	api        *apple.Client
	state      ipc.State
	subs       map[*conn]*subscriber
	refreshing bool

	quit chan struct{}
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
		subs:    map[*conn]*subscriber{},
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
	d.broadcast(ipc.Message{State: &st}, false)
}

// boot connects to Apple Music first — so the library refreshes while
// Chrome is still being installed — then starts playback.
func (d *Daemon) boot() {
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
	d.startEngine(cfg)
}

func (d *Daemon) startEngine(cfg config.Config) {

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

// loop polls the page: the spectrum every frame while someone watches it,
// the player state every stateEvery frames, pushing state only on change.
func (d *Daemon) loop() {
	t := time.NewTicker(frame)
	defer t.Stop()
	fails := 0
	for n := 0; ; n++ {
		select {
		case <-d.quit:
			return
		case <-t.C:
		}
		eng := d.engine()
		if eng == nil {
			continue
		}
		if bands := d.maxBands(); bands > 0 {
			if spec, err := eng.Spectrum(bands); err == nil {
				d.broadcast(ipc.Message{Spectrum: spec}, true)
			}
		}
		if n%stateEvery != 0 {
			continue
		}
		es, err := eng.State()
		if err != nil {
			if fails++; fails == maxFails {
				fails = 0
				d.setStatus(ipc.StatusStarting, "restarting player")
				go d.boot()
			}
			continue
		}
		fails = 0
		d.apply(es)
	}
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
		d.broadcast(ipc.Message{State: &next}, false)
	}
}

func (d *Daemon) maxBands() int {
	d.mu.Lock()
	defer d.mu.Unlock()
	n := 0
	for _, s := range d.subs {
		n = max(n, s.bands)
	}
	return n
}

func (d *Daemon) broadcast(m ipc.Message, spectrum bool) {
	d.mu.Lock()
	var targets []*conn
	for c, s := range d.subs {
		if !spectrum || s.bands > 0 {
			targets = append(targets, c)
		}
	}
	d.mu.Unlock()
	for _, c := range targets {
		c.send(m)
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
	c := &conn{enc: json.NewEncoder(nc)}
	defer func() {
		d.mu.Lock()
		delete(d.subs, c)
		d.mu.Unlock()
		nc.Close()
	}()
	sc := bufio.NewScanner(nc)
	for sc.Scan() {
		var r ipc.Request
		if json.Unmarshal(sc.Bytes(), &r) != nil {
			continue
		}
		reply := ipc.Message{ID: r.ID}
		if err := d.handle(c, r, &reply); err != nil {
			reply.Error = err.Error()
		}
		c.send(reply)
	}
}

func (d *Daemon) handle(c *conn, r ipc.Request, reply *ipc.Message) error {
	switch r.Cmd {
	case ipc.CmdSubscribe:
		d.mu.Lock()
		d.subs[c] = &subscriber{conn: c, bands: min(r.Bands, 256)}
		st := d.state
		d.mu.Unlock()
		reply.State = &st
		return nil
	case ipc.CmdPlaylists:
		if cached, ok := d.lib.playlists(); ok {
			reply.Playlists = cached
			return nil
		}
		api, err := d.client()
		if err != nil {
			return err
		}
		if reply.Playlists, err = api.Playlists(); err != nil {
			return d.authCheck(err)
		}
		d.lib.setPlaylists(reply.Playlists)
		return nil
	case ipc.CmdTracks:
		if cached, ok := d.lib.tracks(r.Playlist); ok {
			reply.Tracks = cached
			return nil
		}
		api, err := d.client()
		if err != nil {
			return err
		}
		if reply.Tracks, err = api.Tracks(r.Playlist); err != nil {
			return d.authCheck(err)
		}
		d.lib.setTracks(r.Playlist, reply.Tracks)
		return nil
	case ipc.CmdReload:
		go d.reload()
		return nil
	case ipc.CmdQuit:
		close(d.quit)
		return nil
	}

	eng := d.engine()
	if eng == nil {
		return errors.New("player is not running")
	}
	switch r.Cmd {
	case ipc.CmdPlay:
		return eng.PlayPlaylist(r.Playlist, r.Index)
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
	}
	return fmt.Errorf("unknown command %q", r.Cmd)
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

// refresh re-reads the whole library from Apple Music — playlists first,
// then every playlist's tracks — and tells clients once it changed.
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

	playlists, err := api.Playlists()
	if err != nil {
		_ = d.authCheck(err)
		return
	}
	d.lib.setPlaylists(playlists)
	d.broadcast(ipc.Message{Library: true}, false)
	for _, p := range playlists {
		tracks, err := api.Tracks(p.ID)
		if errors.Is(err, apple.ErrUnauthorized) {
			_ = d.authCheck(err)
			return
		}
		if err != nil {
			continue // one broken playlist should not stop the rest
		}
		d.lib.setTracks(p.ID, tracks)
	}
	if err := d.lib.save(); err != nil {
		log.Printf("library cache: %v", err)
	}
	d.broadcast(ipc.Message{Library: true}, false)
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
