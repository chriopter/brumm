// Package login signs the user in to Apple Music in their own browser and
// stores the resulting Music User Token.
//
// MusicKit's authorize() needs a visible window for Apple's OAuth popup (in
// headless Chrome it hangs on an invisible one), so the login runs on a
// loopback page opened in the default browser, which posts the token back.
package login

import (
	"context"
	"crypto/rand"
	_ "embed"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"html/template"
	"net"
	"net/http"
	"os/exec"
	"sync"
	"time"

	"github.com/chriopter/brumm/internal/config"
)

//go:embed login.html
var pageHTML string

var pageTmpl = template.Must(template.New("login").Parse(pageHTML))

// A fixed port keeps the page's origin stable between logins.
const addr = "127.0.0.1:7777"

// Run opens the login page and blocks until a token arrives or the context
// ends. The token is saved to the config.
func Run(ctx context.Context, status func(string)) error {
	cfg, err := config.Load()
	if err != nil {
		return err
	}
	if cfg.Developer() == "" {
		return errors.New("no Apple Music developer token in this build")
	}

	ln, err := net.Listen("tcp", addr)
	if err != nil {
		return fmt.Errorf("login: %w (is another login open?)", err)
	}
	// A secret per login: the page lives at /<secret>/ and posts the token
	// back with it, so nothing else — a local process, another web page —
	// can plant a token of its own. The origin must be this page's.
	raw := make([]byte, 16)
	if _, err := rand.Read(raw); err != nil {
		return err
	}
	secret := hex.EncodeToString(raw)
	origin := "http://" + addr
	tokens := make(chan string, 1)
	var once sync.Once
	mux := http.NewServeMux()
	mux.HandleFunc("GET /"+secret+"/{$}", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		w.Header().Set("Cache-Control", "no-store")
		_ = pageTmpl.Execute(w, map[string]string{"DeveloperToken": cfg.Developer()})
	})
	mux.HandleFunc("POST /"+secret+"/token", func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Origin") != origin || r.Header.Get("Content-Type") != "application/json" {
			http.Error(w, "forbidden", http.StatusForbidden)
			return
		}
		var body struct {
			Token string `json:"token"`
		}
		if json.NewDecoder(http.MaxBytesReader(w, r.Body, 64<<10)).Decode(&body) != nil || body.Token == "" {
			http.Error(w, "missing token", http.StatusBadRequest)
			return
		}
		once.Do(func() { tokens <- body.Token })
		w.WriteHeader(http.StatusNoContent)
	})
	srv := &http.Server{Handler: mux, ReadHeaderTimeout: 5 * time.Second}
	go func() { _ = srv.Serve(ln) }()
	defer srv.Close()

	url := origin + "/" + secret + "/"
	status("opening Apple Music sign-in in your browser…")
	if err := exec.Command("xdg-open", url).Start(); err != nil {
		status("open " + url + " to sign in")
	}

	ctx, cancel := context.WithTimeout(ctx, 5*time.Minute)
	defer cancel()
	select {
	case tok := <-tokens:
		cfg.UserToken = tok
		return cfg.Save()
	case <-ctx.Done():
		return errors.New("login timed out")
	}
}
