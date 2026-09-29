package tui

import (
	"encoding/json"
	"fmt"
	"math"
	"os/exec"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/art"
	"github.com/chriopter/brumm/internal/config"
	"github.com/chriopter/brumm/internal/ipc"
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

// coverChoices are the cover styles this terminal can show.
func coverChoices() []string {
	if hasKitty {
		return coverStyles
	}
	return coverStyles[:2]
}

// The menu's rows, in order.
const (
	optCover = iota
	optColors
	optAutoplay
	optMotion
	optKeys
	optBar
	numOptions
)

// optLabels and optHints name each row and say what it does.
var (
	optLabels = [numOptions]string{
		optCover:    "cover",
		optColors:   "cover colors",
		optAutoplay: "autoplay",
		optMotion:   "reduce motion",
		optKeys:     "show shortcuts",
		optBar:      "top bar player",
	}
	optHints = [numOptions]string{
		optCover:    "How album covers are drawn.",
		optColors:   "Tint the progress bar and buttons with the cover's colors.",
		optAutoplay: "When the queue ends, keep playing similar music.",
		optMotion:   "Keep long titles still and covers from sliding in.",
		optKeys:     "Show each button's key on the button.",
		optBar:      "The playing song in Omarchy's top bar; click it for controls.",
	}
)

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
	case "space", " ", "enter", "right", "l":
		return m.changeOption(m.optSel, 1)
	case "left", "h":
		return m.changeOption(m.optSel, -1)
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

// changeOption flips a switch, or steps a choice by dir.
func (m *Model) changeOption(i, dir int) tea.Cmd {
	var cmd tea.Cmd
	switch i {
	case optCover:
		m.opts.Cover = step(coverChoices(), m.opts.cover(), dir)
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
	m.opts.save()
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
	var choice []string
	for _, v := range coverChoices() {
		if v == m.opts.cover() {
			choice = append(choice, sHere.Render("●")+" "+v)
		} else {
			choice = append(choice, sDim.Render("○ "+v))
		}
	}
	values := [numOptions]string{
		optCover:    strings.Join(choice, "   "),
		optColors:   check(!m.opts.NoCoverColors),
		optAutoplay: check(!m.opts.NoAutoplay),
		optMotion:   check(m.opts.ReduceMotion),
		optKeys:     check(m.opts.AlwaysTips),
		optBar:      check(m.barOn),
	}
	keys := sKey.Render("↑↓") + sDim.Render(" move   ") + sKey.Render("space") + sDim.Render(" change   ") + sKey.Render("esc") + sDim.Render(" close")
	w := lipgloss.Width(keys)
	for i := range numOptions {
		if m.optShown(i) {
			w = max(w, 16+lipgloss.Width(values[i]), len(optHints[i]))
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
		lines = append(lines, gutter+fit(fmt.Sprintf("%-16s", optLabels[i])+values[i], w-3))
	}
	hint := ""
	if m.optSel >= 0 && m.optSel < numOptions {
		hint = optHints[m.optSel]
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
