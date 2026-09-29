package tui

import "math"

// ─── 5. tunnel ─────────────────────────────────────────────────────────────
// Demoscene tunnel: every cell looks up its angle and depth in a table
// twice the screen's size, so the vanishing point can wander without any
// trigonometry per frame. A checkered, twisting texture streams toward the
// viewer, faster with loudness and in a lurch on every kick; the walls are
// lit by the spectrum (bass on the floor, highs on the ceiling) and fog
// darkens the far end, where a light blinks with the kicks.
var vizTunnelPal = [3]uint8{vcBlue, vcMagenta, vcCyan}

func (v *visualizer) drawTunnel(dt float64) {
	w, h := v.w, v.h
	TW, TH := 2*w, 2*h
	if v.tnW != w || v.tnH != h {
		v.tnW, v.tnH = w, h
		n := TW * TH
		v.tnZ = make([]float32, n)
		v.tnU = make([]float32, n)
		v.tnFog = make([]float32, n)
		v.tnB = make([]uint8, n)
		rs := math.Max(float64(h)*0.5, 1) // radius of the screen's inscribed circle, in rows
		for ty := 0; ty < TH; ty++ {
			for tx := 0; tx < TW; tx++ {
				dx := (float64(tx-w) + 0.5) * 0.5 // cells are twice as tall as wide
				dy := float64(ty-h) + 0.5
				r := math.Hypot(dx, dy)
				i := ty*TW + tx
				v.tnZ[i] = float32(rs / math.Max(r, 0.2))
				v.tnU[i] = float32(math.Atan2(dy, dx)/(2*math.Pi) + 0.5)
				// linear fog hides the far end, where the texture would alias
				v.tnFog[i] = float32(min(max((r-1)/(rs*0.9), 0), 1))
				// 0 straight down … 1 straight up
				v.tnB[i] = uint8(min(math.Abs(math.Atan2(dx, dy))/math.Pi*32, 31))
			}
		}
	}
	a := &v.au
	target := 0.25 + a.live*(0.5+3*a.level) + 4*a.beat
	v.tnSpeed += (target - v.tnSpeed) * min(1, dt*4)
	v.tnGo += v.tnSpeed * dt
	v.tnRot += dt * (0.4 + 2*a.mid)
	v.bands = vizResample(v.bands, a.sm, 32)

	// The vanishing point drifts on a slow Lissajous path.
	ox := int(math.Sin(a.t*0.31) * float64(w) * 0.16)
	oy := int(math.Cos(a.t*0.23) * float64(h) * 0.16)
	sx, sy := w-w/2-ox, h-h/2-oy // table offset of screen (0, 0)
	// Wrapped at a multiple of 6 rings: parity and hue survive, and so
	// does float32 precision after hours of travel.
	travel := float32(math.Mod(v.tnGo*3, 600))
	rot := float32(math.Mod(v.tnRot, 2))
	flash := float32(0.35 * a.beat)
	for y := 0; y < h; y++ {
		ti := (y+sy)*TW + sx
		ro := y * w
		for x := 0; x < w; x++ {
			i := ti + x
			fog := v.tnFog[i]
			if fog < 0.02 {
				continue
			}
			z := v.tnZ[i]
			rp := z*3 + travel
			sp := v.tnU[i]*8 + rot + z*0.6 // twist with depth
			ri := int(rp)
			lum := fog * (0.3 + 0.9*float32(v.bands[v.tnB[i]]))
			if (ri^int(sp))&1 == 0 {
				lum *= 0.3
			}
			if rp-float32(ri) < 0.12 { // bright ring edges
				lum *= 1.35
			}
			lum += flash * fog
			k := int(lum * 5.5)
			if k <= 0 {
				continue
			}
			k = min(k, 5)
			c := vizTunnelPal[ri%3]
			if k >= 4 {
				c = vizBright(c)
			}
			v.glyph[ro+x], v.col[ro+x] = vizShade[k], c
		}
	}
	if a.beat > 0.3 {
		v.set(w/2+ox, h/2+oy, '*', vcBWhite)
	} else {
		v.set(w/2+ox, h/2+oy, '·', vcGray)
	}
}

// ─── 8. stars ──────────────────────────────────────────────────────────────
// Warp starfield on a braille canvas, so stars glide a quarter cell at a
// time: they fly out of the center while the field slowly rolls, speed
// eased toward the loudness and kicked into hyperspace on every beat, when
// the streaks stretch long. Distance sets color (gray → blue → cyan →
// white); the nearest stars swell to fat dots.
type vizStar struct{ x, y, z float64 }

var vizStarPal = [5]uint8{vcBWhite, vcBCyan, vcCyan, vcBlue, vcGray}

func (v *visualizer) drawStars(dt float64) {
	w, h := v.w, v.h
	W, H := float64(w*2), float64(h*4)
	cx, cy := W/2, H/2
	S := H / 2 // dots per unit at z = 1; braille dots are square
	R := math.Hypot(cx/S, 1) * 1.05
	maxN := min(max(int(W*H)/50, 80), 3000)
	for len(v.stars) < maxN {
		v.stars = append(v.stars, vizStar{(v.rf()*2 - 1) * R, (v.rf()*2 - 1) * R, 0.05 + v.rf()*0.95})
	}
	v.stars = v.stars[:maxN]
	a := &v.au
	target := 0.06 + a.live*(0.25+1.6*a.level) + 3.5*a.beat
	v.stSpeed += (target - v.stSpeed) * min(1, dt*5)
	v.stRol += dt * (0.03 + 0.3*a.level)
	cs, sn := math.Cos(v.stRol), math.Sin(v.stRol)
	tail := v.stSpeed * 0.06

	v.dotReset()
	for i := range v.stars {
		st := &v.stars[i]
		st.z -= v.stSpeed * dt
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
