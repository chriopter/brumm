package tui

import (
	"encoding/json"
	"fmt"
	"math"
	"os/exec"
	"reflect"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/art"
	"github.com/chriopter/brumm/internal/config"
	"github.com/chriopter/brumm/internal/ipc"
	"github.com/chriopter/brumm/internal/launch"
)

// Cover styles.
const (
	coverPixel    = "pixel"    // dithered sextant pixel art in a few colors
	coverSmooth   = "smooth"   // true-color half blocks
	coverOriginal = "original" // the real image, through the kitty graphics protocol
)

var coverStyles = []string{coverPixel, coverSmooth, coverOriginal}

// The fullscreen visualizer's frame rates; the renderer allows up to 120.
// 0 is auto: the screen's own refresh rate.
var drawRates = []int{0, 30, 60, 120}

const maxDrawFPS = 120

// drawFPS is the rate the fullscreen visualizer draws at.
func (m *Model) drawFPS() int {
	if r := m.opts.VizFPS; r == 30 || r == 60 || r == 120 {
		return r
	}
	return m.refresh
}

// screenRefresh is the focused monitor's refresh rate, as Hyprland reports
// it, kept between 30 and what the renderer allows; 60 if unknown.
func screenRefresh() int {
	out, err := exec.Command("hyprctl", "monitors", "-j").Output()
	if err != nil {
		return 60
	}
	var mons []struct {
		Focused bool    `json:"focused"`
		Rate    float64 `json:"refreshRate"`
	}
	if json.Unmarshal(out, &mons) != nil {
		return 60
	}
	for _, mon := range mons {
		if mon.Focused && mon.Rate > 0 {
			return max(30, min(maxDrawFPS, int(math.Round(mon.Rate))))
		}
	}
	return 60
}

// fpsLabel names the draw rate for the visualizer's button.
func (m *Model) fpsLabel() string {
	if m.opts.VizFPS == 0 {
		return fmt.Sprintf("auto %d fps", m.refresh)
	}
	return fmt.Sprintf("%d fps", m.opts.VizFPS)
}

// options wraps the saved switches with what the menu needs.
type options struct{ config.Options }

// loadOptions reads them; switches reduce motion took over are written
// back as it, so the bar widget reads it too.
func loadOptions() options {
	o := config.LoadOptions()
	if o.ReduceMotion != o.NoBarScroll {
		_ = o.Save()
	}
	return options{o}
}

func (o options) save() { _ = o.Options.Save() }

// saveOptions keeps the options: the daemon saves them and tells every
// player; without one (tests) they go to the file here.
func (m *Model) saveOptions() {
	if m.client == nil {
		m.opts.save()
		return
	}
	// Only what changed, and in order: a switch set in the window
	// meanwhile is not overwritten by an old copy from here.
	now := m.opts.Fields()
	delete(now, "bar") // switched by setBar alone: it can be refused
	set := map[string]any{}
	for k, v := range now {
		if m.sentOpts == nil || !reflect.DeepEqual(m.sentOpts[k], v) {
			set[k] = v
		}
	}
	m.sentOpts = now
	if len(set) == 0 {
		return
	}
	if m.optQueue == nil {
		m.optQueue = make(chan map[string]any, 32)
		go func(client *ipc.Client, q chan map[string]any) {
			for set := range q {
				_, _ = client.Do(ipc.Request{Cmd: ipc.CmdOptions, Options: set})
			}
		}(m.client, m.optQueue)
	}
	select {
	case m.optQueue <- set:
	default: // 32 behind: the daemon is gone; the next start reads the file
	}
}

// optionsChanged takes options another player set.
func (m *Model) optionsChanged(o config.Options) {
	if o.Cover != m.opts.Cover {
		m.rendered, m.thumbs = map[art.Size][]string{}, map[string][]string{}
	}
	m.opts = options{o}
	m.sentOpts = o.Fields() // what the daemon has now
	delete(m.sentOpts, "bar")
	if m.hasOmarchy && o.Bar != "" {
		m.barOn = o.Bar == "on"
	}
}

// hasKitty: the terminal shows real images, as guessed once at start.
var hasKitty = art.KittySupported()

// cover is the style to draw covers in: original falls back to smooth in
// terminals without kitty graphics.
func (o options) cover() string {
	switch {
	case o.Cover == coverOriginal && !hasKitty:
		return coverSmooth
	case o.Cover == coverSmooth || o.Cover == coverOriginal:
		return o.Cover
	}
	return coverPixel
}

// chosen is the cover style picked in the menu, whatever this terminal shows.
func (o options) chosen() string {
	if o.Cover == coverSmooth || o.Cover == coverOriginal {
		return o.Cover
	}
	return coverPixel
}

// coverChoices are the cover styles on offer: all three everywhere, so
// original can be picked ahead of a terminal that shows it (see cover).
func coverChoices() []string { return coverStyles }

// The menu's rows, in order: those the window's menu has too, in its
// order and words (gui/qml/Store.qml menuRows), the terminal's own before
// the top bar's.
const (
	optColors = iota
	optAutoplay
	optMotion
	optCover
	optKeys
	optBar
	numOptions
)

// optLabels and optHints name each row and say what it does.
var (
	optLabels = [numOptions]string{
		optColors:   "Cover Colors",
		optAutoplay: "Autoplay",
		optMotion:   "Reduce Motion",
		optCover:    "Cover Style",
		optKeys:     "Show Shortcuts",
		optBar:      "Show in Top Bar",
	}
	optHints = [numOptions]string{
		optColors:   "Use the cover's colors for the app.",
		optCover:    "How album covers are drawn.",
		optAutoplay: "Play on after the last song.",
		optMotion:   "No scrolling text, fewer wobbles.",
		optKeys:     "Show keys on the buttons.",
		optBar:      "The song in Omarchy's top bar.",
	}
)

// optHint explains row i; the cover's says what original needs where the
// terminal cannot show it.
func optHint(i int) string {
	if i == optCover && !hasKitty {
		return "Covers as pixels or smooth; original needs kitty or Ghostty."
	}
	return optHints[i]
}

// optLabel names row i; the colors' by what they are now.
func (m *Model) optLabel(i int) string {
	if i == optColors && m.opts.NoCoverColors {
		return "Theme Colors"
	}
	return optLabels[i]
}

// optShown: the top bar's row only where omarchy can switch it.
func (m *Model) optShown(i int) bool { return i != optBar || m.hasOmarchy }

// stepOption moves the selection by dir, over rows that do not show.
func (m *Model) stepOption(dir int) {
	for {
		m.optSel = (m.optSel + dir + numOptions) % numOptions
		if m.optShown(m.optSel) {
			return
		}
	}
}

// optionsKey handles a key while the options are open; every key stays in
// the menu.
func (m *Model) optionsKey(k string) tea.Cmd {
	switch k {
	case "up", "k", "shift+tab":
		m.stepOption(-1)
	case "down", "j", "tab":
		m.stepOption(1)
	case "space", " ", "enter":
		return m.changeOption(m.optSel, 1)
	case "right", "l", "left", "h":
		// The cover style steps either way; a switch goes on to the right,
		// off to the left, whatever it was (colors: cover to the right).
		on := k == "right" || k == "l"
		if m.optSel == optCover || m.optionOn(m.optSel) != on {
			return m.changeOption(m.optSel, map[bool]int{true: 1, false: -1}[on])
		}
	case "o", "esc", "q":
		m.optOpen = false
	}
	return nil
}

// step moves through a choice by dir, wrapping.
func step[T comparable](list []T, cur T, dir int) T {
	i := 0
	for j, v := range list {
		if v == cur {
			i = j
		}
	}
	return list[(i+dir+len(list))%len(list)]
}

// choices draws a pick of one, cur marked.
func choices(list []string, cur string) string {
	var out []string
	for _, v := range list {
		if v == cur {
			out = append(out, sHere.Render("●")+" "+v)
		} else {
			out = append(out, sDim.Render("○ "+v))
		}
	}
	return strings.Join(out, "   ")
}

// toWindow is g: brumm goes over to its window, which is what it opens
// from now on; g there comes back here. The music plays on.
func (m *Model) toWindow() tea.Cmd {
	if _, err := launch.GUIPath(); err != nil {
		m.setFlash(err.Error())
		return nil
	}
	m.opts.Start = launch.GUI
	m.setFlash("opening the gui…")
	client, o, here := m.client, m.opts, m.here()
	return func() tea.Msg {
		leavePlace(client, here) // the window opens where this was
		if client == nil {
			o.save()
		} else if _, err := client.Do(ipc.Request{Cmd: ipc.CmdOptions, Options: map[string]any{"start": launch.GUI}}); err != nil {
			return errMsg{err}
		}
		if err := launch.Open(launch.GUI); err != nil {
			return errMsg{err}
		}
		return tea.QuitMsg{}
	}
}

// optionOn says whether switch i is on.
func (m *Model) optionOn(i int) bool {
	switch i {
	case optColors:
		return !m.opts.NoCoverColors
	case optAutoplay:
		return !m.opts.NoAutoplay
	case optMotion:
		return m.opts.ReduceMotion
	case optKeys:
		return m.opts.AlwaysTips
	case optBar:
		return m.barOn
	}
	return false
}

// changeOption flips a switch, or steps a choice by dir.
func (m *Model) changeOption(i, dir int) tea.Cmd {
	var cmd tea.Cmd
	switch i {
	case optCover:
		m.opts.Cover = step(coverChoices(), m.opts.chosen(), dir)
		m.rendered, m.thumbs = map[art.Size][]string{}, map[string][]string{}
	case optColors:
		m.opts.NoCoverColors = !m.opts.NoCoverColors // the next frame picks them (accent.go)
	case optAutoplay:
		m.opts.NoAutoplay = !m.opts.NoAutoplay
		cmd = m.send(ipc.Request{Cmd: ipc.CmdAutoplay, Value: map[bool]float64{true: 1}[!m.opts.NoAutoplay]})
	case optMotion:
		m.opts.ReduceMotion = !m.opts.ReduceMotion // the bar widget follows the saved file
	case optKeys:
		m.opts.AlwaysTips = !m.opts.AlwaysTips
	case optBar:
		cmd = m.setBar(!m.barOn)
	}
	m.saveOptions()
	return cmd
}

// optionsBox is the menu, drawn over the lower left corner: the rows, a
// gap, what the selected one does, the keys. optLines records which line
// each option is on, for the mouse.
func (m *Model) optionsBox() []string {
	check := func(on bool) string {
		if on {
			return sHere.Render("󰄲")
		}
		return sDim.Render("󰄱")
	}
	values := [numOptions]string{
		optColors: choices([]string{"theme", "cover"}, map[bool]string{true: "cover", false: "theme"}[m.optionOn(optColors)]),
		optCover:  choices(coverChoices(), m.opts.chosen()),
	}
	for _, i := range []int{optAutoplay, optMotion, optKeys, optBar} {
		values[i] = check(m.optionOn(i))
	}
	keys := sKey.Render("↑↓") + sDim.Render(" move   ") + sKey.Render("space") + sDim.Render(" change   ") + sKey.Render("esc") + sDim.Render(" close")
	w := lipgloss.Width(keys)
	for i := range numOptions {
		if m.optShown(i) {
			w = max(w, 16+lipgloss.Width(values[i]), len(optHint(i)))
		}
	}
	w = min(w+3, m.width-2*margin-6) // the box as wide as its widest hint: it keeps its size
	var lines []string
	m.optLines = m.optLines[:0]
	for i := range numOptions {
		if !m.optShown(i) {
			m.optLines = append(m.optLines, -1)
			continue
		}
		gutter := "   "
		if i == m.optSel {
			gutter = sHere.Render("▌") + "  "
		}
		m.optLines = append(m.optLines, len(lines)+1) // +1: the padding line
		lines = append(lines, gutter+fit(fmt.Sprintf("%-16s", m.optLabel(i))+values[i], w-3))
	}
	hint := ""
	if m.optSel >= 0 && m.optSel < numOptions {
		hint = optHint(m.optSel)
	}
	lines = append(append([]string{""}, lines...), "", "   "+fit(sDim.Render(hint), w-3), "   "+fit(keys, w-3), "")
	for i := range lines {
		lines[i] = " " + fit(lines[i], w)
	}
	return strings.Split(box("options", false, "", lines, w+4, len(lines)+2), "\n")
}

// overlay draws a menu box over the lower left of the screen, its bottom
// on the blank line above the footer, and records where its rows are for
// the mouse.
func (m *Model) overlay(content string, b []string) string {
	lines := strings.Split(content, "\n")
	bw := lipgloss.Width(b[0])
	y0 := max(0, len(lines)-1-len(b))
	m.geo.options = rect{margin, y0, margin + bw, y0 + len(b)}
	m.geo.optRow0 = y0 + 1
	for i, l := range b {
		y := y0 + i
		if y >= len(lines) {
			break
		}
		under := lines[y]
		left := ansi.Cut(under, 0, margin)
		left += strings.Repeat(" ", max(0, margin-lipgloss.Width(left)))
		lines[y] = left + l + ansi.Cut(under, margin+bw, m.width)
	}
	return strings.Join(lines, "\n")
}

// optionsClick changes the option under the mouse; a click outside closes
// the menu.
func (m *Model) optionsClick(x, y int) tea.Cmd {
	if !m.geo.options.has(x, y) {
		m.optOpen = false
		return nil
	}
	for i, l := range m.optLines {
		if l >= 0 && y-m.geo.optRow0 == l {
			m.optSel = i
			return m.changeOption(i, 1)
		}
	}
	return nil
}
