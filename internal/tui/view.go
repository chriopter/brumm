package tui

import (
	"fmt"
	"math"
	"strings"
	"time"

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
	list, crumb, search, divider                   rect
	tabs                                           [numSections]rect
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

	// The list takes its share of the width — half unless dragged — and the
	// stage the rest, with the cover's column centered in it.
	gapW := 2 + 2*stagePad
	navW := int(math.Round(float64(inner) * m.split))
	navW = max(navMin, min(navW, inner-gapW-colMin))
	room := inner - navW - gapW

	var body string
	if room < colMin || navW < navMin {
		mini := m.miniPlayer(inner)
		body = lipgloss.JoinVertical(lipgloss.Left, append([]string{m.nav(inner, bodyH-len(mini), margin, bodyTop)}, mini...)...)
	} else {
		coverH := max(0, bodyH-1-stageBelow)
		coverH = min(coverH, int(float64(room)/m.cellAspect))
		colW := max(colMin, min(room, int(math.Round(float64(coverH)*m.cellAspect))))
		inset := (room - colW) / 2
		stageX := margin + navW + gapW + inset
		m.geo.divider = rect{margin + navW - 1, bodyTop, margin + navW + gapW, bodyTop + bodyH}
		stage := strings.Split(m.stage(colW, coverH, bodyH, stageX, bodyTop), "\n")
		for i := range stage {
			stage[i] = strings.Repeat(" ", inset) + stage[i] + strings.Repeat(" ", room-colW-inset)
		}
		// Both sides are exactly as wide as their column, so rows join
		// without measuring the long colored cover lines again.
		nav := strings.Split(m.nav(navW, bodyH, margin, bodyTop), "\n")
		gap := strings.Repeat(" ", gapW)
		rows := make([]string, max(len(nav), len(stage)))
		for i := range rows {
			l, r := strings.Repeat(" ", navW), strings.Repeat(" ", room)
			if i < len(nav) {
				l = nav[i]
			}
			if i < len(stage) {
				r = stage[i]
			}
			rows[i] = l + gap + r
		}
		body = strings.Join(rows, "\n")
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
	case m.state.Update != "" && m.flash == "":
		right = sDim.Render(m.state.Update+" is available · ") + sKey.Render("U") + sDim.Render(" update") + "   " + right
	case m.state.ExpiresIn > 0 && m.flash == "":
		right = sErr.Render(fmt.Sprintf("Apple Music access in this build ends in %d days: press U to update", m.state.ExpiresIn)) + "   " + right
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

	// Tabs: the active one where-you-are colored, the rest dim; spaced
	// tighter when the list is narrow.
	sep := 3
	if total := len(strings.Join(sectionNames, "")) + sep*(len(sectionNames)-1); total > inner-2 {
		sep = 2
	}
	var tabs []string
	tx := tx0
	for i, name := range sectionNames {
		label := sDim.Render(name)
		if section(i) == m.section {
			label = sHere.Bold(true).Render(name)
		}
		m.geo.tabs[i] = rect{tx, y + 1, tx + len(name), y + 2}
		tabs = append(tabs, label)
		tx += len(name) + sep
	}
	lines := []string{"  " + strings.Join(tabs, strings.Repeat(" ", sep)), ""}

	rows := h - 2 - len(lines) - 1
	top := y + 1 + len(lines)
	if m.section == secSearch && len(m.stack()) == 1 {
		lines = append(lines, "  "+m.searchBox(inner-2), "")
		m.geo.search = rect{tx0, top, x + w - 1, top + 1}
		rows -= 2
		top += 2
	}

	head := -1
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
		head = len(lines) // list rows below are fitted as they are built
		v.off = scroll(v.sel, v.off, rows)
		m.geo.list = rect{x + 1, top, x + w - 1, top + rows}
		// Lists of playlists, albums and artists are short names with room
		// to spare: the right side shows a card for the selected one.
		card, cw := m.previewCard(v, inner, rows)
		listW := inner
		if card != nil {
			listW = inner - cw - 3
		}
		for i := v.off; i < v.off+rows; i++ {
			line := ""
			if i < len(v.rows) {
				line = m.row(v, i, listW)
			} else if card == nil {
				break
			}
			if card != nil {
				c := ""
				if j := i - v.off; j < len(card) {
					c = card[j]
				}
				if c == "" {
					c = strings.Repeat(" ", cw)
				}
				line = fit(line, listW) + "   " + c // card lines are cw wide
			} else {
				line = fit(line, inner)
			}
			lines = append(lines, line)
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
	if head < 0 {
		head = len(lines)
	}
	for i := range head {
		lines[i] = fit(lines[i], inner)
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

// previewCard is the selected item's cover with its name under it, for
// lists of items wide enough to hold it; nil otherwise.
func (m *Model) previewCard(v *view, inner, rows int) ([]string, int) {
	if v.sel >= len(v.rows) || v.rows[v.sel].item == nil || inner < 70 || rows < 12 {
		return nil, 0
	}
	it := v.rows[v.sel].item
	cw := min(inner*2/5, 44)
	chh := min(rows-3, int(float64(cw)/m.cellAspect))
	cw = int(math.Round(float64(chh) * m.cellAspect))
	if chh < 6 {
		return nil, 0
	}
	var out []string
	size := art.Size{Width: cw, Height: chh}
	key := fmt.Sprintf("%s@%dx%d", it.Artwork, cw, chh)
	if lines, ok := m.thumbs[key]; ok {
		out = append(out, lines...)
	} else {
		m.thumbWant = thumbReq{key, it.Artwork, size}
		icon := map[string]string{apple.KindPlaylist: icPlaylist, apple.KindAlbum: icAlbum, apple.KindArtist: icArtist}[it.Kind]
		for i := range chh {
			l := strings.Repeat("░", cw)
			if i == chh/2 {
				l = strings.Repeat("░", cw/2-1) + " " + icon + " " + strings.Repeat("░", cw-cw/2-2)
			}
			out = append(out, sDim.Render(l))
		}
	}
	// Every line is cw wide: the cover by construction, the rest padded.
	out = append(out, strings.Repeat(" ", cw), fit(sBold.Render(ansi.Truncate(it.Name, cw, "…")), cw))
	if it.Artist != "" {
		out = append(out, fit(sDim.Render(ansi.Truncate(it.Artist, cw, "…")), cw))
	}
	return out, cw
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
		// Fixed columns on the right — meter, heart, duration — so hearts
		// and times line up whatever else a row shows.
		title := t.Title
		meter, heart := "   ", " "
		switch playing := t.ID != "" && t.ID == m.state.ID; {
		case playing:
			title = sPlays.Bold(sel).Render(t.Title)
			meter = sPlays.Render(m.miniEQ())
		case sel:
			title = sBold.Render(t.Title)
		}
		if m.loved[t.ID] {
			heart = sErr.Render("♥")
		}
		text = title + "  " + sDim.Render(t.Artist)
		detail = meter + "  " + heart + "  " + sDim.Render(fmt.Sprintf("%5s", clock(t.Duration)))
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
	width  int // known display width; skips measuring long colored lines
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
		text, w := l.text, l.width
		if w == 0 || w > colW {
			text = ansi.Truncate(l.text, colW, "…")
			w = lipgloss.Width(text)
		}
		left := 0
		if l.center {
			left = max(0, (colW-w)/2)
		}
		out[row] = strings.Repeat(" ", left) + text + strings.Repeat(" ", max(0, colW-left-w))
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
		// Playing: title and artist above the cover, progress and controls
		// below, the group centered so the cover sits in the middle. When
		// the cover fills the height, the group spans the panel exactly.
		head, rest := bottom, []stageLine(nil)
		if len(bottom) > 3 {
			head, rest = bottom[:3], bottom[3:] // title, meta, gap
		}
		group := append(append([]stageLine{}, head...), top...)
		if len(top) > 0 {
			group = append(group, stageLine{})
		}
		group = append(group, rest...)
		start := max(0, (h-len(group))/2) // the cover sits in the vertical middle
		for i, l := range group {
			put(start+i, l)
		}
	}
	return strings.Join(out, "\n")
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

func (m *Model) helpLines() []stageLine {
	groups := []struct {
		name string
		keys [][2]string
	}{
		{"browse", [][2]string{{"↑↓ jk", "move"}, {"enter l", "open / play"}, {"esc h", "back"}, {"← → 1–6", "sections"}, {"/", "search (paste a music.apple.com link to open it)"}, {"6", "queue"}, {"a A", "the song's album / artist"}, {"c", "go to what's playing"}}},
		{"play", [][2]string{{"space", "play / pause"}, {"hold space  o", "preview the selected song"}, {"n p", "next / previous (p restarts after 3 s)"}, {"z Z", "add to queue / play next"}, {"*", "favorite ♥"}, {"y", "copy the song's link"}, {"shift ← →", "seek 10 s"}, {"s", "shuffle"}, {"r", "repeat off / all / one"}, {"+ - m", "volume, mute"}}},
		{"brumm", [][2]string{{"f", "fullscreen visualizer (v: next)"}, {"[ ]", "narrower / wider list"}, {"q", "close, music keeps playing"}, {"Q", "stop brumm"}, {"L", "sign in again"}, {"U", "install an available update"}}},
	}
	var out []stageLine
	for i, g := range groups {
		if i > 0 {
			out = append(out, stageLine{})
		}
		out = append(out, stageLine{text: sBold.Render(g.name)})
		for _, kv := range g.keys {
			out = append(out, stageLine{text: sKey.Render(fmt.Sprintf("%-13s", kv[0])) + "  " + sDim.Render(kv[1])})
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
		if line == "" {
			line = strings.Repeat(" ", inner)
		}
		out = append(out, b.Render("│")+line+" "+b.Render("│")) // lines come fitted to inner
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

// fit truncates or pads s to exactly w cells.
func fit(s string, w int) string {
	return pad(ansi.Truncate(s, w, "…"), w)
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
	h := m.height - 1
	lines := m.viz.render(m.vizStyle, m.vizSpec, m.wave, m.width, h, m.frame, m.state.Playing)
	if f := float64(time.Since(m.vizAt)) / float64(vizFade); m.vizPrev >= 0 && f < 1 {
		old := m.viz.render(m.vizPrev, m.vizSpec, m.wave, m.width, h, m.frame, m.state.Playing)
		lines = dissolve(old, lines, m.width, f)
	} else {
		m.vizPrev = -1
	}
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

// dissolve blends two frames: the screen is cut into blocks, each switching
// from a to b once f passes its own threshold, so the new picture appears
// as a scatter of tiles that fills in.
func dissolve(a, b []string, w int, f float64) []string {
	const bw, bh = 6, 2 // block size in cells
	out := make([]string, len(b))
	for y := range b {
		if y >= len(a) {
			out[y] = b[y]
			continue
		}
		var sb strings.Builder
		for x := 0; x < w; x += bw {
			src := a[y]
			// A fixed pseudo-random threshold per block.
			hsh := uint32(x/bw)*2654435761 ^ uint32(y/bh)*2246822519
			hsh ^= hsh >> 15
			if float64(hsh%1000)/1000 < f {
				src = b[y]
			}
			sb.WriteString(ansi.Cut(src, x, min(w, x+bw)))
		}
		out[y] = sb.String()
	}
	return out
}
