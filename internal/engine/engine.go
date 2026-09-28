// Package engine plays Apple Music through MusicKit JS in a headless Chrome.
package engine

import (
	"context"
	_ "embed"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/exec"
	"strings"
	"syscall"
	"time"

	"github.com/chromedp/chromedp"

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
	ctx     context.Context
	cancel  func()
	srv     *http.Server
	profile string
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
	srv := &http.Server{
		Handler: http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
			w.Header().Set("Content-Type", "text/html; charset=utf-8")
			w.Header().Set("Cache-Control", "no-store")
			_, _ = w.Write([]byte(html))
		}),
		ReadHeaderTimeout: 5 * time.Second,
	}
	go func() { _ = srv.Serve(ln) }()

	profile, err := os.MkdirTemp("", "brumm-chrome-")
	if err != nil {
		_ = srv.Close()
		return nil, err
	}

	opts := []chromedp.ExecAllocatorOption{
		chromedp.ExecPath(chrome.Path()),
		chromedp.UserDataDir(profile),
		chromedp.NoFirstRun,
		chromedp.NoDefaultBrowserCheck,
		chromedp.ModifyCmdFunc(func(cmd *exec.Cmd) {
			cmd.SysProcAttr = &syscall.SysProcAttr{Pdeathsig: syscall.SIGKILL}
		}),
	}
	for _, arg := range chrome.Args() {
		name, value, hasValue := strings.Cut(strings.TrimPrefix(arg, "--"), "=")
		if hasValue {
			opts = append(opts, chromedp.Flag(name, value))
		} else {
			opts = append(opts, chromedp.Flag(name, true))
		}
	}
	allocCtx, allocCancel := chromedp.NewExecAllocator(context.Background(), opts...)
	ctx, ctxCancel := chromedp.NewContext(allocCtx)

	e := &Engine{
		ctx:     ctx,
		srv:     srv,
		profile: profile,
		cancel:  func() { ctxCancel(); allocCancel() },
	}
	url := fmt.Sprintf("http://127.0.0.1:%d/", ln.Addr().(*net.TCPAddr).Port)
	// Launch the browser on the long-lived context, then navigate on a
	// bounded one so a dead network cannot hang startup forever.
	if err := chromedp.Run(ctx); err != nil {
		e.Close()
		return nil, fmt.Errorf("chrome: %w", err)
	}
	navCtx, navCancel := context.WithTimeout(ctx, 30*time.Second)
	defer navCancel()
	if err := chromedp.Run(navCtx, chromedp.Navigate(url)); err != nil {
		e.Close()
		return nil, fmt.Errorf("chrome: %w", err)
	}
	return e, nil
}

func (e *Engine) eval(js string, out any) error {
	ctx, cancel := context.WithTimeout(e.ctx, 3*time.Second)
	defer cancel()
	return chromedp.Run(ctx, chromedp.Evaluate(js, out))
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
func (e *Engine) Alive() bool { return e.ctx.Err() == nil }

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

func (e *Engine) Close() {
	e.cancel()
	_ = e.srv.Close()
	_ = os.RemoveAll(e.profile)
}
