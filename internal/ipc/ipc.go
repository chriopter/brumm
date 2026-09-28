// Package ipc is the protocol between the brumm daemon and its clients:
// newline-delimited JSON over a unix socket. Clients send Requests; the
// daemon answers each with a Message carrying the same ID, and pushes
// state and spectrum Messages to connections that subscribed.
package ipc

import (
	"bufio"
	"encoding/json"
	"errors"
	"net"
	"sync"
	"time"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/config"
	"github.com/chriopter/brumm/internal/engine"
)

// Commands.
const (
	CmdSubscribe = "subscribe" // push state (and spectrum when Bands > 0)
	CmdPlaylists = "playlists"
	CmdTracks    = "tracks" // Playlist
	CmdPlay      = "play"   // Playlist, Index
	CmdToggle    = "toggle"
	CmdNext      = "next"
	CmdPrev      = "prev"
	CmdSeek      = "seek"   // Value: absolute seconds
	CmdVolume    = "volume" // Value: 0–1
	CmdReload    = "reload" // re-read credentials after a login
	CmdQuit      = "quit"   // stop the daemon
)

type Request struct {
	ID       int     `json:"id"`
	Cmd      string  `json:"cmd"`
	Playlist string  `json:"playlist,omitempty"`
	Index    int     `json:"index,omitempty"`
	Value    float64 `json:"value,omitempty"`
	Bands    int     `json:"bands,omitempty"`
}

// Status is the daemon's lifecycle phase.
type Status string

const (
	StatusStarting  Status = "starting"
	StatusReady     Status = "ready"
	StatusLoggedOut Status = "logged-out"
	StatusError     Status = "error"
)

type State struct {
	Status  Status `json:"status"`
	Message string `json:"message,omitempty"` // progress or error text
	engine.State
}

type Message struct {
	ID        int              `json:"id,omitempty"`
	Error     string           `json:"error,omitempty"`
	Playlists []apple.Playlist `json:"playlists,omitempty"`
	Tracks    []apple.Track    `json:"tracks,omitempty"`
	State     *State           `json:"state,omitempty"`
	Spectrum  []int            `json:"spectrum,omitempty"`
	Library   bool             `json:"library,omitempty"` // the cached library changed
}

// Client is one connection to the daemon.
type Client struct {
	conn    net.Conn
	enc     *json.Encoder
	mu      sync.Mutex
	nextID  int
	pending map[int]chan Message
	events  chan Message
}

// Dial connects to a running daemon.
func Dial() (*Client, error) {
	conn, err := net.DialTimeout("unix", config.Socket(), time.Second)
	if err != nil {
		return nil, err
	}
	c := &Client{
		conn:    conn,
		enc:     json.NewEncoder(conn),
		pending: map[int]chan Message{},
		events:  make(chan Message, 64),
	}
	go c.read()
	return c, nil
}

func (c *Client) read() {
	sc := bufio.NewScanner(c.conn)
	sc.Buffer(make([]byte, 64<<10), 16<<20)
	for sc.Scan() {
		var m Message
		if json.Unmarshal(sc.Bytes(), &m) != nil {
			continue
		}
		if m.ID != 0 {
			c.mu.Lock()
			ch := c.pending[m.ID]
			delete(c.pending, m.ID)
			c.mu.Unlock()
			if ch != nil {
				ch <- m
			}
			continue
		}
		select {
		case c.events <- m:
		default: // a slow reader drops frames rather than stalling the daemon
		}
	}
	close(c.events)
	c.mu.Lock()
	for id, ch := range c.pending {
		close(ch)
		delete(c.pending, id)
	}
	c.mu.Unlock()
}

// Events delivers pushed state and spectrum; it closes when the daemon goes away.
func (c *Client) Events() <-chan Message { return c.events }

// Do sends a request and waits for its answer.
func (c *Client) Do(r Request) (Message, error) {
	c.mu.Lock()
	c.nextID++
	r.ID = c.nextID
	ch := make(chan Message, 1)
	c.pending[r.ID] = ch
	err := c.enc.Encode(r)
	c.mu.Unlock()
	if err != nil {
		return Message{}, err
	}
	m, ok := <-ch
	if !ok {
		return m, errors.New("daemon disconnected")
	}
	if m.Error != "" {
		return m, errors.New(m.Error)
	}
	return m, nil
}

func (c *Client) Close() error { return c.conn.Close() }
