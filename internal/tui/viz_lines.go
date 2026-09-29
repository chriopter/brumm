package tui

import "math"

// ─── 2. scope ──────────────────────────────────────────────────────────────
// Oscilloscope: the waveform traced as a continuous braille line into a
// phosphor canvas that fades over a third of a second, so the trace leaves
// green afterglow over a dotted graticule. A trigger on a rising zero
// crossing holds periodic sounds still; kicks thicken the beam to white.
// Without wave data (or paused) it synthesizes a calm signal from the
// spectrum.
func (v *visualizer) drawScope(wave []float64, dt float64) {
	w, h := v.w, v.h
	W, H := w*2, h*4
	if len(v.ph) != W*H {
		v.ph = make([]float32, W*H)
	}
	decay := float32(math.Exp(-dt * 7))
	for i := range v.ph {
		v.ph[i] *= decay
	}
	if cap(v.wv) < W {
		v.wv = make([]float64, W)
	}
	s := v.wv[:W]
	a := &v.au

	peak := 0.0
	for _, x := range wave {
		peak = max(peak, math.Abs(x))
	}
	if m := len(wave); m >= 16 && a.live > 0.5 && peak > 0.005 {
		// Show two thirds of the buffer, starting at the first rising zero
		// crossing after the signal dipped (hysteresis against noise).
		L := m * 2 / 3
		start, armed := (m-L)/2, false
		for j := 1; j <= m-L; j++ {
			if wave[j] < -0.1*peak {
				armed = true
			} else if armed && wave[j-1] < 0 && wave[j] >= 0 {
				start = j
				break
			}
		}
		for x := range s {
			p := float64(start) + float64(x)*float64(L-1)/float64(max(1, W-1))
			j := min(int(p), m-1)
			y := wave[j]
			if j+1 < m {
				y += (wave[j+1] - y) * (p - float64(j))
			}
			s[x] = y
		}
	} else {
		// A handful of partials, amplitude from band groups.
		const parts = 7
		var amp [parts]float64
		nb := len(a.spec)
		norm := 0.0
		for k := 0; k < parts; k++ {
			lo, hi := k*nb/parts, max((k+1)*nb/parts, k*nb/parts+1)
			for j := lo; j < hi && j < nb; j++ {
				amp[k] += a.spec[j]
			}
			amp[k] /= float64(hi - lo)
			norm += amp[k]
		}
		t := a.t * 1.5
		for x := range s {
			u := float64(x) / float64(W) * 2 * math.Pi
			y := 0.0
			for k := 0; k < parts; k++ {
				if amp[k] > 0 {
					f := float64(int(1) << k)
					y += amp[k] * math.Sin(u*f*1.5+t*float64(k+1))
				}
			}
			if norm > 0 {
				y *= math.Min(1, norm) / norm
			}
			s[x] = y
		}
	}
	// Auto-gain so quiet passages still fill the screen; paused, the trace
	// shrinks to a gentle ripple.
	pk := 0.0
	for _, x := range s {
		pk = max(pk, math.Abs(x))
	}
	target := 1.0
	if pk > 0.02 {
		target = min(4, 0.85/pk)
	}
	target *= 0.3 + 0.7*a.live
	if v.gain == 0 {
		v.gain = target
	}
	v.gain += (target - v.gain) * min(1, dt*4)

	mid := float64(H-1) / 2
	amp := mid * v.gain
	thick := float32(0.6 * a.beat)
	py := -1
	for x := 0; x < W; x++ {
		y := min(max(int(mid-s[x]*amp+0.5), 0), H-1)
		y0, y1 := y, y
		if py >= 0 {
			if py < y {
				y0 = py + 1
			} else if py > y {
				y1 = py - 1
			}
		}
		for yy := y0; yy <= y1; yy++ {
			v.ph[yy*W+x] = 1
		}
		if thick > 0.2 {
			if y0 > 0 {
				v.ph[(y0-1)*W+x] = max(v.ph[(y0-1)*W+x], thick)
			}
			if y1 < H-1 {
				v.ph[(y1+1)*W+x] = max(v.ph[(y1+1)*W+x], thick)
			}
		}
		py = y
	}

	ramp := [8]uint8{vcGray, vcGray, vcGreen, vcGreen, vcGreen, vcBGreen, vcBGreen, vcBGreen}
	if a.beat > 0.5 {
		ramp[7] = vcBWhite
	}
	v.glow(v.ph, 0.1, &ramp)

	midRow := h / 2
	gx := max(4, w/12)
	gy := max(2, h/6)
	for y := 0; y < h; y++ {
		for x := 0; x < w; x++ {
			i := y*w + x
			if v.glyph[i] != ' ' {
				continue
			}
			switch {
			case y == midRow && x%2 == 0:
				v.glyph[i], v.col[i] = '·', vcGray
			case x%gx == 0 && (y-midRow)%gy == 0:
				v.glyph[i], v.col[i] = '+', vcGray
			}
		}
	}
}

// ─── 3. ridge ──────────────────────────────────────────────────────────────
// Unknown Pleasures: the spectrum (bass in the middle, flat noisy edges) is
// frozen into a ridgeline ten times a second and recedes toward a horizon in
// perspective, each line hiding what lies behind it (floating horizon). The
// front line is live; lines fade white → cyan → blue → gray with distance.
var vizRidgePal = [6]uint8{vcBWhite, vcWhite, vcBCyan, vcCyan, vcBlue, vcGray}

func (v *visualizer) drawRidge(style int, dt float64) {
	w, h := v.w, v.h
	W, H := w*2, h*4
	R := min(max(H/7, 3), 40)
	P := min(max(W/4, 16), 128)
	if v.rgR != R || v.rgP != P {
		v.rg = make([]float32, R*P)
		v.rgLive = make([]float32, P)
		v.rgNoise = make([]float32, P)
		v.rgR, v.rgP, v.rgHead = R, P, 0
	}
	if cap(v.rgHz) < W {
		v.rgHz = make([]int, W)
	}
	hz := v.rgHz[:W]
	a := &v.au

	pushes := v.ticks(style, dt, 10, 2)
	if pushes > 0 {
		for p := range v.rgNoise {
			v.rgNoise[p] = float32(v.rf())
		}
	}
	for p := range v.rgLive {
		u := float64(p) / float64(P-1)
		d := math.Abs(u-0.5) * 2
		// flat edges, the music in the middle
		e := min(max((d-0.5)/0.45, 0), 1)
		env := 1 - e*e*(3-2*e)
		n := float64(v.rgNoise[p])
		v.rgLive[p] = float32(env*vizAt(a.sm, d/0.95)*(0.8+0.4*n) + 0.015*n)
	}
	for ; pushes > 0; pushes-- {
		v.rgHead = (v.rgHead + 1) % R
		copy(v.rg[v.rgHead*P:(v.rgHead+1)*P], v.rgLive)
	}

	v.dotReset()
	for x := range hz {
		hz[x] = H
	}
	f := v.acc[style] // how far the history has slid toward the next slot
	kp := 3 / float64(R)
	hy, yb := float64(H)*0.12, float64(H-1)
	cx := float64(W) / 2
	line := func(row []float32, d float64, rank uint8, boost float64) {
		s := 1 / (1 + d*kp)
		base := hy + (yb-hy)*s
		A := float64(H) * 0.42 * s * boost
		half := cx * (0.3 + 0.68*s)
		x0 := cx - half
		py := -1
		for xd := max(int(x0), 0); xd <= min(int(cx+half), W-1); xd++ {
			p := min(max((float64(xd)-x0)/(2*half), 0), 1) * float64(P-1)
			j := int(p)
			val := float64(row[j])
			if j+1 < P {
				val += (float64(row[j+1]) - val) * (p - float64(j))
			}
			y := int(base - A*val)
			lo, hi := y, y
			if py >= 0 {
				if py < y {
					lo = py + 1
				} else if py > y {
					hi = py - 1
				}
			}
			for yy := max(lo, 0); yy <= hi && yy < hz[xd]; yy++ {
				v.dot(xd, yy, rank)
			}
			hz[xd] = min(hz[xd], y)
			py = y
		}
	}
	line(v.rgLive, 0, 0, 1+0.25*a.beat)
	for j := 0; j < R; j++ {
		k := ((v.rgHead-j)%R + R) % R
		line(v.rg[k*P:(k+1)*P], float64(j)+f, uint8(1+j*5/R), 1)
	}
	v.dotFlush(vizRidgePal[:])
}
