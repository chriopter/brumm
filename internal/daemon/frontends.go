package daemon

import (
	"context"
	"encoding/json"
	"errors"
	"log"
	"sync"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/config"
	"github.com/chriopter/brumm/internal/ipc"
	"github.com/chriopter/brumm/internal/login"
	"github.com/chriopter/brumm/internal/update"
)

// What the players ask of the daemon beyond playing: the options they
// share, signing in, and each other. A player needs nothing but the
// socket; the files and programs behind these stay the daemon's.

var optionsMu sync.Mutex // one change of the options at a time

// options answers the saved options, after changing those named in set.
// A change is announced to every subscribed player.
func (d *Daemon) options(set map[string]any, reply *ipc.Message) error {
	optionsMu.Lock()
	defer optionsMu.Unlock()
	o := config.LoadOptions()
	if len(set) > 0 {
		was := o
		b, err := json.Marshal(set)
		if err != nil {
			return err
		}
		if err := json.Unmarshal(b, &o); err != nil {
			return errors.New("options: " + err.Error())
		}
		if _, ok := set["bar"]; ok && o.Bar != was.Bar {
			if err := update.SetPlugin(o.Bar == "on"); err != nil {
				return err
			}
		}
		if err := o.Save(); err != nil {
			return err
		}
		o = config.LoadOptions()
		if o.NoAutoplay != was.NoAutoplay {
			if eng := d.engine(); eng != nil {
				_ = eng.SetAutoplay(!o.NoAutoplay)
			}
		}
		defer d.broadcast(ipc.Message{Options: &o})
	}
	reply.Options = &o
	reply.Omarchy = update.HasOmarchy()
	return nil
}

var (
	placeMu sync.Mutex
	placeAt json.RawMessage
)

// place keeps where a player is, and answers where the last one was: a
// player switched to (g) opens where the other left off. It lives only as
// long as the daemon; the daemon never looks inside.
func (d *Daemon) place(set json.RawMessage, reply *ipc.Message) {
	placeMu.Lock()
	defer placeMu.Unlock()
	if len(set) > 0 && len(set) < 64<<10 {
		placeAt = append(json.RawMessage(nil), set...)
	}
	reply.Place = placeAt
}

var loginMu sync.Mutex

// login opens the sign-in page, once at a time, and reloads the player
// with the new session.
func (d *Daemon) login() {
	if !loginMu.TryLock() {
		return // the page is open already
	}
	go func() {
		defer loginMu.Unlock()
		if err := login.Run(context.Background(), func(s string) { log.Printf("login: %s", s) }); err != nil {
			log.Printf("login: %v", err)
			// The players say what went wrong: the report carries it.
			d.mu.Lock()
			st := d.state
			d.mu.Unlock()
			st.Err = "sign-in: " + err.Error()
			d.broadcast(ipc.Message{State: &st})
			return
		}
		d.reload()
	}()
}

// resolve finds what a music.apple.com link names; a song's link names
// its album, with the song picked out.
func (d *Daemon) resolve(link string, reply *ipc.Message) error {
	it, song, ok := apple.ParseLink(link)
	if !ok {
		return errors.New("not a music.apple.com link")
	}
	if it.ID == "" {
		api, err := d.client()
		if err != nil {
			return err
		}
		if it, err = api.AlbumOf(song); err != nil {
			return d.authCheck(err)
		}
	}
	reply.Items = []apple.Item{it}
	if song != "" {
		reply.IDs = []string{song}
	}
	return nil
}
