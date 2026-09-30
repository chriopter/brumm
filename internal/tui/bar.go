package tui

import (
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/chriopter/brumm/internal/ipc"
	"github.com/chriopter/brumm/internal/update"
)

// The Omarchy bar widget: a switch in the options, and a question on the
// first start when it is off. The installer turns it on for a fresh
// install, so the question is for those who turned it off or never had it.

// hasOmarchy says whether omarchy is here to manage the bar; a variable,
// so tests decide.
var hasOmarchy = update.HasOmarchy

type (
	barMsg    struct{ on, ok bool } // what the Omarchy shell says; ok false when it cannot tell
	barSetMsg struct {
		on  bool
		err error
	}
)

// barKnown is the switch's state until the shell answers: as last set or
// seen here, else on when the widget is installed.
func barKnown(o options) bool {
	if o.Bar != "" {
		return o.Bar == "on"
	}
	return update.PluginLinked()
}

func onOff(on bool) string { return map[bool]string{true: "on", false: "off"}[on] }

// readBar asks the Omarchy shell whether the widget is on, off the loop.
func (m *Model) readBar() tea.Cmd {
	if !m.hasOmarchy {
		return nil
	}
	return func() tea.Msg {
		on, ok := update.PluginEnabled()
		return barMsg{on, ok}
	}
}

// barRead takes the shell's word, and asks once when the widget is off.
func (m *Model) barRead(msg barMsg) {
	if !msg.ok {
		return
	}
	m.barOn = msg.on
	if m.opts.Bar != onOff(msg.on) {
		m.opts.Bar = onOff(msg.on)
		m.saveOptions()
	}
	m.barAsk = !msg.on && !m.opts.BarOffered
}

// setBar switches the widget, showing the new state at once; barSet takes
// it back if omarchy refuses.
func (m *Model) setBar(on bool) tea.Cmd {
	m.barOn, m.opts.Bar = on, onOff(on)
	client := m.client
	return func() tea.Msg {
		if client == nil {
			return barSetMsg{on, update.SetPlugin(on)}
		}
		_, err := client.Do(ipc.Request{Cmd: ipc.CmdOptions, Options: map[string]any{"bar": onOff(on)}})
		return barSetMsg{on, err}
	}
}

func (m *Model) barSet(msg barSetMsg) {
	if msg.err != nil {
		m.barOn, m.opts.Bar = !msg.on, onOff(!msg.on)
		m.saveOptions()
		m.setFlash("bar widget: " + msg.err.Error())
		return
	}
	m.setFlash("bar widget " + onOff(msg.on))
}

// barAsking: the question shows once the player is past signing in, and
// never over another popup.
func (m *Model) barAsking() bool {
	return m.barAsk && m.state.Status == ipc.StatusReady && m.upd == nil && m.pick == nil && !m.optOpen && !m.full
}

// barAnswer: whatever the answer, it is not asked again.
func (m *Model) barAnswer(yes bool) tea.Cmd {
	m.barAsk, m.opts.BarOffered = false, true
	var cmd tea.Cmd
	if yes {
		cmd = m.setBar(true)
	}
	m.saveOptions()
	return cmd
}

// barKey: enter or y says yes, esc or n no; other keys wait for an answer.
func (m *Model) barKey(k string) tea.Cmd {
	switch k {
	case "enter", "y":
		return m.barAnswer(true)
	case "esc", "n", "q":
		return m.barAnswer(false)
	}
	return nil
}

// barClick: a click on the question says yes, one outside no.
func (m *Model) barClick(x, y int) tea.Cmd {
	return m.barAnswer(m.geo.options.has(x, y))
}

func (m *Model) barBox() []string {
	lines := []string{sBold.Render("Show brumm in the Omarchy bar?"), sDim.Render("the options can change it later"), "",
		sKey.Render("enter") + sDim.Render(" yes  ") + sKey.Render("esc") + sDim.Render(" no")}
	w := 34
	for _, l := range lines {
		w = max(w, lipgloss.Width(l)+2)
	}
	for i := range lines {
		lines[i] = " " + fit(lines[i], w)
	}
	lines = append(append([]string{""}, lines...), "")
	return strings.Split(box("bar widget", false, "", lines, w+4, len(lines)+2), "\n")
}
