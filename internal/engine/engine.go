// Package engine plays Apple Music through MusicKit JS in a headless Chrome.
package engine

import (
	"context"
	"crypto/rand"
	_ "embed"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/chrome"
)

//go:embed player.html
var page string

// State mirrors brumm.state() in player.html.
type State struct {
	Ready     bool    `json:"ready"`
	Err       string  `json:"err"`
	NeedsAuth bool    `json:"needsAuth"`
	Playing   bool    `json:"playing"`
	Title     string  `json:"title"`
	Artist    string  `json:"artist"`
	Album     string  `json:"album"`
	Artwork   string  `json:"artwork"`
	Pos       float64 `json:"pos"`
	Dur       float64 `json:"dur"`
	Index     int     `json:"index"`
	Length    int     `json:"length"`
	ID        string  `json:"id"`     // the playing song
	Source    string  `json:"source"` // key of the list it was started from
	Volume    float64 `json:"volume"`
	Shuffle   bool    `json:"shuffle"`
	Repeat    int     `json:"repeat"` // 0 off, 1 one, 2 all
	Preview   *Clip   `json:"preview,omitempty"`
}

// Clip is a song being previewed.
type Clip struct {
	Title   string `json:"title"`
	Artist  string `json:"artist"`
	Artwork string `json:"artwork"`
}

type Engine struct {
	cmd       *exec.Cmd
	cdp       *cdp
	session   string        // the playback page
	exited    chan struct{} // closed when Chrome has exited
	srv       *http.Server
	profile   string
	closeOnce sync.Once
}

// Start launches Chrome on the playback page. The Chrome process dies with
// the calling thread's process (Pdeathsig), so a killed daemon leaves no
// orphaned browser behind.
func Start(developerToken, userToken, version string) (*Engine, error) {
	cfg, err := json.Marshal(map[string]string{
		"developerToken": developerToken,
		"userToken":      userToken,
		"version":        version,
	})
	if err != nil {
		return nil, err
	}
	html := strings.ReplaceAll(page, "__BRUMM_CFG__", string(cfg))

	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return nil, err
	}
	// The page carries the user's Apple Music token. It is served only at an
	// unguessable path and only to requests addressed to this loopback port,
	// so other local users cannot read it.
	secret := make([]byte, 16)
	if _, err := rand.Read(secret); err != nil {
		ln.Close()
		return nil, err
	}
	pagePath := "/" + hex.EncodeToString(secret)
	host := ln.Addr().String()
	srv := &http.Server{
		Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if r.URL.Path != pagePath || r.Host != host {
				http.NotFound(w, r)
				return
			}
			w.Header().Set("Content-Type", "text/html; charset=utf-8")
			w.Header().Set("Cache-Control", "no-store")
			w.Header().Set("Referrer-Policy", "no-referrer")
			_, _ = w.Write([]byte(html))
		}),
		ReadHeaderTimeout: 5 * time.Second,
	}
	go func() { _ = srv.Serve(ln) }()

	profile, err := os.MkdirTemp(profileRoot(), "brumm-chrome-")
	if err != nil {
		_ = srv.Close()
		return nil, err
	}

	// Chrome talks DevTools over two pipes (fd 3 in, fd 4 out) and opens
	// no debugging port another local user could connect to.
	toChrome, ourW, err := os.Pipe()
	if err != nil {
		_ = srv.Close()
		_ = os.RemoveAll(profile)
		return nil, err
	}
	ourR, fromChrome, err := os.Pipe()
	if err != nil {
		toChrome.Close()
		ourW.Close()
		_ = srv.Close()
		_ = os.RemoveAll(profile)
		return nil, err
	}
	args := append(chrome.Args(),
		"--remote-debugging-pipe",
		"--user-data-dir="+profile,
		"--no-first-run",
		"--no-default-browser-check",
		"about:blank",
	)
	cmd := exec.Command(chrome.Path(), args...)
	if runtime := os.Getenv("BRUMM_DESKTOP_RUNTIME"); runtime != "" {
		cmd.Env = os.Environ()
		if os.Getenv("PULSE_SERVER") == "" {
			cmd.Env = append(cmd.Env, "PULSE_SERVER=unix:"+filepath.Join(runtime, "pulse/native"))
		}
		if os.Getenv("PIPEWIRE_RUNTIME_DIR") == "" {
			cmd.Env = append(cmd.Env, "PIPEWIRE_RUNTIME_DIR="+runtime)
		}
	}
	cmd.ExtraFiles = []*os.File{toChrome, fromChrome}
	cmd.SysProcAttr = &syscall.SysProcAttr{Pdeathsig: syscall.SIGKILL}
	err = cmd.Start()
	toChrome.Close() // Chrome holds its own copies now
	fromChrome.Close()
	if err != nil {
		ourW.Close()
		ourR.Close()
		_ = srv.Close()
		_ = os.RemoveAll(profile)
		return nil, fmt.Errorf("chrome: %w", err)
	}
	e := &Engine{
		cmd:     cmd,
		cdp:     newCDP(ourW, ourR),
		exited:  make(chan struct{}),
		srv:     srv,
		profile: profile,
	}
	go func() {
		_ = cmd.Wait()
		close(e.exited)
	}()

	if err := e.open("http://" + host + pagePath); err != nil {
		e.Close()
		return nil, err
	}
	return e, nil
}

// open attaches to Chrome's tab and loads url in it, bounded so a Chrome
// that never comes up or a dead network cannot hang startup.
func (e *Engine) open(url string) error {
	ctx, cancel := context.WithTimeout(context.Background(), launchTimeout)
	defer cancel()
	var targets struct {
		Infos []struct {
			ID   string `json:"targetId"`
			Type string `json:"type"`
		} `json:"targetInfos"`
	}
	if err := e.cdp.call(ctx, "", "Target.getTargets", nil, &targets); err != nil {
		return err
	}
	id := ""
	for _, t := range targets.Infos {
		if t.Type == "page" {
			id = t.ID
			break
		}
	}
	if id == "" {
		var created struct {
			ID string `json:"targetId"`
		}
		if err := e.cdp.call(ctx, "", "Target.createTarget", map[string]any{"url": "about:blank"}, &created); err != nil {
			return err
		}
		id = created.ID
	}
	var attached struct {
		Session string `json:"sessionId"`
	}
	if err := e.cdp.call(ctx, "", "Target.attachToTarget", map[string]any{"targetId": id, "flatten": true}, &attached); err != nil {
		return err
	}
	e.session = attached.Session
	var nav struct {
		ErrorText string `json:"errorText"`
	}
	if err := e.cdp.call(ctx, e.session, "Page.navigate", map[string]any{"url": url}, &nav); err != nil {
		return err
	}
	if nav.ErrorText != "" {
		return fmt.Errorf("chrome: loading the player: %s", nav.ErrorText)
	}
	return nil
}

// launchTimeout bounds how long Chrome may take to come up.
const launchTimeout = 30 * time.Second

// eval runs js in the page and decodes its value into out.
func (e *Engine) eval(js string, out any) error {
	return e.evalWait(js, 3*time.Second, false, out)
}

// evalWait is eval with its own time limit; await makes it wait for a
// promise the expression returns.
func (e *Engine) evalWait(js string, limit time.Duration, await bool, out any) error {
	ctx, cancel := context.WithTimeout(context.Background(), limit)
	defer cancel()
	var r struct {
		Result struct {
			Type  string          `json:"type"`
			Value json.RawMessage `json:"value"`
		} `json:"result"`
		Exception *struct {
			Text      string `json:"text"`
			Exception struct {
				Description string `json:"description"`
			} `json:"exception"`
		} `json:"exceptionDetails"`
	}
	err := e.cdp.call(ctx, e.session, "Runtime.evaluate", map[string]any{
		"expression":    js,
		"returnByValue": true,
		"awaitPromise":  await,
	}, &r)
	if err != nil {
		return err
	}
	if x := r.Exception; x != nil {
		msg := x.Exception.Description
		if msg == "" {
			msg = x.Text
		}
		return fmt.Errorf("page: %s", msg)
	}
	if r.Result.Value == nil {
		return fmt.Errorf("page: %s result", r.Result.Type)
	}
	return json.Unmarshal(r.Result.Value, out)
}

// call runs a command without awaiting any promise it returns.
func (e *Engine) call(format string, args ...any) error {
	var ok bool
	return e.eval(fmt.Sprintf(format, args...)+"; true", &ok)
}

func (e *Engine) State() (State, error) {
	var raw string
	var s State
	if err := e.eval(`window.brumm ? brumm.state() : "{}"`, &raw); err != nil {
		return s, err
	}
	err := json.Unmarshal([]byte(raw), &s)
	return s, err
}

// Watch returns the player state once it is newer than rev — the revision
// of the last answer, 0 to ask at once — or after heartbeat at the latest.
// The page answers on its own events, so calling it in a loop follows the
// player without polling it. A page that has not answered well after the
// heartbeat is hung, and that is an error.
func (e *Engine) Watch(rev int64, heartbeat time.Duration) (State, int64, error) {
	var raw string
	js := fmt.Sprintf(`window.brumm ? brumm.wait(%d, %d) : new Promise((r) => setTimeout(() => r("{}"), 300))`,
		rev, heartbeat.Milliseconds())
	var s State
	if err := e.evalWait(js, heartbeat+15*time.Second, true, &raw); err != nil {
		return s, rev, err
	}
	var r struct {
		Rev int64 `json:"rev"`
	}
	if err := json.Unmarshal([]byte(raw), &s); err != nil {
		return s, rev, err
	}
	_ = json.Unmarshal([]byte(raw), &r)
	return s, r.Rev, nil
}

// Spectrum returns n log-spaced frequency bands, 0–255 each.
func (e *Engine) Spectrum(n int) ([]int, error) {
	var bands []int
	err := e.eval(fmt.Sprintf(`window.brumm && brumm.spectrum ? brumm.spectrum(%d) : []`, n), &bands)
	return bands, err
}

// Wave returns n waveform samples, -100…100.
func (e *Engine) Wave(n int) ([]int, error) {
	var samples []int
	err := e.eval(fmt.Sprintf(`window.brumm && brumm.wave ? brumm.wave(%d) : []`, n), &samples)
	return samples, err
}

// Preview plays a song's 30-second clip over a paused queue.
func (e *Engine) Preview(id string) error { return e.call(`brumm.preview(%q)`, id) }
func (e *Engine) StopPreview() error      { return e.call(`brumm.stopPreview()`) }

// PlayIDs queues songs by id and starts at startID, startAt seconds in;
// source names the list they came from so clients can mark it.
func (e *Engine) PlayIDs(ids []string, startID, source string, startAt float64) error {
	list, err := json.Marshal(ids)
	if err != nil {
		return err
	}
	return e.call(`brumm.playIds(%s, %q, %q, %f)`, list, startID, source, startAt)
}

// PlayStation plays a radio station: endless, chosen by Apple.
func (e *Engine) PlayStation(id, source string) error {
	return e.call(`brumm.playStation(%q, %q)`, id, source)
}

// Warm looks songs up in the page ahead of a play, so starting one of them
// skips the round trip to Apple.
func (e *Engine) Warm(ids []string) error {
	list, err := json.Marshal(ids)
	if err != nil {
		return err
	}
	return e.call(`brumm.warm(%s)`, list)
}

// SetAutoplay lets similar music play on when the queue runs out.
func (e *Engine) SetAutoplay(on bool) error { return e.call(`brumm.autoplay(%t)`, on) }

// Queue is the current song and what follows it.
type Queue struct {
	Pos   int           `json:"pos"`
	Items []apple.Track `json:"items"`
}

func (e *Engine) Queue(n int) (Queue, error) {
	var raw string
	var q Queue
	if err := e.eval(fmt.Sprintf(`window.brumm && brumm.queue ? brumm.queue(%d) : "{}"`, n), &raw); err != nil {
		return q, err
	}
	err := json.Unmarshal([]byte(raw), &q)
	return q, err
}

// Jump plays the queue item at index i.
func (e *Engine) Jump(i int) error { return e.call(`brumm.jump(%d)`, i) }

// Enqueue adds songs right after the current one (next) or at the end.
func (e *Engine) Enqueue(ids []string, next bool) error {
	list, err := json.Marshal(ids)
	if err != nil {
		return err
	}
	return e.call(`brumm.enqueue(%s, %t)`, list, next)
}

// Alive reports whether Chrome is still there.
func (e *Engine) Alive() bool {
	select {
	case <-e.exited:
		return false
	case <-e.cdp.gone:
		return false
	default:
		return true
	}
}

func (e *Engine) Toggle() error               { return e.call(`brumm.toggle()`) }
func (e *Engine) Play() error                 { return e.call(`brumm.play()`) }
func (e *Engine) Pause() error                { return e.call(`brumm.pause()`) }
func (e *Engine) Next() error                 { return e.call(`brumm.next()`) }
func (e *Engine) Prev() error                 { return e.call(`brumm.prev()`) }
func (e *Engine) Seek(sec float64) error      { return e.call(`brumm.seek(%f)`, sec) }
func (e *Engine) SetVolume(v float64) error   { return e.call(`brumm.volume(%f)`, v) }
func (e *Engine) SetUserToken(t string) error { return e.call(`brumm.setToken(%q)`, t) }
func (e *Engine) SetShuffle(on bool) error    { return e.call(`brumm.shuffle(%t)`, on) }
func (e *Engine) SetRepeat(mode int) error    { return e.call(`brumm.repeat(%d)`, mode) }

// Close ends Chrome — asking first, killing it if it does not exit within
// a few seconds — waits for it so its profile is not still being written,
// and removes the profile. It never blocks longer than that.
func (e *Engine) Close() {
	e.closeOnce.Do(func() {
		ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
		_ = e.cdp.call(ctx, "", "Browser.close", nil, nil)
		cancel()
		select {
		case <-e.exited:
		case <-time.After(5 * time.Second):
			_ = e.cmd.Process.Kill()
			<-e.exited
		}
		e.cdp.close()
		_ = e.srv.Close()
		_ = os.RemoveAll(e.profile)
	})
}

// profileRoot keeps Chrome's throwaway profiles in the user's runtime
// directory: private to the user and cleared at logout.
func profileRoot() string {
	if d := os.Getenv("XDG_RUNTIME_DIR"); d != "" {
		return d
	}
	return os.TempDir()
}

// SweepProfiles removes profiles a killed daemon left behind. Call it
// before starting Chrome.
func SweepProfiles() {
	old, _ := filepath.Glob(filepath.Join(profileRoot(), "brumm-chrome-*"))
	for _, dir := range old {
		_ = os.RemoveAll(dir)
	}
}
