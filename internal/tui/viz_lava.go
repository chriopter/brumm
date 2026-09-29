package tui

import "math"

// ─── lava ──────────────────────────────────────────────────────────────────
// Lava lamp: seven metaballs drift on slow Lissajous paths through a
// half-block canvas (square pixels, two per cell); where their fields add
// up past one they merge into a blob, shaded from a dithered glow at the
// rim to a hot core. Each ball is as big as its part of the spectrum (the
// bass the biggest), a kick swells them all at once, the loudness hurries
// the drift, and every eighth kick pours a new color into the lamp,
// dissolving over a second and a half.
const vizBalls = 7

type vizLava struct {
	ph, speed, pulse float64
	swell            float64 // pulse, eased in so a kick swells rather than jumps
	k                int
	pal, old         int       // palette, and the one it's dissolving from
	mix              float64   // 0 → 1 over the dissolve
	dx2, dy2         []float32 // squared distances: ball × column, ball × pixel row
}

var vizLavaPal = [4][5]uint8{
	{vcBlue, vcMagenta, vcBMagenta, vcBRed, vcBYellow},
	{vcRed, vcRed, vcBRed, vcYellow, vcBYellow},
	{vcBlue, vcBlue, vcCyan, vcBCyan, vcBWhite},
	{vcGreen, vcGreen, vcBGreen, vcBYellow, vcBWhite},
}

// per ball: x and y frequency, x and y phase
var vizBallPath = [vizBalls][4]float64{
	{0.31, 0.43, 0.0, 1.7},
	{0.23, 0.37, 2.1, 0.4},
	{0.41, 0.29, 4.0, 2.9},
	{0.19, 0.53, 1.1, 5.2},
	{0.37, 0.21, 5.5, 3.3},
	{0.27, 0.47, 3.2, 0.9},
	{0.45, 0.33, 0.7, 4.4},
}

func (v *visualizer) drawLava(dt float64) {
	w, h := v.w, v.h
	P := 2 * h
	s := &v.lava
	if len(s.dx2) != vizBalls*w || len(s.dy2) != vizBalls*P {
		s.dx2 = make([]float32, vizBalls*w)
		s.dy2 = make([]float32, vizBalls*P)
	}
	a := &v.au
	target := 0.25 + a.live*(0.2+1.2*a.level)
	s.speed += (target - s.speed) * min(1, dt*1.5)
	s.ph += dt * s.speed
	if a.kicks != s.k {
		s.k = a.kicks
		s.pulse = min(s.pulse+0.22, 0.4)
	}
	s.pulse *= math.Exp(-dt * 4)
	s.swell += (s.pulse - s.swell) * min(1, dt*12)
	if p := (a.kicks / 8) % len(vizLavaPal); p != s.pal {
		s.old, s.pal, s.mix = s.pal, p, 0
	}
	s.mix = min(1, s.mix+dt/1.5)

	fw, fp := float64(w), float64(P)
	base := math.Min(fw, fp) * 0.12
	var r2 [vizBalls]float32
	for k, bp := range vizBallPath {
		bx := fw/2 + fw*0.32*math.Sin(s.ph*bp[0]*2+bp[2])
		by := fp/2 + fp*0.34*math.Sin(s.ph*bp[1]*2+bp[3])
		r := base * (0.6 + 1.1*vizAt(a.sm, float64(k)/(vizBalls-1)) + s.swell)
		r2[k] = float32(r * r)
		dx := s.dx2[k*w : k*w+w]
		for x := range dx {
			d := float64(x) + 0.5 - bx
			dx[x] = float32(d*d) + 1
		}
		dy := s.dy2[k*P : k*P+P]
		for y := range dy {
			d := float64(y) + 0.5 - by
			dy[y] = float32(d * d)
		}
	}
	pal, old := &vizLavaPal[s.pal], &vizLavaPal[s.old]
	mix := float32(s.mix)
	pix := v.pixReset()
	for y := 0; y < P; y++ {
		var dy [vizBalls]float32
		var most float32 // the field can't beat this anywhere on the row
		for k := range dy {
			dy[k] = s.dy2[k*P+y]
			most += r2[k] / (dy[k] + 1)
		}
		if most < 0.6 {
			continue
		}
		by := &vizBayer[y&3]
		out := pix[y*w : y*w+w]
		for x := range out {
			var f float32
			for k := 0; k < vizBalls; k++ {
				f += r2[k] / (s.dx2[k*w+x] + dy[k])
			}
			if f < 0.6 {
				continue
			}
			d := by[x&3]
			p := pal
			if d >= mix { // a new color pours in dot by dot
				p = old
			}
			if f < 1 { // the glow around a blob, thinning outward
				if (f-0.6)*2.5 > d {
					out[x] = p[0]
				}
				continue
			}
			// depth into the blob: 0 at its skin, toward 1 deep inside
			t := 1 - 1/f
			out[x] = p[1+min(int(t*4.2+d-0.5), 3)]
		}
	}
	v.pixFlush(0, h)
}
