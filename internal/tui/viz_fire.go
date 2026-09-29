package tui

import "math"

// ─── fire ─────────────────────────────────────────────────────────────────
// Flames in half blocks, two square pixels per cell. Rather than a cellular
// automaton (which at terminal resolution boils into speckle) the heat is a
// smooth field: each column burns as high as its part of the spectrum (bass
// in the middle), and two octaves of value noise scrolling upward at
// different speeds cut it into tongues that lick, split and break off, more
// turbulent toward the tips. Being a function of the clock, it moves
// exactly as smoothly at 120 fps as at 30. Loudness makes the flames reach
// higher and rise faster, a kick flares them and throws sparks; the heat
// maps through red, orange and yellow to a white-hot base, ordered-dithered
// only where one color meets the next.
type vizSpark struct{ x, y, vx, vy, life float64 }

type vizFire struct {
	w, h       int
	src        []float64
	reach      []float32 // flame height per column, in pixels
	off1, off2 float64   // noise scroll, in texels
	flare      float64
	sparks     []vizSpark
	k          int
}

var vizFirePal = [6]uint8{0, vcRed, vcBRed, vcYellow, vcBYellow, vcBWhite}

// vizNoise is a 256×256 tile of smooth, periodic value noise (a 32×32
// lattice, smoothstep-interpolated), sampled bilinearly.
var (
	vizNoise      []float32
	vizNoiseReady bool
)

func vizNoiseInit() {
	if vizNoiseReady {
		return
	}
	const L, S = 32, 8 // lattice size, texels per lattice cell
	var lat [L * L]float32
	r := uint64(0x2545F4914F6CDD1D)
	for i := range lat {
		r ^= r << 13
		r ^= r >> 7
		r ^= r << 17
		lat[i] = float32(r>>40) / (1 << 24)
	}
	vizNoise = make([]float32, 256*256)
	for y := 0; y < 256; y++ {
		ly, fy := y/S, float32(y%S)/S
		fy = fy * fy * (3 - 2*fy)
		for x := 0; x < 256; x++ {
			lx, fx := x/S, float32(x%S)/S
			fx = fx * fx * (3 - 2*fx)
			a := lat[ly*L+lx] + (lat[ly*L+(lx+1)%L]-lat[ly*L+lx])*fx
			b := lat[((ly+1)%L)*L+lx] + (lat[((ly+1)%L)*L+(lx+1)%L]-lat[((ly+1)%L)*L+lx])*fx
			vizNoise[y*256+x] = a + (b-a)*fy
		}
	}
	vizNoiseReady = true
}

// vizNoiseAt samples the noise tile at texel (u, v), both ≥ 0, wrapping.
func vizNoiseAt(u, v float32) float32 {
	iu, iv := int(u), int(v)
	fu, fv := u-float32(iu), v-float32(iv)
	r0, r1 := (iv&255)<<8, ((iv+1)&255)<<8
	x0, x1 := iu&255, (iu+1)&255
	a := vizNoise[r0+x0] + (vizNoise[r0+x1]-vizNoise[r0+x0])*fu
	b := vizNoise[r1+x0] + (vizNoise[r1+x1]-vizNoise[r1+x0])*fu
	return a + (b-a)*fv
}

func (v *visualizer) drawFire(style int, dt float64) {
	vizNoiseInit()
	w, h := v.w, v.h
	P := 2 * h
	s := &v.fire
	if s.w != w || s.h != h {
		s.w, s.h = w, h
		s.reach = make([]float32, w)
		s.sparks = s.sparks[:0]
	}
	a := &v.au
	s.src = vizMirror(s.src, a.sm, w)
	s.flare += (a.beat - s.flare) * min(1, dt*14)
	fp := float64(P)
	reach := fp * (0.28 + 0.5*min(1, a.level*1.8) + 0.3*s.flare)
	var top float32 // the highest reach: nothing burns above 1.5× it
	for x, e := range s.src {
		s.reach[x] = float32(max(2, reach*(0.45+0.75*math.Sqrt(e))))
		top = max(top, s.reach[x])
	}
	// texels per pixel of the coarse and fine octaves, stretched tall
	const f1x, f1y, f2x, f2y = 2.2, 0.8, 4.6, 1.9
	rise := fp * (0.7 + 0.6*a.level + 0.5*s.flare) // pixels a second
	s.off1 = math.Mod(s.off1+dt*rise*f1y, 256)
	s.off2 = math.Mod(s.off2+dt*rise*f2y*1.3, 256)
	o1, o2 := float32(s.off1), float32(s.off2)

	pix := v.pixReset()
	for y := 0; y < P; y++ {
		d := float32(P - 1 - y) // height above the bottom
		if d > top*1.5 {
			continue
		}
		by := &vizBayer[y&3]
		out := pix[y*w : y*w+w]
		v1, v2 := float32(y)*f1y+o1, float32(y)*f2y+o2+97
		for x := range out {
			rel := d / s.reach[x] // 0 at the base, 1 at the flame's reach
			if rel > 1.5 {
				continue
			}
			n := 0.6*vizNoiseAt(float32(x)*f1x, v1) + 0.4*vizNoiseAt(float32(x)*f2x+61, v2)
			// calm at the base, torn into tongues toward the tips
			heat := (1-rel)*1.05 + (n-0.5)*(0.6+2.8*min(rel*rel, 1))
			heat = min(heat, 1.25-rel) // tips cool to red, however they tear
			l := int(heat*5.2 + (by[x&3]-0.5)*0.7)
			if l > 0 {
				out[x] = vizFirePal[min(l, 5)]
			}
		}
	}

	// Sparks: a burst per kick, a trickle with loudness, at most a few hundred.
	fw, fh := float64(w), float64(P)
	emit := 0
	if a.kicks != s.k {
		s.k = a.kicks
		emit = 6 + w/10
	}
	if rate := a.level * a.live * fw * 0.5 * dt; v.rf() < rate-math.Floor(rate) {
		emit += int(rate) + 1
	} else {
		emit += int(rate)
	}
	for ; emit > 0 && len(s.sparks) < 400; emit-- {
		s.sparks = append(s.sparks, vizSpark{
			x:    fw/2 + (v.rf()-0.5)*fw*0.6,
			y:    fh * (1 - 0.3*v.rf()),
			vx:   (v.rf() - 0.5) * fh * 0.3,
			vy:   -(0.5 + 0.6*v.rf()) * fh,
			life: 0.5 + 0.9*v.rf(),
		})
	}
	for i := 0; i < len(s.sparks); {
		p := &s.sparks[i]
		p.life -= dt
		p.x += p.vx * dt
		p.y += p.vy * dt
		p.vy += fh * 0.35 * dt // air slows them
		p.vx += (v.rf() - 0.5) * fh * 1.2 * dt
		if p.life <= 0 || p.y < 0 || p.x < 0 || p.x >= fw || p.y >= fh {
			s.sparks[i] = s.sparks[len(s.sparks)-1]
			s.sparks = s.sparks[:len(s.sparks)-1]
			continue
		}
		c := uint8(vcRed)
		switch {
		case p.life > 0.9:
			c = vcBWhite
		case p.life > 0.5:
			c = vcBYellow
		case p.life > 0.25:
			c = vcYellow
		}
		j := int(p.y)*w + int(p.x)
		if pix[j] == 0 || pix[j] == vcRed {
			pix[j] = c
		}
		i++
	}
	v.pixFlush(0, h)
}

// ─── fireworks ─────────────────────────────────────────────────────────────
// Shells launched on kicks (and steadily with the mids) climb from the
// city on a spark trail and burst at their peak: peonies, rings, willows
// that droop in gold, two-color crossettes. The bass sets how big a burst
// is, the loudness how far it throws, the highs make dying stars glitter.
// Sparks fall under gravity and drag on a braille canvas that fades in a
// sixth of a second, so every star draws a short trail; a burst's core
// flashes white. Paused, a small shell goes up now and then.
type vizParticle struct {
	x, y, vx, vy, life, span float32
	hue, kind                uint8
}

const (
	fwStar = iota
	fwWillow
	fwRocket
	fwTrail
)

type vizFireworks struct {
	c    []float32 // brightness per dot
	hue  []uint8   // color per dot
	rows []bool    // cell rows with anything lit
	p    []vizParticle
	k    int
	acc  float64
	city []int8 // building height per column, in cells
	w    int
}

// add keeps a particle if there's room, never growing the slice.
func (s *vizFireworks) add(p vizParticle) {
	if len(s.p) < cap(s.p) {
		s.p = append(s.p, p)
	}
}

var vizFwHues = [6]uint8{vcRed, vcGreen, vcYellow, vcBlue, vcMagenta, vcCyan}

func (v *visualizer) drawFireworks(dt float64) {
	w, h := v.w, v.h
	W, H := w*2, h*4
	s := &v.fw
	if len(s.c) != W*H || len(s.rows) != h {
		s.c = make([]float32, W*H)
		s.hue = make([]uint8, W*H)
		s.rows = make([]bool, h)
		s.p = make([]vizParticle, 0, 4000) // never grown: a live pointer may be held while adding
	}
	if s.w != w {
		s.w = w
		s.city = make([]int8, w)
		for x := 0; x < w; {
			bw := 3 + int(v.rnd()%6)
			bh := int8(1 + v.rnd()%uint64(max(1, h/7)))
			for i := x; i < min(x+bw, w); i++ {
				s.city[i] = bh
			}
			x += bw
		}
	}
	a := &v.au
	vizDecayRows(s.c, W, s.rows, float32(math.Exp(-dt*7)))

	fW, fH := float32(W), float32(H)
	g := fH * 0.5
	scale := float32(vizClamp(math.Sqrt(float64(W*H))/220, 0.2, 1.4))
	launch := func(small bool) {
		apex := fH * float32(0.35+0.4*v.rf())
		if small {
			apex *= 0.7
		}
		x := fW * float32(0.12+0.76*v.rf())
		s.add(vizParticle{
			x: x, y: fH - 1,
			vx:   (fW/2 - x) * float32(0.05*v.rf()),
			vy:   -float32(math.Sqrt(float64(2 * g * apex))),
			life: 9, span: 9,
			hue:  vizFwHues[v.rnd()%6],
			kind: fwRocket,
		})
	}
	if a.kicks != s.k {
		s.k = a.kicks
		launch(false)
		if a.bass > 0.45 {
			launch(false)
		}
	}
	s.acc += dt * (0.3 + 1.6*a.mid*a.live + 0.3*(1-a.live))
	for ; s.acc >= 1; s.acc-- {
		launch(a.live < 0.5)
	}

	fdt := float32(dt)
	drag := float32(math.Exp(-dt * 1.3))
	wdrag := float32(math.Exp(-dt * 3))
	glitter := a.high > 0.15
	n := len(s.p)
	for i := 0; i < n; i++ {
		p := &s.p[i]
		x0, y0 := p.x, p.y
		p.life -= fdt
		switch p.kind {
		case fwRocket:
			p.vy += g * fdt
			if v.rnd()&1 == 0 {
				s.add(vizParticle{x: p.x, y: p.y, vx: (float32(v.rf()) - 0.5) * 6, vy: 4, life: 0.35, span: 0.35, hue: vcYellow, kind: fwTrail})
			}
			if p.vy > -fH*0.04 { // the peak: burst
				p.life = 0
				v.fwBurst(p.x, p.y, p.hue, scale)
			}
		case fwWillow:
			p.vx *= wdrag
			p.vy = p.vy*wdrag + g*0.35*fdt
		default:
			p.vx *= drag
			p.vy = p.vy*drag + g*0.45*fdt
		}
		p.x += p.vx * fdt
		p.y += p.vy * fdt
		if p.life <= 0 || p.y >= fH || p.x < -fW || p.x > 2*fW {
			continue
		}
		b := 0.2 + 0.7*p.life/p.span
		if p.kind == fwRocket {
			b = 0.7
		} else if glitter && p.life < p.span*0.5 && v.rnd()%3 == 0 {
			b = 1
		}
		vizPlotHue(s.c, s.hue, s.rows, W, H, x0, y0, p.x, p.y, b, p.hue)
	}
	// drop the dead, keeping order irrelevant
	live := s.p[:0]
	for _, p := range s.p {
		if p.life > 0 && p.y < fH && p.x >= -fW && p.x <= 2*fW {
			live = append(live, p)
		}
	}
	s.p = live

	// Cells: the brightest dot names the color; fresh bursts flash white.
	for cy := 0; cy < h; cy++ {
		if !s.rows[cy] {
			continue
		}
		for cx := 0; cx < w; cx++ {
			var bits uint8
			var m float32
			var hu uint8
			base := cy*4*W + cx*2
			for sy := 0; sy < 4; sy++ {
				for sx := 0; sx < 2; sx++ {
					if b := s.c[base+sx]; b > 0.1 {
						bits |= vizBrBit[sx][sy]
						if b > m {
							m, hu = b, s.hue[base+sx]
						}
					}
				}
				base += W
			}
			if bits == 0 {
				continue
			}
			c := hu
			switch {
			case m > 0.95:
				c = vcBWhite
			case m > 0.45:
				c = vizBright(hu)
			}
			i := cy*w + cx
			v.glyph[i], v.col[i] = rune(0x2800+int(bits)), c
		}
	}
	// the skyline in front, a few windows lit
	for x := 0; x < w; x++ {
		for k := 0; k < int(s.city[x]); k++ {
			y := h - 1 - k
			if y < 0 {
				break
			}
			i := y*w + x
			v.glyph[i], v.col[i], v.bg[i] = ' ', 0, vcGray
			if hsh := uint32(x*7919+y*104729) * 2654435761; hsh>>27 == 0 {
				v.glyph[i], v.col[i] = '▪', vcYellow
			}
		}
	}
}

// fwBurst turns a shell at (x, y) into stars.
func (v *visualizer) fwBurst(x, y float32, hue uint8, scale float32) {
	s := &v.fw
	a := &v.au
	H := float32(v.h * 4)
	kind := v.rnd() % 4
	n := int(float32(40+110*a.bass+50*a.beat) * scale)
	speed := H * float32(0.3+0.25*a.level)
	hue2 := vizFwHues[v.rnd()%6]
	for i := 0; i < n; i++ {
		// a shell bursts as a sphere: seen from here, stars crowd its rim
		ang := float64(i)/float64(n)*2*math.Pi + v.rf()*0.2
		u := v.rf()*2 - 1
		sp := speed * float32(math.Sqrt(1-u*u)*(0.9+0.1*v.rf()))
		life := float32(0.9 + 0.8*v.rf())
		p := vizParticle{x: x, y: y, hue: hue, kind: fwStar}
		switch kind {
		case 1: // ring
			sp = speed * float32(0.92+0.08*v.rf())
		case 2: // willow: slow, long, gold
			sp *= 0.8
			life *= 1.6
			p.kind, p.hue = fwWillow, vcYellow
		case 3: // crossette: two colors
			if i&1 == 0 {
				p.hue = hue2
			}
		}
		p.vx = float32(math.Cos(ang)) * sp
		p.vy = float32(math.Sin(ang)) * sp
		p.life, p.span = life, life
		s.add(p)
	}
	// the flash
	vizPlotHue(s.c, s.hue, s.rows, v.w*2, v.h*4, x-1, y, x+1, y, 1, vcWhite)
}

// vizPlotHue draws a line into a brightness + color canvas.
func vizPlotHue(c []float32, hue []uint8, rows []bool, W, H int, x0, y0, x1, y1, b float32, hu uint8) {
	dx, dy := x1-x0, y1-y0
	n := int(max(dx, -dx, dy, -dy))
	n = min(max(n, 1), 32)
	sx, sy := dx/float32(n), dy/float32(n)
	for k := 0; k <= n; k++ {
		if x0 >= 0 && y0 >= 0 {
			if xi, yi := int(x0), int(y0); xi < W && yi < H {
				j := yi*W + xi
				if b >= c[j] {
					c[j], hue[j] = b, hu
					rows[yi>>2] = true
				}
			}
		}
		x0 += sx
		y0 += sy
	}
}
