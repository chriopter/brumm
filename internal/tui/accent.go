package tui

import (
	"image"
	"image/color"
	"math"

	"charm.land/lipgloss/v2"
)

// ── accent ──────────────────────────────────────────────────────────────

// The few things that say "this plays" — the meter's fill, the selection's
// gutter, the playing row and its equalizer, the play button — take the
// cover's most vivid color, made readable on the theme's background.
// It is picked once per cover (and background); a frame only compares a
// key and uses the styles. Nothing playing, a gray cover or the option
// off, they are the theme's own.

// accent holds the ready styles.
type accent struct {
	key   accentKey
	here  lipgloss.Style // the meter's fill, the selection's gutter
	plays lipgloss.Style // the playing row, its equalizer, the play button's rim
	bold  lipgloss.Style // plays, bold: the selected playing row
	pill  lipgloss.Style // the play button
}

// accentKey is what the styles were picked for.
type accentKey struct {
	url   string // the cover; empty for the theme's
	ready bool   // its image is here
	bg    color.RGBA
	known bool // bg is the terminal's, not a guess
}

// themeAccent is the theme's own colors.
func themeAccent() accent {
	return accent{here: sHere, plays: sPlays, bold: sPlays.Bold(true), pill: sPill}
}

// syncAccent picks the accent again when the cover or the background
// changed; otherwise it is a comparison.
func (m *Model) syncAccent() {
	var k accentKey
	playing := m.state.Title != "" || m.state.Preview != nil
	if playing && !m.opts.NoCoverColors && m.coverURL != "" {
		k = accentKey{url: m.coverURL, ready: m.cover != nil}
		if m.termBg != nil {
			k.bg, k.known = rgba(m.termBg), true
		}
	}
	if k == m.acc.key {
		return
	}
	m.acc = themeAccent()
	m.acc.key = k
	if !k.ready {
		return
	}
	c, ok := vivid(m.cover)
	if !ok {
		return // a gray cover: the theme's
	}
	c = readable(c, k.bg, k.known)
	s := lipgloss.NewStyle().Foreground(c)
	m.acc.here, m.acc.plays, m.acc.bold = s, s, s.Bold(true)
	m.acc.pill = s.Reverse(true)
}

func rgba(c color.Color) color.RGBA {
	r, g, b, _ := c.RGBA()
	return color.RGBA{uint8(r >> 8), uint8(g >> 8), uint8(b >> 8), 255}
}

// vivid is the cover's dominant vivid color, false when it has none. A
// sample of pixels votes by hue, each weighed by its saturation; grays,
// near black and near white do not vote.
func vivid(img image.Image) (color.RGBA, bool) {
	const bins = 24
	var w [bins]float64
	var sum [bins][3]float64
	b := img.Bounds()
	step := max(1, max(b.Dx(), b.Dy())/48)
	n := 0
	for y := b.Min.Y; y < b.Max.Y; y += step {
		for x := b.Min.X; x < b.Max.X; x += step {
			c := rgba(img.At(x, y))
			r, g, bl := float64(c.R)/255, float64(c.G)/255, float64(c.B)/255
			h, s, v := hsv(r, g, bl)
			n++
			if s < 0.25 || v < 0.2 {
				continue
			}
			i := int(h*bins) % bins
			wt := s * s * v
			w[i] += wt
			sum[i][0], sum[i][1], sum[i][2] = sum[i][0]+r*wt, sum[i][1]+g*wt, sum[i][2]+bl*wt
		}
	}
	// The strongest hue with its neighbors, so a hue split over two bins
	// is not outvoted.
	best, score := 0, 0.0
	for i := range bins {
		if s := w[(i+bins-1)%bins]/2 + w[i] + w[(i+1)%bins]/2; s > score {
			best, score = i, s
		}
	}
	if n == 0 || score < 0.03*float64(n) { // a few specks of color: gray
		return color.RGBA{}, false
	}
	var tw float64
	var rgb [3]float64
	for _, i := range []int{(best + bins - 1) % bins, best, (best + 1) % bins} {
		tw += w[i]
		for k := range rgb {
			rgb[k] += sum[i][k]
		}
	}
	h, s, l := hsl(rgb[0]/tw, rgb[1]/tw, rgb[2]/tw)
	return fromHSL(h, max(s, 0.55), l), true // averaging dulls it: vivid again
}

// readable moves c's lightness until it stands out from bg as text does
// (4.5:1). Without a known background it keeps to the middle, which
// reads on dark and light alike.
func readable(c, bg color.RGBA, known bool) color.RGBA {
	h, s, l := hsl(float64(c.R)/255, float64(c.G)/255, float64(c.B)/255)
	if !known {
		return fromHSL(h, s, max(0.45, min(0.65, l)))
	}
	dir := 0.02 // lighter on a dark background, darker on a light one
	if contrast(color.RGBA{0, 0, 0, 255}, bg) > contrast(color.RGBA{255, 255, 255, 255}, bg) {
		dir = -0.02
	}
	for range 50 {
		if contrast(c, bg) >= 4.5 || l <= 0 || l >= 1 {
			break
		}
		l = max(0, min(1, l+dir))
		c = fromHSL(h, s, l)
	}
	return c
}

// contrast is the WCAG contrast ratio of two colors.
func contrast(a, b color.RGBA) float64 {
	la, lb := luminance(a), luminance(b)
	return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)
}

func luminance(c color.RGBA) float64 {
	lin := func(v uint8) float64 {
		f := float64(v) / 255
		if f <= 0.03928 {
			return f / 12.92
		}
		return math.Pow((f+0.055)/1.055, 2.4)
	}
	return 0.2126*lin(c.R) + 0.7152*lin(c.G) + 0.0722*lin(c.B)
}

// hsv: hue 0–1, saturation and value 0–1.
func hsv(r, g, b float64) (h, s, v float64) {
	hi, lo := max(r, g, b), min(r, g, b)
	if hi > 0 {
		s = (hi - lo) / hi
	}
	return hue(r, g, b, hi, lo), s, hi
}

func hsl(r, g, b float64) (h, s, l float64) {
	hi, lo := max(r, g, b), min(r, g, b)
	l = (hi + lo) / 2
	if d := hi - lo; d > 0 {
		s = d / (1 - math.Abs(2*l-1))
	}
	return hue(r, g, b, hi, lo), min(1, s), l
}

func hue(r, g, b, hi, lo float64) float64 {
	d := hi - lo
	var h float64
	switch {
	case d == 0:
		return 0
	case hi == r:
		h = math.Mod((g-b)/d, 6)
	case hi == g:
		h = (b-r)/d + 2
	default:
		h = (r-g)/d + 4
	}
	if h < 0 {
		h += 6
	}
	return h / 6
}

func fromHSL(h, s, l float64) color.RGBA {
	c := (1 - math.Abs(2*l-1)) * s
	x := c * (1 - math.Abs(math.Mod(h*6, 2)-1))
	var r, g, b float64
	switch int(h*6) % 6 {
	case 0:
		r, g = c, x
	case 1:
		r, g = x, c
	case 2:
		g, b = c, x
	case 3:
		g, b = x, c
	case 4:
		r, b = x, c
	default:
		r, b = c, x
	}
	o := l - c/2
	u := func(v float64) uint8 { return uint8(math.Round(max(0, min(1, v+o)) * 255)) }
	return color.RGBA{u(r), u(g), u(b), 255}
}
