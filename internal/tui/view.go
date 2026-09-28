package tui

import (
	"fmt"
	"math"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/art"
	"github.com/chriopter/brumm/internal/ipc"
)

// Colors come only from the terminal's ANSI palette, so the Omarchy theme
// decides them; the cover is the one exception. Each has one job:
// magenta is where you are, green is what plays, cyan is a key, blue is
// the music itself, bright black is everything secondary.
var (
	sHere  = lipgloss.NewStyle().Foreground(lipgloss.Magenta)
	sPlays = lipgloss.NewStyle().Foreground(lipgloss.Green)
	sKey   = lipgloss.NewStyle().Foreground(lipgloss.Cyan)
	sMusic = lipgloss.NewStyle().Foreground(lipgloss.Blue)
	sDim   = lipgloss.NewStyle().Foreground(lipgloss.BrightBlack)
	sErr   = lipgloss.NewStyle().Foreground(lipgloss.Red)
	sBold  = lipgloss.NewStyle().Bold(true)
	sPill  = lipgloss.NewStyle().Foreground(lipgloss.Green).Reverse(true)
)

// Nerd Font glyphs (Material Design), one cell each.
const (
	icPrev      = "󰒮"
	icPlay      = "󰐊"
	icPause     = "󰏤"
	icNext      = "󰒭"
	icShuffle   = "󰒝"
	icRepeat    = "󰑖"
	icRepeatOne = "󰑘"
	icVolume    = "󰕾"
	icMuted     = "󰖁"
	icPlaylist  = "󰲸"
	icAlbum     = "󰀥"
	icArtist    = "󰠃"
	icSearch    = "󰍉"
)

var spinner = []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}

const (
	margin   = 2 // columns left and right of everything
	bodyTop  = 1 // one blank line above the panels
	navMin   = 44
	colMin   = 30
	stagePad = 2
	listTop  = 3 // list rows start this far below the panel's top border
)

// rect is a clickable screen area, x1/y1 exclusive.
type rect struct{ x0, y0, x1, y1 int }

func (r rect) has(x, y int) bool { return x >= r.x0 && x < r.x1 && y >= r.y0 && y < r.y1 }

// geometry records where things were drawn, for mouse hit-testing.
type geometry struct {
	list, crumb, search                            rect
	tabs                                           [5]rect
	prev, play, next, shuffle, repeat, volume, bar rect
}

func (m *Model) View() tea.View {
	v := tea.NewView(m.render())
	v.AltScreen = true
	v.MouseMode = tea.MouseModeCellMotion
	v.WindowTitle = "brumm"
	return v
}

// listRows is how many list rows fit in the browser panel.
func (m *Model) listRows() int { return max(1, m.height-bodyTop-3-listTop-1) }

func (m *Model) render() string {
	m.geo = geometry{}
	if m.width < 40 || m.height < 14 {
		return sDim.Render("ʕ•ᴥ•ʔ brumm needs a bigger window")
	}
	inner := m.width - 2*margin
	bodyH := m.height - bodyTop - 2 // blank line + footer

	// The stage is one column as wide as the cover; the list gets the rest.
	const below = 11 // gap, title, meta, gap, spectrum×3, progress, gap, controls, gap
	coverH := max(0, bodyH-1-below)
	colW := int(math.Round(float64(coverH) * m.cellAspect))
	if maxCol := inner - navMin - 2 - 2*stagePad; colW > maxCol {
		colW = maxCol
		coverH = int(float64(colW) / m.cellAspect)
	}
	colW = max(colW, colMin)
	navW := inner - colW - 2*stagePad - 2

	var body string
	if navW < navMin {
		mini := m.miniPlayer(inner)
		body = lipgloss.JoinVertical(lipgloss.Left, append([]string{m.nav(inner, bodyH-len(mini), margin, bodyTop)}, mini...)...)
	} else {
		stageX := margin + navW + 2 + stagePad
		body = lipgloss.JoinHorizontal(lipgloss.Top,
			m.nav(navW, bodyH, margin, bodyTop), strings.Repeat(" ", 2+stagePad),
			m.stage(colW, coverH, bodyH, stageX, bodyTop))
	}
	pad := strings.Repeat(" ", margin)
	lines := strings.Split(body, "\n")
	for i := range lines {
		lines[i] = pad + lines[i]
	}
	return strings.Join([]string{"", strings.Join(lines, "\n"), "", m.footer()}, "\n")
}

// ── footer ──────────────────────────────────────────────────────────────

func (m *Model) bear() string {
	if !m.state.Playing {
		return "ʕ-ᴥ-ʔ"
	}
	return []string{"ʕ•ᴥ•ʔ", "ʕ•ᴥ•ʔ♪", "ʕ·ᴥ·ʔ♫", "ʕ•ᴥ•ʔ♪"}[(m.frame/5)%4]
}

func (m *Model) footer() string {
	keys := [][2]string{{"enter", "play"}, {"esc", "back"}, {"/", "search"}, {"?", "keys"}}
	if m.state.Status == ipc.StatusLoggedOut {
		keys = append([][2]string{{"L", "sign in"}}, keys...)
	}
	parts := make([]string, len(keys))
	for i, kv := range keys {
		parts[i] = sKey.Render(kv[0]) + " " + sDim.Render(kv[1])
	}
	left := strings.Repeat(" ", margin+3) + strings.Join(parts, "   ")

	right := sHere.Render(m.bear())
	switch {
	case m.flash != "":
		right = sDim.Render(m.flash) + "   " + right
	case m.state.Status == ipc.StatusLoggedOut:
		right = sErr.Render("not signed in") + "   " + right
	case m.state.Status == ipc.StatusError:
		right = sErr.Render(m.state.Message) + "   " + right
	case m.state.Status == ipc.StatusStarting:
		right = sDim.Render(spinner[m.frame%len(spinner)]+" "+m.state.Message) + "   " + right
	}
	room := m.width - margin - lipgloss.Width(left) - 3
	right = ansi.TruncateLeft(right, max(0, lipgloss.Width(right)-room), "…")
	return left + strings.Repeat(" ", max(1, m.width-margin-lipgloss.Width(left)-lipgloss.Width(right))) + right
}

// ── browser panel ───────────────────────────────────────────────────────

func (m *Model) nav(w, h, x, y int) string {
	inner := w - 6
	cx := x + 3 // first content column
	v := m.cur()

	// Tabs: the active one where-you-are colored, the rest dim.
	var tabs []string
	tx := cx
	for i, name := range sectionNames {
		label := sDim.Render(name)
		if section(i) == m.section {
			label = sHere.Bold(true).Render(name)
		}
		m.geo.tabs[i] = rect{tx, y + 1, tx + len(name), y + 2}
		tabs = append(tabs, label)
		tx += len(name) + 3
	}
	lines := []string{strings.Join(tabs, "   "), ""}

	rows := h - 2 - len(lines) - 1
	top := y + 1 + len(lines)
	if m.section == secSearch && len(m.stack()) == 1 {
		lines = append(lines, m.searchBox(inner), "")
		m.geo.search = rect{cx, top, cx + inner, top + 1}
		rows -= 2
		top += 2
	}

	spin := spinner[m.frame%len(spinner)]
	switch {
	case v.err != nil && len(v.rows) == 0:
		lines = append(lines, centered(sErr.Render(v.err.Error()), inner, rows)...)
	case !v.loaded && m.state.Status == ipc.StatusLoggedOut:
		lines = append(lines, centered(sDim.Render("sign in to see your library"), inner, rows)...)
	case !v.loaded:
		lines = append(lines, centered(sDim.Render(spin+"  loading"), inner, rows)...)
	case v.key == "search:":
		lines = append(lines, centered(sDim.Render("search Apple Music: type and press ")+sKey.Render("enter"), inner, rows)...)
	case len(v.rows) == 0:
		lines = append(lines, centered(sDim.Render("nothing here"), inner, rows)...)
	default:
		v.off = scroll(v.sel, v.off, rows)
		m.geo.list = rect{cx, top, cx + inner, top + rows}
		for i := v.off; i < len(v.rows) && i < v.off+rows; i++ {
			lines = append(lines, m.row(v, i, inner))
		}
	}

	// Breadcrumb in the top border; position in the bottom one.
	var crumb []string
	for _, sv := range m.stack() {
		crumb = append(crumb, sv.title)
	}
	title := strings.Join(crumb, " / ")
	if len(crumb) > 1 {
		parent := strings.Join(crumb[:len(crumb)-1], " / ")
		m.geo.crumb = rect{x + 3, y, x + 3 + lipgloss.Width(parent), y + 1}
	}
	pos := ""
	if n := len(v.rows); n > 0 && v.loaded {
		pos = fmt.Sprintf("%d of %d", v.sel+1, n)
	}
	return box(title, len(crumb) > 1, pos, lines, w, h)
}

func (m *Model) searchBox(w int) string {
	q := m.query
	cursor := ""
	if m.searching {
		cursor = sHere.Render("▏")
	}
	if q == "" && !m.searching {
		return sDim.Render(icSearch + "  search Apple Music")
	}
	return sHere.Render(icSearch) + "  " + ansi.TruncateLeft(q, max(0, lipgloss.Width(q)-(w-5)), "…") + cursor
}

// row renders one list line: gutter, marker, text (the selected row
// scrolls when it does not fit), and a right-aligned detail.
func (m *Model) row(v *view, i, w int) string {
	r := v.rows[i]
	sel := i == v.sel
	gutter := "  "
	if sel {
		gutter = sHere.Render("▌") + " "
	}

	var mark, text, detail string
	if t := r.track; t != nil {
		playing := t.ID != "" && t.ID == m.state.ID
		mark = sDim.Render(fmt.Sprintf("%3d", i+1))
		title := t.Title
		switch {
		case playing:
			mark = sPlays.Render(m.miniEQ())
			title = sPlays.Bold(sel).Render(t.Title)
		case sel:
			title = sBold.Render(t.Title)
		}
		text = title + "  " + sDim.Render(t.Artist)
		detail = sDim.Render(fmt.Sprintf("%5s", clock(t.Duration)))
	} else {
		it := r.item
		icon := map[string]string{apple.KindPlaylist: icPlaylist, apple.KindAlbum: icAlbum, apple.KindArtist: icArtist}[it.Kind]
		mark = "  " + sDim.Render(icon)
		name := it.Name
		if sel {
			mark = "  " + sHere.Render(icon)
			name = sBold.Render(name)
		}
		text = name
		if it.Artist != "" {
			text += "  " + sDim.Render(it.Artist)
		}
		if it.Key() == m.state.Source && m.state.Title != "" {
			detail = sPlays.Render("♪")
		}
	}
	avail := w - 2 - 3 - 2 - 2 - lipgloss.Width(detail)
	if sel {
		text = marquee(text, avail, m.frame-v.selAt)
	} else {
		text = ansi.Truncate(text, avail, "…")
	}
	return gutter + mark + "  " + pad(text, avail) + "  " + detail
}

// miniEQ is a three-bar level meter for the playing row.
func (m *Model) miniEQ() string {
	if !m.state.Playing || len(m.spec) == 0 {
		return " ♪ "
	}
	ramp := []rune("▁▂▃▄▅▆▇█")
	var sb strings.Builder
	for _, band := range []int{2, len(m.spec) / 3, len(m.spec) * 2 / 3} {
		lv := m.spec[min(band, len(m.spec)-1)]
		sb.WriteRune(ramp[min(len(ramp)-1, int(lv*float64(len(ramp))))])
	}
	return sb.String()
}

// marquee scrolls text that does not fit: it rests, glides to the end,
// rests again, and starts over.
func marquee(s string, w, t int) string {
	over := lipgloss.Width(s) - w
	if over <= 0 {
		return s
	}
	const rest = 8 // ticks of 100 ms
	step := max(0, t-rest) % (over + 2*rest)
	off := min(step, over)
	return ansi.Cut(s, off, off+w)
}

// ── stage ───────────────────────────────────────────────────────────────

// stageLine is one line of the stage column, with an optional click target
// resolved once its screen position is known.
type stageLine struct {
	text   string
	center bool
	hit    func(x, y int)
}

func (m *Model) stage(colW, coverH, h, x, y int) string {
	st := m.state
	var lines []stageLine
	add := func(s string) { lines = append(lines, stageLine{text: s}) }
	switch {
	case m.help:
		lines = m.helpLines()
	case st.Status == ipc.StatusLoggedOut:
		add(sHere.Render("ʕ•ᴥ•ʔ"))
		add("")
		add(sBold.Render("Sign in to Apple Music"))
		add(sDim.Render("press ") + sKey.Render("L") + sDim.Render(" to open the sign-in page"))
	case st.Title == "":
		add(sHere.Render("ʕ-ᴥ-ʔ zZ"))
		add("")
		add(sBold.Render("nothing playing"))
		add(sDim.Render("pick something and press ") + sKey.Render("enter"))
	default:
		lines = m.nowPlaying(colW, coverH)
	}

	out := make([]string, h)
	for i := range out {
		out[i] = strings.Repeat(" ", colW)
	}
	top := 1 // align with the tabs row
	if st.Title == "" && !m.help {
		top = max(1, (h-len(lines))/2)
	}
	for i, l := range lines {
		row := top + i
		if row >= h {
			break
		}
		text := ansi.Truncate(l.text, colW, "…")
		left := 0
		if l.center {
			left = max(0, (colW-lipgloss.Width(text))/2)
		}
		out[row] = pad(strings.Repeat(" ", left)+text, colW)
		if l.hit != nil {
			l.hit(x+left, y+row)
		}
	}
	return strings.Join(out, "\n")
}

func (m *Model) nowPlaying(colW, coverH int) []stageLine {
	st := m.state
	var out []stageLine
	add := func(s string) { out = append(out, stageLine{text: s}) }

	if coverH >= 6 {
		coverW := min(colW, int(math.Round(float64(coverH)*m.cellAspect)))
		for _, l := range m.coverLines(art.Size{Width: coverW, Height: coverH}) {
			out = append(out, stageLine{text: l, center: true})
		}
		add("")
	}
	add(sBold.Render(st.Title))
	meta := st.Artist
	if st.Album != "" && st.Album != st.Title {
		meta += sDim.Render("  ·  " + st.Album)
	}
	add(meta)
	add("")
	for _, l := range m.spectrum(colW, 3) {
		add(l)
	}

	left, right := clock(m.position())+"  ", "  "+clock(st.Dur)
	barLen := colW - lipgloss.Width(left) - lipgloss.Width(right)
	out = append(out, stageLine{text: m.progress(colW), hit: func(x, y int) {
		m.geo.bar = rect{x + lipgloss.Width(left), y, x + lipgloss.Width(left) + barLen, y + 1}
	}})
	add("")
	out = append(out, m.controls(colW))
	return out
}

// controls: transport on the left with play as the one filled button,
// the playback modes on the right, lit when on.
func (m *Model) controls(w int) stageLine {
	st := m.state
	playIcon := icPlay
	if st.Playing {
		playIcon = icPause
	}
	prev := sDim.Render(" " + icPrev + " ")
	play := sPlays.Render("") + sPill.Render(" "+playIcon+" ") + sPlays.Render("")
	next := sDim.Render(" " + icNext + " ")
	transport := prev + " " + play + " " + next

	lit := func(on bool, s string) string {
		if on {
			return s
		}
		return sDim.Render(s)
	}
	repeatIcon := icRepeat
	if st.Repeat == 1 {
		repeatIcon = icRepeatOne
	}
	volIcon := icVolume
	if st.Volume == 0 {
		volIcon = icMuted
	}
	shuffle := lit(st.Shuffle, icShuffle)
	repeat := lit(st.Repeat != 0, repeatIcon)
	volume := sDim.Render(fmt.Sprintf("%s %d", volIcon, int(math.Round(st.Volume*100))))
	modes := shuffle + "   " + repeat + "   " + volume

	gap := max(2, w-lipgloss.Width(transport)-lipgloss.Width(modes))
	return stageLine{text: transport + strings.Repeat(" ", gap) + modes, hit: func(x, y int) {
		m.geo.prev = rect{x, y, x + 3, y + 1}
		m.geo.play = rect{x + 4, y, x + 9, y + 1}
		m.geo.next = rect{x + 10, y, x + 13, y + 1}
		mx := x + lipgloss.Width(transport) + gap
		m.geo.shuffle = rect{mx - 1, y, mx + 2, y + 1}
		m.geo.repeat = rect{mx + 3, y, mx + 6, y + 1}
		m.geo.volume = rect{mx + 7, y, mx + 7 + lipgloss.Width(volume), y + 1}
	}}
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

// spectrum draws the live bands as solid bars, eighth-block precise, with
// a resting baseline so the shape stays visible when quiet.
func (m *Model) spectrum(w, rows int) []string {
	levels := make([]float64, w)
	if n := len(m.spec); n > 0 {
		for i := range levels {
			x := float64(i) * float64(n-1) / float64(max(1, w-1))
			lo := int(x)
			hi := min(lo+1, n-1)
			f := x - float64(lo)
			levels[i] = m.spec[lo]*(1-f) + m.spec[hi]*f
		}
	}
	ramp := []rune(" ▁▂▃▄▅▆▇█")
	out := make([]string, rows)
	for r := range rows {
		row := rows - 1 - r // 0 is the bottom row
		var sb strings.Builder
		for _, lv := range levels {
			fill := lv*float64(rows*8) - float64(row*8)
			ch := ' '
			switch {
			case fill >= 8:
				ch = '█'
			case fill >= 0.5:
				ch = ramp[int(math.Round(fill))]
			case row == 0:
				ch = '▁'
			}
			sb.WriteRune(ch)
		}
		out[r] = sMusic.Render(sb.String())
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
		sHere.Render(strings.Repeat("━", done)+"●") +
		sDim.Render(strings.Repeat("─", max(0, bar-done-1))+right)
}

func (m *Model) helpLines() []stageLine {
	groups := []struct {
		name string
		keys [][2]string
	}{
		{"browse", [][2]string{{"↑↓ jk", "move"}, {"enter l", "open / play"}, {"esc h", "back"}, {"1–5 tab", "sections"}, {"/", "search"}, {"c", "go to what's playing"}}},
		{"play", [][2]string{{"space", "play / pause"}, {"n p", "next / previous"}, {"← →", "seek 10 s"}, {"s", "shuffle"}, {"r", "repeat off / all / one"}, {"+ - m", "volume, mute"}}},
		{"brumm", [][2]string{{"q", "close, music keeps playing"}, {"Q", "stop brumm"}, {"L", "sign in again"}}},
	}
	var out []stageLine
	for i, g := range groups {
		if i > 0 {
			out = append(out, stageLine{})
		}
		out = append(out, stageLine{text: sBold.Render(g.name)})
		for _, kv := range g.keys {
			out = append(out, stageLine{text: sKey.Render(fmt.Sprintf("%-9s", kv[0])) + "  " + sDim.Render(kv[1])})
		}
	}
	return append(out, stageLine{}, stageLine{text: sDim.Render("mouse: click to open or play, right click to go back")})
}

// miniPlayer is the two-line player for narrow windows.
func (m *Model) miniPlayer(w int) []string {
	st := m.state
	if st.Title == "" {
		return []string{"", sDim.Render("nothing playing")}
	}
	icon := sPlays.Render(icPause)
	if !st.Playing {
		icon = sDim.Render(icPlay)
	}
	line := icon + "  " + sBold.Render(st.Title) + sDim.Render("  "+st.Artist)
	return []string{"", ansi.Truncate(line, w, "…"), m.progress(w)}
}

// ── drawing helpers ─────────────────────────────────────────────────────

// box draws a rounded panel of exactly w×h cells: the breadcrumb set into
// the top border (all but its last part dimmed) and a label in the bottom.
func box(title string, crumbed bool, label string, lines []string, w, h int) string {
	b := sDim
	t := ansi.Truncate(title, max(1, w-8), "…")
	if i := strings.LastIndex(t, " / "); crumbed && i >= 0 {
		t = sDim.Render(t[:i+3]) + sBold.Render(t[i+3:])
	} else {
		t = sBold.Render(t)
	}
	top := b.Render("╭─ ") + t + " "
	out := []string{top + b.Render(strings.Repeat("─", max(0, w-1-lipgloss.Width(top)))+"╮")}
	for i := range h - 2 {
		line := ""
		if i < len(lines) {
			line = lines[i]
		}
		out = append(out, b.Render("│")+"  "+pad(ansi.Truncate(line, w-6, "…"), w-6)+"  "+b.Render("│"))
	}
	bottom := b.Render("╰")
	if label != "" {
		lab := " " + sDim.Render(label) + " "
		bottom += b.Render(strings.Repeat("─", max(0, w-4-lipgloss.Width(lab)))) + lab + b.Render("─╯")
	} else {
		bottom += b.Render(strings.Repeat("─", w-2) + "╯")
	}
	return strings.Join(append(out, bottom), "\n")
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
