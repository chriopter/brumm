package tui

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/chriopter/brumm/internal/ipc"
)

// updatePopup answers U: it checks for a release right then and says what
// it found — the newest already, or a newer one to install with enter.
type updatePopup struct {
	checking   bool
	installing bool
	installed  bool // done: offer to restart into it now
	current    string
	latest     string
	newer      bool
	notes      []string // what the newer release says is new
	err        string
}

// The update box shows this much of the release's notes.
const (
	maxNotes  = 10
	noteWidth = 64
)

type updateMsg struct {
	reply ipc.Message
	err   error
}

type installedMsg struct{ err error }

func (m *Model) checkUpdate() tea.Cmd {
	if m.upd != nil && m.upd.newer && !m.upd.installing {
		return m.installUpdate()
	}
	m.upd = &updatePopup{checking: true}
	m.optOpen, m.pick, m.help = false, nil, false
	client := m.client
	return func() tea.Msg {
		reply, err := client.Do(ipc.Request{Cmd: ipc.CmdUpdate, Value: 2})
		return updateMsg{reply, err}
	}
}

func (m *Model) updateChecked(msg updateMsg) {
	if m.upd == nil {
		return
	}
	u := m.upd
	u.checking, u.current, u.latest, u.newer = false, msg.reply.Version, msg.reply.Link, msg.reply.Pos == 1
	u.notes = msg.reply.Notes
	if msg.err != nil {
		u.err = msg.err.Error()
	}
}

func (m *Model) installUpdate() tea.Cmd {
	m.upd.installing = true
	client := m.client
	return func() tea.Msg {
		_, err := client.Do(ipc.Request{Cmd: ipc.CmdUpdate, Value: 1})
		return installedMsg{err}
	}
}

func (m *Model) updateInstalled(msg installedMsg) {
	if msg.err != nil {
		m.upd = nil
		m.setFlash("update: " + msg.err.Error())
		return
	}
	if m.upd == nil {
		m.upd = &updatePopup{}
	}
	m.upd.installing, m.upd.installed = false, true
}

// restartNow has the player restart into the new release at once; it
// picks up where it was, and the window follows (closedMsg).
func (m *Model) restartNow() tea.Cmd {
	m.upd = nil
	m.setFlash("restarting…")
	client := m.client
	return func() tea.Msg {
		_, err := client.Do(ipc.Request{Cmd: ipc.CmdUpdate, Value: 3})
		if err != nil {
			return errMsg{err}
		}
		return nil
	}
}

// updateKey: enter installs a newer release, anything else closes.
func (m *Model) updateKey(k string) tea.Cmd {
	u := m.upd
	switch {
	case u.installing:
		return nil
	case u.installed && (k == "enter" || k == "U"):
		return m.restartNow()
	case u.installed:
		if k == "esc" || k == "q" || k == "space" || k == " " {
			m.upd = nil
			m.setFlash("brumm restarts into it at the next pause or song change")
		}
		return nil
	case (k == "enter" || k == "U") && u.newer:
		return m.installUpdate()
	case k == "enter", k == "esc", k == "q", k == "U", k == "space", k == " ":
		m.upd = nil
	}
	return nil
}

// updateClick: a click outside closes, on the popup installs if it can.
func (m *Model) updateClick(x, y int) tea.Cmd {
	if !m.geo.options.has(x, y) {
		if !m.upd.installing {
			m.upd = nil
		}
		return nil
	}
	return m.updateKey("enter")
}

func (m *Model) updateBox() []string {
	u := m.upd
	var lines []string
	switch {
	case u.checking:
		lines = []string{sDim.Render(spinner[m.frame%len(spinner)] + "  looking for a new release…")}
	case u.installing:
		lines = []string{sDim.Render(spinner[m.frame%len(spinner)] + "  installing " + u.latest + "…")}
	case u.installed:
		name := u.latest
		if name == "" {
			name = "the update"
		}
		lines = []string{sPlays.Render("✓") + " " + sBold.Render(name) + " is installed", sDim.Render("the music goes on where it is"), "",
			sKey.Render("enter") + sDim.Render(" restart now  ") + sKey.Render("esc") + sDim.Render(" at the next pause")}
	case u.err != "":
		lines = []string{sErr.Render(u.err), "", sKey.Render("esc") + sDim.Render(" close")}
	case u.newer:
		lines = []string{sBold.Render(u.latest) + " is available", sDim.Render("you have " + u.current), ""}
		for _, n := range u.notes[:min(len(u.notes), maxNotes)] {
			for i, l := range wrap(n, min(noteWidth, max(34, m.width-12)), 3) {
				if i > 0 {
					l = "   " + l // under the words, past the emoji
				}
				lines = append(lines, l)
			}
		}
		if len(u.notes) > maxNotes {
			lines = append(lines, sDim.Render("… and more"))
		}
		if len(u.notes) > 0 {
			lines = append(lines, "")
		}
		lines = append(lines,
			sKey.Render("enter")+sDim.Render(" install  ")+sKey.Render("esc")+sDim.Render(" later"))
	default:
		lines = []string{sPlays.Render("✓") + " " + sBold.Render(u.current) + " is the newest version", "",
			sKey.Render("esc") + sDim.Render(" close")}
	}
	w := 34
	for _, l := range lines {
		w = max(w, lipgloss.Width(l)+2)
	}
	for i := range lines {
		lines[i] = " " + fit(lines[i], w)
	}
	lines = append(append([]string{""}, lines...), "")
	return strings.Split(box("update", false, "", lines, w+4, len(lines)+2), "\n")
}
