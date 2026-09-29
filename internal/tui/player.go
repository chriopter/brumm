package tui

import (
	"fmt"
	"math"
	"strings"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/art"
)

// The now-playing block of the stage: the cover on top, then a block
// anchored to the panel's bottom border:
//
//	Title                                    3/10
//	Artist · Album
//
//	━━━━━━━━━━━━━━━━━━━━╸━━━━━━━━━━━━━━━━━   meter, half-cell precise
//	2:44 / 5:08                         -2:24
//
//	󰒝  󰑖        󰒮   ▐ 󰏤 ▌   󰒭         󰕾 100
//
// Modes on the left, transport in the middle, volume on the right: three
// groups, the play button the one filled element.

// Lines besides the cover: title, meta, gap above it; meter, times, gap,
// transport below (view.go adds the gap between cover and meter).
const stageBelow = 2 + 1 + 2 + 1 + 1

// nowPlaying returns the stage in two parts: the cover (top-anchored) and
// the block under it (bottom-anchored); len(bottom) == stageBelow.
func (m *Model) nowPlaying(colW, coverH int) (top, bottom []stageLine) {
	st := m.state
	add := func(s string) { bottom = append(bottom, stageLine{text: s}) }

	if coverH >= 6 {
		coverW := min(colW, int(math.Round(float64(coverH)*m.cellAspect)))
		for _, l := range m.stageCover(art.Size{Width: coverW, Height: coverH}) {
			top = append(top, stageLine{text: l, center: true, width: coverW})
		}
	}

	title, artist, album := st.Title, st.Artist, st.Album
	var tag string
	switch {
	case st.Preview != nil:
		title, artist, album = st.Preview.Title, st.Preview.Artist, ""
		tag = sPill.Render(" PREVIEW ")
	case st.Length > 1 && st.Index >= 0:
		tag = sDim.Render(fmt.Sprintf("%d/%d", st.Index+1, st.Length))
	}
	heading := sBold.Render(title)
	if st.Preview == nil && m.rating[st.ID] != 0 {
		heading += "  " + m.ratingMark(st.ID)
	}
	add(spread(heading, tag, colW))
	meta := artist
	if album != "" && album != title {
		meta += sDim.Render("  ·  " + album)
	}
	aw := min(lipgloss.Width(artist), colW)
	hasAlbum := album != "" && album != title && aw+5 < colW
	bottom = append(bottom, stageLine{text: ansi.Truncate(meta, colW, "…"), hit: func(x, y int) {
		if st.Preview != nil {
			return
		}
		m.geo.artist = rect{x, y, x + aw, y + 1}
		if hasAlbum {
			m.geo.album = rect{x + aw + 5, y, x + min(colW, aw+5+lipgloss.Width(album)), y + 1}
		}
	}})
	// The gap under the names carries a and A while keytips show, when
	// they act on this song (the selected row is not another song).
	if m.showTips() && st.Preview == nil && m.tipsOnPlaying() {
		bs := []badge{{0, "A"}}
		if hasAlbum {
			bs = append(bs, badge{aw + 5, "a"})
		}
		add(m.badges(colW, bs))
	} else {
		add("")
	}
	if st.Preview != nil {
		// The clip's own position is not reported: a sweeping meter says
		// "a clip is playing", the line under it what comes back after.
		add(m.sweep(colW))
		back := ""
		if st.Title != "" {
			back = sDim.Render("then ") + st.Title
		}
		add(spread(sDim.Render("30 s clip"), back, colW))
	} else {
		bottom = append(bottom, stageLine{text: m.meter(colW), hit: func(x, y int) {
			m.geo.bar = rect{x, y, x + colW, y + 2} // the times row seeks too
		}})
		add(m.times(colW))
	}
	if m.showTips() {
		add(m.controlTips(colW))
	} else {
		add("")
	}
	bottom = append(bottom, m.controls(colW))
	return top, bottom
}

// spread puts left and right at the two edges of w cells, cutting left
// short when both do not fit.
func spread(left, right string, w int) string {
	rw := lipgloss.Width(right)
	if rw == 0 {
		return left
	}
	room := w - rw - 2
	if room < 1 {
		return ansi.Truncate(left, w, "…")
	}
	left = ansi.Truncate(left, room, "…")
	return left + strings.Repeat(" ", w-lipgloss.Width(left)-rw) + right
}

// meter is the progress rule: played in magenta, the rest dim, with a
// half-cell step (╸ ╺) so it moves every second on long songs too.
func (m *Model) meter(w int) string {
	pos, dur := m.position(), m.state.Dur
	halves := 0
	if dur > 0 {
		halves = min(2*w, int(float64(2*w)*pos/dur))
	}
	full, half := halves/2, halves%2 == 1
	var b strings.Builder
	b.WriteString(sHere.Render(strings.Repeat("━", full)))
	rest := w - full
	if half {
		b.WriteString(sHere.Render("╸"))
		rest--
		b.WriteString(sDim.Render(strings.Repeat("━", rest)))
	} else if rest > 0 {
		b.WriteString(sDim.Render("╺" + strings.Repeat("━", rest-1)))
	}
	return b.String()
}

// sweep is the meter while a preview clip plays: a short magenta segment
// travelling along a dim rule.
func (m *Model) sweep(w int) string {
	seg := max(3, w/8)
	p := m.frame % (w + seg)
	lo, hi := max(0, p-seg), min(w, p)
	return sDim.Render(strings.Repeat("━", lo)) + sHere.Render(strings.Repeat("━", hi-lo)) +
		sDim.Render(strings.Repeat("━", w-hi))
}

// times sits under the meter: elapsed and length on the left, what is left
// on the right, and a quiet "paused" in the middle when paused.
func (m *Model) times(w int) string {
	st := m.state
	pos := m.position()
	left := clock(pos) + sDim.Render(" / "+clock(st.Dur))
	if !st.Playing {
		left = sDim.Render(clock(pos) + " / " + clock(st.Dur))
	}
	right := sDim.Render("-" + clock(max(0, st.Dur-pos)))
	line := spread(left, right, w)
	if !st.Playing {
		const mid = icPause + " paused"
		lw, mw, rw := lipgloss.Width(left), lipgloss.Width(mid), lipgloss.Width(right)
		at := (w - mw) / 2
		if at >= lw+2 && at+mw <= w-rw-2 {
			line = left + strings.Repeat(" ", at-lw) + sHere.Render(icPause) + sDim.Render(" paused") +
				strings.Repeat(" ", w-at-mw-rw) + right
		} else {
			line = spread(sHere.Render(icPause)+" "+left, right, w)
		}
	}
	return line
}

// progress is the one-line meter with times at its ends, for the mini
// player.
func (m *Model) progress(w int) string {
	left, right := clock(m.position())+"  ", "  "+clock(m.state.Dur)
	bar := max(0, w-lipgloss.Width(left)-lipgloss.Width(right))
	return sDim.Render(left) + m.meter(bar) + sDim.Render(right)
}

// controls: shuffle and repeat on the left edge, previous / play / next
// centered, volume on the right edge. Each target is padded a cell so it
// is easy to hit.
func (m *Model) controls(w int) stageLine {
	st := m.state
	lit := func(on bool, s string) string {
		if on {
			return sPlays.Render(s)
		}
		return sDim.Render(s)
	}
	repeatIcon := icRepeat
	if st.Repeat == 1 {
		repeatIcon = icRepeatOne
	}
	modes := lit(st.Shuffle, icShuffle) + "  " + lit(st.Repeat != 0, repeatIcon)

	playIcon := icPlay
	if st.Playing {
		playIcon = icPause
	}
	// A key cap: half blocks round the reverse cell off into a square
	// button two cells wider than the icon.
	play := sPlays.Render("▐") + sPill.Render(" "+playIcon+" ") + sPlays.Render("▌")
	volume := m.volumeLabel()
	start, gap, tw, vw, all := controlLayout(w, lipgloss.Width(volume))
	sp := strings.Repeat(" ", gap)
	transport := icPrev + sp + play + sp + icNext

	var line string
	if all {
		line = modes + strings.Repeat(" ", start-4) + transport +
			strings.Repeat(" ", w-start-tw-vw) + volume
	} else {
		// Too narrow for three groups: transport alone, centered.
		line = strings.Repeat(" ", start) + transport
		vw = -1
	}
	return stageLine{text: line, hit: func(x, y int) {
		cell := func(c, r int) rect { return rect{x + c - r, y, x + c + 1 + r, y + 1} }
		if vw >= 0 {
			m.geo.shuffle = rect{x, y, x + 2, y + 1}
			m.geo.repeat = rect{x + 2, y, x + 5, y + 1}
			m.geo.volume = rect{x + w - vw - 1, y, x + w, y + 1}
		}
		m.geo.prev = cell(start, 1)
		m.geo.play = rect{x + start + 1 + gap, y, x + start + 1 + gap + 5, y + 1}
		m.geo.next = cell(start+tw-1, 1)
	}}
}

func (m *Model) volumeLabel() string {
	vol := int(math.Round(m.state.Volume * 100))
	if vol > 0 {
		return icVolume + " " + sDim.Render(fmt.Sprintf("%3d", vol))
	}
	return sDim.Render(icMuted) + " " + sDim.Render("mute")
}

// controlLayout places the controls in w cells: the transport's first
// column, the gap around the play button, the transport's width, the
// volume's, and whether modes and volume fit beside the transport.
func controlLayout(w, vw int) (start, gap, tw, _ int, all bool) {
	gap = max(2, min(5, w/14))
	tw = 1 + gap + 5 + gap + 1
	start = max(0, (w-tw)/2)
	return start, gap, tw, vw, start >= 4+2 && start+tw+2+vw <= w
}

// controlTips is the line above the controls while keytips show: each
// key under what it presses.
func (m *Model) controlTips(w int) string {
	start, gap, tw, vw, all := controlLayout(w, lipgloss.Width(m.volumeLabel()))
	bs := []badge{{start, "p"}, {start + 1 + gap, "space"}, {start + tw - 1, "n"}}
	if all {
		bs = append(bs, badge{0, "s"}, badge{3, "r"}, badge{w - vw, "- + m"})
	}
	return m.badges(w, bs)
}

// tipsOnPlaying is whether a and A would act on the playing song.
func (m *Model) tipsOnPlaying() bool {
	v := m.cur()
	return v.sel >= len(v.rows) || v.rows[v.sel].track == nil || v.rows[v.sel].track.ID == m.state.ID
}
