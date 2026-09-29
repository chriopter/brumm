package tui

import "math"

// ─── stars ─────────────────────────────────────────────────────────────────
// Warp starfield on a braille canvas, so stars glide a quarter cell at a
// time: they fly out of the center while the field slowly rolls, speed
// eased toward the loudness and kicked into hyperspace on every beat, when
// the streaks stretch long. Distance sets color (gray → blue → cyan →
// white); the nearest stars swell to fat dots.
type vizStar struct{ x, y, z float64 }

type vizStars struct {
	st         []vizStar
	speed, rol float64
}

var vizStarPal = [5]uint8{vcBWhite, vcBCyan, vcCyan, vcBlue, vcGray}

func (v *visualizer) drawStars(dt float64) {
	w, h := v.w, v.h
	s := &v.stars
	W, H := float64(w*2), float64(h*4)
	cx, cy := W/2, H/2
	S := H / 2 // dots per unit at z = 1; braille dots are square
	R := math.Hypot(cx/S, 1) * 1.05
	maxN := min(max(int(W*H)/50, 80), 3000)
	for len(s.st) < maxN {
		s.st = append(s.st, vizStar{(v.rf()*2 - 1) * R, (v.rf()*2 - 1) * R, 0.05 + v.rf()*0.95})
	}
	s.st = s.st[:maxN]
	a := &v.au
	target := 0.06 + a.live*(0.25+1.6*a.level) + 3.5*a.beat
	s.speed += (target - s.speed) * min(1, dt*5)
	s.rol += dt * (0.03 + 0.3*a.level)
	cs, sn := math.Cos(s.rol), math.Sin(s.rol)
	tail := s.speed * 0.06

	v.dotReset()
	for i := range s.st {
		st := &s.st[i]
		st.z -= s.speed * dt
		if st.z < 0.03 {
			*st = vizStar{(v.rf()*2 - 1) * R, (v.rf()*2 - 1) * R, 0.6 + v.rf()*0.4}
		}
		px, py := st.x*cs-st.y*sn, st.x*sn+st.y*cs
		hx, hy := cx+px/st.z*S, cy+py/st.z*S
		if hx < -1 || hy < -1 || hx >= W+1 || hy >= H+1 {
			st.z = 0 // respawn next frame
			continue
		}
		var r uint8
		switch {
		case st.z > 0.75:
			r = 4
		case st.z > 0.5:
			r = 3
		case st.z > 0.3:
			r = 2
		case st.z > 0.15:
			r = 1
		}
		tz := st.z + tail
		v.dotLine(cx+px/tz*S, cy+py/tz*S, hx, hy, r)
		if st.z < 0.2 {
			x, y := int(hx), int(hy)
			v.dot(x+1, y, r)
			v.dot(x, y+1, r)
			v.dot(x+1, y+1, r)
		}
	}
	v.dotFlush(vizStarPal[:])
}

// ─── wire ──────────────────────────────────────────────────────────────────
// Vector-display 3D: a wireframe spins in perspective on a braille phosphor
// that fades in a twenty-fifth of a second, so fast turns smear into motion blur.
// Three shapes take turns (every 16 kicks or 24 seconds): an icosahedron
// whose vertices spike out with their spectrum band around a counter-
// rotating inner one, a tesseract turning through the fourth dimension,
// and a torus whose tube swells with the spectrum around its ring. Near
// edges burn white, far ones fade to blue; the bass pumps the size, every
// kick throws in a spin that eases off, every fourth recolors the beam.
type vizWire struct {
	c              []float32 // phosphor, one value per dot
	rows           []bool    // cell rows with anything lit
	ax, ay, az, aw float64
	spin           float64
	k              int
	px, py, pz     [vizTorusN]float64 // projected vertices: dots, dots, depth 0 near…1 far
}

const (
	vizTorusM = 20 // segments around the ring
	vizTorusS = 8  // around the tube
	vizTorusN = vizTorusM * vizTorusS
)

var (
	vizIco      [12][3]float64
	vizIcoEdges [][2]int
	vizTesEdges [][2]int
	vizWireRamp = [3][8]uint8{
		{vcGray, vcBlue, vcBlue, vcCyan, vcCyan, vcBCyan, vcBCyan, vcBWhite},
		{vcGray, vcBlue, vcMagenta, vcMagenta, vcBMagenta, vcBMagenta, vcBWhite, vcBWhite},
		{vcGray, vcGreen, vcGreen, vcBGreen, vcBGreen, vcBYellow, vcBYellow, vcBWhite},
	}
)

func init() {
	phi := (1 + math.Sqrt(5)) / 2
	n := math.Sqrt(1 + phi*phi)
	k := 0
	for _, s1 := range []float64{-1, 1} {
		for _, s2 := range []float64{-1, 1} {
			vizIco[k] = [3]float64{0, s1 / n, s2 * phi / n}
			vizIco[k+1] = [3]float64{s1 / n, s2 * phi / n, 0}
			vizIco[k+2] = [3]float64{s2 * phi / n, 0, s1 / n}
			k += 3
		}
	}
	for i := range vizIco {
		for j := i + 1; j < len(vizIco); j++ {
			d := 0.0
			for c := 0; c < 3; c++ {
				d += (vizIco[i][c] - vizIco[j][c]) * (vizIco[i][c] - vizIco[j][c])
			}
			if math.Abs(math.Sqrt(d)-2/n) < 1e-6 {
				vizIcoEdges = append(vizIcoEdges, [2]int{i, j})
			}
		}
	}
	for i := 0; i < 16; i++ {
		for b := 0; b < 4; b++ {
			if j := i ^ 1<<b; j > i {
				vizTesEdges = append(vizTesEdges, [2]int{i, j})
			}
		}
	}
}

func (v *visualizer) drawWire(dt float64) {
	w, h := v.w, v.h
	W, H := w*2, h*4
	s := &v.wire
	if len(s.c) != W*H || len(s.rows) != h {
		s.c = make([]float32, W*H)
		s.rows = make([]bool, h)
	}
	vizDecayRows(s.c, W, s.rows, float32(math.Exp(-dt*25)))
	a := &v.au
	if a.kicks != s.k {
		s.k = a.kicks
		s.spin = min(s.spin+0.9, 2)
	}
	s.spin *= math.Exp(-dt * 3)
	sp := 0.3 + a.live*0.6*a.level + s.spin
	s.ay += dt * sp
	s.ax += dt * sp * 0.61
	s.az += dt * sp * 0.23
	s.aw += dt * (0.3 + 0.6*a.mid + 0.4*s.spin)

	// rotation matrix Rz·Rx·Ry
	cx, sx := math.Cos(s.ax), math.Sin(s.ax)
	cy, sy := math.Cos(s.ay), math.Sin(s.ay)
	cz, sz := math.Cos(s.az), math.Sin(s.az)
	m := [3][3]float64{
		{cz*cy - sz*sx*sy, -sz * cx, cz*sy + sz*sx*cy},
		{sz*cy + cz*sx*sy, cz * cx, sz*sy - cz*sx*cy},
		{-cx * sy, sx, cx * cy},
	}
	ox, oy := float64(W)/2, float64(H)/2
	S := math.Min(float64(W), float64(H)) * 0.3 * (1 + 0.16*a.bass + 0.1*a.beat)
	const dist = 3.2
	put := func(i int, x, y, z float64, inv bool) {
		var X, Y, Z float64
		if inv { // the transpose: the opposite turn
			X = m[0][0]*x + m[1][0]*y + m[2][0]*z
			Y = m[0][1]*x + m[1][1]*y + m[2][1]*z
			Z = m[0][2]*x + m[1][2]*y + m[2][2]*z
		} else {
			X = m[0][0]*x + m[0][1]*y + m[0][2]*z
			Y = m[1][0]*x + m[1][1]*y + m[1][2]*z
			Z = m[2][0]*x + m[2][1]*y + m[2][2]*z
		}
		f := dist / (Z + dist + 0.001) * S
		s.px[i], s.py[i] = ox+X*f, oy+Y*f
		s.pz[i] = vizClamp((Z+1.4)/2.8, 0, 1)
	}
	edge := func(i, j int, bright float32) {
		b0 := bright * float32(1-0.72*s.pz[i])
		b1 := bright * float32(1-0.72*s.pz[j])
		vizPlot(s.c, s.rows, W, H, s.px[i], s.py[i], s.px[j], s.py[j], b0, b1)
	}
	vert := func(i int, b float32) {
		x, y := int(s.px[i]), int(s.py[i])
		for d := 0; d < 4; d++ {
			xx, yy := x+d&1, y+d>>1
			if xx >= 0 && yy >= 0 && xx < W && yy < H {
				s.c[yy*W+xx] = max(s.c[yy*W+xx], b*float32(1-0.6*s.pz[i]))
				s.rows[yy>>2] = true
			}
		}
	}
	spark := float32(0.55 + 0.45*min(1, a.high*2.5))

	switch (int(a.t/24) + a.kicks/16) % 3 {
	case 0: // icosahedron with spectrum spikes around a smaller, opposite one
		for i, p := range vizIco {
			r := 1 + 0.4*vizAt(a.sm, float64(i)/11)
			put(i, p[0]*r, p[1]*r, p[2]*r, false)
			put(12+i, p[0]*0.42, p[1]*0.42, p[2]*0.42, true)
		}
		for _, e := range vizIcoEdges {
			edge(e[0], e[1], 1)
			edge(12+e[0], 12+e[1], 0.75)
		}
		for i := 0; i < 12; i++ {
			vert(i, spark)
		}
	case 1: // tesseract: turn in the xw and yw planes, project 4D → 3D
		c1, s1 := math.Cos(s.aw), math.Sin(s.aw)
		c2, s2 := math.Cos(s.aw*0.7), math.Sin(s.aw*0.7)
		inner := 1 + 0.35*a.bass
		for i := 0; i < 16; i++ {
			p := [4]float64{-1, -1, -1, -1}
			for b := 0; b < 4; b++ {
				if i>>b&1 != 0 {
					p[b] = 1
				}
			}
			p[3] *= inner
			x, ww := p[0]*c1-p[3]*s1, p[0]*s1+p[3]*c1
			y, ww2 := p[1]*c2-ww*s2, p[1]*s2+ww*c2
			k := 0.55 * 2.4 / (2.4 - ww2*0.6)
			put(i, x*k, y*k, p[2]*k, false)
		}
		for _, e := range vizTesEdges {
			edge(e[0], e[1], 1)
		}
		for i := 0; i < 16; i++ {
			vert(i, spark)
		}
	default: // torus, the tube swelling with the spectrum around the ring
		for i := 0; i < vizTorusM; i++ {
			u := float64(i) / vizTorusM * 2 * math.Pi
			f := math.Abs(float64(i)/vizTorusM*2 - 1) // mirrored: bass at one side
			tube := 0.3 * (1 + 1.1*vizAt(a.sm, 1-f))
			cu, su := math.Cos(u), math.Sin(u)
			for j := 0; j < vizTorusS; j++ {
				t := float64(j)/vizTorusS*2*math.Pi + s.aw
				r := 0.82 + tube*math.Cos(t)
				put(i*vizTorusS+j, r*cu, r*su, tube*math.Sin(t), false)
			}
		}
		for i := 0; i < vizTorusM; i++ {
			for j := 0; j < vizTorusS; j++ {
				k := i*vizTorusS + j
				edge(k, i*vizTorusS+(j+1)%vizTorusS, 0.8)
				edge(k, ((i+1)%vizTorusM)*vizTorusS+j, 1)
			}
		}
	}
	v.glow(s.c, s.rows, 0.12, &vizWireRamp[(a.kicks/4)%3])
}
