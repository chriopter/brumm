package tui

import (
	"fmt"
	"math"
	"slices"
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
// decides them; the cover, and the accent it lends (accent.go), are the
// exceptions. Each has one job: magenta is where you are, green is what
// plays, cyan is a key, blue is the music itself, bright black is
// everything secondary.
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
	icHome      = "󰋜"
	icSong      = "󰎈"
	icQueue     = "󰐑"
	icStation   = "󰐹"
	icShelf     = "󰄨"
	icDislike   = "󰔑"
)

// kindIcon is the glyph in front of an item of kind.
func kindIcon(kind string) string {
	switch kind {
	case apple.KindPlaylist:
		return icPlaylist
	case apple.KindAlbum:
		return icAlbum
	case apple.KindArtist:
		return icArtist
	case apple.KindStation:
		return icStation
	case apple.KindTerm:
		return icSearch
	}
	return icShelf
}

var spinner = []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}

const (
	margin   = 2  // columns left and right of everything
	bodyTop  = 1  // one blank line above the panels
	navMin   = 16 // the narrowest the list can be dragged: less shows nothing worth reading
	navRoom  = 44 // the list may take this much from the stage's controls
	durMin   = 40 // narrower, songs drop their lengths
	colMin   = 30 // narrower, the stage gives way to the mini player
	ctlMin   = 40 // the controls' width, however small the cover
	stagePad = 2
	listTop  = 3 // list rows start this far below the panel's top border

)

// rect is a clickable screen area, x1/y1 exclusive.
type rect struct{ x0, y0, x1, y1 int }

func (r rect) has(x, y int) bool { return x >= r.x0 && x < r.x1 && y >= r.y0 && y < r.y1 }

// footHit is a footer entry and the key it stands for.
type footHit struct {
	r      rect
	action string
}

// geometry records where things were drawn, for mouse hit-testing.
type geometry struct {
	list, crumb, search, divider                          rect
	tabs                                                  [numSections]rect
	prev, play, next, shuffle, repeat, queue, volume, bar rect
	artist, album                                         rect          // under the now-playing title
	upnext                                                [nextMax]rect // the covers coming up (upnext.go)
	upsongs                                               rect          // or the songs, one row each
	options                                               rect          // the options menu
	foot                                                  []footHit
	cards                                                 []cardHit
	nowPlaying                                            rect
	optRow0                                               int // its first row
}

func (m *Model) View() tea.View {
	if m.still && m.drawn.Content != "" {
		return m.drawn // nothing on screen changed: a wheel notch between frames, a hover
	}
	if m.motion && m.patch() {
		return m.drawn // only what moves was drawn again (frame.go)
	}
	m.fr.ok = false
	var content string
	if m.full && m.width >= 20 && m.height >= 6 {
		// The browser is not on screen: nothing of it is drawn or wanted.
		m.marquee, m.eqShown, m.cardShown = false, false, false
		m.kittyWant = m.kittyWant[:0]
		content = m.fullscreen()
	} else {
		content = m.render()
		m.fr.ok = m.fr.colW > 0 && !m.overlaid()
	}
	v := tea.NewView(content)
	v.AltScreen = true
	// Every move, not only drags: the pointer changes over buttons (pointer.go).
	v.MouseMode = tea.MouseModeAllMotion
	v.KeyboardEnhancements.ReportEventTypes = true // hold space to preview
	v.ReportFocus = true
	switch {
	case m.full && m.vizList:
		v.Content = m.overlay(content, m.vizListBox())
	case m.full:
	case m.featureForm != nil:
		v.Content = m.overlay(content, m.featureBox())
	case m.fb != nil:
		v.Content = m.overlay(content, m.feedbackBox())
	case m.upd != nil:
		v.Content = m.overlay(content, m.updateBox())
	case m.barAsking():
		v.Content = m.overlay(content, m.barBox())
	case m.pick != nil:
		v.Content = m.overlay(content, m.pickerBox())
	case m.optOpen:
		v.Content = m.overlay(content, m.optionsBox())
	}
	v.WindowTitle = "brumm"
	m.drawn = v
	return v
}

// listRows is how many list rows fit in the browser panel.
func (m *Model) listRows() int { return max(1, m.height-bodyTop-3-listTop-1) }

func (m *Model) render() string {
	m.geo = geometry{}
	m.marquee, m.eqShown, m.cardShown = false, false, false
	m.kittyWant = m.kittyWant[:0]
	f := &m.fr
	f.live, f.meter, f.times, f.colW = f.live[:0], -1, -1, 0
	if m.width < 40 || m.height < 14 {
		return sDim.Render("ʕ•ᴥ•ʔ brumm needs a bigger window")
	}
	m.syncAccent()
	inner := m.width - 2*margin
	bodyH := m.height - bodyTop - 2 // blank line + footer

	// The divider sets the cover's size: the stage is the cover's column
	// and no wider. What the cover cannot use, held back by the height,
	// goes to the list; the controls under it keep ctlMin.
	gapW := 2 + 2*stagePad
	pad := strings.Repeat(" ", margin)
	lines := []string{""}
	navW, colW, coverH := m.layout(inner, bodyH)
	if m.discovering() {
		dockH := min(8, bodyH/2)
		nav := strings.Split(m.nav(inner, bodyH-dockH, margin, bodyTop), "\n")
		dockW := colW
		if dockW == 0 {
			dockW = inner
		}
		dockX := inner - dockW
		dock := m.stage(dockW, 0, dockH, margin+dockX, bodyTop+bodyH-dockH)
		for i := range dock {
			dock[i] = strings.Repeat(" ", dockX) + dock[i]
		}
		for _, l := range append(nav, dock...) {
			lines = append(lines, pad+l)
		}
	} else if colW == 0 {
		mini := m.miniPlayer(inner)
		body := lipgloss.JoinVertical(lipgloss.Left, append([]string{m.nav(inner, bodyH-len(mini), margin, bodyTop)}, mini...)...)
		for _, l := range strings.Split(body, "\n") {
			lines = append(lines, pad+l)
		}
	} else {
		stageX := margin + navW + gapW
		m.geo.divider = rect{margin + navW - 1, bodyTop, margin + navW + gapW, bodyTop + bodyH}
		stage := m.stage(colW, coverH, bodyH, stageX, bodyTop)
		// Both sides are exactly as wide as their column, so rows join
		// without measuring the long colored cover lines again.
		nav := strings.Split(m.nav(navW, bodyH, margin, bodyTop), "\n")
		gap := strings.Repeat(" ", gapW)
		n := max(len(nav), len(stage))
		f.left, f.right = slices.Grow(f.left[:0], n)[:n], slices.Grow(f.right[:0], n)[:n]
		navBlank, colBlank := strings.Repeat(" ", navW), strings.Repeat(" ", colW)
		for i := range n {
			l, r := navBlank, colBlank
			if i < len(nav) {
				l = nav[i]
			}
			if i < len(stage) {
				r = stage[i]
			}
			f.left[i], f.right[i] = l, r
			lines = append(lines, pad+l+gap+r)
		}
		f.pad, f.gap, f.colW = pad, gap, colW
	}
	f.bear, f.frameAt, f.clock = m.bear(), m.frame, m.clockAt(colW)
	lines = append(lines, "", m.footer())
	f.lines, f.w, f.h = lines, m.width, m.height
	f.draws++
	return strings.Join(lines, "\n")
}

// layout splits inner cells between the list and the stage's column:
// the list's width, the column's, and the cover's height; colW is 0 when
// only the mini player fits.
func (m *Model) layout(inner, bodyH int) (navW, colW, coverH int) {
	gapW := 2 + 2*stagePad
	if inner-gapW-navRoom < colMin {
		return inner, 0, 0
	}
	// Where the divider asks to be, leaving the stage room for the controls.
	navW = int(math.Round(float64(inner) * m.split))
	navW = max(navMin, min(navW, max(navRoom, inner-gapW-ctlMin)))
	room := inner - navW - gapW
	coverH = max(0, min(bodyH-1-stageBelow, int(float64(room)/m.cellAspect)))
	coverW := min(room, int(math.Round(float64(coverH)*m.cellAspect)))
	colW = min(room, max(coverW, ctlMin))
	return inner - gapW - colW, colW, coverH
}

// ── footer ──────────────────────────────────────────────────────────────

func (m *Model) bear() string {
	if !m.state.Playing {
		return "ʕ-ᴥ-ʔ"
	}
	return []string{"ʕ•ᴥ•ʔ", "ʕ•ᴥ•ʔ♪", "ʕ·ᴥ·ʔ♫", "ʕ•ᴥ•ʔ♪"}[(m.frame/5)%4]
}

func (m *Model) footer() string {
	// Each entry is also a button: a click does what the key does.
	type entry struct{ key, label, action string }
	preview := entry{"space", "preview", "O"}
	if !m.releases {
		preview = entry{"O", "preview", "O"}
	}
	keys := []entry{{"enter", "play", "enter"}, {"/", "find", "/"}, preview, {"o", "options", "o"},
		{"f", "visualizer", "f"}, {"g", "gui", "g"}, {"?", "keys", "?"}, {"!", "feedback", "!"}}
	if m.showTips() { // the rest is on the buttons: the footer has what is not
		keys = []entry{{"esc", "back", "esc"}, {"f", "visualizer", "f"}, {"c", "now playing", "c"},
			{"Q", "close, music plays on", "Q"}, {"q", "quit", "q"}, {"?", "all keys", "?"}}
	}
	if m.searching { // the box has the keys
		keys = []entry{{"esc", "done", "esc"}, {"enter", "search", "enter"}}
	}
	if m.help {
		keys = []entry{{"esc", "back", "esc"}, {"?", "close the keys", "?"}}
	}
	if m.state.Status == ipc.StatusLoggedOut {
		keys = append([]entry{{"shift+L", "sign in", "L"}}, keys...)
	}
	render := func(e entry) string {
		return sKey.Render(e.key) + " " + sDim.Render(e.label)
	}
	width := func(es []entry) int {
		w := margin + 3
		for i, e := range es {
			w += lipgloss.Width(render(e))
			if i > 0 {
				w += 3
			}
		}
		return w
	}
	for len(keys) > 2 && width(keys) > m.width-margin-10 { // what does not fit is in the list; ? stays
		keys = append(keys[:len(keys)-2], keys[len(keys)-1])
	}
	parts := make([]string, len(keys))
	x := margin + 3
	m.geo.foot = m.geo.foot[:0]
	for i, e := range keys {
		parts[i] = render(e)
		w := lipgloss.Width(parts[i])
		m.geo.foot = append(m.geo.foot, footHit{rect{x, m.height - 1, x + w, m.height}, e.action})
		x += w + 3
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
	// tighter when the list is narrow, and only icons for the inactive
	// ones when even that does not fit.
	sep, icons := 3, false
	navNames := sectionNames[:secQueue]
	names := strings.Join(navNames, "")
	digits := 0 // "1 " before each tab while keytips show
	if m.showTips() {
		digits = 2 * len(navNames)
	}
	switch n := len(navNames) - 1; {
	case len(names)+digits+3*n <= inner-2:
	case len(names)+digits+2*n <= inner-2:
		sep = 2
	default:
		sep, icons = 2, true
	}
	var tabs []string
	tx := tx0
	for i, name := range navNames {
		if section(i) == secSearch && tx+15 < x+w-1 {
			style := sDim
			if m.playerView {
				style = sHere.Bold(true)
			}
			label := "♪ Now Playing"
			tabs = append(tabs, style.Render(label))
			m.geo.nowPlaying = rect{tx, y + 1, tx + 13, y + 2}
			tx += 13 + sep
		}
		active := section(i) == m.section
		if icons && !active {
			name = sectionIcons[i]
		}
		label := sDim.Render(name)
		if active {
			label = sHere.Bold(true).Render(name)
		}
		w := lipgloss.Width(name)
		if digits > 0 {
			label, w = m.tip(string(rune('1'+i)))+" "+label, w+2
		}
		m.geo.tabs[i] = rect{tx, y + 1, tx + w, y + 2}
		tabs = append(tabs, label)
		tx += w + sep
	}
	lines := []string{"  " + strings.Join(tabs, strings.Repeat(" ", sep)), ""}

	rows := h - 2 - len(lines) - 1
	top := y + 1 + len(lines)
	if m.section == secSearch && len(m.stack()) == 1 {
		lines = append(lines, "  "+m.searchBox(inner-2), "")
		m.geo.search = rect{tx0, top, x + w - 1, top + 1}
		rows -= 2
		top += 2
	} else if m.filtering || v.all != nil {
		lines = append(lines, "  "+m.filterBox(inner-2), "")
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
		if m.discovering() {
			lines = append(lines, m.discoveryRows(inner, rows, x+1, top)...)
		} else {
			v.off = scroll(v.sel, v.off, rows)
			m.geo.list = rect{x + 1, top, x + w - 1, top + rows}
			// Lists of playlists, albums and artists are short names with room
			// to spare: the right side shows a card for the selected one.
			var card []string // the selected item's card is on the stage now
			cw := 0
			listW := inner
			if card != nil {
				listW = inner - cw - 3
			}
			f := &m.fr
			f.v, f.off, f.sel, f.listW, f.inner = v, v.off, v.sel, listW, inner
			for i := v.off; i < v.off+rows; i++ {
				line := ""
				if i < len(v.rows) {
					eq, mq := m.eqShown, m.marquee
					m.eqShown, m.marquee = false, false
					line = m.row(v, i, listW)
					if m.eqShown || m.marquee { // it moves by itself: a patch draws it again
						m.fr.live = append(m.fr.live, liveRow{1 + len(lines), i, m.eqShown, m.marquee}) // under the box's top border
					}
					m.eqShown, m.marquee = m.eqShown || eq, m.marquee || mq
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
	}

	// Breadcrumb in the top border; position in the bottom one.
	var crumb []string
	for _, sv := range m.stack() {
		crumb = append(crumb, sv.title)
	}
	// Too long, it gives up its first parts: … / Deftones / Koi No Yokan.
	if lipgloss.Width(strings.Join(crumb, " / ")) > w-8 {
		for k := 1; k < len(crumb)-1; k++ {
			c := append([]string{"…"}, crumb[k:]...)
			if k == len(crumb)-2 || lipgloss.Width(strings.Join(c, " / ")) <= w-8 {
				crumb = c
				break
			}
		}
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
	label := sDim.Render(pos)
	if m.showTips() {
		if t := m.rowTips(v); t != "" {
			label = t + sDim.Render("   "+pos)
		}
	}
	return box(title, len(crumb) > 1, label, lines, w, h)
}

func (m *Model) searchBox(w int) string {
	q := m.query
	cursor := ""
	if m.searching {
		cursor = sHere.Render("▏")
	}
	if q == "" && !m.searching {
		return sDim.Render(icSearch+"  search Apple Music  ") + sKey.Render("/")
	}
	return sHere.Render(icSearch) + "  " + ansi.TruncateLeft(q, max(0, lipgloss.Width(q)-(w-5)), "…") + cursor
}

// row renders one list line: the selection gutter, the text (the
// selected row scrolls when it does not fit) and a right-aligned detail.
func (m *Model) row(v *view, i, w int) string {
	r := v.rows[i]
	switch {
	case r.head != "":
		return "  " + fit(sMusic.Bold(true).Render(r.head), w-2)
	case r.note != "":
		return "  " + fit(sDim.Render(r.note), w-2)
	case !r.selectable():
		return ""
	}
	sel := i == v.sel
	gutter := "  "
	if sel {
		gutter = m.acc.here.Render("▌") + " "
	}

	var text, detail string
	if t := r.track; t != nil {
		// Fixed columns on the right — heart, duration — so hearts and
		// times line up whatever else a row shows. The playing song has
		// its little equalizer in front of the title.
		title := t.Title
		heart := " "
		switch playing := t.ID != "" && t.ID == m.state.ID; {
		case playing:
			style := m.acc.plays
			if sel {
				style = m.acc.bold
			}
			title = m.miniEQ() + " " + style.Render(t.Title)
			m.eqShown = true
		case sel:
			title = sBold.Render(t.Title)
		}
		heart = m.ratingMark(t.ID)
		text = title + "  " + sDim.Render(t.Artist)
		if v.item != nil && v.item.Kind == apple.KindAlbum {
			n := t.Number
			if n == 0 { // cached before numbers were kept: count
				for _, r := range v.rows[:i+1] {
					if r.track != nil {
						n++
					}
				}
			}
			text = sDim.Render(fmt.Sprintf("%2d  ", n)) + title
			if t.Artist != v.item.Artist { // compilations, features
				text += "  " + sDim.Render(t.Artist)
			}
		}
		detail = heart
		if w >= durMin { // a narrow list keeps the titles, not the times
			detail += "  " + sDim.Render(fmt.Sprintf("%5s", clock(t.Duration)))
		}
	} else {
		it := r.item
		icon := kindIcon(it.Kind)
		name := sDim.Render(icon) + "  " + it.Name
		if sel {
			name = sHere.Render(icon) + "  " + sBold.Render(it.Name)
		}
		text = name
		if it.Artist != "" {
			text += "  " + sDim.Render(it.Artist)
		}
		playing := it.Key() == m.state.Source || (it.Kind == apple.KindStation && m.state.Source == "station:"+it.ID)
		detail = m.ratingMark(it.ID)
		if playing && m.state.Title != "" {
			detail = m.acc.plays.Render("♪") + " " + detail
		}
	}
	avail := w - 2 - 2 - lipgloss.Width(detail)
	if sel && !m.opts.ReduceMotion {
		m.marquee = m.marquee || lipgloss.Width(text) > avail
		text = marquee(text, avail, m.frame-v.selAt)
	} else {
		text = ansi.Truncate(text, avail, "…")
	}
	return gutter + pad(text, avail) + "  " + detail
}

// miniEQ is the playing row's four-bar equalizer: the real spectrum, or
// four bars wobbling on their own while none streams. Paused, the bars
// rest low and dim.
func (m *Model) miniEQ() string {
	if !m.state.Playing {
		return eqPaused
	}
	// The bars move at most every eqEvery: frames in between keep them.
	if now := time.Now(); m.eqBars == "" || now.Sub(m.eqAt) >= eqEvery {
		m.eqBars, m.eqAt = m.eqLevels(now), now
	}
	return m.acc.plays.Render(m.eqBars)
}

var eqPaused = sDim.Render("▁▂▁▃")

// eqLevels are the four bars now.
func (m *Model) eqLevels(now time.Time) string {
	ramp := []rune("▁▂▃▄▅▆▇█")
	var sb strings.Builder
	t := now.Sub(m.start).Seconds()
	for i := range 4 {
		var lv float64
		if n := len(m.spec); n > 0 {
			lv = m.spec[min(n-1, []int{1, n / 5, n * 2 / 5, n * 3 / 5}[i])]
		} else {
			// Two sines per bar at unrelated speeds never quite repeat.
			f := []float64{2.1, 3.3, 2.7, 3.9}[i]
			lv = 0.5 + 0.3*math.Sin(t*f+float64(i)*1.7) + 0.2*math.Sin(t*f*2.3+float64(i))
		}
		sb.WriteRune(ramp[max(0, min(len(ramp)-1, int(lv*float64(len(ramp)))))])
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
	live   int // it moves by itself: liveMeter, liveTimes (frame.go)
}

const (
	liveMeter = 1 + iota
	liveTimes
)

// stage draws the column colW wide, a line per row.
func (m *Model) stage(colW, coverH, h, x, y int) []string {
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
		add(sDim.Render("it opens in your browser; ") + sKey.Render("shift+L") + sDim.Render(" opens it again"))
	case !playing && m.idleCard(colW, coverH) != nil:
		top = m.idleCard(colW, coverH)
	case !playing:
		add(sHere.Render("ʕ-ᴥ-ʔ zZ"))
		add("")
		add(sBold.Render("nothing playing"))
		add(sDim.Render("pick something and press ") + sKey.Render("enter"))
	default:
		top, bottom = m.nowPlaying(colW, coverH)
	}

	out := make([]string, h)
	blank := strings.Repeat(" ", colW)
	for i := range out {
		out[i] = blank
	}
	put := func(row int, l stageLine) {
		if row < 0 || row >= h {
			return
		}
		var left int
		out[row], left = placeAt(l, colW)
		if l.hit != nil {
			l.hit(x+left, y+row)
		}
		switch l.live {
		case liveMeter:
			m.fr.meter = row
		case liveTimes:
			m.fr.times = row
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
		group = append(group, m.upNext(colW, h-len(group))...) // room to spare: what comes next
		start := max(0, (h-len(group))/2)                      // the cover sits in the vertical middle
		for i, l := range group {
			put(start+i, l)
		}
	}
	return out
}

// placeLine lays a stage line out in its column, colW wide.
func placeLine(l stageLine, colW int) string {
	s, _ := placeAt(l, colW)
	return s
}

// placeAt is placeLine, and where in the column the text starts.
func placeAt(l stageLine, colW int) (string, int) {
	text, w := l.text, l.width
	if w == 0 || w > colW {
		text = ansi.Truncate(l.text, colW, "…")
		w = lipgloss.Width(text)
	}
	left := 0
	if l.center {
		left = max(0, (colW-w)/2)
	}
	return strings.Repeat(" ", left) + text + strings.Repeat(" ", max(0, colW-left-w)), left
}

// coverLines draws the stage's cover at size; dim darkens it, to sit
// behind a card (kitty images cannot be dimmed and stay as they are).
func (m *Model) coverLines(size art.Size, dim bool) []string {
	if m.cover == nil {
		return placeholder(size, "") // while the cover downloads, same footprint
	}
	if m.opts.cover() == coverOriginal {
		if lines, ok := m.kittyLines(m.coverURL, size); ok {
			return lines
		}
	}
	key := size
	img := m.cover
	if dim {
		key.Height = -size.Height // the dimmed one's slot in the same cache
		if m.coverDim == nil {
			m.coverDim = dimmed(m.cover)
		}
		img = m.coverDim
	}
	if lines, ok := m.rendered[key]; ok {
		return lines
	}
	lines := drawCover(m.opts.cover(), img, size)
	if len(m.rendered) > 8 {
		m.rendered = map[art.Size][]string{}
	}
	m.rendered[key] = lines
	return lines
}

func (m *Model) helpLines() []stageLine {
	groups := []struct {
		name string
		keys [][2]string
	}{
		{"browse", [][2]string{{"↑↓ jk", "move"}, {"enter l", "open / play"}, {"esc h", "back"}, {"← → tab 1–9", "sections"}, {"/", "filter the list; tab searches all of Apple Music (or paste a link)"}, {"a A", "the song's album / artist (or click them)"}, {"c", "go to what's playing"}}},
		{"play", [][2]string{{"space", "play / pause"}, {"hold space  O", "preview the selected song"}, {"n p", "next / previous (p restarts after 3 s)"}, {"R", "radio: a station from the song or artist"}, {"z Z", "add to queue / play next"}, {"shift ← →", "seek 10 s"}, {"s", "shuffle"}, {"r", "repeat off / all / one"}, {"+ - m", "volume, mute"}}},
		{"library", [][2]string{{"* d", "love / dislike"}, {"i", "add to your library"}, {"P", "add to a playlist, or a new one"}, {"y", "copy the song's link"}}},
		{"brumm", [][2]string{{"?", "this list (keys on the buttons: in the options)"}, {"o", "options: covers, autoplay and more"}, {"f", "fullscreen visualizer (tab: next, v: all styles, a: auto-change, F: frame rate)"}, {"[ ]", "narrower / wider list"}, {"g", "go over to the gui (and g there comes back)"}, {"Q", "close, music keeps playing"}, {"q", "quit: stop the music"}, {"shift+L", "sign in again"}, {"U", "look for an update, install it"}, {"!", "send feedback: a new issue on GitHub"}}},
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
	icon := m.acc.plays.Render(icPause)
	if !st.Playing {
		icon = sDim.Render(icPlay)
	}
	line := icon + "  " + sBold.Render(st.Title) + sDim.Render("  "+st.Artist)
	return []string{"", ansi.Truncate(line, w, "…"), m.progress(w)}
}

// ── drawing helpers ─────────────────────────────────────────────────────

// boxSide is a panel's side border.
var boxSide = sDim.Render("│")

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
		out = append(out, boxSide+line+" "+boxSide) // lines come fitted to inner
	}
	bottom := "╰" + strings.Repeat("─", w-2) + "╯"
	if label != "" {
		lab := " " + ansi.Truncate(label, max(0, w-8), "…") + " " // comes styled
		fill := max(0, w-3-lipgloss.Width(lab))
		out = append(out, b.Render("╰"+strings.Repeat("─", fill))+lab+b.Render("─╯"))
		return strings.Join(out, "\n")
	}
	return strings.Join(append(out, b.Render(bottom)), "\n")
}

// wrap breaks plain text into at most n lines of w cells, ending the last
// with … when it does not all fit.
func wrap(s string, w, n int) []string {
	var out []string
	line := ""
	for _, word := range strings.Fields(s) {
		switch {
		case line == "":
			line = word
		case lipgloss.Width(line)+1+lipgloss.Width(word) <= w:
			line += " " + word
		default:
			out = append(out, line)
			line = word
		}
	}
	if line != "" {
		out = append(out, line)
	}
	if len(out) > n {
		out = out[:n]
		out[n-1] = ansi.Truncate(out[n-1]+" …", w, "…")
		if !strings.HasSuffix(out[n-1], "…") {
			out[n-1] += "…"
		}
	}
	for i := range out {
		out[i] = ansi.Truncate(out[i], w, "…")
	}
	return out
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
	// A new style dissolves in over the old one, block by block, mixed
	// in the visualizer's own grid.
	from, f := m.vizPrev, float64(time.Since(m.vizAt))/float64(vizFade)
	if from < 0 || f >= 1 {
		from, f, m.vizPrev = -1, 1, -1
	}
	lines := m.viz.renderMix(from, m.vizStyle, f, m.vizSpec, m.wave, m.width, h, m.state.Playing)
	return strings.Join(append(lines, m.vizStatusLine(h)), "\n")
}

// vizStatus is what the visualizer's status line shows: it is built
// again only when that changes, about once a second.
type vizStatus struct {
	title, artist, flash, fps string
	pos, dur, style, w, h     int
	auto                      bool
}

// vizStatusLine is the line under the visualizer at row h: what plays, and
// the buttons, each one a click target too.
func (m *Model) vizStatusLine(h int) string {
	st, pos := m.state, m.position()
	key := vizStatus{st.Title, st.Artist, m.flash, m.fpsLabel(), int(pos), int(st.Dur), m.vizStyle, m.width, h, !m.opts.NoVizCycle}
	if key == m.vizKey && m.vizLine != "" {
		m.geo = geometry{foot: m.vizFoot}
		return m.vizLine
	}
	info := sDim.Render("nothing playing")
	if st.Title != "" {
		info = sBold.Render(st.Title) + sDim.Render("  "+st.Artist+"  ·  "+clock(pos)+" / "+clock(st.Dur))
	}
	// The buttons on the right: each one a click target too.
	auto := sHere.Render("●")
	if m.opts.NoVizCycle {
		auto = sDim.Render("○")
	}
	buttons := []struct{ key, label, action string }{
		{"tab", vizNames[m.vizStyle], "tab"}, {"v", "styles", "v"}, {"a", "auto " + auto, "a"},
		{"F", m.fpsLabel(), "F"}, {"f", "close", "f"},
	}
	var parts []string
	for _, b := range buttons {
		parts = append(parts, sKey.Render(b.key)+" "+sDim.Render(b.label))
	}
	hint := strings.Join(parts, "   ")
	if m.flash != "" {
		hint = sDim.Render(m.flash+"   ") + hint
	}
	gap := max(1, m.width-4-lipgloss.Width(info)-lipgloss.Width(hint))
	status := "  " + ansi.Truncate(info+strings.Repeat(" ", gap)+hint, m.width-4, "…")
	m.geo = geometry{} // the browser is not on screen: nothing of it is clickable
	x := 2 + lipgloss.Width(info) + gap
	if m.flash != "" {
		x += lipgloss.Width(m.flash) + 3
	}
	for i, b := range buttons {
		w := lipgloss.Width(parts[i])
		if x+w <= m.width-2 {
			m.geo.foot = append(m.geo.foot, footHit{rect{x, h, x + w, h + 1}, b.action})
		}
		x += w + 3
	}
	m.vizKey, m.vizLine, m.vizFoot = key, status, m.geo.foot
	return status
}

// vizListBox is the list of styles, over the visualizer's lower left.
func (m *Model) vizListBox() []string {
	var lines []string
	for i, name := range vizNames {
		gutter := "  "
		if i == m.vizSel {
			gutter = sHere.Render("▌") + " "
		}
		dot := sDim.Render("○")
		if i == m.vizStyle {
			dot = sHere.Render("●")
		}
		digit := " " // 1–9 and 0 pick the first ten
		if i < 10 {
			digit = fmt.Sprint((i + 1) % 10)
		}
		lines = append(lines, gutter+sKey.Render(digit)+"  "+dot+" "+name)
	}
	auto := "on"
	if m.opts.NoVizCycle {
		auto = "off"
	}
	lines = append(lines, "", sKey.Render("enter")+sDim.Render(" keep  ")+sKey.Render("esc")+sDim.Render(" back  ")+sKey.Render("a")+sDim.Render(" auto: "+auto+"  ")+sKey.Render("v")+sDim.Render(" close"))
	w := 0
	for _, l := range lines {
		w = max(w, lipgloss.Width(l))
	}
	for i := range lines {
		lines[i] = " " + fit(lines[i], w+1)
	}
	return strings.Split(box("styles", false, "", lines, w+5, len(lines)+2), "\n")
}
