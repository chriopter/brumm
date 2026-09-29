package tui

import "math"

// ─── plasma ────────────────────────────────────────────────────────────────
// Demoscene plasma: four interfering sine fields (columns, rows, diagonal,
// radial) index a copper ring of theme colors — dark, blue, cyan, white,
// gold, red, magenta and back — and each cell blends the two colors it
// falls between: ░▒▓ in the one over the other as background, ordered-
// dithered, so ten colors give a smooth gradient; solid cells are plain
// spaces on a background. The color pair only changes at a band edge,
// which keeps the SGR traffic to a few switches a line. Everything
// moves at eased, continuous speeds: loudness drives the flow, the bass
// deepens the radial rings, the mids turn the colors, and every kick sends
// a ripple out from the center instead of jolting the whole screen.
type vizPlasma struct {
	t, speed, hue, bass float64
	rip                 [4]float64 // start times of the kick ripples
	k                   int
	dist                []float32 // distance from the center, in rows
	w, h                int
	sx, sy              []float32
}

var vizPlasmaRing = [...]uint8{0, vcBlue, vcCyan, vcBCyan, vcBWhite, vcBYellow, vcYellow, vcRed, vcMagenta, vcBlue}

var vizShadeRamp = [5]rune{' ', '░', '▒', '▓', '█'} // by quarters

func (v *visualizer) drawPlasma(dt float64) {
	w, h := v.w, v.h
	s := &v.plasma
	if s.w != w || s.h != h {
		s.w, s.h = w, h
		s.dist = make([]float32, w*h)
		s.sx = make([]float32, w)
		s.sy = make([]float32, h)
		cx, cy := float64(w-1)/2, float64(h-1)/2
		for y := 0; y < h; y++ {
			for x := 0; x < w; x++ {
				dx, dy := (float64(x)-cx)*0.5, float64(y)-cy
				s.dist[y*w+x] = float32(math.Sqrt(dx*dx + dy*dy))
			}
		}
	}
	a := &v.au
	target := 0.3 + a.live*1.4*a.level
	s.speed += (target - s.speed) * min(1, dt*1.2)
	s.t = math.Mod(s.t+dt*s.speed, 2*math.Pi*1000)
	s.hue = math.Mod(s.hue+dt*(0.25+0.9*a.mid), float64(len(vizPlasmaRing)))
	s.bass += (a.bass - s.bass) * min(1, dt*5)
	if a.kicks != s.k {
		s.k = a.kicks
		copy(s.rip[1:], s.rip[:3])
		s.rip[0] = a.t
	}
	t := float32(s.t)
	for x := 0; x < w; x++ {
		s.sx[x] = vsin(float32(x)*0.08 + t)
	}
	for y := 0; y < h; y++ {
		s.sy[y] = vsin(float32(y)*0.19 - t*1.3)
	}
	// Ripples: a ring 8 rows wide running out at 30 rows a second, fading.
	var rr, ra [4]float32
	nr := 0
	for _, t0 := range s.rip {
		if age := a.t - t0; t0 > 0 && age < 1.6 {
			rr[nr], ra[nr] = float32(age*30), float32(0.9*(1-age/1.6))
			nr++
		}
	}
	amp := float32(0.7 + 0.9*s.bass)
	hue := float32(s.hue)
	const K = 13 // palette steps across the field's range
	n := float32(len(vizPlasmaRing))
	for y := 0; y < h; y++ {
		sy := s.sy[y]
		ro := y * w
		fy := float32(y) * 0.11
		by := &vizBayer[y&3]
		for x := 0; x < w; x++ {
			d := s.dist[ro+x]
			p := s.sx[x] + sy + vsin(float32(x)*0.045+fy+t*0.7) + amp*vsin(d*0.33-t*1.7)
			for k := 0; k < nr; k++ {
				if e := d - rr[k]; e > -4 && e < 4 {
					e *= 0.25
					p += ra[k] * (1 - e*e) * (1 - e*e) // a smooth bump
				}
			}
			pos := (p+5)*(K/8) + hue // p ≥ -4.6: never negative
			i := int(pos)
			q := int((pos-float32(i))*4 + by[x&3])
			i %= len(vizPlasmaRing)
			j := i + 1
			if float32(j) >= n {
				j = 0
			}
			A, B := vizPlasmaRing[i], vizPlasmaRing[j]
			c := ro + x
			switch {
			case A == 0 && q == 0, B == 0 && q == 4:
				// the terminal's own background
			case q == 0: // solid: a space on the color is the cheapest cell
				v.glyph[c], v.col[c], v.bg[c] = ' ', B, A
			case q == 4:
				v.glyph[c], v.col[c], v.bg[c] = ' ', B, B
			case A == 0:
				v.glyph[c], v.col[c] = vizShadeRamp[q], B
			case B == 0:
				v.glyph[c], v.col[c] = vizShadeRamp[4-q], A
			default:
				v.glyph[c], v.col[c], v.bg[c] = vizShadeRamp[q], B, A
			}
		}
	}
}

// ─── milkdrop ──────────────────────────────────────────────────────────
// MilkDrop-style feedback on a braille canvas: a rotating ring whose radius
// is the spectrum (mirrored so it's symmetric) and an inner ring bent by the
// waveform are drawn every frame, and 30 times a second the canvas is zoomed
// out, twisted and faded, so echoes spiral outward. A kick punches the zoom
// and fires a shockwave ring; every eighth kick reverses the twist and every
// fourth changes the colors.
type vizMilk struct {
	mk, mkTmp []float32
	mkMap     [4][]int32
	mkW, mkH  int
	mkRot     float64
	mkK       int
}

var vizMilkPal = [6]uint8{vcMagenta, vcCyan, vcBlue, vcGreen, vcYellow, vcRed}

func (v *visualizer) drawMilk(style int, wave []float64, dt float64) {
	w, h := v.w, v.h
	W, H := w*2, h*4
	N := W * H
	cx, cy := float64(W-1)/2, float64(H-1)/2
	if v.milk.mkW != W || v.milk.mkH != H {
		v.milk.mkW, v.milk.mkH = W, H
		v.milk.mk = make([]float32, N)
		v.milk.mkTmp = make([]float32, N)
		// Where each dot pulls its value from: calm / punch zoom, each
		// twisting either way. Built once per size.
		for k := range v.milk.mkMap {
			zoom, rot := 0.95, 0.025
			if k&1 != 0 {
				zoom = 0.87
			}
			if k&2 != 0 {
				rot = -rot
			}
			cs, sn := math.Cos(rot)*zoom, math.Sin(rot)*zoom
			m := make([]int32, N)
			for y := 0; y < H; y++ {
				for x := 0; x < W; x++ {
					dx, dy := float64(x)-cx, float64(y)-cy
					sx := int(math.Round(cx + dx*cs - dy*sn))
					sy := int(math.Round(cy + dx*sn + dy*cs))
					j := int32(-1)
					if sx >= 0 && sy >= 0 && sx < W && sy < H {
						j = int32(sy*W + sx)
					}
					m[y*W+x] = j
				}
			}
			v.milk.mkMap[k] = m
		}
	}
	a := &v.au
	flip := (a.kicks / 8) & 1
	for s := v.ticks(style, dt, 30, 3); s > 0; s-- {
		k := flip * 2
		if a.beat > 0.5 {
			k++
		}
		const decay = 0.82
		for i, j := range v.milk.mkMap[k] {
			if j < 0 {
				v.milk.mkTmp[i] = 0
			} else {
				v.milk.mkTmp[i] = v.milk.mk[j] * decay
			}
		}
		v.milk.mk, v.milk.mkTmp = v.milk.mkTmp, v.milk.mk
	}
	dir := 1.0
	if flip != 0 {
		dir = -1
	}
	v.milk.mkRot += dir * dt * (0.25 + 1.5*a.level)

	nb := 48
	v.bands = vizResample(v.bands, a.sm, nb)
	rmin := math.Min(float64(W), float64(H))
	r0 := rmin * (0.15 + 0.08*a.bass + 0.04*a.beat)
	R := rmin * 0.3
	plot := func(x, y float64, val float32) {
		xi, yi := int(x), int(y)
		if xi >= 0 && yi >= 0 && xi < W && yi < H {
			v.milk.mk[yi*W+xi] = max(v.milk.mk[yi*W+xi], val)
		}
	}
	pts := max(64, int(2*math.Pi*(r0+R)*1.6))
	// wave ring: normalized so a quiet track still bends it
	wp := 0.0
	for _, x := range wave {
		wp = max(wp, math.Abs(x))
	}
	useWave := len(wave) >= 8 && wp > 0.005 && a.live > 0.5
	rw := r0 * 0.55
	for k := 0; k <= pts; k++ {
		u := float64(k) / float64(pts)
		fold := 1 - math.Abs(2*u-1) // 0→1→0 around the ring
		b := vizAt(v.bands, fold)
		ang := u*2*math.Pi + v.milk.mkRot
		r := r0 + b*R
		plot(cx+math.Cos(ang)*r, cy+math.Sin(ang)*r, 1)

		var d float64
		if useWave {
			d = wave[min(int(fold*float64(len(wave)-1)), len(wave)-1)] / wp * 0.35
		} else {
			d = 0.12 * math.Sin(u*12*math.Pi+a.t*2) * (0.3 + 2*a.level)
		}
		a2 := -u*2*math.Pi - v.milk.mkRot*1.7
		r2 := rw * (1 + d)
		plot(cx+math.Cos(a2)*r2, cy+math.Sin(a2)*r2, 0.7)
	}
	if a.kicks != v.milk.mkK {
		v.milk.mkK = a.kicks
		r := r0 + R*1.05
		for k := 0; k < pts; k++ {
			ang := float64(k) / float64(pts) * 2 * math.Pi
			plot(cx+math.Cos(ang)*r, cy+math.Sin(ang)*r, 1)
		}
	}
	hi := int(a.t/15) + a.kicks/4
	hueA := vizMilkPal[hi%6]
	hueB := vizMilkPal[(hi+2)%6]
	ramp := [8]uint8{vcBlue, vcBlue, hueB, hueB, hueA, hueA, hueA, vizBright(hueA)}
	v.glow(v.milk.mk, nil, 0.2, &ramp)
}
