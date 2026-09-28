package tui

import (
	"fmt"
	"math"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/art"
	"github.com/chriopter/brumm/internal/ipc"
)

// Only the terminal's ANSI palette and never a background: the Omarchy
// theme decides the colors. The cover is the one exception — it is a picture.
var (
	sAccent = lipgloss.NewStyle().Foreground(lipgloss.Magenta)
	sHot    = lipgloss.NewStyle().Foreground(lipgloss.BrightMagenta)
	sCool   = lipgloss.NewStyle().Foreground(lipgloss.Blue)
	sDim    = lipgloss.NewStyle().Foreground(lipgloss.BrightBlack)
	sOK     = lipgloss.NewStyle().Foreground(lipgloss.Green)
	sKey    = lipgloss.NewStyle().Foreground(lipgloss.Cyan)
	sErr    = lipgloss.NewStyle().Foreground(lipgloss.Red)
	sBold   = lipgloss.NewStyle().Bold(true)
)

var spinner = []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}

const (
	margin        = 2  // columns left and right of everything
	bodyTop       = 2  // header, blank line
	minStageWidth = 44 // below this the stage folds into a status bar
	specRows      = 5
)

// rect is a clickable screen area, x1/y1 exclusive.
type rect struct{ x0, y0, x1, y1 int }

func (r rect) has(x, y int) bool { return x >= r.x0 && x < r.x1 && y >= r.y0 && y < r.y1 }

// geometry records where things were drawn, for mouse hit-testing.
type geometry struct {
	list      rect // list rows
	rowH, off int
	crumb     rect // the "Playlists ›" part of the nav title
	prev      rect
	play      rect
	next      rect
	bar       rect
}

func (m *Model) View() tea.View {
	v := tea.NewView(m.render())
	v.AltScreen = true
	v.MouseMode = tea.MouseModeCellMotion
	v.WindowTitle = "brumm"
	return v
}

func (m *Model) render() string {
	m.geo = geometry{}
	if m.width < 40 || m.height < 12 {
		return sDim.Render("ʕ•ᴥ•ʔ brumm needs a bigger window")
	}
	inner := m.width - 2*margin
	bodyH := m.height - bodyTop - 2 // blank line + footer
	navW := min(max(inner*42/100, 38), 70)
	stageW := inner - navW - 4
	if stageW < minStageWidth {
		navW, stageW = inner, 0
	}

	var body string
	if stageW == 0 {
		mini := m.miniPlayer(inner)
		nav := m.nav(navW, bodyH-len(mini), margin, bodyTop)
		body = lipgloss.JoinVertical(lipgloss.Left, append([]string{nav}, mini...)...)
	} else {
		stageX := margin + navW + 4
		body = lipgloss.JoinHorizontal(lipgloss.Top,
			m.nav(navW, bodyH, margin, bodyTop), "    ", m.stage(stageW, bodyH, stageX, bodyTop))
	}
	pad := strings.Repeat(" ", margin)
	lines := strings.Split(body, "\n")
	for i := range lines {
		lines[i] = pad + lines[i]
	}
	return strings.Join([]string{m.header(), "", strings.Join(lines, "\n"), "", m.footer()}, "\n")
}

// ── header & footer ─────────────────────────────────────────────────────

func (m *Model) bear() string {
	if !m.state.Playing {
		return "ʕ-ᴥ-ʔ"
	}
	return []string{"ʕ•ᴥ•ʔ", "ʕ•ᴥ•ʔ♪", "ʕ·ᴥ·ʔ♫", "ʕ•ᴥ•ʔ♪"}[(m.frame/5)%4]
}

func (m *Model) header() string {
	left := strings.Repeat(" ", margin) + sAccent.Render(m.bear()) + "  " + sBold.Render("brumm")
	// The right side speaks only when something needs attention.
	var right string
	switch m.state.Status {
	case ipc.StatusLoggedOut:
		right = sErr.Render("not signed in")
	case ipc.StatusError:
		right = sErr.Render(m.state.Message)
	case ipc.StatusStarting:
		right = sDim.Render(spinner[m.frame%len(spinner)] + " " + m.state.Message)
	}
	if m.flash != "" {
		right = sDim.Render(m.flash)
	}
	right = ansi.Truncate(right, max(0, m.width-lipgloss.Width(left)-margin-2), "…")
	gap := max(1, m.width-lipgloss.Width(left)-lipgloss.Width(right)-margin)
	return left + strings.Repeat(" ", gap) + right
}

func (m *Model) footer() string {
	keys := [][2]string{{"enter", "open"}}
	if m.level == levelTracks {
		keys = [][2]string{{"enter", "play"}, {"esc", "back"}}
	}
	keys = append(keys, [][2]string{{"space", "pause"}, {"n p", "skip"}, {"← →", "seek"}, {"?", "more"}}...)
	if m.state.Status == ipc.StatusLoggedOut {
		keys = append([][2]string{{"L", "sign in"}}, keys...)
	}
	parts := make([]string, len(keys))
	for i, kv := range keys {
		parts[i] = sKey.Render(kv[0]) + " " + sDim.Render(kv[1])
	}
	return strings.Repeat(" ", margin) + ansi.Truncate(strings.Join(parts, "    "), m.width-2*margin, "…")
}

// ── navigation panel ────────────────────────────────────────────────────

// listRows is how many list entries fit in the nav panel.
func (m *Model) listRows() int {
	rows := m.height - bodyTop - 2 - 4 // borders + padding lines
	if m.level == levelTracks {
		return max(1, rows/2)
	}
	return max(1, rows)
}

func (m *Model) nav(w, h, x, y int) string {
	rows := h - 4 // border and one blank line top and bottom
	inner := w - 6
	listX := x + 3
	var title, crumb string
	var lines []string
	spin := spinner[m.frame%len(spinner)]

	if m.level == levelPlaylists {
		title = "Playlists"
		switch {
		case m.playlists == nil && m.plErr != nil:
			lines = centered(sErr.Render(m.plErr.Error()), inner, rows)
		case m.playlists == nil:
			lines = centered(sDim.Render(spin+"  loading your library"), inner, rows)
		case len(m.playlists) == 0:
			lines = centered(sDim.Render("no playlists in your library"), inner, rows)
		default:
			m.pl.off = scroll(m.pl.sel, m.pl.off, rows)
			m.geo.list, m.geo.rowH, m.geo.off = rect{listX, y + 2, listX + inner, y + 2 + rows}, 1, m.pl.off
			for i := m.pl.off; i < len(m.playlists) && i < m.pl.off+rows; i++ {
				p := m.playlists[i]
				name := p.Name
				if i == m.pl.sel {
					name = sBold.Render(name)
				}
				mark := ""
				if p.ID == m.state.Playlist && m.state.Title != "" {
					mark = "  " + sOK.Render("♪")
				}
				lines = append(lines, gutter(i == m.pl.sel)+ansi.Truncate(name, inner-2-lipgloss.Width(mark), "…")+mark)
			}
		}
	} else {
		p := m.currentPlaylist()
		crumb = "Playlists"
		title = crumb + "  ›  " + p.Name
		m.geo.crumb = rect{x + 3, y, x + 3 + len(crumb), y + 1}
		tracks, err := m.tracks[p.ID], m.trErr[p.ID]
		slots := max(1, rows/2)
		switch {
		case tracks == nil && err != nil:
			lines = centered(sErr.Render(err.Error()), inner, rows)
		case tracks == nil:
			lines = centered(sDim.Render(spin+"  loading"), inner, rows)
		case len(tracks) == 0:
			lines = centered(sDim.Render("this playlist is empty"), inner, rows)
		default:
			numW := max(2, len(fmt.Sprint(len(tracks))))
			m.tr.sel = min(m.tr.sel, len(tracks)-1)
			m.tr.off = scroll(m.tr.sel, m.tr.off, slots)
			m.geo.list, m.geo.rowH, m.geo.off = rect{listX, y + 2, listX + inner, y + 2 + slots*2}, 2, m.tr.off
			for i := m.tr.off; i < len(tracks) && i < m.tr.off+slots; i++ {
				t := tracks[i]
				sel := i == m.tr.sel
				playing := p.ID == m.state.Playlist && i == m.state.Index && m.state.Title != ""
				num := sDim.Render(fmt.Sprintf("%*d", numW, i+1))
				name := t.Title
				switch {
				case playing:
					num = strings.Repeat(" ", numW-1) + sOK.Render("♪")
					name = sOK.Bold(sel).Render(t.Title)
				case sel:
					name = sBold.Render(t.Title)
				}
				dur := sDim.Render(clock(t.Duration))
				titleW := inner - 2 - numW - 3 - 3 - lipgloss.Width(dur)
				indent := strings.Repeat(" ", numW+3)
				lines = append(lines,
					gutter(sel)+num+"   "+pad(ansi.Truncate(name, titleW, "…"), titleW)+"   "+dur,
					gutter(sel)+indent+ansi.Truncate(sDim.Render(t.Artist), titleW, "…"))
			}
		}
	}

	label := ""
	if m.level == levelTracks {
		if n := len(m.tracks[m.currentPlaylist().ID]); n > 0 {
			label = fmt.Sprintf("%d", n)
		}
	} else if n := len(m.playlists); n > 0 {
		label = fmt.Sprintf("%d", n)
	}
	return box(title, crumb, label, append([]string{""}, lines...), w, h)
}

func gutter(selected bool) string {
	if selected {
		return sAccent.Render("▌") + " "
	}
	return "  "
}

// ── stage ───────────────────────────────────────────────────────────────

// stageLine is one centered line of the stage with an optional click target.
type stageLine struct {
	text string
	hit  func(x, y int) // records geometry once the line's position is known
}

func (m *Model) stage(w, h, x, y int) string {
	if m.help {
		return m.place(m.helpLines(), w, h, x, y)
	}
	st := m.state
	var block []stageLine
	plain := func(ls ...string) {
		for _, l := range ls {
			block = append(block, stageLine{text: l})
		}
	}
	switch {
	case st.Status == ipc.StatusLoggedOut:
		plain(sAccent.Render("ʕ•ᴥ•ʔ"), "", sBold.Render("Sign in to Apple Music"), "",
			sDim.Render("press ")+sKey.Render("L")+sDim.Render(" to open the sign-in page"))
	case st.Status == ipc.StatusError:
		plain(sErr.Render("ʕ×ᴥ×ʔ"), "", sErr.Render(st.Message))
	case st.Status == ipc.StatusStarting && st.Title == "":
		plain(sAccent.Render("ʕ•ᴥ•ʔ"), "", sDim.Render(spinner[m.frame%len(spinner)]+"  "+st.Message))
	case st.Title == "":
		plain(sAccent.Render("ʕ-ᴥ-ʔ zZ"), "", sBold.Render("nothing playing"), "",
			sDim.Render("pick a playlist and press ")+sKey.Render("enter"))
		if st.Err != "" {
			plain("", sErr.Render(st.Err))
		}
	default:
		block = m.nowPlaying(w, h)
	}
	return m.placeLines(block, w, h, x, y)
}

func (m *Model) place(ls []string, w, h, x, y int) string {
	block := make([]stageLine, len(ls))
	for i, l := range ls {
		block[i] = stageLine{text: l}
	}
	return m.placeLines(block, w, h, x, y)
}

// placeLines centers the block in the stage and resolves click targets.
func (m *Model) placeLines(block []stageLine, w, h, x, y int) string {
	top := max(0, (h-len(block))/2)
	out := make([]string, h)
	for i, l := range block {
		row := top + i
		if row >= h {
			break
		}
		text := ansi.Truncate(l.text, w, "…")
		left := max(0, (w-lipgloss.Width(text))/2)
		out[row] = strings.Repeat(" ", left) + text
		if l.hit != nil {
			l.hit(x+left, y+row)
		}
	}
	for i := range out {
		out[i] = pad(out[i], w)
	}
	return strings.Join(out, "\n")
}

func (m *Model) nowPlaying(w, h int) []stageLine {
	st := m.state
	// Lines under the cover: gap, title, artist, album, gap, spectrum,
	// gap, progress, gap, controls.
	const below = 1 + 3 + 1 + specRows + 1 + 1 + 1 + 1
	coverH := min(h-below-2, (w-8)/2, 20)
	barW := min(w-4, 64)
	if coverH >= 6 {
		barW = min(w-4, max(coverH*2, 44))
	}

	var out []stageLine
	add := func(s string) { out = append(out, stageLine{text: s}) }
	if coverH >= 6 {
		for _, l := range m.coverLines(art.Size{Width: coverH * 2, Height: coverH}) {
			add(l)
		}
		add("")
	}
	add(sBold.Render(st.Title))
	add(st.Artist)
	add(sDim.Render(st.Album))
	add("")
	for _, l := range m.spectrum(barW, specRows) {
		add(l)
	}
	add("")

	left, right := clock(m.position())+"  ", "  "+clock(st.Dur)
	barLen := barW - lipgloss.Width(left) - lipgloss.Width(right)
	out = append(out, stageLine{text: m.progress(barW), hit: func(x, y int) {
		m.geo.bar = rect{x + lipgloss.Width(left), y, x + lipgloss.Width(left) + barLen, y + 1}
	}})
	add("")

	play := sOK.Render("⏸")
	if !st.Playing {
		play = sBold.Render("▶")
	}
	controls := sDim.Render("⏮") + "      " + play + "      " + sDim.Render("⏭")
	out = append(out, stageLine{text: controls, hit: func(x, y int) {
		// ⏮ at 0, ▶/⏸ at 7, ⏭ at 14; widen each target by a cell.
		m.geo.prev = rect{x - 1, y, x + 3, y + 1}
		m.geo.play = rect{x + 6, y, x + 10, y + 1}
		m.geo.next = rect{x + 13, y, x + 17, y + 1}
	}})
	return out
}

func (m *Model) coverLines(size art.Size) []string {
	if lines, ok := m.rendered[size]; ok {
		return lines
	}
	if m.cover == nil {
		// Placeholder while the cover downloads, same footprint.
		lines := make([]string, size.Height)
		for i := range lines {
			lines[i] = sDim.Render(strings.Repeat("░", size.Width))
		}
		return lines
	}
	lines := art.RenderDithered(m.cover, size)
	if len(m.rendered) > 8 {
		m.rendered = map[art.Size][]string{}
	}
	m.rendered[size] = lines
	return lines
}

// spectrum draws the live bands as bars, eighth-block precise, cool at the
// base and hot at the peaks.
func (m *Model) spectrum(w, rows int) []string {
	nbars := (w + 1) / 2
	levels := make([]float64, nbars)
	if len(m.spec) > 0 {
		for i := range levels {
			// Resample the bands onto the bar count.
			x := float64(i) * float64(len(m.spec)-1) / float64(max(1, nbars-1))
			lo := int(x)
			hi := min(lo+1, len(m.spec)-1)
			f := x - float64(lo)
			levels[i] = m.spec[lo]*(1-f) + m.spec[hi]*f
		}
	}
	ramp := []rune(" ▁▂▃▄▅▆▇█")
	out := make([]string, rows)
	for r := range rows {
		row := rows - 1 - r // 0 is the bottom row
		style := sCool
		switch {
		case row == rows-1:
			style = sHot
		case row >= rows/2:
			style = sAccent
		}
		var sb strings.Builder
		for i, lv := range levels {
			fill := lv*float64(rows*8) - float64(row*8)
			ch := ' '
			switch {
			case fill >= 8:
				ch = '█'
			case fill > 0:
				ch = ramp[int(math.Round(fill))]
			case row == 0:
				ch = '▁' // a resting baseline keeps the shape visible when quiet
			}
			sb.WriteRune(ch)
			if i < nbars-1 {
				sb.WriteByte(' ')
			}
		}
		out[r] = style.Render(sb.String())
	}
	return out
}

func (m *Model) progress(w int) string {
	pos, dur := m.position(), m.state.Dur
	left, right := clock(pos)+"  ", "  "+clock(dur)
	bar := w - lipgloss.Width(left) - lipgloss.Width(right)
	done := 0
	if dur > 0 {
		done = int(float64(bar-1) * pos / dur)
	}
	return sDim.Render(left) +
		sAccent.Render(strings.Repeat("━", done)+"●") +
		sDim.Render(strings.Repeat("─", max(0, bar-done-1))+right)
}

func (m *Model) helpLines() []string {
	keys := [][2]string{
		{"↑ ↓  j k", "move"}, {"enter  l", "open / play"}, {"esc  h", "back"},
		{"space", "play / pause"}, {"n  p", "next / previous"}, {"← →", "seek 10 s"},
		{"+  -", "volume"}, {"c", "go to what's playing"}, {"q", "close (music keeps playing)"},
		{"Q", "stop brumm"}, {"L", "sign in again"}, {"mouse", "click to open, click again to play"},
	}
	out := []string{sBold.Render("keys"), ""}
	for _, kv := range keys {
		out = append(out, sKey.Render(fmt.Sprintf("%-10s", kv[0]))+"  "+sDim.Render(fmt.Sprintf("%-28s", kv[1])))
	}
	return append(out, "", sDim.Render("press ? to close"))
}

// miniPlayer is the two-line player for narrow windows.
func (m *Model) miniPlayer(w int) []string {
	st := m.state
	if st.Title == "" {
		return []string{"", sDim.Render("nothing playing")}
	}
	icon := sOK.Render("▶")
	if !st.Playing {
		icon = sDim.Render("⏸")
	}
	line := icon + "  " + sBold.Render(st.Title) + sDim.Render("  "+st.Artist)
	return []string{"", ansi.Truncate(line, w, "…"), m.progress(w)}
}

// ── drawing helpers ─────────────────────────────────────────────────────

// box draws a rounded panel of exactly w×h cells with the title set into
// the top border (its crumb prefix dimmed) and a label right-aligned in it.
func box(title, crumb, label string, lines []string, w, h int) string {
	b := sAccent
	t := ansi.Truncate(title, max(1, w-12-lipgloss.Width(label)), "…")
	if crumb != "" && strings.HasPrefix(t, crumb) {
		t = sDim.Render(crumb) + sBold.Render(strings.TrimPrefix(t, crumb))
	} else {
		t = sBold.Render(t)
	}
	top := b.Render("╭─ ") + t + " "
	lab := ""
	if label != "" {
		lab = " " + sDim.Render(label) + " " + b.Render("─")
	}
	gap := w - 1 - lipgloss.Width(top) - lipgloss.Width(lab)
	out := []string{top + b.Render(strings.Repeat("─", max(0, gap))) + lab + b.Render("╮")}
	for i := range h - 2 {
		line := ""
		if i < len(lines) {
			line = lines[i]
		}
		out = append(out, b.Render("│")+"  "+pad(ansi.Truncate(line, w-6, "…"), w-6)+"  "+b.Render("│"))
	}
	out = append(out, b.Render("╰"+strings.Repeat("─", w-2)+"╯"))
	return strings.Join(out, "\n")
}

func pad(s string, w int) string {
	return s + strings.Repeat(" ", max(0, w-lipgloss.Width(s)))
}

func centered(s string, w, rows int) []string {
	out := make([]string, max(0, rows/2-1))
	return append(out, strings.Repeat(" ", max(0, (w-lipgloss.Width(s))/2))+s)
}

// scroll keeps sel inside a window of rows starting at off.
func scroll(sel, off, rows int) int {
	switch {
	case sel < off:
		return sel
	case sel >= off+rows:
		return sel - rows + 1
	}
	return off
}

func clock(sec float64) string {
	s := int(sec)
	return fmt.Sprintf("%d:%02d", s/60, s%60)
}
