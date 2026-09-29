package tui

// Fullscreen visualizer: ten Winamp / demoscene styles drawn with nothing but
// the terminal's 16 ANSI colors. Everything renders into a reusable cell grid
// (rune + SGR color per cell) and is flushed line by line, emitting an SGR
// sequence only when the color changes, so a 250×70 frame costs well under a
// millisecond. A shared listener smooths the spectrum, detects kicks and
// breathes gently while paused, so every style reacts to the same beat.
// Motion follows the wall clock: the model's 100 ms frame counter is too
// coarse for 25 fps, and extra View calls must not speed anything up.

import (
	"math"
	"strconv"
	"time"
	"unicode/utf8"
)

// vizNames are the visualizer styles, in cycle order (digits 1–9, 0).
var vizNames = []string{
	"bars",      // Winamp spectrum, gravity peak caps, kicks flash the tips
	"scope",     // triggered braille oscilloscope with phosphor persistence
	"ridge",     // Unknown Pleasures: spectrum history as receding ridgelines
	"fire",      // demoscene fire fed by the spectrum, kicks flare and spark
	"tunnel",    // checkered warp tunnel, walls lit by the spectrum
	"waterfall", // mirrored spectrogram flowing down, kicks leave marks
	"led",       // hi-fi graphic EQ with LED segments and peak hold
	"stars",     // braille warp starfield, kicks jump to hyperspace
	"plasma",    // classic sine plasma, speed and palette follow the music
	"milkdrop",  // spectrum + waveform rings in zoom/twist feedback
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

// Sine lookup table for the per-cell styles.
const vizSinN = 1024

var vizSin [vizSinN]float32

func vsin(x float32) float32 {
	i := int(x*(vizSinN/(2*math.Pi))) & (vizSinN - 1)
	return vizSin[i]
}

var (
	vizEighths = [9]rune{' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'}
	vizShade   = [6]rune{' ', '·', '░', '▒', '▓', '█'}
	// braille bit for sub-pixel (sx, sy) in a cell
	vizBrBit = [2][4]uint8{{0x01, 0x02, 0x04, 0x40}, {0x08, 0x10, 0x20, 0x80}}
)

// vizPeaks is a set of peak-hold markers that hold briefly, then fall with
// gravity. hold > 0 means the marker was just pushed up.
type vizPeaks struct {
	val, vel, hold []float64
}

const vizGravity = 1.8 // screen heights per second²

func (p *vizPeaks) update(b []float64, dt, hold float64) []float64 {
	if len(p.val) != len(b) {
		p.val = make([]float64, len(b))
		p.vel = make([]float64, len(b))
		p.hold = make([]float64, len(b))
	}
	for i, x := range b {
		if p.hold[i] > 0 {
			p.hold[i] -= dt
		} else {
			p.vel[i] += vizGravity * dt
			p.val[i] -= p.vel[i] * dt
		}
		if x >= p.val[i] {
			p.val[i], p.vel[i], p.hold[i] = x, 0, hold
		}
	}
	return p.val
}

// vizAudio is what every style listens to: the spectrum blended with an idle
// breath, a smoothed copy, band energies and a kick detector.
type vizAudio struct {
	at    time.Time
	t     float64   // animation seconds
	live  float64   // 1 playing … 0 paused, eased so a pause fades out
	spec  []float64 // input (clamped) blended with the idle breath
	sm    []float64 // spec with a fast attack and an eased release
	bass  float64   // smoothed energies of the low eighth, mids, highs, all
	mid   float64
	high  float64
	level float64
	beat  float64 // 1 on a kick, decaying over ~0.4 s
	kicks int     // kicks so far, for events like palette changes
	avg   float64 // slow average of the bass: the kick threshold
	prev  float64 // bass at the previous listen, so kicks need a rise
	since float64 // seconds since the last kick
}

func (a *vizAudio) listen(spec []float64, dt float64, playing bool) {
	a.t += dt
	live := 0.0
	if playing {
		live = 1
	}
	a.live += (live - a.live) * min(1, dt*4)
	n := len(spec)
	if n == 0 {
		n = 48
	}
	if len(a.spec) != n {
		a.spec = make([]float64, n)
		a.sm = make([]float64, n)
	}
	// Paused: a low rolling hill that swells and settles over six seconds,
	// so the screen breathes instead of freezing.
	breath := 0.5 - 0.5*math.Cos(a.t*2*math.Pi/6)
	up, down := min(1, dt*30), min(1, dt*9)
	nl := max(1, n/8)
	var bass, mid, high, level float64
	for i := range a.spec {
		x := 0.0
		if i < len(spec) && spec[i] > 0 { // also drops NaN
			x = min(spec[i], 1)
		}
		if a.live < 0.999 {
			u := (float64(i) + 0.5) / float64(n)
			idle := (0.03 + 0.09*breath) * (1 - 0.6*u) * (0.7 + 0.3*math.Sin(u*9-a.t*0.7))
			x = idle + (x-idle)*a.live
		}
		a.spec[i] = x
		k := down
		if x > a.sm[i] {
			k = up
		}
		a.sm[i] += (x - a.sm[i]) * k
		level += x
		switch {
		case i < nl:
			bass += x
		case i < n/2:
			mid += x
		default:
			high += x
		}
	}
	bass /= float64(nl)
	mid /= float64(max(1, n/2-nl))
	high /= float64(max(1, n-n/2))
	level /= float64(n)
	e := min(1, dt*12)
	a.bass += (bass - a.bass) * e
	a.mid += (mid - a.mid) * e
	a.high += (high - a.high) * e
	a.level += (level - a.level) * e

	// A kick is the bass jumping well above its recent average while
	// rising, at most five per second.
	a.since += dt
	if dt > 0 {
		if playing && bass > a.avg*1.3+0.03 && bass > a.prev && a.since > 0.2 {
			a.beat, a.kicks, a.since = 1, a.kicks+1, 0
		}
		a.avg += (bass - a.avg) * min(1, dt*2.5)
		a.prev = bass
	}
	a.beat *= math.Exp(-dt * 6)
}

// visualizer keeps per-style animation state between frames (peaks, history, particles…).
type visualizer struct {
	w, h  int
	glyph []rune
	col   []uint8
	buf   []byte

	now func() time.Time // nil: the wall clock; tests step it

	au  vizAudio
	at  [vizCount]time.Time // last draw per style
	acc [vizCount]float64   // fixed-rate tick accumulators

	rng uint64

	bands []float64 // scratch resample

	// braille canvas: dot bits and a color rank per cell
	dots, rank []uint8

	barPk, ledPk vizPeaks

	// scope
	wv   []float64
	ph   []float32 // phosphor, one value per dot
	gain float64

	// ridge
	rg       []float32 // history rows, P points each
	rgLive   []float32
	rgNoise  []float32
	rgHz     []int
	rgR, rgP int
	rgHead   int

	// fire
	heat   []float32
	heatW  int
	heatH  int
	fireSd []float64
	sparks []vizSpark
	fireK  int

	// tunnel
	tnZ, tnU, tnFog []float32
	tnB             []uint8
	tnW, tnH        int
	tnSpeed, tnGo   float64
	tnRot           float64

	// waterfall
	wf     []uint8
	wfW    int
	wfH    int
	wfHead int
	wfK    int

	// stars
	stars          []vizStar
	stSpeed, stRol float64

	// plasma
	pt, plLevel, plHue float64
	plDist             []float32
	plW, plH           int
	plSx, plSy         []float32

	// milkdrop
	mk, mkTmp []float32
	mkMap     [4][]int32
	mkW, mkH  int
	mkRot     float64
	mkK       int
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

// render draws style (0..len(vizNames)-1) into exactly h lines of exactly w
// display cells each. frame is unused: timing comes from the clock.
func (v *visualizer) render(style int, spec, wave []float64, w, h, frame int, playing bool) []string {
	if h <= 0 {
		return nil
	}
	if w <= 0 {
		return make([]string, h)
	}
	style = ((style % vizCount) + vizCount) % vizCount
	t := time.Now()
	if v.now != nil {
		t = v.now()
	}
	// During a dissolve two styles draw per frame; the listener's own clock
	// makes the second call see no time pass, so kicks aren't counted twice.
	v.au.listen(spec, vizSince(&v.au.at, t), playing)
	dt := vizSince(&v.at[style], t)
	v.grid(w, h)

	switch style {
	case 0:
		v.drawBars(dt)
	case 1:
		v.drawScope(wave, dt)
	case 2:
		v.drawRidge(style, dt)
	case 3:
		v.drawFire(style, dt)
	case 4:
		v.drawTunnel(dt)
	case 5:
		v.drawWaterfall(style, dt)
	case 6:
		v.drawLED(dt)
	case 7:
		v.drawStars(dt)
	case 8:
		v.drawPlasma(dt)
	case 9:
		v.drawMilk(style, wave, dt)
	}
	return v.flush()
}

// vizSince returns the seconds from *last to t and moves *last: 1/25 s the
// first time, at most 0.1 s so a style coming back after a minute doesn't
// leap.
func vizSince(last *time.Time, t time.Time) float64 {
	d := 0.04
	if !last.IsZero() {
		d = min(max(t.Sub(*last).Seconds(), 0), 0.1)
	}
	*last = t
	return d
}

// ticks turns elapsed time into whole simulation steps at hz for the styles
// that evolve in discrete steps (fire, feedback, history), at most most.
func (v *visualizer) ticks(style int, dt, hz float64, most int) int {
	v.acc[style] += dt * hz
	n := int(v.acc[style])
	v.acc[style] -= float64(n)
	return min(n, most)
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

// ─── braille canvases ───────────────────────────────────────────────────────
// Line styles draw into 2×4 dots per cell. The ranked canvas colors a cell by
// its most prominent dot (lowest rank); the float canvas by its brightest.

func (v *visualizer) dotReset() {
	n := v.w * v.h
	if cap(v.dots) < n {
		v.dots = make([]uint8, n)
		v.rank = make([]uint8, n)
	}
	v.dots, v.rank = v.dots[:n], v.rank[:n]
	clear(v.dots)
	for i := range v.rank {
		v.rank[i] = 255
	}
}

func (v *visualizer) dot(x, y int, r uint8) {
	if x < 0 || y < 0 || x >= v.w*2 || y >= v.h*4 {
		return
	}
	i := (y>>2)*v.w + x>>1
	v.dots[i] |= vizBrBit[x&1][y&3]
	if r < v.rank[i] {
		v.rank[i] = r
	}
}

// dotLine plots a straight run of dots, at most 64 long.
func (v *visualizer) dotLine(x0, y0, x1, y1 float64, r uint8) {
	dx, dy := x1-x0, y1-y0
	n := min(int(math.Max(math.Abs(dx), math.Abs(dy))), 64)
	if n <= 0 {
		v.dot(int(x1), int(y1), r)
		return
	}
	sx, sy := dx/float64(n), dy/float64(n)
	for k := 0; k <= n; k++ {
		v.dot(int(x0), int(y0), r)
		x0 += sx
		y0 += sy
	}
}

// dotFlush writes the ranked canvas into the grid, rank r in pal[r].
func (v *visualizer) dotFlush(pal []uint8) {
	for i, b := range v.dots {
		if b == 0 {
			continue
		}
		r := min(int(v.rank[i]), len(pal)-1)
		v.glyph[i], v.col[i] = rune(0x2800+int(b)), pal[r]
	}
}

// glow writes a float canvas (2w×4h dots, 0..1) into the grid: dots above
// lit are set and the cell takes ramp[brightest dot × 8].
func (v *visualizer) glow(c []float32, lit float32, ramp *[8]uint8) {
	w, W := v.w, v.w*2
	for cy := 0; cy < v.h; cy++ {
		for cx := 0; cx < w; cx++ {
			var bits uint8
			var m float32
			base := cy*4*W + cx*2
			for sy := 0; sy < 4; sy++ {
				a, b := c[base], c[base+1]
				base += W
				if a > lit {
					bits |= vizBrBit[0][sy]
					m = max(m, a)
				}
				if b > lit {
					bits |= vizBrBit[1][sy]
					m = max(m, b)
				}
			}
			if bits == 0 {
				continue
			}
			i := cy*w + cx
			v.glyph[i], v.col[i] = rune(0x2800+int(bits)), ramp[min(int(m*8), 7)]
		}
	}
}

// ─── spectrum helpers ───────────────────────────────────────────────────────

// vizAt samples src (0..1 values) at fraction f of its range, interpolated.
func vizAt(src []float64, f float64) float64 {
	m := len(src)
	if m == 0 {
		return 0
	}
	p := min(max(f, 0), 1) * float64(m-1)
	j := int(p)
	x := src[j]
	if j+1 < m {
		x += (src[j+1] - x) * (p - float64(j))
	}
	return x
}

// vizResample maps spec onto n values (linear interpolation, clamped 0..1).
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

// vizMirror maps spec onto n values with the bass in the middle and the
// highs fanning out to both edges.
func vizMirror(dst []float64, spec []float64, n int) []float64 {
	if cap(dst) < n {
		dst = make([]float64, n)
	}
	dst = dst[:n]
	c := float64(n-1) / 2
	for x := range dst {
		dst[x] = vizAt(spec, math.Abs(float64(x)-c)/(c+0.5))
	}
	return dst
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
