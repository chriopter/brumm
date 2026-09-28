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
	CmdList      = "list"      // List: one of the lists below → Items or Tracks
	CmdOpen      = "open"      // Item → Tracks (playlist, album) or Items (artist)
	CmdSearch    = "search"    // Query → Results
	CmdPlay      = "play"      // IDs, Start, Source
	CmdAlbum     = "album"     // Start: a song id → Items: its album
	CmdPreview   = "preview"   // Start: a song id; Value 0 stops the preview
	CmdQueue     = "queue"     // Value: how many → Tracks from the current song, Pos
	CmdJump      = "jump"      // Value: queue index to play
	CmdEnqueue   = "enqueue"   // IDs; Value 1 plays them next, 0 at the end
	CmdLoved     = "loved"     // IDs → IDs: those marked as favorites
	CmdLove      = "love"      // Start: a song id; Value 1 favorite, 0 not
	CmdArtist    = "artist"    // Start: a song id → Items: its artist
	CmdLink      = "link"      // Start: a song id → Link: its music.apple.com address
	CmdUpdate    = "update"    // Value 1 installs the available update; 0 checks for one
	CmdToggle    = "toggle"
	CmdNext      = "next"
	CmdPrev      = "prev"
	CmdSeek      = "seek"    // Value: absolute seconds
	CmdVolume    = "volume"  // Value: 0–1
	CmdShuffle   = "shuffle" // Value: 0 or 1
	CmdRepeat    = "repeat"  // Value: 0 off, 1 one, 2 all
	CmdReload    = "reload"  // re-read credentials after a login
	CmdQuit      = "quit"    // stop the daemon
)

// Library lists.
const (
	ListPlaylists = "playlists"
	ListAlbums    = "albums"
	ListArtists   = "artists"
	ListSongs     = "songs"
)

type Request struct {
	ID     int         `json:"id"`
	Cmd    string      `json:"cmd"`
	List   string      `json:"list,omitempty"`
	Item   *apple.Item `json:"item,omitempty"`
	Query  string      `json:"query,omitempty"`
	IDs    []string    `json:"ids,omitempty"`
	Start  string      `json:"start,omitempty"`
	Source string      `json:"source,omitempty"`
	Value  float64     `json:"value,omitempty"`
	Bands  int         `json:"bands,omitempty"`
	Wave   int         `json:"wave,omitempty"` // waveform samples wanted per frame
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
	// ExpiresIn is set when this build's Apple Music access runs out soon
	// (days left); an update brings a fresh one.
	ExpiresIn int `json:"expiresIn,omitempty"`
	// Update names a newer release when one is available.
	Update string `json:"update,omitempty"`
	engine.State
}

type Message struct {
	ID       int            `json:"id,omitempty"`
	Error    string         `json:"error,omitempty"`
	Items    []apple.Item   `json:"items,omitempty"`
	Tracks   []apple.Track  `json:"tracks,omitempty"`
	Results  *apple.Results `json:"results,omitempty"`
	State    *State         `json:"state,omitempty"`
	Spectrum []int          `json:"spectrum,omitempty"`
	Wave     []int          `json:"wave,omitempty"`
	Library  bool           `json:"library,omitempty"` // the cached library changed
	Pos      int            `json:"pos,omitempty"`
	IDs      []string       `json:"ids,omitempty"`
	Link     string         `json:"link,omitempty"`
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
		events:  make(chan Message, 256),
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
		// Never block here: replies share this reader. A client that falls
		// this far behind loses frames, and state catches up with the next.
		select {
		case c.events <- m:
		default:
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
