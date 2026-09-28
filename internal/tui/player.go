package tui

import (
	"fmt"
	"math"
	"strings"
	"time"

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
//	▆ █ ▃   ▅ ─ ▂        spectrum, bars with gaps and falling peaks
//	█ █ █ ▂ █ █ █ ▁ ▁
//
//	━━━━━━━━━━━━━━━━━━━━╸━━━━━━━━━━━━━━━━━   meter, half-cell precise
//	2:44 / 5:08                         -2:24
//
//	󰒝  󰑖        󰒮   ▐ 󰏤 ▌   󰒭         󰕾 100
//
// Modes on the left, transport in the middle, volume on the right: three
// groups, the play button the one filled element.

const (
	specRows = 4
	// title, meta, gap, spectrum, gap, meter, times, gap, transport
	stageBelow = 2 + 1 + specRows + 1 + 2 + 1 + 1
)

// Raw SGR for the per-cell spectrum: one escape per color run instead of a
// lipgloss render per run. Only the 16 theme colors.
const (
	sgrBlue  = "\x1b[34m"
	sgrDim   = "\x1b[90m"
	sgrPeak  = "\x1b[39m"
	sgrReset = "\x1b[m"
)

// nowPlaying returns the stage in two parts: the cover (top-anchored) and
// the block under it (bottom-anchored); len(bottom) == stageBelow.
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
	var tag string
	switch {
	case st.Preview != nil:
		title, artist, album = st.Preview.Title, st.Preview.Artist, ""
		tag = sPill.Render(" PREVIEW ")
	case st.Length > 1 && st.Index >= 0:
		tag = sDim.Render(fmt.Sprintf("%d/%d", st.Index+1, st.Length))
	}
	heading := sBold.Render(title)
	if st.Preview == nil && m.loved[st.ID] {
		heading += "  " + sErr.Render("♥")
	}
	add(spread(heading, tag, colW))
	meta := artist
	if album != "" && album != title {
		meta += sDim.Render("  ·  " + album)
	}
	add(ansi.Truncate(meta, colW, "…"))
	add("")
	for _, l := range m.spectrum(colW, specRows) {
		add(l)
	}
	add("")
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
	add("")
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
	gap := max(2, min(5, w/14))
	sp := strings.Repeat(" ", gap)
	transport := icPrev + sp + play + sp + icNext
	tw := 1 + gap + 5 + gap + 1

	vol := int(math.Round(st.Volume * 100))
	volume := sDim.Render(icMuted) + " " + sDim.Render("mute")
	if vol > 0 {
		volume = icVolume + " " + sDim.Render(fmt.Sprintf("%3d", vol))
	}
	vw := lipgloss.Width(volume)

	start := max(0, (w-tw)/2)
	var line string
	if start >= 4+2 && start+tw+2+vw <= w {
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

// peaks remembers the falling peak marks of the stage spectrum between
// renders; there is one stage, so one set.
var peaks struct {
	v    []float64
	hold []time.Time
	at   time.Time
}

// spectrum draws the bands as thin bars with one-cell gaps, eighth-block
// precise, each with a peak mark that holds briefly and then falls. When
// paused the bars rest on a dim baseline.
func (m *Model) spectrum(w, rows int) []string {
	n := max(1, (w+1)/2)
	lead := (w - (2*n - 1)) / 2
	levels := make([]float64, n)
	if k := len(m.spec); k > 0 {
		for i := range levels {
			// Each bar takes the loudest of its share of the bands.
			lo := i * k / n
			hi := max(lo+1, (i+1)*k/n)
			v := 0.0
			for _, b := range m.spec[lo:min(hi, k)] {
				v = max(v, b)
			}
			// The analyser runs hot; a curve keeps the bars off the
			// ceiling so the shape reads.
			levels[i] = math.Pow(v, 1.6)
		}
	}

	now := time.Now()
	if len(peaks.v) != n {
		peaks.v, peaks.hold = make([]float64, n), make([]time.Time, n)
	}
	dt := min(0.5, now.Sub(peaks.at).Seconds())
	peaks.at = now
	for i, lv := range levels {
		switch {
		case lv >= peaks.v[i]:
			peaks.v[i], peaks.hold[i] = lv, now.Add(400*time.Millisecond)
		case now.After(peaks.hold[i]):
			peaks.v[i] = max(lv, peaks.v[i]-0.6*dt)
		}
	}

	playing := m.state.Playing || m.state.Preview != nil
	barColor := sgrBlue
	if !playing {
		barColor = sgrDim
	}
	ramp := []rune(" ▁▂▃▄▅▆▇█")
	out := make([]string, rows)
	var sb strings.Builder
	for r := range rows {
		row := rows - 1 - r // 0 is the bottom row
		sb.Reset()
		sb.WriteString(strings.Repeat(" ", lead))
		cur := ""
		put := func(color string, ch rune) {
			if color != cur && ch != ' ' {
				sb.WriteString(color)
				cur = color
			}
			sb.WriteRune(ch)
		}
		for i, lv := range levels {
			if i > 0 {
				sb.WriteByte(' ')
			}
			fill := lv*float64(rows*8) - float64(row*8)
			pk := peaks.v[i]*float64(rows) - float64(row) // peak within this row: 0..1
			switch {
			case fill >= 8:
				put(barColor, '█')
			case fill >= 1:
				put(barColor, ramp[int(fill)])
			case row == 0:
				put(barColor, '▁')
			case playing && pk >= 0 && pk < 1 && peaks.v[i] > lv+0.5/float64(rows*8):
				// Only where the bar does not reach: a thin mark at the
				// peak's height in the cell.
				ch := '▁'
				if pk >= 0.66 {
					ch = '▔'
				} else if pk >= 0.33 {
					ch = '─'
				}
				put(sgrPeak, ch)
			default:
				sb.WriteByte(' ')
			}
		}
		if cur != "" {
			sb.WriteString(sgrReset)
		}
		out[r] = sb.String()
	}
	return out
}
