package tui

// Fullscreen visualizer: ten Winamp / demoscene styles drawn with nothing but
// the terminal's 16 ANSI colors. Everything renders into a reusable cell grid
// (rune + SGR color per cell) and is flushed line by line, emitting an SGR
// sequence only when the color changes, so a 250×70 frame costs well under a
// millisecond or two. All animation state lives in the visualizer struct.

import (
	"math"
	"strconv"
	"unicode/utf8"
)

// vizNames are the visualizer styles, in cycle order.
var vizNames = []string{
	"bars",      // Winamp spectrum with falling peak caps
	"mirror",    // bars mirrored around the center, bass in the middle
	"scope",     // braille oscilloscope with phosphor afterglow
	"braille",   // hi-res braille spectrum with peak dots
	"fire",      // demoscene fire fed by the spectrum
	"waterfall", // scrolling spectrogram
	"led",       // hi-fi graphic EQ with LED segments
	"stars",     // warp starfield driven by the bass
	"plasma",    // classic sine plasma, speed follows loudness
	"milkdrop",  // rotating spectrum ring with zoom/rotate feedback trails
}

const vizCount = 10

// ANSI SGR foreground codes (the theme's 16 colors). 0 means "no color".
const (
	vcRed      = 31
	vcGreen    = 32
	vcYellow   = 33
	vcBlue     = 34
	vcMagenta  = 35
	vcCyan     = 36
	vcWhite    = 37
	vcGray     = 90
	vcBRed     = 91
	vcBGreen   = 92
	vcBYellow  = 93
	vcBBlue    = 94
	vcBMagenta = 95
	vcBCyan    = 96
	vcBWhite   = 97
)

var vizSGR [98][]byte

func init() {
	for c := 30; c < 98; c++ {
		vizSGR[c] = []byte("\x1b[" + strconv.Itoa(c) + "m")
	}
	vizSGR[0] = []byte("\x1b[0m")
	for i := range vizSin {
		vizSin[i] = float32(math.Sin(float64(i) * 2 * math.Pi / vizSinN))
	}
}

// Sine lookup table for the plasma.
const vizSinN = 1024

var vizSin [vizSinN]float32

func vsin(x float32) float32 {
	i := int(x*(vizSinN/(2*math.Pi))) & (vizSinN - 1)
	return vizSin[i]
}

var (
	vizEighths = [9]rune{' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'}
	// braille bits for the bottom k dots of the left / right sub-column
	vizBrL = [5]uint8{0, 0x40, 0x44, 0x46, 0x47}
	vizBrR = [5]uint8{0, 0x80, 0xA0, 0xB0, 0xB8}
	// braille bit for sub-pixel (sx, sy) in a cell
	vizBrBit = [2][4]uint8{{0x01, 0x02, 0x04, 0x40}, {0x08, 0x10, 0x20, 0x80}}
)

// vizPeaks is a set of peak-hold markers that hold briefly, then fall with
// gravity.
type vizPeaks struct {
	val, vel []float64
	hold     []int
}

func (p *vizPeaks) update(b []float64, steps, hold int) []float64 {
	if len(p.val) != len(b) {
		p.val = make([]float64, len(b))
		p.vel = make([]float64, len(b))
		p.hold = make([]int, len(b))
	}
	for i, x := range b {
		for s := 0; s < steps; s++ {
			if p.hold[i] > 0 {
				p.hold[i]--
				continue
			}
			p.vel[i] += 0.0025
			p.val[i] -= p.vel[i]
		}
		if x >= p.val[i] {
			p.val[i], p.vel[i], p.hold[i] = x, 0, hold
		}
	}
	return p.val
}

type vizStar struct{ x, y, z float64 }

// visualizer keeps per-style animation state between frames (peaks, history, particles…).
type visualizer struct {
	w, h  int
	glyph []rune
	col   []uint8
	buf   []byte

	rng       uint64
	lastFrame [vizCount]int
	seen      [vizCount]bool

	bands  []float64 // scratch resample
	bands2 []float64

	barPk, ledPk, brPk vizPeaks

	// scope
	wv         []float64
	dots, prev []uint8
	gain       float64

	// fire
	heat   []float32
	heatW  int
	heatH  int
	fireSd []float64

	// waterfall
	wf     []uint8
	wfW    int
	wfH    int
	wfHead int

	// stars
	stars []vizStar

	// plasma
	pt, plLevel float64
	plDist      []float32
	plW, plH    int
	plSx, plSy  []float32

	// milkdrop
	mk, mkTmp []float32
	mkMap     []int32
	mkW, mkH  int
	mkRot     float64
	mkLevel   float64
}

func (v *visualizer) rnd() uint64 {
	if v.rng == 0 {
		v.rng = 0x9E3779B97F4A7C15
	}
	v.rng ^= v.rng << 13
	v.rng ^= v.rng >> 7
	v.rng ^= v.rng << 17
	return v.rng
}

func (v *visualizer) rf() float64 { return float64(v.rnd()>>11) / (1 << 53) }

// render draws style (0..len(vizNames)-1) into exactly h lines of exactly w display cells each.
func (v *visualizer) render(style int, spec, wave []float64, w, h, frame int, playing bool) []string {
	if h <= 0 {
		return nil
	}
	if w <= 0 {
		return make([]string, h)
	}
	style = ((style % vizCount) + vizCount) % vizCount
	v.grid(w, h)
	steps := v.steps(style, frame, playing)

	switch style {
	case 0:
		v.drawBars(spec, steps)
	case 1:
		v.drawMirror(spec)
	case 2:
		v.drawScope(spec, wave, frame)
	case 3:
		v.drawBraille(spec, steps)
	case 4:
		v.drawFire(spec, steps)
	case 5:
		v.drawWaterfall(spec, steps)
	case 6:
		v.drawLED(spec, steps)
	case 7:
		v.drawStars(spec, steps, playing)
	case 8:
		v.drawPlasma(spec, steps)
	case 9:
		v.drawMilk(spec, steps, frame)
	}
	return v.flush()
}

// steps is how many simulation ticks to advance: the frame delta while
// playing (so extra View calls don't speed things up), one per call while
// paused so everything settles.
func (v *visualizer) steps(style, frame int, playing bool) int {
	if !playing {
		v.lastFrame[style] = frame
		v.seen[style] = true
		return 1
	}
	if !v.seen[style] {
		v.seen[style] = true
		v.lastFrame[style] = frame
		return 1
	}
	d := frame - v.lastFrame[style]
	v.lastFrame[style] = frame
	return min(max(d, 0), 4)
}

func (v *visualizer) grid(w, h int) {
	n := w * h
	if cap(v.glyph) < n {
		v.glyph = make([]rune, n)
		v.col = make([]uint8, n)
	}
	v.glyph, v.col = v.glyph[:n], v.col[:n]
	v.w, v.h = w, h
	for i := range v.glyph {
		v.glyph[i] = ' '
		v.col[i] = 0
	}
}

func (v *visualizer) set(x, y int, g rune, c uint8) {
	if x < 0 || y < 0 || x >= v.w || y >= v.h {
		return
	}
	i := y*v.w + x
	v.glyph[i], v.col[i] = g, c
}

// flush turns the grid into lines, switching SGR color only on change.
func (v *visualizer) flush() []string {
	lines := make([]string, v.h)
	for y := 0; y < v.h; y++ {
		b := v.buf[:0]
		var cur uint8
		row := y * v.w
		for x := 0; x < v.w; x++ {
			g := v.glyph[row+x]
			if g == ' ' {
				b = append(b, ' ')
				continue
			}
			if c := v.col[row+x]; c != cur {
				b = append(b, vizSGR[c]...)
				cur = c
			}
			if g < utf8.RuneSelf {
				b = append(b, byte(g))
			} else {
				b = utf8.AppendRune(b, g)
			}
		}
		if cur != 0 {
			b = append(b, vizSGR[0]...)
		}
		lines[y] = string(b)
		v.buf = b
	}
	return lines
}

// resample maps spec onto n values (linear interpolation, clamped 0..1).
func vizResample(dst []float64, spec []float64, n int) []float64 {
	if cap(dst) < n {
		dst = make([]float64, n)
	}
	dst = dst[:n]
	m := len(spec)
	if m == 0 {
		clear(dst)
		return dst
	}
	for i := range dst {
		p := (float64(i)+0.5)*float64(m)/float64(n) - 0.5
		if p < 0 {
			p = 0
		}
		j := int(p)
		f := p - float64(j)
		a := spec[j]
		b := a
		if j+1 < m {
			b = spec[j+1]
		}
		x := a + (b-a)*f
		if !(x > 0) { // also catches NaN
			x = 0
		} else if x > 1 {
			x = 1
		}
		dst[i] = x
	}
	return dst
}

// vizEnergy returns the mean of the lowest `frac` of the bands and of all bands.
func vizEnergy(spec []float64, frac float64) (bass, level float64) {
	n := len(spec)
	if n == 0 {
		return 0, 0
	}
	k := max(1, int(float64(n)*frac))
	for i, x := range spec {
		if !(x > 0) {
			continue
		}
		x = min(x, 1)
		level += x
		if i < k {
			bass += x
		}
	}
	return bass / float64(k), level / float64(n)
}

// meterColor is the LED-meter green→yellow→red coloring by height fraction.
func meterColor(f float64) uint8 {
	switch {
	case f < 0.35:
		return vcGreen
	case f < 0.6:
		return vcBGreen
	case f < 0.8:
		return vcBYellow
	case f < 0.9:
		return vcRed
	}
	return vcBRed
}

func vizBright(c uint8) uint8 {
	if c >= 30 && c <= 37 {
		return c + 60
	}
	return c
}

// ─── 1. bars ───────────────────────────────────────────────────────────────
// Classic Winamp spectrum: thick bars with 1-cell gaps, eighth-block tops,
// LED coloring by height and peak caps that hold, then fall with gravity.
func (v *visualizer) drawBars(spec []float64, steps int) {
	w, h := v.w, v.h
	bw := 1
	if w >= 60 {
		bw = 2
	}
	if w >= 120 {
		bw = 3
	}
	nb := max(1, (w+1)/(bw+1))
	off := (w - (nb*(bw+1) - 1)) / 2
	v.bands = vizResample(v.bands, spec, nb)
	pk := v.barPk.update(v.bands, steps, 10)
	h8 := h * 8
	for i, b := range v.bands {
		x0 := off + i*(bw+1)
		e := int(b*float64(h8) + 0.5)
		for row := 0; row < h; row++ {
			fill := e - row*8
			if fill <= 0 {
				break
			}
			g := '█'
			if fill < 8 {
				g = vizEighths[fill]
			}
			c := meterColor((float64(row) + 0.5) / float64(h))
			for dx := 0; dx < bw; dx++ {
				v.set(x0+dx, h-1-row, g, c)
			}
		}
		if pk[i] > 0.015 {
			p8 := min(int(pk[i]*float64(h8)), h8-1)
			pr := p8 / 8
			g := '▁'
			if p8%8 >= 4 {
				g = '▔'
			}
			if top := (e + 7) / 8; pr < top { // cap sits on the bar
				pr, g = top, '▁'
			}
			if pr < h {
				c := vizBright(meterColor((float64(pr) + 0.5) / float64(h)))
				for dx := 0; dx < bw; dx++ {
					v.set(x0+dx, h-1-pr, g, c)
				}
			}
		}
	}
}

// ─── 2. mirror ─────────────────────────────────────────────────────────────
// Bars grow up and down out of the center line; bass sits in the middle and
// highs fan out to both edges. Top half bright, bottom half its reflection.
func (v *visualizer) drawMirror(spec []float64) {
	w, h := v.w, v.h
	bw := 1
	if w >= 100 {
		bw = 2
	}
	nb := max(1, (w+1)/(bw+1))
	off := (w - (nb*(bw+1) - 1)) / 2
	half := (nb + 1) / 2
	v.bands = vizResample(v.bands, spec, half)
	ht := h / 2
	hb := h - ht
	mid := float64(nb-1) / 2
	for i := 0; i < nb; i++ {
		d := int(math.Abs(float64(i)-mid) + 0.5)
		if nb%2 == 1 {
			d = int(math.Abs(float64(i) - mid))
		}
		d = min(d, half-1)
		b := v.bands[d]
		x0 := off + i*(bw+1)
		// upper half, growing upward from row ht-1
		e := int(b*float64(ht*8) + 0.5)
		for r := 0; r < ht; r++ {
			fill := e - r*8
			if fill <= 0 {
				break
			}
			g := '█'
			if fill < 8 {
				g = vizEighths[fill]
			}
			c := mirrorColor(float64(r) / float64(max(1, ht)))
			for dx := 0; dx < bw; dx++ {
				v.set(x0+dx, ht-1-r, g, vizBright(c))
			}
		}
		// lower half, growing downward, half-block precision
		e2 := int(b*float64(hb*2) + 0.5)
		for r := 0; r < hb; r++ {
			fill := e2 - r*2
			if fill <= 0 {
				break
			}
			g := '█'
			if fill < 2 {
				g = '▀'
			}
			if r == 0 && ht > 0 {
				g = '▓' // shimmering waterline
				if fill < 2 {
					g = '▀'
				}
			}
			c := mirrorColor(float64(r) / float64(max(1, hb)))
			for dx := 0; dx < bw; dx++ {
				v.set(x0+dx, ht+r, g, c)
			}
		}
	}
}

func mirrorColor(f float64) uint8 {
	switch {
	case f < 0.3:
		return vcCyan
	case f < 0.65:
		return vcBlue
	}
	return vcMagenta
}

// ─── 3. scope ──────────────────────────────────────────────────────────────
// Oscilloscope: the waveform traced with braille dots (2×4 per cell) as a
// continuous line, bright green phosphor with a dimmer afterglow of the
// previous frame over a dotted graticule. Without wave data it synthesizes a
// signal from the spectrum.
func (v *visualizer) drawScope(spec, wave []float64, frame int) {
	w, h := v.w, v.h
	W, H := w*2, h*4
	n := w * h
	if len(v.dots) != n {
		v.dots = make([]uint8, n)
		v.prev = make([]uint8, n)
	}
	v.dots, v.prev = v.prev, v.dots
	clear(v.dots)

	if cap(v.wv) < W {
		v.wv = make([]float64, W)
	}
	s := v.wv[:W]
	if len(wave) >= 2 {
		m := len(wave)
		for x := range s {
			p := float64(x) * float64(m-1) / float64(max(1, W-1))
			j := int(p)
			f := p - float64(j)
			a := wave[j]
			b := a
			if j+1 < m {
				b = wave[j+1]
			}
			s[x] = a + (b-a)*f
		}
	} else {
		// synthesize: a handful of partials, amplitude from band groups
		const parts = 7
		var amp [parts]float64
		nb := len(spec)
		norm := 0.0
		for k := 0; k < parts; k++ {
			if nb > 0 {
				lo, hi := k*nb/parts, (k+1)*nb/parts
				for j := lo; j < max(hi, lo+1) && j < nb; j++ {
					if spec[j] > 0 {
						amp[k] += min(spec[j], 1)
					}
				}
				amp[k] /= float64(max(1, hi-lo))
			}
			norm += amp[k]
		}
		t := float64(frame) * 0.15
		for x := range s {
			u := float64(x) / float64(W) * 2 * math.Pi
			y := 0.0
			for k := 0; k < parts; k++ {
				if amp[k] == 0 {
					continue
				}
				f := float64(int(1) << k) // 1,2,4,… cycles across the screen
				y += amp[k] * math.Sin(u*f*1.5+t*float64(k+1))
			}
			if norm > 0 {
				y *= math.Min(1, norm) / norm
			}
			s[x] = y
		}
	}
	// auto-gain so quiet passages still fill the screen
	peak := 0.0
	for _, x := range s {
		peak = max(peak, math.Abs(x))
	}
	target := 1.0
	if peak > 0.02 {
		target = min(4, 0.85/peak)
	}
	if v.gain == 0 {
		v.gain = target
	}
	v.gain += (target - v.gain) * 0.15

	mid := float64(H-1) / 2
	amp := mid * v.gain
	py := -1
	for x := 0; x < W; x++ {
		y := int(mid - s[x]*amp + 0.5)
		y = min(max(y, 0), H-1)
		y0, y1 := y, y
		if py >= 0 {
			if py < y {
				y0 = py + 1
			} else if py > y {
				y1 = py - 1
			}
		}
		cx := x >> 1
		for yy := y0; yy <= y1; yy++ {
			v.dots[(yy>>2)*w+cx] |= vizBrBit[x&1][yy&3]
		}
		py = y
	}

	midRow := h / 2
	gx := max(4, w/12)
	gy := max(2, h/6)
	for y := 0; y < h; y++ {
		for x := 0; x < w; x++ {
			i := y*w + x
			switch {
			case v.dots[i] != 0:
				v.glyph[i], v.col[i] = rune(0x2800+int(v.dots[i])), vcBGreen
			case v.prev[i] != 0:
				v.glyph[i], v.col[i] = rune(0x2800+int(v.prev[i])), vcGreen
			case y == midRow && x%2 == 0:
				v.glyph[i], v.col[i] = '·', vcGray
			case (x%gx == 0) && (y-midRow)%gy == 0:
				v.glyph[i], v.col[i] = '+', vcGray
			}
		}
	}
}

// ─── 4. braille ────────────────────────────────────────────────────────────
// High-resolution filled spectrum: every cell carries two bands and four
// height steps in braille dots, blue→magenta→bright magenta with bright
// white falling peak dots.
func (v *visualizer) drawBraille(spec []float64, steps int) {
	w, h := v.w, v.h
	n := w * 2
	H := h * 4
	v.bands = vizResample(v.bands, spec, n)
	pk := v.brPk.update(v.bands, steps, 6)
	for x := 0; x < w; x++ {
		hl := int(v.bands[2*x]*float64(H) + 0.5)
		hr := int(v.bands[2*x+1]*float64(H) + 0.5)
		pl := min(int(pk[2*x]*float64(H)), H-1)
		pr := min(int(pk[2*x+1]*float64(H)), H-1)
		for cy := 0; cy < h; cy++ {
			base := (h - 1 - cy) * 4 // sub-rows below this cell
			fl := min(max(hl-base, 0), 4)
			fr := min(max(hr-base, 0), 4)
			bits := vizBrL[fl] | vizBrR[fr]
			filled := bits != 0
			if pk[2*x] > 0.02 && pl >= base && pl < base+4 && pl >= hl {
				bits |= vizBrBit[0][3-(pl-base)]
			}
			if pk[2*x+1] > 0.02 && pr >= base && pr < base+4 && pr >= hr {
				bits |= vizBrBit[1][3-(pr-base)]
			}
			if bits == 0 {
				continue
			}
			f := float64(h-1-cy) / float64(h)
			c := uint8(vcBMagenta)
			switch {
			case !filled:
				c = vcBWhite
			case f < 0.3:
				c = vcBlue
			case f < 0.6:
				c = vcMagenta
			}
			i := cy*w + x
			v.glyph[i], v.col[i] = rune(0x2800+int(bits)), c
		}
	}
}

// ─── 5. fire ───────────────────────────────────────────────────────────────
// Demoscene fire: each column's energy feeds heat at the bottom, which rises,
// spreads, flickers and cools; ░▒▓█ in red → yellow → bright yellow → white.
func (v *visualizer) drawFire(spec []float64, steps int) {
	w, h := v.w, v.h
	H := h + 2
	if v.heatW != w || v.heatH != H {
		v.heat = make([]float32, w*H)
		v.heatW, v.heatH = w, H
	}
	v.fireSd = vizResample(v.fireSd, spec, w)
	cool := float32(2.0 / float64(h))
	for s := 0; s < min(steps, 3); s++ {
		// seed the two bottom rows
		for x := 0; x < w; x++ {
			e := v.fireSd[x]
			e = math.Sqrt(e) * (0.85 + 0.65*v.rf())
			if e > 1.4 {
				e = 1.4
			}
			v.heat[(H-1)*w+x] = float32(e)
			v.heat[(H-2)*w+x] = float32(e * (0.85 + 0.15*v.rf()))
		}
		// propagate upward
		for y := 0; y < H-2; y++ {
			r1 := (y + 1) * w
			r2 := (y + 2) * w
			ro := y * w
			for x := 0; x < w; x++ {
				r := v.rnd()
				xs := x + int(r%3) - 1
				xs = min(max(xs, 0), w-1)
				xl := max(x-1, 0)
				xr := min(x+1, w-1)
				t := (v.heat[r1+xl] + v.heat[r1+xs] + v.heat[r1+xr] + v.heat[r2+x]) * 0.25
				t -= cool * float32((r>>8)&255) / 255
				if t < 0 {
					t = 0
				}
				v.heat[ro+x] = t
			}
		}
	}
	for y := 0; y < h; y++ {
		ro := y * w
		for x := 0; x < w; x++ {
			t := v.heat[ro+x]
			var g rune
			var c uint8
			switch {
			case t < 0.06:
				continue
			case t < 0.14:
				g, c = '░', vcRed
			case t < 0.24:
				g, c = '▒', vcRed
			case t < 0.36:
				g, c = '▓', vcRed
			case t < 0.48:
				g, c = '▓', vcBRed
			case t < 0.6:
				g, c = '▓', vcYellow
			case t < 0.74:
				g, c = '█', vcYellow
			case t < 0.9:
				g, c = '█', vcBYellow
			default:
				g, c = '█', vcBWhite
			}
			v.glyph[ro+x], v.col[ro+x] = g, c
		}
	}
}

// ─── 6. waterfall ──────────────────────────────────────────────────────────
// Scrolling spectrogram: the newest spectrum row enters at the bottom and
// history drifts upward; intensity as " ·░▒▓█" in blue → cyan → white.
var vizWfGlyph = [9]rune{' ', '·', '░', '▒', '▒', '▓', '▓', '█', '█'}
var vizWfCol = [9]uint8{0, vcBlue, vcBlue, vcBlue, vcCyan, vcCyan, vcBCyan, vcBCyan, vcBWhite}

func (v *visualizer) drawWaterfall(spec []float64, steps int) {
	w, h := v.w, v.h
	if v.wfW != w || v.wfH != h {
		v.wf = make([]uint8, w*h)
		v.wfW, v.wfH, v.wfHead = w, h, 0
	}
	v.bands = vizResample(v.bands, spec, w)
	for s := 0; s < steps; s++ {
		v.wfHead = (v.wfHead + 1) % h
		row := v.wf[v.wfHead*w : (v.wfHead+1)*w]
		for x, b := range v.bands {
			l := int((0.55*b+0.45*math.Sqrt(b))*9.4 - 0.5)
			row[x] = uint8(min(max(l, 0), 8))
		}
	}
	for y := 0; y < h; y++ {
		src := ((v.wfHead-(h-1-y))%h + h) % h
		row := v.wf[src*w : (src+1)*w]
		ro := y * w
		for x, l := range row {
			if l == 0 {
				continue
			}
			v.glyph[ro+x], v.col[ro+x] = vizWfGlyph[l], vizWfCol[l]
		}
	}
}

// ─── 7. led ────────────────────────────────────────────────────────────────
// Hi-fi graphic equalizer: wide columns of ▆ segments (gaps both ways),
// green/yellow/red zones, unlit segments glowing faintly, white peak-hold LED.
func (v *visualizer) drawLED(spec []float64, steps int) {
	w, h := v.w, v.h
	nb := min(max((w+1)/6, 3), 32)
	bw := (w+1)/nb - 1
	if bw < 1 {
		bw, nb = 1, max(1, (w+1)/2)
	}
	off := (w - (nb*(bw+1) - 1)) / 2
	v.bands = vizResample(v.bands, spec, nb)
	pk := v.ledPk.update(v.bands, steps, 14)
	for i, b := range v.bands {
		x0 := off + i*(bw+1)
		lit := int(b*float64(h) + 0.5)
		pr := -1
		if pk[i] > 0.02 {
			pr = min(int(pk[i]*float64(h)+0.5), h) - 1
		}
		for r := 0; r < h; r++ {
			f := (float64(r) + 0.5) / float64(h)
			var c uint8
			switch {
			case r == pr && r >= lit-1:
				c = vcBWhite
			case r < lit:
				switch {
				case f < 0.6:
					c = vcBGreen
				case f < 0.85:
					c = vcBYellow
				default:
					c = vcBRed
				}
			default:
				c = vcGray
			}
			ro := (h-1-r)*w + x0
			for dx := 0; dx < bw; dx++ {
				v.glyph[ro+dx], v.col[ro+dx] = '▆', c
			}
		}
	}
}

// ─── 8. stars ──────────────────────────────────────────────────────────────
// Warp starfield: stars fly out of the center, speed and density pumped by
// the bass; distance sets glyph (· • + *) and color (blue → cyan → white),
// fast near stars leave streaks.
func (v *visualizer) drawStars(spec []float64, steps int, playing bool) {
	w, h := v.w, v.h
	maxN := min(max(w*h/10, 60), 1800)
	cx, cy := float64(w)/2, float64(h)/2
	S := math.Max(cy, 1)
	ax := cx / (2 * S) // x extent so stars at z=1 cover the screen
	for len(v.stars) < maxN {
		v.stars = append(v.stars, vizStar{
			x: (v.rf()*2 - 1) * ax * 1.2, y: (v.rf()*2 - 1) * 1.2, z: 0.05 + v.rf()*0.95,
		})
	}
	bass, level := vizEnergy(spec, 0.15)
	speed := 0.004 + 0.045*bass*bass + 0.01*level
	if !playing {
		speed = 0.002
	}
	active := int(float64(maxN) * (0.3 + 0.7*math.Min(1, level*1.8+bass*0.5)))
	for s := 0; s < steps; s++ {
		for i := range v.stars {
			st := &v.stars[i]
			st.z -= speed
			sx := st.x / st.z * 2 * S
			sy := st.y / st.z * S
			if st.z <= 0.02 || math.Abs(sx) > cx+1 || math.Abs(sy) > cy+1 {
				st.x = (v.rf()*2 - 1) * ax * 1.2
				st.y = (v.rf()*2 - 1) * 1.2
				st.z = 0.7 + v.rf()*0.3
			}
		}
	}
	// streaks first, heads on top
	for i := 0; i < active && i < len(v.stars); i++ {
		st := v.stars[i]
		if st.z > 0.5 || speed < 0.012 {
			continue
		}
		x1, y1 := cx+st.x/st.z*2*S, cy+st.y/st.z*S
		pz := st.z + speed*3
		x0, y0 := cx+st.x/pz*2*S, cy+st.y/pz*S
		dx, dy := x1-x0, y1-y0
		n := int(math.Max(math.Abs(dx), math.Abs(dy)))
		n = min(n, 16)
		for k := 0; k < n; k++ {
			t := float64(k) / float64(n)
			x, y := int(x0+dx*t), int(y0+dy*t)
			if x >= 0 && y >= 0 && x < w && y < h && v.glyph[y*w+x] == ' ' {
				v.glyph[y*w+x], v.col[y*w+x] = '·', vcBlue
			}
		}
	}
	for i := 0; i < active && i < len(v.stars); i++ {
		st := v.stars[i]
		x := int(cx + st.x/st.z*2*S)
		y := int(cy + st.y/st.z*S)
		if x < 0 || y < 0 || x >= w || y >= h {
			continue
		}
		var g rune
		var c uint8
		switch {
		case st.z > 0.75:
			g, c = '·', vcBlue
		case st.z > 0.5:
			g, c = '·', vcCyan
		case st.z > 0.3:
			g, c = '•', vcBCyan
		case st.z > 0.15:
			g, c = '+', vcBWhite
		default:
			g, c = '*', vcBWhite
		}
		v.glyph[y*w+x], v.col[y*w+x] = g, c
	}
}

// ─── 9. plasma ─────────────────────────────────────────────────────────────
// Demoscene plasma: four interfering sine fields (columns, rows, diagonal,
// radial) shaded ░▒▓█ in cycling rainbow colors; loudness drives speed and
// how solid the shading gets.
var vizPlasmaPal = [6]uint8{vcRed, vcYellow, vcGreen, vcCyan, vcBlue, vcMagenta}
var vizShade = [5]rune{' ', '░', '▒', '▓', '█'}

func (v *visualizer) drawPlasma(spec []float64, steps int) {
	w, h := v.w, v.h
	if v.plW != w || v.plH != h {
		v.plW, v.plH = w, h
		v.plDist = make([]float32, w*h)
		v.plSx = make([]float32, w)
		v.plSy = make([]float32, h)
		cx, cy := float64(w)/2, float64(h)/2
		for y := 0; y < h; y++ {
			for x := 0; x < w; x++ {
				dx, dy := (float64(x)-cx)*0.5, float64(y)-cy
				v.plDist[y*w+x] = float32(math.Sqrt(dx*dx+dy*dy) * 0.35)
			}
		}
	}
	_, level := vizEnergy(spec, 0.15)
	v.plLevel += (level - v.plLevel) * 0.2
	for s := 0; s < steps; s++ {
		v.pt += 0.03 + 0.22*v.plLevel
	}
	t := float32(v.pt)
	for x := 0; x < w; x++ {
		v.plSx[x] = vsin(float32(x)*0.09 + t)
	}
	for y := 0; y < h; y++ {
		v.plSy[y] = vsin(float32(y)*0.21 - t*1.3)
	}
	loud := float32(0.35 + 0.65*math.Min(1, v.plLevel*2.2))
	hue := t * 0.4
	for y := 0; y < h; y++ {
		sy := v.plSy[y]
		ro := y * w
		fy := float32(y) * 0.13
		for x := 0; x < w; x++ {
			p := v.plSx[x] + sy +
				vsin(float32(x)*0.05+fy+t*0.7) +
				vsin(v.plDist[ro+x]-t*1.7)
			f := (p + 4) * 0.125 // 0..1
			// triangle wave → contour bands
			u := f*3 + t*0.05
			u -= float32(int(u))
			sh := u * 2
			if sh > 1 {
				sh = 2 - sh
			}
			sh *= loud
			k := int(sh*5 + 0.25)
			if k <= 0 {
				continue
			}
			k = min(k, 4)
			ci := int(f*9+hue) % 6
			if ci < 0 {
				ci += 6
			}
			c := vizPlasmaPal[ci]
			if k >= 4 {
				c += 60
			}
			v.glyph[ro+x], v.col[ro+x] = vizShade[k], c
		}
	}
}

// ─── 10. milkdrop ──────────────────────────────────────────────────────────
// MilkDrop-style feedback: a rotating ring whose radius is the spectrum
// (mirrored so it's symmetric) is drawn into a braille canvas every frame;
// the previous canvas is zoomed out and twisted slightly, so echoes of each
// beat spiral outward and fade. Colors cycle slowly through the palette.
var vizMilkPal = [6]uint8{vcMagenta, vcCyan, vcBlue, vcGreen, vcYellow, vcRed}

func (v *visualizer) drawMilk(spec []float64, steps, frame int) {
	w, h := v.w, v.h
	W, H := w*2, h*4
	N := W * H
	if v.mkW != W || v.mkH != H {
		v.mkW, v.mkH = W, H
		v.mk = make([]float32, N)
		v.mkTmp = make([]float32, N)
		v.mkMap = make([]int32, N)
		cx, cy := float64(W-1)/2, float64(H-1)/2
		const zoom, rot = 0.94, 0.025
		cs, sn := math.Cos(rot)*zoom, math.Sin(rot)*zoom
		for y := 0; y < H; y++ {
			for x := 0; x < W; x++ {
				dx, dy := float64(x)-cx, float64(y)-cy
				sx := int(math.Round(cx + dx*cs - dy*sn))
				sy := int(math.Round(cy + dx*sn + dy*cs))
				m := int32(-1)
				if sx >= 0 && sy >= 0 && sx < W && sy < H {
					m = int32(sy*W + sx)
				}
				v.mkMap[y*W+x] = m
			}
		}
	}
	bass, level := vizEnergy(spec, 0.15)
	v.mkLevel += (level - v.mkLevel) * 0.2
	nb := 48
	v.bands2 = vizResample(v.bands2, spec, nb)
	cx, cy := float64(W-1)/2, float64(H-1)/2
	rmin := math.Min(float64(W), float64(H))
	r0 := rmin * (0.16 + 0.07*bass)
	R := rmin * 0.3
	for s := 0; s < min(steps, 2); s++ {
		// feedback: zoom + twist + fade
		decay := float32(0.8)
		for i, m := range v.mkMap {
			if m < 0 {
				v.mkTmp[i] = 0
			} else {
				v.mkTmp[i] = v.mk[m] * decay
			}
		}
		v.mk, v.mkTmp = v.mkTmp, v.mk
		v.mkRot += 0.015 + 0.06*v.mkLevel
	}
	// draw the ring (always, so the current spectrum is crisp)
	circ := 2 * math.Pi * (r0 + R)
	pts := max(64, int(circ*1.6))
	px, py := -1, -1
	for k := 0; k <= pts; k++ {
		u := float64(k) / float64(pts)
		fold := 1 - math.Abs(2*u-1) // 0→1→0 around the ring
		bp := fold * float64(nb-1)
		j := int(bp)
		b := v.bands2[j]
		if j+1 < nb {
			b += (v.bands2[j+1] - b) * (bp - float64(j))
		}
		a := u*2*math.Pi + v.mkRot
		r := r0 + b*R
		x := int(cx + math.Cos(a)*r)
		y := int(cy + math.Sin(a)*r)
		if x == px && y == py {
			continue
		}
		px, py = x, y
		if x >= 0 && y >= 0 && x < W && y < H {
			v.mk[y*W+x] = 1
		}
		// inner counter-rotating dotted halo pulsing with the bass
		if k%3 == 0 {
			a2 := -u*2*math.Pi - v.mkRot*1.7
			r2 := r0 * (0.45 + 0.35*bass)
			x2 := int(cx + math.Cos(a2)*r2)
			y2 := int(cy + math.Sin(a2)*r2)
			if x2 >= 0 && y2 >= 0 && x2 < W && y2 < H {
				v.mk[y2*W+x2] = max(v.mk[y2*W+x2], 0.7)
			}
		}
	}
	hueA := vizMilkPal[(frame/75)%6]
	hueB := vizMilkPal[(frame/75+2)%6]
	for cyc := 0; cyc < h; cyc++ {
		for cxc := 0; cxc < w; cxc++ {
			var bits uint8
			var m float32
			base := cyc*4*W + cxc*2
			for sy := 0; sy < 4; sy++ {
				row := base + sy*W
				a, b := v.mk[row], v.mk[row+1]
				if a > 0.2 {
					bits |= vizBrBit[0][sy]
					m = max(m, a)
				}
				if b > 0.2 {
					bits |= vizBrBit[1][sy]
					m = max(m, b)
				}
			}
			if bits == 0 {
				continue
			}
			var c uint8
			switch {
			case m > 0.8:
				c = vizBright(hueA)
			case m > 0.45:
				c = hueA
			case m > 0.25:
				c = hueB
			default:
				c = vcBlue
			}
			i := cyc*w + cxc
			v.glyph[i], v.col[i] = rune(0x2800+int(bits)), c
		}
	}
}
