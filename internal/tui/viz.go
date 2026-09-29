package tui

// Fullscreen visualizer: twelve Winamp / demoscene styles drawn with nothing
// but the terminal's 16 ANSI colors (foreground and background). Everything
// renders into a reusable cell grid (rune + fg + bg per cell) and is flushed
// line by line, emitting SGR only when a color changes, so a 200×60 frame
// costs well under a millisecond. A shared listener smooths the spectrum,
// detects kicks and breathes gently while paused, so every style reacts to
// the same beat. Motion follows the wall clock: extra View calls must not
// speed anything up, and a slow frame must not slow the picture down.
//
// What a frame costs downstream matters as much as drawing it: bubbletea
// parses every line back into cells and diffs them, paying most for
// non-ASCII glyphs and color switches. So solid areas are spaces on a
// background color, shading changes the glyph within one color pair rather
// than the color, and unchanged rows are handed back as the same string (as
// omacom/ttfx does: it re-encodes only the rows whose cells changed).

import (
	"math"
	"slices"
	"strconv"
	"time"
	"unicode/utf8"
)

// vizNames are the visualizer styles, best first: digits 1–9, 0 pick the
// first ten, the list (v) shows all of them.
var vizNames = []string{
	"milkdrop",  // spectrum + waveform rings in zoom/twist feedback
	"synth",     // synthwave: striped sun, spectrum mountains, racing grid
	"stars",     // braille warp starfield, kicks jump to hyperspace
	"wire",      // 3D wireframes (icosahedron, tesseract, torus) in braille
	"fireworks", // shells launched by kicks, bursting in color
	"lava",      // metaballs in half blocks, sized by the bands
	"scope",     // triggered braille oscilloscope with phosphor persistence
	"plasma",    // sine plasma blending theme colors with shade glyphs
	"fire",      // half-block demoscene fire, flames lick with the bass
	"matrix",    // digital rain shaped by the spectrum, driven by the highs
	"ridge",     // Unknown Pleasures: spectrum history as receding ridgelines
	"led",       // hi-fi graphic EQ with LED segments and peak hold
}

const (
	vzMilk = iota
	vzSynth
	vzStars
	vzWire
	vzFireworks
	vzLava
	vzScope
	vzPlasma
	vzFire
	vzMatrix
	vzRidge
	vzLED
	vizCount
)

// ANSI SGR foreground codes (the theme's 16 colors). 0 means "no color".
// A background uses the same code; the flush adds 10.
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

func init() {
	for i := range vizSin {
		vizSin[i] = float32(math.Sin(float64(i) * 2 * math.Pi / vizSinN))
	}
}

// Sine lookup table for the per-cell styles, interpolated so slow motion
// advances evenly at any frame rate instead of in table steps.
const vizSinN = 4096

var vizSin [vizSinN + 1]float32

func vsin(x float32) float32 {
	f := x * (vizSinN / (2 * math.Pi))
	fl := float32(int32(f))
	if fl > f {
		fl--
	}
	i := int(int32(fl)) & (vizSinN - 1)
	return vizSin[i] + (vizSin[i+1]-vizSin[i])*(f-fl)
}

var (
	// braille bit for sub-pixel (sx, sy) in a cell
	vizBrBit = [2][4]uint8{{0x01, 0x02, 0x04, 0x40}, {0x08, 0x10, 0x20, 0x80}}
	// 4×4 ordered-dither thresholds in (0, 1)
	vizBayer = [4][4]float32{
		{0.5 / 16, 8.5 / 16, 2.5 / 16, 10.5 / 16},
		{12.5 / 16, 4.5 / 16, 14.5 / 16, 6.5 / 16},
		{3.5 / 16, 11.5 / 16, 1.5 / 16, 9.5 / 16},
		{15.5 / 16, 7.5 / 16, 13.5 / 16, 5.5 / 16},
	}
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
	col   []uint8 // foreground code, 0 = default
	bg    []uint8 // background as a foreground code, 0 = default
	buf   []byte

	// last flushed frame, to hand back unchanged rows as the same string
	pw, ph    int
	pGlyph    []rune
	pCol, pBg []uint8
	pLines    []string

	mixG       []rune // the outgoing style during a dissolve
	mixC, mixB []uint8

	now        func() time.Time // nil: the wall clock; tests step it
	au         vizAudio
	at         [vizCount]time.Time // last draw per style
	acc        [vizCount]float64   // fixed-rate tick accumulators
	rng        uint64
	bands      []float64 // scratch resample
	ints       []int     // scratch
	dots, rank []uint8   // braille canvas: dot bits and a color rank per cell
	pix        []uint8   // half-block canvas: w × 2h colors

	// per-style state
	ledPk  vizPeaks
	scope  vizScope
	ridge  vizRidge
	fire   vizFire
	stars  vizStars
	plasma vizPlasma
	milk   vizMilk
	synth  vizSynth
	wire   vizWire
	fw     vizFireworks
	lava   vizLava
	mx     vizMatrix
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
	return v.renderMix(-1, style, 1, spec, wave, w, h, playing)
}

// renderMix draws a dissolve from style from to style to, f (0..1) of the
// way through, in blocks of 6×2 cells that switch over in a fixed random
// order. It mixes the two grids before encoding, so a dissolve costs two
// draws and one flush. from < 0 or f ≥ 1 draws to alone.
func (v *visualizer) renderMix(from, to int, f float64, spec, wave []float64, w, h int, playing bool) []string {
	if h <= 0 {
		return nil
	}
	if w <= 0 {
		return make([]string, h)
	}
	t := time.Now()
	if v.now != nil {
		t = v.now()
	}
	v.au.listen(spec, vizSince(&v.au.at, t), playing)
	to = vizWrap(to)
	if from >= 0 && f < 1 {
		from = vizWrap(from)
		v.draw(from, wave, w, h, t)
		n := w * h
		if cap(v.mixG) < n {
			v.mixG, v.mixC, v.mixB = make([]rune, n), make([]uint8, n), make([]uint8, n)
		}
		v.mixG, v.mixC, v.mixB = v.mixG[:n], v.mixC[:n], v.mixB[:n]
		copy(v.mixG, v.glyph)
		copy(v.mixC, v.col)
		copy(v.mixB, v.bg)
		v.draw(to, wave, w, h, t)
		const bw, bh = 6, 2 // block size in cells
		for y := 0; y < h; y++ {
			for x0 := 0; x0 < w; x0 += bw {
				hsh := uint32(x0/bw)*2654435761 ^ uint32(y/bh)*2246822519
				hsh ^= hsh >> 15
				if float64(hsh%1000)/1000 < f {
					continue
				}
				i, j := y*w+x0, y*w+min(x0+bw, w)
				copy(v.glyph[i:j], v.mixG[i:j])
				copy(v.col[i:j], v.mixC[i:j])
				copy(v.bg[i:j], v.mixB[i:j])
			}
		}
	} else {
		v.draw(to, wave, w, h, t)
	}
	return v.flush()
}

func vizWrap(style int) int { return ((style % vizCount) + vizCount) % vizCount }

func (v *visualizer) draw(style int, wave []float64, w, h int, t time.Time) {
	dt := vizSince(&v.at[style], t)
	v.grid(w, h)
	switch style {
	case vzMilk:
		v.drawMilk(style, wave, dt)
	case vzSynth:
		v.drawSynth(dt)
	case vzStars:
		v.drawStars(dt)
	case vzWire:
		v.drawWire(dt)
	case vzFireworks:
		v.drawFireworks(dt)
	case vzLava:
		v.drawLava(dt)
	case vzScope:
		v.drawScope(wave, dt)
	case vzPlasma:
		v.drawPlasma(dt)
	case vzFire:
		v.drawFire(style, dt)
	case vzMatrix:
		v.drawMatrix(dt)
	case vzRidge:
		v.drawRidge(style, dt)
	case vzLED:
		v.drawLED(dt)
	}
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
		v.bg = make([]uint8, n)
	}
	v.glyph, v.col, v.bg = v.glyph[:n], v.col[:n], v.bg[:n]
	v.w, v.h = w, h
	for i := range v.glyph {
		v.glyph[i] = ' '
	}
	clear(v.col)
	clear(v.bg)
}

func (v *visualizer) set(x, y int, g rune, c uint8) {
	if x < 0 || y < 0 || x >= v.w || y >= v.h {
		return
	}
	i := y*v.w + x
	v.glyph[i], v.col[i] = g, c
}

// flush turns the grid into lines, switching SGR colors only on change. A
// row identical to the last flush comes back as the same string.
func (v *visualizer) flush() []string {
	w, h := v.w, v.h
	n := w * h
	same := v.pw == w && v.ph == h
	if !same {
		v.pw, v.ph = w, h
		v.pGlyph = slices.Grow(v.pGlyph[:0], n)[:n]
		v.pCol = slices.Grow(v.pCol[:0], n)[:n]
		v.pBg = slices.Grow(v.pBg[:0], n)[:n]
		v.pLines = make([]string, h)
	}
	lines := make([]string, h)
	for y := 0; y < h; y++ {
		r0, r1 := y*w, y*w+w
		g, c, bk := v.glyph[r0:r1], v.col[r0:r1], v.bg[r0:r1]
		if same && slices.Equal(g, v.pGlyph[r0:r1]) && slices.Equal(c, v.pCol[r0:r1]) && slices.Equal(bk, v.pBg[r0:r1]) {
			lines[y] = v.pLines[y]
			continue
		}
		b := v.buf[:0]
		var fg, bg uint8
		used := false
		for x, r := range g {
			cf, cb := c[x], bk[x]
			if r == ' ' {
				if cb == 0 && bg == 0 {
					b = append(b, ' ')
					continue
				}
				cf = fg // a space shows no foreground
			}
			if cf != fg || cb != bg {
				b = vizSGR(b, cf, cb, fg, bg)
				fg, bg, used = cf, cb, true
			}
			if r < utf8.RuneSelf {
				b = append(b, byte(r))
			} else {
				b = utf8.AppendRune(b, r)
			}
		}
		if used {
			b = append(b, "\x1b[0m"...)
		}
		lines[y] = string(b)
		v.buf = b
	}
	copy(v.pGlyph, v.glyph)
	copy(v.pCol, v.col)
	copy(v.pBg, v.bg)
	copy(v.pLines, lines)
	return lines
}

// vizSGR appends one SGR sequence moving the pen from (fg0, bg0) to (fg, bg).
func vizSGR(b []byte, fg, bg, fg0, bg0 uint8) []byte {
	b = append(b, 0x1b, '[')
	if fg != fg0 {
		if fg == 0 {
			b = append(b, '3', '9')
		} else {
			b = strconv.AppendUint(b, uint64(fg), 10)
		}
		if bg != bg0 {
			b = append(b, ';')
		}
	}
	if bg != bg0 {
		if bg == 0 {
			b = append(b, '4', '9')
		} else {
			b = strconv.AppendUint(b, uint64(bg)+10, 10)
		}
	}
	return append(b, 'm')
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
// rows, if not nil, says which cell rows hold anything at all.
func (v *visualizer) glow(c []float32, rows []bool, lit float32, ramp *[8]uint8) {
	w, W := v.w, v.w*2
	for cy := 0; cy < v.h; cy++ {
		if rows != nil && !rows[cy] {
			continue
		}
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

// vizDecay multiplies a float canvas by k, skipping the work when it's dark.
func vizDecay(c []float32, k float32) {
	for i, x := range c {
		if x != 0 {
			if x *= k; x < 0.02 {
				x = 0
			}
			c[i] = x
		}
	}
}

// vizDecayRows is vizDecay for a sparse braille canvas, W dots wide: it only
// visits the cell rows marked in rows, and unmarks those gone dark.
func vizDecayRows(c []float32, W int, rows []bool, k float32) {
	for r, on := range rows {
		if !on {
			continue
		}
		any := false
		seg := c[r*4*W : (r+1)*4*W]
		for i, x := range seg {
			if x != 0 {
				if x *= k; x < 0.02 {
					x = 0
				} else {
					any = true
				}
				seg[i] = x
			}
		}
		rows[r] = any
	}
}

// vizPlot draws a line into a float canvas of width W, brightness i0 → i1,
// keeping the brighter value per dot and marking the cell rows it touches.
func vizPlot(c []float32, rows []bool, W, H int, x0, y0, x1, y1 float64, i0, i1 float32) {
	dx, dy := x1-x0, y1-y0
	n := int(math.Max(math.Abs(dx), math.Abs(dy)))
	if n > 4*(W+H) { // off in the weeds: a degenerate projection
		return
	}
	n = max(n, 1)
	sx, sy := dx/float64(n), dy/float64(n)
	di := (i1 - i0) / float32(n)
	for k := 0; k <= n; k++ {
		xi, yi := int(x0), int(y0)
		if x0 >= 0 && y0 >= 0 && xi < W && yi < H {
			j := yi*W + xi
			c[j] = max(c[j], i0)
			rows[yi>>2] = true
		}
		x0 += sx
		y0 += sy
		i0 += di
	}
}

// ─── half-block canvas ──────────────────────────────────────────────────────
// Solid styles draw pixels of w × 2h (square, two per cell) in theme colors.
// A cell becomes ▀ with the top pixel as foreground and the bottom one as
// background, ▄ or ▀ alone over the terminal's background, or a plain space
// on a background color when both halves agree, which is the cheapest cell
// a terminal can be sent.

func (v *visualizer) pixReset() []uint8 {
	n := v.w * v.h * 2
	if cap(v.pix) < n {
		v.pix = make([]uint8, n)
	}
	v.pix = v.pix[:n]
	clear(v.pix)
	return v.pix
}

// pixFlush writes cell rows y0..y1-1 of the half-block canvas into the grid.
func (v *visualizer) pixFlush(y0, y1 int) {
	w := v.w
	for y := max(y0, 0); y < min(y1, v.h); y++ {
		top := v.pix[2*y*w : 2*y*w+w]
		bot := v.pix[2*y*w+w : 2*y*w+2*w]
		ro := y * w
		for x, a := range top {
			b := bot[x]
			i := ro + x
			switch {
			case a == b:
				if a != 0 {
					v.glyph[i], v.col[i], v.bg[i] = ' ', 0, a
				}
			case a == 0:
				v.glyph[i], v.col[i], v.bg[i] = '▄', b, 0
			case b == 0:
				v.glyph[i], v.col[i], v.bg[i] = '▀', a, 0
			default:
				v.glyph[i], v.col[i], v.bg[i] = '▀', a, b
			}
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

func vizBright(c uint8) uint8 {
	if c >= 30 && c <= 37 {
		return c + 60
	}
	return c
}

func vizClamp(x, lo, hi float64) float64 { return min(max(x, lo), hi) }
