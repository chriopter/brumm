package tui

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/chriopter/brumm/internal/ipc"
)

// feedback is !: a box to write what is wrong. enter opens the browser on
// a GitHub issue filled in with it; tab hands it to Omarchy's agent, which
// looks into the logs first.
type feedback struct {
	input   string
	sending bool
}

type feedbackMsg struct {
	how string
	err error
}

func (m *Model) feedbackKey(msg tea.KeyPressMsg) tea.Cmd {
	f := m.fb
	if f.sending {
		return nil
	}
	switch k := msg.String(); k {
	case "esc":
		m.fb = nil
	case "enter", "tab":
		text := strings.TrimSpace(f.input)
		if text == "" {
			return nil
		}
		f.sending = true
		req := ipc.Request{Cmd: ipc.CmdFeedback, Query: text, Source: "tui"}
		if k == "tab" {
			req.Value = 1
		}
		client := m.client
		return func() tea.Msg {
			reply, err := client.Do(req)
			return feedbackMsg{reply.Link, err}
		}
	case "backspace":
		if r := []rune(f.input); len(r) > 0 {
			f.input = string(r[:len(r)-1])
		}
	case "ctrl+u":
		f.input = ""
	default:
		f.input += msg.Text
	}
	return nil
}

func (m *Model) feedbackDone(msg feedbackMsg) {
	m.fb = nil
	switch {
	case msg.err != nil:
		m.setFlash(msg.err.Error())
	case msg.how == "agent":
		m.setFlash("your agent is looking into it")
	default:
		m.setFlash("one click left: send it in your browser")
	}
}

func (m *Model) feedbackBox() []string {
	f := m.fb
	w := max(34, min(64, m.width-2*margin-10))
	lines := []string{sBold.Render("What is wrong, or what would you like?"), ""}
	// The words, wrapped to the box, the cursor after the last.
	text := f.input
	if !f.sending {
		text += "▏"
	}
	for _, l := range strings.Split(lipgloss.NewStyle().Width(w).Render(text), "\n") {
		lines = append(lines, strings.TrimRight(l, " "))
	}
	lines = append(lines, "")
	if f.sending {
		lines = append(lines, sDim.Render("opening…"))
	} else {
		lines = append(lines, sDim.Render("it becomes a public issue on GitHub"),
			sKey.Render("enter")+sDim.Render(" open on GitHub  ")+sKey.Render("tab")+sDim.Render(" my agent looks into it  ")+sKey.Render("esc")+sDim.Render(" cancel"))
	}
	for _, l := range lines {
		w = max(w, lipgloss.Width(l))
	}
	for i := range lines {
		lines[i] = " " + fit(lines[i], w)
	}
	lines = append(append([]string{""}, lines...), "")
	return strings.Split(box("feedback", false, "", lines, w+4, len(lines)+2), "\n")
}
