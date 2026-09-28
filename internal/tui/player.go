package tui

import (
	"fmt"
	"math"
	"strings"

	"charm.land/lipgloss/v2"

	"github.com/chriopter/brumm/internal/art"
)

// The now-playing block of the stage: the cover on top, then title, meta,
// spectrum, progress and transport controls anchored to the bottom.

const (
	// Stage lines under the cover: title, meta, gap, spectrum, gap,
	// progress, gap, controls.
	specRows   = 4
	stageBelow = 3 + specRows + 4
)

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

