package tui

import "math"

// ─── synth ─────────────────────────────────────────────────────────────────
// Synthwave: a striped sun sinks into the horizon, its bands sliding down,
// behind a range of mountains raised by the spectrum (bass at the flanks,
// the valley kept open for the sun); stars twinkle with the highs. Below,
// a braille grid races toward the viewer at the loudness's pace while the
// camera sways, lines glowing magenta near and blue far. The sun and
// mountains are half blocks; a kick swells the sun and flashes the grid.
type vizSynth struct {
	speed, run, stripe float64
	flare              float64   // the beat, eased in
	noise              []float64 // per column, the mountains' own jaggedness
}

var vizSunPal = [6]uint8{vcBYellow, vcBYellow, vcYellow, vcBRed, vcRed, vcMagenta}

func (v *visualizer) drawSynth(dt float64) {
	w, h := v.w, v.h
	s := &v.synth
	a := &v.au
	if len(s.noise) != w {
		s.noise = make([]float64, w)
		p1, p2, p3 := v.rf()*6, v.rf()*6, v.rf()*6
		for x := range s.noise {
			f := float64(x)
			s.noise[x] = 0.5 + 0.22*math.Sin(f*0.13+p1) + 0.16*math.Sin(f*0.37+p2) + 0.1*math.Sin(f*0.91+p3)
		}
	}
	s.flare += (a.beat - s.flare) * min(1, dt*12)
	target := 0.3 + a.live*(0.4+2.6*a.level) + 1.2*s.flare
	s.speed += (target - s.speed) * min(1, dt*3)
	s.run += dt * s.speed
	s.stripe = math.Mod(s.stripe+dt*(0.2+0.5*a.level), 1)

	hz := max(1, h*11/20) // sky rows 0..hz-1, the floor below
	P := 2 * hz
	pix := v.pixReset()
	cx := float64(w) / 2
	fp := float64(P)

	// the sun
	R := math.Min(fp*0.62, float64(w)*0.2) * (1 + 0.05*a.bass + 0.07*s.flare)
	sy := fp - R*0.35
	for py := max(0, int(sy-R)); py < P && float64(py) <= sy+R; py++ {
		dy := float64(py) + 0.5 - sy
		hw := math.Sqrt(max(0, R*R-dy*dy))
		u := max(0, (dy+R)/(2*R)) // 0 top … 1 bottom
		if u > 0.4 {
			if f := math.Mod(u*14-s.stripe, 1); f < (u-0.4)*1.6 {
				continue // a gap in the stripes
			}
		}
		c := vizSunPal[min(int(u/0.68*6), 5)]
		for x := max(0, int(cx-hw+0.5)); x < min(w, int(cx+hw+0.5)); x++ {
			pix[py*w+x] = c
		}
	}
	// the mountains: a ridge line with dark strata below, hiding the sun
	// Two ranges: a far one in blue with the highs, then the near one in
	// cyan with the bass, hiding what's behind it.
	tops := v.bandsInt(w)
	for layer := 0; layer < 2; layer++ {
		for x := 0; x < w; x++ {
			u := math.Abs(float64(x)+0.5-cx) / cx
			var m float64
			if layer == 0 {
				e := vizClamp((u-0.2)/0.6, 0, 1)
				m = fp * 0.72 * e * (0.3 + 0.4*s.noise[(x+w/3)%w] + 0.5*vizAt(a.sm, 0.3+0.7*u))
			} else {
				e := vizClamp((u-0.12)/0.75, 0, 1)
				e = e * e * (3 - 2*e)
				m = fp * 0.55 * e * (0.25 + 0.4*s.noise[x] + 0.6*vizAt(a.sm, (1-u)*0.9))
			}
			tops[x] = P - 1 - int(m)
		}
		ridge := uint8(vcBlue)
		if layer == 1 {
			ridge = vcBCyan
		}
		for x := 0; x < w; x++ {
			top := tops[x]
			if top >= P-1 {
				continue
			}
			lo := top
			if x > 0 {
				lo = max(lo, tops[x-1]-1)
			}
			if x < w-1 {
				lo = max(lo, tops[x+1]-1)
			}
			for py := max(top, 0); py < P; py++ {
				c := uint8(0)
				if py <= lo {
					c = ridge
				}
				pix[py*w+x] = c
			}
		}
	}
	v.pixFlush(0, hz)

	// stars on the empty sky
	for y := 0; y < hz-1; y++ {
		for x := 0; x < w; x++ {
			i := y*w + x
			if v.glyph[i] != ' ' || v.bg[i] != 0 {
				continue
			}
			hsh := uint32(x)*73856093 ^ uint32(y)*19349663
			hsh ^= hsh >> 13
			hsh *= 0x5bd1e995
			hsh ^= hsh >> 15
			if hsh%61 != 0 {
				continue
			}
			tw := math.Sin(a.t*(0.7+float64(hsh>>8&7)*0.3) + float64(hsh>>12&63))
			switch {
			case tw > 0.75-0.8*a.high:
				v.glyph[i], v.col[i] = '+', vcBWhite
			case tw > -0.3:
				v.glyph[i], v.col[i] = '·', vcWhite
			default:
				v.glyph[i], v.col[i] = '·', vcGray
			}
		}
	}

	// the floor: braille grid in perspective
	v.dotReset()
	W, H := 2*w, 4*h
	HY := hz * 4
	D := float64(H - 1 - HY)
	if D < 2 {
		v.dotFlush([]uint8{vcBMagenta})
		return
	}
	rank := func(z float64) uint8 {
		switch {
		case z < 1.8:
			return 0
		case z < 4.5:
			return 1
		}
		return 2
	}
	for x := 0; x < W; x++ {
		v.dot(x, HY, 0)
	}
	const gap = 0.6
	off := math.Mod(s.run, gap)
	last := float64(H)
	for n := 0; n < 200; n++ {
		z := 1 + float64(n)*gap - off
		y := float64(HY) + D/z
		if y >= float64(H) {
			continue
		}
		if last-y < 4 { // lines crowding into the horizon: stop
			break
		}
		last = y
		r := rank(z)
		for x := 0; x < W; x++ {
			v.dot(x, int(y), r)
		}
	}
	cam := math.Sin(a.t*0.17) * 0.7
	spread := float64(W) / 8
	for j := -12; j <= 12; j++ {
		X := (float64(j) - cam) * spread
		// start where neighboring lines are apart, not in a knot
		y0 := HY + max(2, int(1.5/spread*D))
		px, py := float64(W)/2+X*float64(y0-HY)/D, float64(y0)
		for y := y0 + 2; y < H; y += 2 {
			k := float64(y-HY) / D
			x := float64(W)/2 + X*k
			if (x < 0 && px < 0) || (x >= float64(W) && px >= float64(W)) {
				break
			}
			v.dotLine(px, py, x, float64(y), rank(1/k))
			px, py = x, float64(y)
		}
	}
	pal := []uint8{vcBMagenta, vcMagenta, vcBlue}
	if a.beat > 0.5 {
		pal[0] = vcBWhite
	}
	v.dotFlush(pal)
}

// bandsInt is a scratch []int of n.
func (v *visualizer) bandsInt(n int) []int {
	if cap(v.ints) < n {
		v.ints = make([]int, n)
	}
	return v.ints[:n]
}

// ─── matrix ────────────────────────────────────────────────────────────────
// Digital rain: glyphs (half-width katakana, digits and marks: the symbol
// set of the matrix effect in TerminalTextEffects by ChrisBuilds, MIT, as
// ported to Rust by omacom/ttfx, MIT) fall in columns, each head white,
// its trail fading bright green → green → gray as the rain moves on. The
// spectrum decides where it rains (bass in the middle), the highs how hard
// and how fast the glyphs mutate, the loudness how fast it falls; a kick
// sends a squall down from the top and flashes the trails.
type vizDrop struct {
	x     int
	y, sp float32
}

type vizMatrix struct {
	w, h  int
	g     []rune
	b     []float32
	drops []vizDrop
	col   []float64
	flow  float64
	flash float64
	k     int
}

var vizMatrixGlyphs = []rune("ｦｱｳｴｵｶｷｹｺｻｼｽｾｿﾀﾂﾃﾅﾆﾇﾈﾊﾋﾎﾏﾐﾑﾒﾓﾔﾕﾗﾘﾜ2598Z*):.\"=+-¦|_")

func (v *visualizer) drawMatrix(dt float64) {
	w, h := v.w, v.h
	s := &v.mx
	if s.w != w || s.h != h {
		s.w, s.h = w, h
		s.g = make([]rune, w*h)
		s.b = make([]float32, w*h)
		s.drops = make([]vizDrop, 0, 2*w+64)
		for i := range s.g {
			s.g[i] = vizMatrixGlyphs[v.rnd()%uint64(len(vizMatrixGlyphs))]
		}
	}
	a := &v.au
	glyph := func() rune { return vizMatrixGlyphs[v.rnd()%uint64(len(vizMatrixGlyphs))] }
	s.col = vizMirror(s.col, a.sm, w)
	target := float64(h) * (0.3 + a.live*(0.2+1.2*a.level))
	s.flow += (target - s.flow) * min(1, dt*2)
	add := func(x int, y float32) {
		if len(s.drops) < cap(s.drops) {
			s.drops = append(s.drops, vizDrop{x, y, float32(0.6 + 0.8*v.rf())})
		}
	}
	if a.kicks != s.k {
		s.k = a.kicks
		s.flash = 1
		for n := 2 + w/24; n > 0; n-- {
			add(int(v.rnd()%uint64(w)), -float32(v.rf()*3))
		}
	}
	s.flash *= math.Exp(-dt * 5)
	for x, e := range s.col {
		rate := (0.03 + 0.4*e*e + 0.2*a.high) * dt // drops a second in this column
		if v.rf() < rate {
			add(x, -float32(v.rf()*2))
		}
	}
	// trails a third of the screen (at least 8 rows) long at any speed
	trail := float64(h)*0.35 + 8
	vizDecay(s.b, float32(math.Exp(-dt*2.8*s.flow/trail)))
	// mutations
	for n := int(float64(w*h) * dt * (0.02 + 0.3*a.high)); n > 0; n-- {
		i := int(v.rnd() % uint64(w*h))
		s.g[i] = glyph()
	}
	fl := float32(s.flow * dt)
	live := s.drops[:0]
	for _, d := range s.drops {
		y0 := d.y
		d.y += d.sp * fl
		for r := int(math.Floor(float64(y0))) + 1; r <= int(math.Floor(float64(d.y))); r++ {
			if r >= 0 && r < h {
				i := r*w + d.x
				s.b[i], s.g[i] = 1, glyph()
			}
		}
		if d.y < float32(h)+1 {
			live = append(live, d)
		}
	}
	s.drops = live

	boost := float32(0.35 * s.flash)
	for i, b := range s.b {
		if b < 0.06 {
			continue
		}
		b += boost
		c := uint8(vcGray)
		switch {
		case b > 0.8:
			c = vcBGreen
		case b > 0.3:
			c = vcGreen
		}
		v.glyph[i], v.col[i] = s.g[i], c
	}
	for _, d := range s.drops {
		if r := int(d.y); r >= 0 && r < h && d.y >= 0 {
			v.col[r*w+d.x] = vcBWhite
			v.glyph[r*w+d.x] = s.g[r*w+d.x]
		}
	}
}

// ─── led ───────────────────────────────────────────────────────────────────
// Hi-fi graphic equalizer: wide columns of ▆ segments (gaps both ways),
// green/yellow/red zones with the leading segment bright, unlit segments
// glowing faintly, a white peak-hold LED. A kick lights whole columns.
func (v *visualizer) drawLED(dt float64) {
	w, h := v.w, v.h
	nb := min(max((w+1)/6, 3), 32)
	bw := (w+1)/nb - 1
	if bw < 1 {
		bw, nb = 1, max(1, (w+1)/2)
	}
	off := (w - (nb*(bw+1) - 1)) / 2
	v.bands = vizResample(v.bands, v.au.sm, nb)
	pk := v.ledPk.update(v.bands, dt, 0.6)
	all := v.au.beat > 0.45
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
					c = vcGreen
				case f < 0.85:
					c = vcYellow
				default:
					c = vcRed
				}
				if all || r == lit-1 {
					c = vizBright(c)
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
