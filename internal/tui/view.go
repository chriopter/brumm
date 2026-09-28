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

	// Stage lines under the cover: title, meta, gap, spectrum, gap,
	// progress, gap, controls.
	specRows   = 4
	stageBelow = 3 + specRows + 4
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
	content := m.render()
	if m.full && m.width >= 20 && m.height >= 6 {
		content = m.fullscreen()
	}
	v := tea.NewView(content)
	v.AltScreen = true
	v.MouseMode = tea.MouseModeCellMotion
	v.KeyboardEnhancements.ReportEventTypes = true // hold space to preview
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
	coverH := max(0, bodyH-1-stageBelow)
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
	keys := [][2]string{{"enter", "play"}, {"space", "hold to preview"}, {"esc", "back"}, {"f", "visualizer"}, {"?", "keys"}}
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
	inner := w - 3 // between the borders, less one cell of right padding
	tx0 := x + 3   // text column: under the breadcrumb, after the gutter
	v := m.cur()

	// Tabs: the active one where-you-are colored, the rest dim.
	var tabs []string
	tx := tx0
	for i, name := range sectionNames {
		label := sDim.Render(name)
		if section(i) == m.section {
			label = sHere.Bold(true).Render(name)
		}
		m.geo.tabs[i] = rect{tx, y + 1, tx + len(name), y + 2}
		tabs = append(tabs, label)
		tx += len(name) + 3
	}
	lines := []string{"  " + strings.Join(tabs, "   "), ""}

	rows := h - 2 - len(lines) - 1
	top := y + 1 + len(lines)
	if m.section == secSearch && len(m.stack()) == 1 {
		lines = append(lines, "  "+m.searchBox(inner-2), "")
		m.geo.search = rect{tx0, top, x + w - 1, top + 1}
		rows -= 2
		top += 2
	}

	msg := func(s string) []string { return centered(s, inner, rows) }
	switch {
	case v.err != nil && len(v.rows) == 0:
		lines = append(lines, msg(sErr.Render(v.err.Error()))...)
	case !v.loaded && m.state.Status == ipc.StatusLoggedOut:
		lines = append(lines, msg(sDim.Render("sign in to see your library"))...)
	case !v.loaded:
		lines = append(lines, msg(sDim.Render(spinner[m.frame%len(spinner)]+"  loading"))...)
	case v.key == "search:":
		lines = append(lines, msg(sDim.Render("search Apple Music: type and press ")+sKey.Render("enter"))...)
	case len(v.rows) == 0:
		lines = append(lines, msg(sDim.Render("nothing here"))...)
	default:
		v.off = scroll(v.sel, v.off, rows)
		m.geo.list = rect{x + 1, top, x + w - 1, top + rows}
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
		m.geo.crumb = rect{tx0, y, tx0 + lipgloss.Width(parent), y + 1}
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

// row renders one list line: the selection gutter, the text (the
// selected row scrolls when it does not fit) and a right-aligned detail.
func (m *Model) row(v *view, i, w int) string {
	r := v.rows[i]
	sel := i == v.sel
	gutter := "  "
	if sel {
		gutter = sHere.Render("▌") + " "
	}

	var text, detail string
	if t := r.track; t != nil {
		title := t.Title
		dur := sDim.Render(fmt.Sprintf("%5s", clock(t.Duration)))
		switch playing := t.ID != "" && t.ID == m.state.ID; {
		case playing:
			title = sPlays.Bold(sel).Render(t.Title)
			dur = sPlays.Render(m.miniEQ()) + " " + dur
		case sel:
			title = sBold.Render(t.Title)
		}
		text = title + "  " + sDim.Render(t.Artist)
		detail = dur
	} else {
		it := r.item
		icon := map[string]string{apple.KindPlaylist: icPlaylist, apple.KindAlbum: icAlbum, apple.KindArtist: icArtist}[it.Kind]
		name := sDim.Render(icon) + "  " + it.Name
		if sel {
			name = sHere.Render(icon) + "  " + sBold.Render(it.Name)
		}
		text = name
		if it.Artist != "" {
			text += "  " + sDim.Render(it.Artist)
		}
		if it.Key() == m.state.Source && m.state.Title != "" {
			detail = sPlays.Render("♪")
		}
	}
	avail := w - 2 - 2 - lipgloss.Width(detail)
	if sel {
		text = marquee(text, avail, m.frame-v.selAt)
	} else {
		text = ansi.Truncate(text, avail, "…")
	}
	return gutter + pad(text, avail) + "  " + detail
}

// miniEQ is a three-bar level meter for the playing row.
func (m *Model) miniEQ() string {
	if !m.state.Playing || len(m.spec) == 0 {
		return "  ♪"
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
	var top, bottom []stageLine
	add := func(s string) { top = append(top, stageLine{text: s}) }
	playing := st.Title != "" || st.Preview != nil
	switch {
	case m.help:
		top = m.helpLines()
	case st.Status == ipc.StatusLoggedOut:
		add(sHere.Render("ʕ•ᴥ•ʔ"))
		add("")
		add(sBold.Render("Sign in to Apple Music"))
		add(sDim.Render("press ") + sKey.Render("L") + sDim.Render(" to open the sign-in page"))
	case !playing:
		add(sHere.Render("ʕ-ᴥ-ʔ zZ"))
		add("")
		add(sBold.Render("nothing playing"))
		add(sDim.Render("pick something and press ") + sKey.Render("enter"))
	default:
		top, bottom = m.nowPlaying(colW, coverH)
	}

	out := make([]string, h)
	for i := range out {
		out[i] = strings.Repeat(" ", colW)
	}
	put := func(row int, l stageLine) {
		if row < 0 || row >= h {
			return
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
	if bottom == nil {
		// Idle and help: centered in the stage.
		start := max(0, (h-len(top))/2)
		if m.help {
			start = 1
		}
		for i, l := range top {
			put(start+i, l)
		}
	} else {
		// Playing: the cover's top meets the panel's top border, the
		// controls sit on its bottom border.
		for i, l := range top {
			put(i, l)
		}
		for i, l := range bottom {
			put(h-len(bottom)+i, l)
		}
	}
	return strings.Join(out, "\n")
}

// nowPlaying returns the stage in two parts: the cover (top-anchored) and
// the title, spectrum, progress and controls (bottom-anchored).
func (m *Model) nowPlaying(colW, coverH int) (top, bottom []stageLine) {
	st := m.state
	add := func(s string) { bottom = append(bottom, stageLine{text: s}) }

	if coverH >= 6 {
		coverW := min(colW, int(math.Round(float64(coverH)*m.cellAspect)))
		for _, l := range m.coverLines(art.Size{Width: coverW, Height: coverH}) {
			top = append(top, stageLine{text: l, center: true})
		}
	}
	title, artist, album := st.Title, st.Artist, st.Album
	if p := st.Preview; p != nil {
		title, artist, album = p.Title, p.Artist, ""
	}
	if st.Preview != nil {
		add(sPill.Render(" preview ") + "  " + sBold.Render(title))
	} else {
		add(sBold.Render(title))
	}
	meta := artist
	if album != "" && album != title {
		meta += sDim.Render("  ·  " + album)
	}
	add(meta)
	add("")
	for _, l := range m.spectrum(colW, specRows) {
		add(l)
	}
	add("")
	left, right := clock(m.position())+"  ", "  "+clock(st.Dur)
	barLen := colW - lipgloss.Width(left) - lipgloss.Width(right)
	bottom = append(bottom, stageLine{text: m.progress(colW), hit: func(x, y int) {
		m.geo.bar = rect{x + lipgloss.Width(left), y, x + lipgloss.Width(left) + barLen, y + 1}
	}})
	add("")
	bottom = append(bottom, m.controls(colW))
	return top, bottom
}

// controls: shuffle, previous, play, next and repeat centered under the
// cover, play the one filled button; volume at the right edge.
func (m *Model) controls(w int) stageLine {
	st := m.state
	playIcon := icPlay
	if st.Playing {
		playIcon = icPause
	}
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
	const gap = "   "
	play := sPlays.Render("") + sPill.Render(" "+playIcon+" ") + sPlays.Render("")
	group := lit(st.Shuffle, icShuffle) + gap + icPrev + gap + play + gap + icNext + gap + lit(st.Repeat != 0, repeatIcon)
	gw := lipgloss.Width(group)
	volume := sDim.Render(fmt.Sprintf("%s %d", volIcon, int(math.Round(st.Volume*100))))
	start := max(0, (w-gw)/2)
	line := strings.Repeat(" ", start) + group
	if room := w - lipgloss.Width(line) - lipgloss.Width(volume); room >= 2 {
		line += strings.Repeat(" ", room) + volume
	}
	return stageLine{text: line, hit: func(x, y int) {
		// Cells: shuffle 0, prev 4, play 8–12, next 16, repeat 20.
		x += start
		m.geo.shuffle = rect{x - 1, y, x + 2, y + 1}
		m.geo.prev = rect{x + 3, y, x + 6, y + 1}
		m.geo.play = rect{x + 7, y, x + 14, y + 1}
		m.geo.next = rect{x + 15, y, x + 18, y + 1}
		m.geo.repeat = rect{x + 19, y, x + 22, y + 1}
		vx := x - start + w - lipgloss.Width(volume)
		m.geo.volume = rect{vx, y, vx + lipgloss.Width(volume), y + 1}
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

// progress is one rule in two colors — played and to come — with the
// times at its ends, like a meter in btop rather than a web slider.
func (m *Model) progress(w int) string {
	pos, dur := m.position(), m.state.Dur
	left, right := clock(pos)+"  ", "  "+clock(dur)
	bar := w - lipgloss.Width(left) - lipgloss.Width(right)
	done := 0
	if dur > 0 {
		done = min(bar, int(math.Round(float64(bar)*pos/dur)))
	}
	return sDim.Render(left) + sHere.Render(strings.Repeat("━", done)) +
		sDim.Render(strings.Repeat("━", bar-done)+right)
}

func (m *Model) helpLines() []stageLine {
	groups := []struct {
		name string
		keys [][2]string
	}{
		{"browse", [][2]string{{"↑↓ jk", "move"}, {"enter l", "open / play"}, {"esc h", "back"}, {"1–5 tab", "sections"}, {"/", "search"}, {"a", "go to the song's album"}, {"c", "go to what's playing"}}},
		{"play", [][2]string{{"space", "play / pause"}, {"hold space", "preview the selected song"}, {"n p", "next / previous"}, {"← →", "seek 10 s"}, {"s", "shuffle"}, {"r", "repeat off / all / one"}, {"+ - m", "volume, mute"}}},
		{"brumm", [][2]string{{"f", "fullscreen visualizer (v: next)"}, {"q", "close, music keeps playing"}, {"Q", "stop brumm"}, {"L", "sign in again"}}},
	}
	var out []stageLine
	for i, g := range groups {
		if i > 0 {
			out = append(out, stageLine{})
		}
		out = append(out, stageLine{text: sBold.Render(g.name)})
		for _, kv := range g.keys {
			out = append(out, stageLine{text: sKey.Render(fmt.Sprintf("%-11s", kv[0])) + "  " + sDim.Render(kv[1])})
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
// Lines start right after the left border; they carry their own gutter.
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
	inner := w - 3
	for i := range h - 2 {
		line := ""
		if i < len(lines) {
			line = lines[i]
		}
		out = append(out, b.Render("│")+pad(ansi.Truncate(line, inner, "…"), inner)+" "+b.Render("│"))
	}
	bottom := "╰" + strings.Repeat("─", w-2) + "╯"
	if label != "" {
		lab := " " + label + " "
		fill := max(0, w-3-lipgloss.Width(lab))
		out = append(out, b.Render("╰"+strings.Repeat("─", fill))+sDim.Render(lab)+b.Render("─╯"))
		return strings.Join(out, "\n")
	}
	return strings.Join(append(out, b.Render(bottom)), "\n")
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

// fullscreen is the visualizer over the whole terminal with one quiet line
// of what is playing.
func (m *Model) fullscreen() string {
	// The stage's spectrum is tuned hot for a few rows; spread it out so
	// full-height bars mean loud, not merely audible.
	if len(m.vizSpec) != len(m.spec) {
		m.vizSpec = make([]float64, len(m.spec))
	}
	for i, v := range m.spec {
		m.vizSpec[i] = v * v * v
	}
	lines := m.viz.render(m.vizStyle, m.vizSpec, m.wave, m.width, m.height-1, m.frame, m.state.Playing)
	st := m.state
	info := sDim.Render("nothing playing")
	if st.Title != "" {
		info = sBold.Render(st.Title) + sDim.Render("  "+st.Artist+"  ·  "+clock(m.position())+" / "+clock(st.Dur))
	}
	hint := sDim.Render(vizNames[m.vizStyle]+"  ") + sKey.Render("v") + sDim.Render(" next  ") + sKey.Render("f") + sDim.Render(" close")
	if m.flash != "" {
		hint = sDim.Render(m.flash+"   ") + hint
	}
	gap := max(1, m.width-4-lipgloss.Width(info)-lipgloss.Width(hint))
	status := "  " + ansi.Truncate(info+strings.Repeat(" ", gap)+hint, m.width-4, "…")
	return strings.Join(append(lines, status), "\n")
}
