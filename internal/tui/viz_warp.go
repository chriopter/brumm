package tui

import "math"

// ─── 9. plasma ─────────────────────────────────────────────────────────────
// Demoscene plasma: four interfering sine fields (columns, rows, diagonal,
// radial) shaded ·░▒▓█ in cycling rainbow colors. Loudness drives speed and
// how solid the shading gets, the bass swells the radial rings, and every
// kick rolls the palette one color on.
var vizPlasmaPal = [6]uint8{vcRed, vcYellow, vcGreen, vcCyan, vcBlue, vcMagenta}

func (v *visualizer) drawPlasma(dt float64) {
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
	a := &v.au
	v.plLevel += (a.level - v.plLevel) * min(1, dt*5)
	v.pt += dt * (0.25 + 2.2*v.plLevel + 1.5*a.beat)
	v.plHue += (float64(a.kicks) - v.plHue) * min(1, dt*6) // one color per kick, rolled in
	t := float32(math.Mod(v.pt, 2*math.Pi*100))
	for x := 0; x < w; x++ {
		v.plSx[x] = vsin(float32(x)*0.09 + t)
	}
	for y := 0; y < h; y++ {
		v.plSy[y] = vsin(float32(y)*0.21 - t*1.3)
	}
	loud := float32(0.35 + 0.65*math.Min(1, v.plLevel*2.2) + 0.25*a.beat)
	swell := float32(1 + 0.6*a.bass)
	hue := float32(math.Mod(v.pt*0.4+v.plHue, 6))
	for y := 0; y < h; y++ {
		sy := v.plSy[y]
		ro := y * w
		fy := float32(y) * 0.13
		for x := 0; x < w; x++ {
			p := v.plSx[x] + sy +
				vsin(float32(x)*0.05+fy+t*0.7) +
				vsin(v.plDist[ro+x]*swell-t*1.7)
			f := (p + 4) * 0.125 // 0..1
			// triangle wave → contour bands
			u := f*3 + t*0.05
			u -= float32(int(u))
			sh := u * 2
			if sh > 1 {
				sh = 2 - sh
			}
			k := int(sh*loud*6 + 0.2)
			if k <= 0 {
				continue
			}
			k = min(k, 5)
			c := vizPlasmaPal[int(f*9+hue)%6]
			if k >= 4 {
				c += 60
			}
			v.glyph[ro+x], v.col[ro+x] = vizShade[k], c
		}
	}
}

// ─── 10. milkdrop ──────────────────────────────────────────────────────────
// MilkDrop-style feedback on a braille canvas: a rotating ring whose radius
// is the spectrum (mirrored so it's symmetric) and an inner ring bent by the
// waveform are drawn every frame, and 30 times a second the canvas is zoomed
// out, twisted and faded, so echoes spiral outward. A kick punches the zoom
// and fires a shockwave ring; every eighth kick reverses the twist and every
// fourth changes the colors.
var vizMilkPal = [6]uint8{vcMagenta, vcCyan, vcBlue, vcGreen, vcYellow, vcRed}

func (v *visualizer) drawMilk(style int, wave []float64, dt float64) {
	w, h := v.w, v.h
	W, H := w*2, h*4
	N := W * H
	cx, cy := float64(W-1)/2, float64(H-1)/2
	if v.mkW != W || v.mkH != H {
		v.mkW, v.mkH = W, H
		v.mk = make([]float32, N)
		v.mkTmp = make([]float32, N)
		// Where each dot pulls its value from: calm / punch zoom, each
		// twisting either way. Built once per size.
		for k := range v.mkMap {
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
			v.mkMap[k] = m
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
		for i, j := range v.mkMap[k] {
			if j < 0 {
				v.mkTmp[i] = 0
			} else {
				v.mkTmp[i] = v.mk[j] * decay
			}
		}
		v.mk, v.mkTmp = v.mkTmp, v.mk
	}
	dir := 1.0
	if flip != 0 {
		dir = -1
	}
	v.mkRot += dir * dt * (0.25 + 1.5*a.level)

	nb := 48
	v.bands = vizResample(v.bands, a.sm, nb)
	rmin := math.Min(float64(W), float64(H))
	r0 := rmin * (0.15 + 0.08*a.bass + 0.04*a.beat)
	R := rmin * 0.3
	plot := func(x, y float64, val float32) {
		xi, yi := int(x), int(y)
		if xi >= 0 && yi >= 0 && xi < W && yi < H {
			v.mk[yi*W+xi] = max(v.mk[yi*W+xi], val)
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
		ang := u*2*math.Pi + v.mkRot
		r := r0 + b*R
		plot(cx+math.Cos(ang)*r, cy+math.Sin(ang)*r, 1)

		var d float64
		if useWave {
			d = wave[min(int(fold*float64(len(wave)-1)), len(wave)-1)] / wp * 0.35
		} else {
			d = 0.12 * math.Sin(u*12*math.Pi+a.t*2) * (0.3 + 2*a.level)
		}
		a2 := -u*2*math.Pi - v.mkRot*1.7
		r2 := rw * (1 + d)
		plot(cx+math.Cos(a2)*r2, cy+math.Sin(a2)*r2, 0.7)
	}
	if a.kicks != v.mkK {
		v.mkK = a.kicks
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
	v.glow(v.mk, 0.2, &ramp)
}
