package tui

import (
	"fmt"
	"math"
	"slices"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/art"
	"github.com/chriopter/brumm/internal/ipc"
)

// ── up next ─────────────────────────────────────────────────────────────

// Room to spare under the player — a cover held back by the width, a
// tall window — shows what comes next: a row of the next covers in the
// queue, one per album, so while an album plays through it shows the ones
// after it. A click plays from there. The queue is asked for only when the
// song or the queue changes; the row is drawn once and then only spliced.
// When only the rest of the playing album comes, its songs show instead,
// one line each.

const (
	nextMax  = 5   // covers in the row at most
	nextFour = 4   // a cover is at most a quarter of the row, however few come
	nextLook = 150 // queue songs looked through for them
	nextMinH = 4   // rows a cover needs to be worth showing
	songMax  = 8   // songs listed at most, when no other cover comes
)

// nextSong is a song coming up, listed when no other cover does.
type nextSong struct {
	title, artist string // artist only when not the playing one's
	num           int
	dur           float64
	pos           int
}

// nextList is the list as last drawn, for drawing it again as it is.
type nextList struct {
	gen, w, n int
	rows      []string // w wide, then "+N more" when more follow
}

// nextUp is a cover coming up and the first queue position that shows it.
type nextUp struct {
	url, name, artist string
	pos               int
}

// nextKey is what the queue ahead depends on.
type nextKey struct {
	id            string
	index, length int
	shuffle       bool
}

type nextMsg struct {
	seq    int
	tracks []apple.Track
	pos    int
	ok     bool
}

// nextStrip is the row as last drawn, for drawing it again as it is.
type nextStrip struct {
	gen, w, tw, th, n int
	style             string
	done              bool     // every cover was there: nothing to look up again
	rows              []string // w wide
	label             string   // the line above them
	labelW            int
	kitty             []string // its kitty images, kept from eviction while shown
}

// fetchNext asks the daemon for the queue ahead when it may have changed
// since last asked, or at once with force.
func (m *Model) fetchNext(force bool) tea.Cmd {
	st := m.state
	if st.ID == "" || m.client == nil {
		m.setNext(nil, nil, 0)
		return nil
	}
	k := nextKey{st.ID, st.Index, st.Length, st.Shuffle}
	if k == m.nextAsked && !force {
		return nil
	}
	m.nextAsked = k
	m.nextSeq++
	seq, client := m.nextSeq, m.client
	return func() tea.Msg {
		reply, err := client.Do(ipc.Request{Cmd: ipc.CmdQueue, Value: nextLook})
		if err != nil {
			return nextMsg{seq: seq}
		}
		return nextMsg{seq, reply.Tracks, reply.Pos, true}
	}
}

// gotNext takes the queue's answer: its distinct covers after the
// playing song's, whose images are then fetched; with none, its songs.
func (m *Model) gotNext(msg nextMsg) tea.Cmd {
	if msg.seq != m.nextSeq {
		return nil
	}
	if !msg.ok || msg.pos < 0 { // the player was not ready: ask again on its next change
		m.nextAsked = nextKey{}
		m.setNext(nil, nil, 0)
		return nil
	}
	next := upcoming(msg.tracks, msg.pos, m.coverURL)
	var songs []nextSong
	if next == nil {
		songs = songsAhead(msg.tracks, msg.pos)
	}
	m.setNext(next, songs, max(0, len(msg.tracks)-1))
	var cmds []tea.Cmd
	for _, n := range next {
		if m.covers[n.url] == nil {
			cmds = append(cmds, m.fetchCover(n.url))
		}
	}
	return tea.Batch(cmds...)
}

// upcoming is the queue's covers after its first song (the playing one),
// each once: songs of one album share one cover, and the one on the stage
// is not shown again.
func upcoming(tracks []apple.Track, pos int, playing string) []nextUp {
	seen := map[string]bool{playing: true}
	album := func(t apple.Track) string { return "album:" + t.Album + "\x00" + t.Artist }
	var out []nextUp
	for i, t := range tracks {
		if i == 0 {
			seen[t.Artwork], seen[album(t)] = true, t.Album != ""
			continue
		}
		if t.Artwork == "" || seen[t.Artwork] || (t.Album != "" && seen[album(t)]) {
			continue
		}
		seen[t.Artwork], seen[album(t)] = true, t.Album != ""
		name := t.Album
		if name == "" {
			name = t.Title
		}
		out = append(out, nextUp{t.Artwork, name, t.Artist, pos + i})
		if len(out) == nextMax {
			break
		}
	}
	return out
}

// songsAhead is the queue's first songs after the playing one.
func songsAhead(tracks []apple.Track, pos int) []nextSong {
	if len(tracks) < 2 {
		return nil
	}
	out := make([]nextSong, 0, min(songMax, len(tracks)-1))
	for i, t := range tracks[1:min(len(tracks), songMax+1)] {
		s := nextSong{t.Title, t.Artist, t.Number, t.Duration, pos + i + 1}
		if s.num == 0 {
			s.num = i + 1 // not on an album: its place in the queue
		}
		if t.Artist == tracks[0].Artist {
			s.artist = ""
		}
		out = append(out, s)
	}
	return out
}

// setNext replaces the covers or songs coming up and the count of songs
// ahead, if they changed.
func (m *Model) setNext(next []nextUp, songs []nextSong, ahead int) {
	if slices.Equal(next, m.next) && slices.Equal(songs, m.songs) && (ahead == m.ahead || songs == nil) {
		return
	}
	m.next, m.songs, m.ahead = next, songs, ahead
	m.nextGen++
}

// nextFit sizes the row for a column colW wide with free rows under the
// player: how many covers and each one's size; n is 0 when none fits. A
// gap and the label take two rows. In a narrow column fewer come, each
// still nextMinH rows high; held back by the height, they are smaller
// and one more fits.
func (m *Model) nextFit(colW, free int) (n, tw, th int) {
	if len(m.next) == 0 {
		return 0, 0, 0
	}
	tw = (colW - 2*(nextFour-1)) / nextFour
	th = int(float64(tw) / m.cellAspect)
	if th < nextMinH {
		th = nextMinH
		tw = int(math.Ceil(float64(th) * m.cellAspect))
	}
	if th > free-2 {
		th = free - 2
		tw = int(float64(th) * m.cellAspect)
	}
	if th < nextMinH || tw < 1 || tw > colW {
		return 0, 0, 0
	}
	return min(len(m.next), nextMax, (colW+2)/(tw+2)), tw, th
}

// upNext is the stage's lines for the row, none when it does not fit.
func (m *Model) upNext(colW, free int) []stageLine {
	if len(m.next) == 0 {
		return m.upSongs(colW, free)
	}
	n, tw, th := m.nextFit(colW, free)
	if n == 0 {
		return nil
	}
	rows := m.nextRows(colW, n, tw, th)
	out := []stageLine{{}, {text: m.nextDraw.label, width: m.nextDraw.labelW}}
	for i, r := range rows {
		l := stageLine{text: r, width: colW}
		if i == 0 {
			l.hit = func(x, y int) {
				for j := range n {
					x0 := x + j*(tw+2)
					m.geo.upnext[j] = rect{x0, y, x0 + tw, y + th}
				}
			}
		}
		out = append(out, l)
	}
	return out
}

// nextRows is the row of covers, colW wide: drawn again only when the
// covers, the size or the style change, or one was still missing.
func (m *Model) nextRows(colW, n, tw, th int) []string {
	d := &m.nextDraw
	style := m.opts.cover()
	if d.done && d.gen == m.nextGen && d.w == colW && d.tw == tw && d.th == th && d.n == n && d.style == style {
		for _, k := range d.kitty {
			img := m.kitty[k]
			if img == nil {
				d.done = false // evicted after all: draw it again
				return m.nextRows(colW, n, tw, th)
			}
			m.kittyClock++
			img.used = m.kittyClock
		}
		return d.rows
	}
	first := m.next[0]
	label := ansi.Truncate(sDim.Render("up next  ")+first.name+sDim.Render("  "+first.artist), colW, "…")
	*d = nextStrip{gen: m.nextGen, w: colW, tw: tw, th: th, n: n, style: style, done: true, rows: make([]string, th), kitty: d.kitty[:0],
		label: label, labelW: lipgloss.Width(label)}
	size := art.Size{Width: tw, Height: th}
	var b [nextMax][]string
	for i := range n {
		url := m.next[i].url
		var ok bool
		if style == coverOriginal && m.covers[url] != nil {
			if b[i], ok = m.kittyLines(url, size); ok {
				d.kitty = append(d.kitty, url+"@"+sizeKey(size))
			}
		}
		if !ok {
			b[i], ok = m.thumbs[m.thumbKey(url, size)]
		}
		if !ok {
			b[i], d.done = placeholder(size, icAlbum), false
		}
	}
	gap := "  "
	tail := strings.Repeat(" ", colW-n*tw-(n-1)*len(gap))
	for y := range th {
		var sb strings.Builder
		for i := range n {
			if i > 0 {
				sb.WriteString(gap)
			}
			sb.WriteString(b[i][y])
		}
		sb.WriteString(tail)
		d.rows[y] = sb.String()
	}
	return d.rows
}

// nextThumbWant is an up-next cover whose loaded image is not drawn at
// the row's size yet, for drawing in the background.
func (m *Model) nextThumbWant() thumbReq {
	d := m.nextDraw
	if d.done || d.th == 0 {
		return thumbReq{}
	}
	size := art.Size{Width: d.tw, Height: d.th}
	for i := range min(d.n, len(m.next)) {
		url := m.next[i].url
		if key := m.thumbKey(url, size); m.covers[url] != nil && m.thumbs[key] == nil {
			return thumbReq{key, url, size}
		}
	}
	return thumbReq{}
}

// upSongs is the stage's lines for the songs coming up, as many as fit
// under a gap and the label, the last one saying how many more follow;
// none when fewer than two fit.
func (m *Model) upSongs(colW, free int) []stageLine {
	k := min(free-2, songMax)
	if len(m.songs) == 0 || k < 2 {
		return nil
	}
	n := min(len(m.songs), k)
	if m.ahead > n && n == k {
		n-- // room for the line saying more follow
	}
	rows := m.songRows(colW, n)
	out := make([]stageLine, 0, len(rows)+2)
	out = append(out, stageLine{}, stageLine{text: songLabel, width: 7})
	for i, r := range rows {
		l := stageLine{text: r, width: colW}
		if i == 0 {
			l.hit = func(x, y int) { m.geo.upsongs = rect{x, y, x + colW, y + n} }
		}
		out = append(out, l)
	}
	return out
}

var songLabel = sDim.Render("up next")

// songRows is the list, colW wide: drawn again only when the songs, the
// width or how many fit change. Like the list's rows, calmer: dim number,
// the title, the artist when not the playing one's, the time on the right.
func (m *Model) songRows(colW, n int) []string {
	d := &m.songDraw
	if d.gen == m.nextGen && d.w == colW && d.n == n && d.rows != nil {
		return d.rows
	}
	*d = nextList{gen: m.nextGen, w: colW, n: n, rows: make([]string, 0, n+1)}
	for _, s := range m.songs[:n] {
		text := sDim.Render(fmt.Sprintf("%2d  ", s.num)) + s.title
		if s.artist != "" {
			text += "  " + sDim.Render(s.artist)
		}
		dur := ""
		if s.dur > 0 {
			dur = clock(s.dur)
		}
		avail := colW - 2 - len(dur)
		if dur == "" || avail < 1 { // no time, or too narrow for it
			d.rows = append(d.rows, fit(text, colW))
			continue
		}
		d.rows = append(d.rows, fit(text, avail)+"  "+sDim.Render(dur))
	}
	if more := m.ahead - n; more > 0 {
		d.rows = append(d.rows, fit(sDim.Render(fmt.Sprintf("    +%d more", more)), colW))
	}
	return d.rows
}

// nextAt is the queue position of the cover or song at x, y, or -1.
func (m *Model) nextAt(x, y int) int {
	for i, r := range m.geo.upnext {
		if r.has(x, y) && i < len(m.next) {
			return m.next[i].pos
		}
	}
	if r := m.geo.upsongs; r.has(x, y) && y-r.y0 < len(m.songs) {
		return m.songs[y-r.y0].pos
	}
	return -1
}

// nextClick plays from the cover at x, y, if there is one.
func (m *Model) nextClick(x, y int) (tea.Cmd, bool) {
	if pos := m.nextAt(x, y); pos >= 0 {
		return m.send(ipc.Request{Cmd: ipc.CmdJump, Value: float64(pos)}), true
	}
	return nil, false
}
