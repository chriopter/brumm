// Package tui is brumm's terminal interface: a drill-in list on the left
// (playlists, then a playlist's tracks) and a borderless stage on the right
// with the cover, a live spectrum and the playhead.
package tui

import (
	"context"
	"fmt"
	"image"
	"math"
	"net/http"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/art"
	"github.com/chriopter/brumm/internal/ipc"
	"github.com/chriopter/brumm/internal/login"
)

// bands is how many spectrum bands the TUI asks the daemon for.
const bands = 64

type level int

const (
	levelPlaylists level = iota
	levelTracks
)

type (
	eventMsg     ipc.Message
	closedMsg    struct{}
	playlistsMsg struct {
		list []apple.Playlist
		err  error
	}
	tracksMsg struct {
		id     string
		tracks []apple.Track
		err    error
	}
	coverMsg struct {
		url string
		img image.Image
	}
	loginMsg struct{ err error }
	errMsg   struct{ err error }
	tickMsg  struct{}
)

type list struct{ sel, off int }

type Model struct {
	client *ipc.Client
	http   *http.Client

	width, height int
	frame         int

	state   ipc.State
	stateAt time.Time
	spec    []float64

	level     level
	playlists []apple.Playlist
	plErr     error
	loadingPL bool
	pl        list
	tracks    map[string][]apple.Track
	trErr     map[string]error
	loadingTR map[string]bool
	tr        list

	coverURL string
	cover    image.Image
	rendered map[art.Size][]string

	flash   string // transient message in the header
	flashAt time.Time
	help    bool
	geo     geometry
}

func New(client *ipc.Client, initial ipc.State) *Model {
	return &Model{
		client:    client,
		http:      &http.Client{Timeout: 10 * time.Second},
		state:     initial,
		stateAt:   time.Now(),
		tracks:    map[string][]apple.Track{},
		trErr:     map[string]error{},
		loadingTR: map[string]bool{},
		rendered:  map[art.Size][]string{},
	}
}

func (m *Model) Init() tea.Cmd {
	return tea.Batch(m.listen(), tick(), m.maybeLoadPlaylists(), m.maybeFetchCover())
}

func tick() tea.Cmd {
	return tea.Tick(100*time.Millisecond, func(time.Time) tea.Msg { return tickMsg{} })
}

func (m *Model) listen() tea.Cmd {
	return func() tea.Msg {
		msg, ok := <-m.client.Events()
		if !ok {
			return closedMsg{}
		}
		return eventMsg(msg)
	}
}

// send fires a command at the daemon; failures surface in the footer.
func (m *Model) send(r ipc.Request) tea.Cmd {
	return func() tea.Msg {
		if _, err := m.client.Do(r); err != nil {
			return errMsg{err}
		}
		return nil
	}
}

// maybeLoadPlaylists asks the daemon for the library. Its cache answers
// at once, even while the player itself is still starting.
func (m *Model) maybeLoadPlaylists() tea.Cmd {
	if m.playlists != nil || m.loadingPL || m.state.Status == ipc.StatusLoggedOut {
		return nil
	}
	m.loadingPL = true
	return func() tea.Msg {
		r, err := m.client.Do(ipc.Request{Cmd: ipc.CmdPlaylists})
		return playlistsMsg{r.Playlists, err}
	}
}

func (m *Model) loadTracks(id string) tea.Cmd {
	if _, ok := m.tracks[id]; ok || m.loadingTR[id] {
		return nil
	}
	m.loadingTR[id] = true
	return func() tea.Msg {
		r, err := m.client.Do(ipc.Request{Cmd: ipc.CmdTracks, Playlist: id})
		return tracksMsg{id, r.Tracks, err}
	}
}

func (m *Model) maybeFetchCover() tea.Cmd {
	url := m.state.Artwork
	if url == m.coverURL {
		return nil
	}
	m.coverURL, m.cover, m.rendered = url, nil, map[art.Size][]string{}
	if url == "" {
		return nil
	}
	client := m.http
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		img, _ := art.Fetch(ctx, client, url)
		return coverMsg{url, img}
	}
}

func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
	case tickMsg:
		m.frame++
		if m.flash != "" && time.Since(m.flashAt) > 4*time.Second {
			m.flash = ""
		}
		return m, tick()
	case eventMsg:
		cmds := []tea.Cmd{m.listen()}
		if msg.State != nil {
			m.state, m.stateAt = *msg.State, time.Now()
			cmds = append(cmds, m.maybeLoadPlaylists(), m.maybeFetchCover())
		}
		if msg.Spectrum != nil {
			m.feedSpectrum(msg.Spectrum)
		}
		if msg.Library {
			// Refreshed in the background: reload from the daemon's cache.
			m.playlists, m.plErr = nil, nil
			m.tracks, m.trErr = map[string][]apple.Track{}, map[string]error{}
			cmds = append(cmds, m.maybeLoadPlaylists())
			if p := m.currentPlaylist(); p != nil {
				cmds = append(cmds, m.loadTracks(p.ID))
			}
		}
		return m, tea.Batch(cmds...)
	case closedMsg:
		m.state = ipc.State{Status: ipc.StatusError, Message: "the brumm daemon stopped"}
		return m, nil
	case playlistsMsg:
		m.loadingPL = false
		m.plErr = msg.err
		if msg.err == nil {
			m.playlists = msg.list
			if m.playlists == nil {
				m.playlists = []apple.Playlist{}
			}
			m.pl.sel = min(m.pl.sel, max(0, len(m.playlists)-1))
			if p := m.currentPlaylist(); p != nil {
				return m, m.loadTracks(p.ID) // preload what's under the cursor
			}
		}
	case tracksMsg:
		delete(m.loadingTR, msg.id)
		m.trErr[msg.id] = msg.err
		if msg.err == nil {
			if msg.tracks == nil {
				msg.tracks = []apple.Track{}
			}
			m.tracks[msg.id] = msg.tracks
		}
	case coverMsg:
		if msg.url == m.coverURL {
			m.cover = msg.img
		}
	case loginMsg:
		if msg.err != nil {
			m.setFlash("sign-in failed: " + msg.err.Error())
			return m, nil
		}
		m.setFlash("signed in")
		m.playlists, m.plErr = nil, nil
		return m, m.send(ipc.Request{Cmd: ipc.CmdReload})
	case errMsg:
		m.setFlash(msg.err.Error())
	case tea.KeyPressMsg:
		return m, m.key(msg.String())
	case tea.MouseClickMsg:
		return m, m.click(msg.Mouse())
	case tea.MouseWheelMsg:
		if msg.Button == tea.MouseWheelUp {
			return m, m.move("up")
		}
		return m, m.move("down")
	}
	return m, nil
}

func (m *Model) setFlash(s string) { m.flash, m.flashAt = s, time.Now() }

// feedSpectrum eases the bars: they jump up at once and fall back slowly,
// which reads as motion rather than flicker.
func (m *Model) feedSpectrum(in []int) {
	if len(m.spec) != len(in) {
		m.spec = make([]float64, len(in))
	}
	for i, v := range in {
		f := float64(v) / 255
		m.spec[i] = max(f, m.spec[i]*0.86)
	}
}

// position extrapolates the playhead between state updates.
func (m *Model) position() float64 {
	p := m.state.Pos
	if m.state.Playing {
		p += time.Since(m.stateAt).Seconds()
	}
	if m.state.Dur > 0 {
		p = min(p, m.state.Dur)
	}
	return p
}

func (m *Model) currentPlaylist() *apple.Playlist {
	if m.pl.sel < len(m.playlists) {
		return &m.playlists[m.pl.sel]
	}
	return nil
}

func (m *Model) key(k string) tea.Cmd {
	switch k {
	case "q", "ctrl+c":
		return tea.Quit
	case "Q":
		return tea.Sequence(m.send(ipc.Request{Cmd: ipc.CmdQuit}), tea.Quit)
	case "?":
		m.help = !m.help
		return nil
	case "L":
		return func() tea.Msg {
			return loginMsg{login.Run(context.Background(), func(string) {})}
		}
	case "space", " ":
		return m.send(ipc.Request{Cmd: ipc.CmdToggle})
	case "n":
		return m.send(ipc.Request{Cmd: ipc.CmdNext})
	case "p", "b":
		return m.send(ipc.Request{Cmd: ipc.CmdPrev})
	case "left", "right":
		delta := 10.0
		if k == "left" {
			delta = -10
		}
		return m.send(ipc.Request{Cmd: ipc.CmdSeek, Value: max(0, m.position()+delta)})
	case "+", "=", "-":
		delta := 0.05
		if k == "-" {
			delta = -0.05
		}
		v := min(1, max(0, m.state.Volume+delta))
		m.state.Volume = v // feel immediate; the daemon confirms
		m.setFlash(fmt.Sprintf("volume %d%%", int(math.Round(v*100))))
		return m.send(ipc.Request{Cmd: ipc.CmdVolume, Value: v})
	case "esc", "h", "backspace":
		if m.help {
			m.help = false
		}
		m.level = levelPlaylists
		return nil
	case "enter", "l":
		return m.open()
	case "c":
		return m.jumpToPlaying()
	}
	return m.move(k)
}

// open drills into the selected playlist, or plays the selected track.
func (m *Model) open() tea.Cmd {
	p := m.currentPlaylist()
	if p == nil {
		return nil
	}
	if m.level == levelPlaylists {
		m.level, m.tr = levelTracks, list{}
		if p.ID == m.state.Playlist && m.state.Index >= 0 {
			m.tr.sel = m.state.Index
		}
		return m.loadTracks(p.ID)
	}
	if len(m.tracks[p.ID]) == 0 {
		return nil
	}
	return m.send(ipc.Request{Cmd: ipc.CmdPlay, Playlist: p.ID, Index: m.tr.sel})
}

// jumpToPlaying opens the playing playlist with the current track selected.
func (m *Model) jumpToPlaying() tea.Cmd {
	for i, p := range m.playlists {
		if p.ID == m.state.Playlist {
			m.pl.sel, m.level, m.tr = i, levelTracks, list{sel: max(0, m.state.Index)}
			return m.loadTracks(p.ID)
		}
	}
	return nil
}

func (m *Model) move(k string) tea.Cmd {
	l, n := &m.pl, len(m.playlists)
	if m.level == levelTracks {
		l = &m.tr
		n = 0
		if p := m.currentPlaylist(); p != nil {
			n = len(m.tracks[p.ID])
		}
	}
	if n == 0 {
		return nil
	}
	page := max(1, m.listRows()-1)
	switch k {
	case "up", "k":
		l.sel--
	case "down", "j":
		l.sel++
	case "pgup", "ctrl+u":
		l.sel -= page
	case "pgdown", "ctrl+d":
		l.sel += page
	case "g", "home":
		l.sel = 0
	case "G", "end":
		l.sel = n - 1
	default:
		return nil
	}
	l.sel = max(0, min(l.sel, n-1))
	if m.level == levelPlaylists {
		return m.loadTracks(m.playlists[l.sel].ID) // preload before it is opened
	}
	return nil
}

// click maps a mouse click onto the layout recorded by the last render.
// A playlist opens on click; a track is selected on the first click and
// played on the second. Right click goes back.
func (m *Model) click(ms tea.Mouse) tea.Cmd {
	g := m.geo
	if ms.Button == tea.MouseRight {
		m.level = levelPlaylists
		return nil
	}
	if ms.Button != tea.MouseLeft {
		return nil
	}
	switch {
	case g.crumb.has(ms.X, ms.Y):
		m.level = levelPlaylists
	case g.play.has(ms.X, ms.Y):
		return m.send(ipc.Request{Cmd: ipc.CmdToggle})
	case g.prev.has(ms.X, ms.Y):
		return m.send(ipc.Request{Cmd: ipc.CmdPrev})
	case g.next.has(ms.X, ms.Y):
		return m.send(ipc.Request{Cmd: ipc.CmdNext})
	case g.bar.has(ms.X, ms.Y) && m.state.Dur > 0:
		f := float64(ms.X-g.bar.x0) / float64(max(1, g.bar.x1-g.bar.x0-1))
		return m.send(ipc.Request{Cmd: ipc.CmdSeek, Value: f * m.state.Dur})
	case g.list.has(ms.X, ms.Y):
		idx := g.off + (ms.Y-g.list.y0)/max(1, g.rowH)
		if m.level == levelPlaylists {
			if idx >= len(m.playlists) {
				return nil
			}
			m.pl.sel = idx
			return m.open()
		}
		p := m.currentPlaylist()
		if p == nil || idx >= len(m.tracks[p.ID]) {
			return nil
		}
		again := idx == m.tr.sel
		m.tr.sel = idx
		if again {
			return m.open()
		}
	}
	return nil
}
