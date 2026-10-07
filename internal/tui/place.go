package tui

import (
	"encoding/json"
	"strings"

	tea "charm.land/bubbletea/v2"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/ipc"
)

// A place is where a player is — the section, the views opened in it, the
// selected row and the search — so that g opens the other one right
// there. The daemon keeps it (CmdPlace) without reading it; the window
// (gui/qml/Store.qml) writes and reads the same shape.
type place struct {
	Section     int         `json:"section"`
	SectionName string      `json:"sectionName,omitempty"`
	PlayerView  bool        `json:"playerView,omitempty"`
	Views       []placeView `json:"views,omitempty"` // the stack above the section's root
	Sel         placeSel    `json:"sel"`
	Query       string      `json:"query,omitempty"`
}

// placeView is an opened view, enough to open it again.
type placeView struct {
	Key   string      `json:"key"`
	Title string      `json:"title"`
	Item  *apple.Item `json:"item,omitempty"`
}

// placeSel is the selected row: by its song's or item's id, else its index.
type placeSel struct {
	ID    string `json:"id,omitempty"`
	Index int    `json:"index"`
}

// placeMsg is the daemon's answer on start: nil when no place was left.
type placeMsg struct{ p *place }

// here is the place the player is at.
func (m *Model) here() place {
	p := place{Section: int(m.section), SectionName: sectionNames[m.section], PlayerView: m.playerView, Query: m.query}
	for _, v := range m.stack()[1:] {
		if v.item != nil {
			p.Views = append(p.Views, placeView{v.key, v.title, v.item})
		}
	}
	v := m.cur()
	p.Sel.Index = v.sel
	if v.sel < len(v.rows) {
		switch r := v.rows[v.sel]; {
		case r.track != nil:
			p.Sel.ID = r.track.ID
		case r.item != nil:
			p.Sel.ID = r.item.ID
		}
	}
	return p
}

// leavePlace tells the daemon where the player is, for the other one.
func leavePlace(client *ipc.Client, p place) {
	if b, err := json.Marshal(p); err == nil && client != nil {
		_, _ = client.Do(ipc.Request{Cmd: ipc.CmdPlace, Place: b})
	}
}

// askPlace asks the daemon for a place left by the other player, and
// takes it off, so a later start opens as ever.
func (m *Model) askPlace() tea.Cmd {
	client := m.client
	return func() tea.Msg {
		reply, err := client.Do(ipc.Request{Cmd: ipc.CmdPlace})
		var p *place
		if err != nil || len(reply.Place) == 0 || json.Unmarshal(reply.Place, &p) != nil || p == nil {
			return placeMsg{}
		}
		_, _ = client.Do(ipc.Request{Cmd: ipc.CmdPlace, Place: json.RawMessage("null")})
		return placeMsg{p}
	}
}

// gotPlace opens the place left, instead of going to what plays; with
// none, brumm resumes as ever.
func (m *Model) gotPlace(msg placeMsg) tea.Cmd {
	m.placing = false
	if msg.p == nil {
		return m.maybeResume()
	}
	m.resumed = true
	return m.goPlace(*msg.p)
}

// goPlace opens section, views and selection as p has them, loading what
// is not yet.
func (m *Model) goPlace(p place) tea.Cmd {
	s := section(max(0, min(p.Section, int(numSections)-1)))
	for i, name := range sectionNames {
		if name == p.SectionName {
			s = section(i)
			break
		}
	}
	m.playerView = p.PlayerView
	m.query = p.Query
	if q := strings.TrimSpace(p.Query); len([]rune(q)) >= 2 {
		m.stacks[secSearch] = []*view{{title: "Search", key: "search:" + q}}
	}
	stack := m.stacks[s][:1]
	for _, pv := range p.Views {
		if pv.Item != nil && pv.Key != "" {
			stack = append(stack, &view{title: pv.Title, key: pv.Key, item: pv.Item})
		}
	}
	m.stacks[s], m.section, m.searching, m.filtering = stack, s, false, false
	top := stack[len(stack)-1]
	top.sel, top.want = max(0, p.Sel.Index), p.Sel.ID
	var cmds []tea.Cmd
	for _, v := range stack {
		if s == secQueue {
			v.loaded = false // always fresh
		}
		if !v.loaded {
			cmds = append(cmds, m.load(v))
		} else if v == top {
			m.selectSong(v, v.want)
			v.want = ""
		}
	}
	return tea.Batch(cmds...)
}
