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

// Cover sizes: large fills the stage's height, the others leave it room.
const (
	sizeSmall  = "small"
	sizeMedium = "medium"
	sizeLarge  = "large"
)

var coverSizes = []string{sizeSmall, sizeMedium, sizeLarge}

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

func loadOptions() options { return options{config.LoadOptions()} }

func (o options) save() { _ = o.Options.Save() }

// cover is the style to draw covers in: original falls back to smooth in
// terminals without kitty graphics.
func (o options) cover() string {
	switch {
	case o.Cover == coverOriginal && !art.KittySupported():
		return coverSmooth
	case o.Cover == coverSmooth || o.Cover == coverOriginal:
		return o.Cover
	}
	return coverPixel
}

// coverSize is the cover's size, large unless set.
func (o options) coverSize() string {
	if o.CoverSize == sizeSmall || o.CoverSize == sizeMedium {
		return o.CoverSize
	}
	return sizeLarge
}

// coverScale is the share of the stage's height the cover takes.
func (o options) coverScale() float64 {
	switch o.coverSize() {
	case sizeSmall:
		return 0.5
	case sizeMedium:
		return 0.75
	}
	return 1
}

// The menu's rows, in order: two choices, then plain switches.
const (
	optCover = iota
	optSize
	optBackdrop
	optMeter
	optScroll
	optBar
	optBarScroll
	optCards
	optAutoplay
	optKeys
	numOptions
)

// optShown: the bar widget's row only where omarchy can switch it.
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
		cur := m.opts.Cover
		if cur == "" {
			cur = coverPixel
		}
		m.opts.Cover = step(coverStyles, cur, dir)
		m.rendered, m.thumbs = map[art.Size][]string{}, map[string][]string{}
	case optSize:
		m.opts.CoverSize = step(coverSizes, m.opts.coverSize(), dir)
	case optBackdrop:
		m.opts.NoBackdrop = !m.opts.NoBackdrop
	case optMeter:
		m.opts.NoMeter = !m.opts.NoMeter
		cmd = m.subscribe() // no meter, no spectrum stream
	case optScroll:
		m.opts.NoScroll = !m.opts.NoScroll
	case optBar:
		cmd = m.setBar(!m.barOn)
	case optBarScroll:
		m.opts.NoBarScroll = !m.opts.NoBarScroll
	case optCards:
		m.opts.NoCards = !m.opts.NoCards
	case optAutoplay:
		m.opts.NoAutoplay = !m.opts.NoAutoplay
		cmd = m.send(ipc.Request{Cmd: ipc.CmdAutoplay, Value: map[bool]float64{true: 1}[!m.opts.NoAutoplay]})
	case optKeys:
		m.opts.AlwaysTips = !m.opts.AlwaysTips
	}
	m.opts.save()
	return cmd
}

// optionsBox is the menu, drawn over the lower left corner: the two
// choices, a gap, the switches, a gap, the keys. optLines records which
// line each option is on, for the mouse.
func (m *Model) optionsBox() []string {
	check := func(on bool) string {
		if on {
			return sHere.Render("󰄲")
		}
		return sDim.Render("󰄱")
	}
	choice := func(list []string, cur string) string {
		var out []string
		for _, v := range list {
			if v == cur {
				out = append(out, sHere.Render("●")+" "+v)
			} else {
				out = append(out, sDim.Render("○ "+v))
			}
		}
		return strings.Join(out, "    ")
	}
	label := func(s string) string { return fmt.Sprintf("%-12s", s) }
	cover := choice(coverStyles, m.opts.cover())
	if m.opts.Cover == coverOriginal && !art.KittySupported() {
		cover = choice(coverStyles, coverOriginal) + sDim.Render("  needs kitty or Ghostty")
	}
	rows := [numOptions]string{
		optCover:     label("cover") + cover,
		optSize:      label("size") + choice(coverSizes, m.opts.coverSize()),
		optBackdrop:  check(!m.opts.NoBackdrop) + "   backdrop",
		optMeter:     check(!m.opts.NoMeter) + "   level meter",
		optScroll:    check(!m.opts.NoScroll) + "   scroll long names",
		optBar:       check(m.barOn) + "   bar widget",
		optBarScroll: check(!m.opts.NoBarScroll) + "   scroll in the bar",
		optCards:     check(!m.opts.NoCards) + "   cover cards",
		optAutoplay:  check(!m.opts.NoAutoplay) + "   autoplay",
		optKeys:      check(m.opts.AlwaysTips) + "   keys on buttons",
	}
	hint := sKey.Render("↑↓") + sDim.Render(" move    ") + sKey.Render("space") + sDim.Render(" change    ") + sKey.Render("o") + sDim.Render(" close")
	w := lipgloss.Width(hint)
	for i, r := range rows {
		if !m.optShown(i) {
			continue
		}
		w = max(w, lipgloss.Width(r)+3)
	}
	w = min(w+2, m.width-2*margin-6)
	var lines []string
	m.optLines = m.optLines[:0]
	for i, r := range rows {
		if !m.optShown(i) {
			m.optLines = append(m.optLines, -1)
			continue
		}
		if i == optBackdrop {
			lines = append(lines, "") // choices above, switches below
		}
		gutter := "   "
		if i == m.optSel {
			gutter = sHere.Render("▌") + "  "
		}
		m.optLines = append(m.optLines, len(lines)+1) // +1: the padding line
		lines = append(lines, gutter+fit(r, w-3))
	}
	lines = append(append([]string{""}, lines...), "", "   "+fit(hint, w-3), "")
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
