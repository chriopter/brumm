package tui

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/ipc"
)

// selectable rows are songs and things that open or play; headings and
// notes are only read.
func (r row) selectable() bool { return r.track != nil || r.item != nil }

// fill turns a daemon reply into rows: shelves become a heading and their
// contents, plain lists stay plain. An album or playlist opens with its
// facts and Apple's note above the songs.
func fill(v *view, reply ipc.Message) {
	var rows []row
	if it := v.item; it != nil && (it.Kind == apple.KindAlbum || it.Kind == apple.KindPlaylist) {
		if it.Info != "" {
			rows = append(rows, row{note: it.Info})
		}
		if it.Note != "" {
			rows = append(rows, row{note: it.Note})
		}
		if len(rows) > 0 {
			rows = append(rows, row{})
		}
	}
	for s := range reply.Shelves {
		sh := &reply.Shelves[s]
		if len(rows) > 0 {
			rows = append(rows, row{})
		}
		rows = append(rows, row{head: sh.Title})
		for i := range sh.Tracks {
			rows = append(rows, row{track: &sh.Tracks[i]})
		}
		for i := range sh.Items {
			rows = append(rows, row{item: &sh.Items[i]})
		}
	}
	for i := range reply.Items {
		rows = append(rows, row{item: &reply.Items[i]})
	}
	for i := range reply.Tracks {
		rows = append(rows, row{track: &reply.Tracks[i]})
	}
	v.rows, v.loaded, v.err = rows, true, nil
	if v.all != nil { // a filter shows: keep it over the new rows
		v.all = rows
		applyFilter(v)
	}
	v.qpos = max(0, reply.Pos)
	// An album opened from a link has no name yet; its songs carry it.
	if v.item != nil && v.item.Kind == apple.KindAlbum && v.title == "Album" && len(reply.Tracks) > 0 {
		v.title = reply.Tracks[0].Album
	}
	v.sel = min(v.sel, max(0, len(rows)-1))
	v.sel = nearest(v.rows, v.sel, 1)
}

// nearest is the selectable row at i, or the next one in direction dir,
// or failing that the next one the other way.
func nearest(rows []row, i, dir int) int {
	for _, d := range []int{dir, -dir} {
		for j := i; j >= 0 && j < len(rows); j += d {
			if rows[j].selectable() {
				return j
			}
		}
	}
	return max(0, min(i, len(rows)-1))
}

// rateable is what a rating applies to: the selected song, album,
// playlist or station, else the song playing.
func (m *Model) rateable() (apple.Ref, string, bool) {
	v := m.cur()
	if v.sel < len(v.rows) {
		r := v.rows[v.sel]
		if r.track != nil {
			return apple.Ref{Kind: "song", ID: r.track.ID}, r.track.Title, true
		}
		if it := r.item; it != nil {
			switch it.Kind {
			case apple.KindAlbum, apple.KindPlaylist, apple.KindStation:
				return apple.Ref{Kind: it.Kind, ID: it.ID}, it.Name, true
			}
		}
	}
	if m.state.ID != "" && m.state.Preview == nil {
		return apple.Ref{Kind: "song", ID: m.state.ID}, m.state.Title, true
	}
	return apple.Ref{}, "", false
}

// rate toggles a love (apple.Love) or a dislike (apple.Dislike).
func (m *Model) rate(value int) tea.Cmd {
	ref, name, ok := m.rateable()
	if !ok {
		return nil
	}
	if m.rating[ref.ID] == value {
		value = 0
	}
	if value == 0 {
		delete(m.rating, ref.ID)
	} else {
		m.rating[ref.ID] = value
	}
	m.setFlash(map[int]string{apple.Love: "♥ " + name, apple.Dislike: "disliked " + name, 0: name + ": no rating"}[value])
	return m.send(ipc.Request{Cmd: ipc.CmdRate, Refs: []apple.Ref{ref}, Value: float64(value)})
}

type ratingsMsg map[string]int

// fetchRatings asks which songs and albums, playlists and stations of a
// view are loved or disliked.
func (m *Model) fetchRatings(v *view) tea.Cmd {
	var refs []apple.Ref
	for _, r := range v.rows {
		if len(refs) >= 1000 {
			break
		}
		switch {
		case r.track != nil:
			refs = append(refs, apple.Ref{Kind: "song", ID: r.track.ID})
		case r.item != nil && (r.item.Kind == apple.KindAlbum || r.item.Kind == apple.KindPlaylist || r.item.Kind == apple.KindStation):
			refs = append(refs, apple.Ref{Kind: r.item.Kind, ID: r.item.ID})
		}
	}
	if len(refs) == 0 {
		return nil
	}
	client := m.client
	return func() tea.Msg {
		reply, err := client.Do(ipc.Request{Cmd: ipc.CmdRatings, Refs: refs})
		if err != nil {
			return nil
		}
		return ratingsMsg(reply.Ratings)
	}
}

// ratingMark is the heart or thumb shown for id, one cell wide.
func (m *Model) ratingMark(id string) string {
	switch m.rating[id] {
	case apple.Love:
		return sErr.Render("♥")
	case apple.Dislike:
		return sDim.Render(icDislike)
	}
	return " "
}

// playStation starts a station: the selected one, or the one Apple makes
// from the selected song or artist, or from the song playing.
func (m *Model) playStation() tea.Cmd {
	req := ipc.Request{Cmd: ipc.CmdStation}
	v := m.cur()
	switch {
	case v.sel < len(v.rows) && v.rows[v.sel].track != nil:
		req.Start = v.rows[v.sel].track.ID
	case v.sel < len(v.rows) && v.rows[v.sel].item != nil &&
		(v.rows[v.sel].item.Kind == apple.KindStation || v.rows[v.sel].item.Kind == apple.KindArtist):
		it := *v.rows[v.sel].item
		req.Item = &it
	case m.state.ID != "":
		req.Start = m.state.ID
	default:
		m.setFlash("select a song or an artist to start its station")
		return nil
	}
	return m.startStation(req)
}

func (m *Model) startStation(req ipc.Request) tea.Cmd {
	client := m.client
	m.setFlash("tuning in…")
	return func() tea.Msg {
		reply, err := client.Do(req)
		if err != nil {
			return errMsg{err}
		}
		if len(reply.Items) > 0 {
			return flashMsg("▶ " + reply.Items[0].Name)
		}
		return nil
	}
}

// songsOf lists the songs a row stands for: itself, or what is inside.
func songsOf(client *ipc.Client, r row) ([]string, error) {
	if r.track != nil {
		return []string{r.track.ID}, nil
	}
	if r.item == nil || r.item.Kind == apple.KindStation || r.item.Kind == apple.KindTerm {
		return nil, fmt.Errorf("nothing to add here")
	}
	reply, err := client.Do(ipc.Request{Cmd: ipc.CmdOpen, Item: r.item})
	if err != nil {
		return nil, err
	}
	var ids []string
	for _, t := range reply.Tracks {
		ids = append(ids, t.ID)
	}
	for _, s := range reply.Shelves {
		for _, t := range s.Tracks {
			ids = append(ids, t.ID)
		}
	}
	return ids, nil
}

// ── add to playlist ─────────────────────────────────────────────────────

// picker chooses the playlist that songs go to, or names a new one.
type picker struct {
	what    row    // what is being added
	name    string // its name, for the title
	lists   []apple.Item
	loading bool
	sel     int // 0 is "new playlist"
	off     int
	naming  bool
	input   string
}

type pickerListsMsg struct {
	lists []apple.Item
	err   error
}

const pickerRows = 10

// openPicker starts adding the selected song, album or playlist — or the
// song playing — to a playlist.
func (m *Model) openPicker() tea.Cmd {
	v := m.cur()
	p := &picker{loading: true}
	switch {
	case v.sel < len(v.rows) && v.rows[v.sel].track != nil:
		p.what, p.name = v.rows[v.sel], v.rows[v.sel].track.Title
	case v.sel < len(v.rows) && v.rows[v.sel].item != nil &&
		(v.rows[v.sel].item.Kind == apple.KindAlbum || v.rows[v.sel].item.Kind == apple.KindPlaylist):
		p.what, p.name = v.rows[v.sel], v.rows[v.sel].item.Name
	case m.state.ID != "":
		p.what, p.name = row{track: &apple.Track{ID: m.state.ID, Title: m.state.Title}}, m.state.Title
	default:
		m.setFlash("select a song, album or playlist to add")
		return nil
	}
	m.pick, m.optOpen, m.help = p, false, false
	client := m.client
	return func() tea.Msg {
		reply, err := client.Do(ipc.Request{Cmd: ipc.CmdList, List: ipc.ListPlaylists})
		var lists []apple.Item
		for _, it := range reply.Items {
			if it.Editable {
				lists = append(lists, it)
			}
		}
		return pickerListsMsg{lists, err}
	}
}

func (m *Model) pickerKey(msg tea.KeyPressMsg) tea.Cmd {
	p := m.pick
	k := msg.String()
	if p.naming {
		switch k {
		case "esc":
			p.naming = false
		case "enter":
			if strings.TrimSpace(p.input) != "" {
				return m.addToPlaylist("", strings.TrimSpace(p.input))
			}
		case "backspace":
			if r := []rune(p.input); len(r) > 0 {
				p.input = string(r[:len(r)-1])
			}
		case "ctrl+u":
			p.input = ""
		default:
			if t := msg.Text; t != "" {
				p.input += t
			}
		}
		return nil
	}
	n := len(p.lists) + 1
	switch k {
	case "up", "k":
		p.sel = (p.sel + n - 1) % n
	case "down", "j":
		p.sel = (p.sel + 1) % n
	case "enter", "space", " ", "l":
		return m.pickRow(p.sel)
	case "esc", "q", "P", "h":
		m.pick = nil
	}
	return nil
}

// pickRow acts on row i: 0 names a new playlist, the rest add to one.
func (m *Model) pickRow(i int) tea.Cmd {
	p := m.pick
	if i == 0 {
		p.naming, p.input = true, p.name
		return nil
	}
	if i-1 < len(p.lists) {
		return m.addToPlaylist(p.lists[i-1].ID, p.lists[i-1].Name)
	}
	return nil
}

// addToPlaylist sends the picked songs to playlist id, or to a new one.
func (m *Model) addToPlaylist(id, name string) tea.Cmd {
	what, client := m.pick.what, m.client
	m.pick = nil
	m.setFlash("adding to " + name + "…")
	return func() tea.Msg {
		ids, err := songsOf(client, what)
		if err != nil {
			return errMsg{err}
		}
		req := ipc.Request{Cmd: ipc.CmdPlaylist, Start: id, IDs: ids}
		if id == "" {
			req.Query = name
		}
		if _, err := client.Do(req); err != nil {
			return errMsg{err}
		}
		if id == "" {
			return flashMsg("made the playlist " + name)
		}
		return flashMsg(fmt.Sprintf("added %d songs to %s", len(ids), name))
	}
}

// pickerBox draws the picker for the overlay.
func (m *Model) pickerBox() []string {
	p := m.pick
	w := 44
	var lines []string
	if p.naming {
		lines = append(lines, sDim.Render("name of the new playlist"), "",
			sHere.Render("▌")+" "+p.input+sHere.Render("▏"), "",
			sKey.Render("enter")+sDim.Render(" create  ")+sKey.Render("esc")+sDim.Render(" back"))
	} else {
		n := len(p.lists) + 1
		p.off = scroll(p.sel, p.off, pickerRows)
		for i := p.off; i < min(n, p.off+pickerRows); i++ {
			text := sHere.Render("＋") + " new playlist…"
			if i > 0 {
				text = sDim.Render(icPlaylist) + "  " + p.lists[i-1].Name
			}
			gutter := "  "
			if i == p.sel {
				gutter = sHere.Render("▌") + " "
			}
			lines = append(lines, gutter+text)
		}
		switch {
		case p.loading:
			lines = append(lines, sDim.Render("  "+spinner[m.frame%len(spinner)]+" loading your playlists"))
		case len(p.lists) == 0:
			lines = append(lines, sDim.Render("  Apple lets apps add only to playlists"), sDim.Render("  made with an app — like a new one here"))
		}
		lines = append(lines, "", sKey.Render("↑↓")+sDim.Render(" choose  ")+sKey.Render("enter")+sDim.Render(" add  ")+sKey.Render("esc")+sDim.Render(" cancel"))
	}
	for _, l := range lines {
		w = max(w, lipgloss.Width(l)+2)
	}
	w = min(w, m.width-2*margin-4)
	for i := range lines {
		lines[i] = " " + fit(lines[i], w)
	}
	return strings.Split(box("add "+p.name+" to", false, "", lines, w+4, len(lines)+2), "\n")
}

// pickerClick picks the row under the mouse; outside closes the picker.
func (m *Model) pickerClick(x, y int) tea.Cmd {
	if !m.geo.options.has(x, y) {
		m.pick = nil
		return nil
	}
	if m.pick.naming {
		return nil
	}
	if i := y - m.geo.optRow0; i >= 0 && i < min(len(m.pick.lists)+1, pickerRows) {
		m.pick.sel = m.pick.off + i
		return m.pickRow(m.pick.sel)
	}
	return nil
}
