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
	Playlist  string  `json:"playlist"`
	Volume    float64 `json:"volume"`
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
	if err := chromedp.Run(ctx, chromedp.Navigate(url)); err != nil {
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

func (e *Engine) PlayPlaylist(id string, index int) error {
	return e.call(`brumm.playPlaylist(%q, %d)`, id, index)
}

func (e *Engine) Toggle() error               { return e.call(`brumm.toggle()`) }
func (e *Engine) Play() error                 { return e.call(`brumm.play()`) }
func (e *Engine) Pause() error                { return e.call(`brumm.pause()`) }
func (e *Engine) Next() error                 { return e.call(`brumm.next()`) }
func (e *Engine) Prev() error                 { return e.call(`brumm.prev()`) }
func (e *Engine) Seek(sec float64) error      { return e.call(`brumm.seek(%f)`, sec) }
func (e *Engine) SetVolume(v float64) error   { return e.call(`brumm.volume(%f)`, v) }
func (e *Engine) SetUserToken(t string) error { return e.call(`brumm.setToken(%q)`, t) }

func (e *Engine) Close() {
	e.cancel()
	_ = e.srv.Close()
	_ = os.RemoveAll(e.profile)
}
