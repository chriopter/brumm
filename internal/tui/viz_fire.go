package tui

import "math"

// ─── 4. fire ───────────────────────────────────────────────────────────────
// Demoscene fire at 30 steps a second: the spectrum (bass in the middle)
// feeds heat at the bottom, which rises, spreads, flickers and cools; ░▒▓█
// in red → yellow → bright yellow → white. Kicks flare the flames and throw
// a burst of sparks; loud passages shed embers that drift up and die.
type vizSpark struct{ x, y, vx, vy, life float64 }

func (v *visualizer) drawFire(style int, dt float64) {
	w, h := v.w, v.h
	H := h + 2
	if v.heatW != w || v.heatH != H {
		v.heat = make([]float32, w*H)
		v.heatW, v.heatH = w, H
		v.sparks = v.sparks[:0]
	}
	a := &v.au
	v.fireSd = vizMirror(v.fireSd, a.sm, w)
	cool := float32(2.0 / float64(h))
	flare := 1 + 0.6*a.beat
	for s := v.ticks(style, dt, 30, 3); s > 0; s-- {
		// seed the two bottom rows
		for x := 0; x < w; x++ {
			e := math.Sqrt(v.fireSd[x]) * flare * (0.8 + 0.6*v.rf())
			e = min(e, 1.5)
			v.heat[(H-1)*w+x] = float32(e)
			v.heat[(H-2)*w+x] = float32(e * (0.85 + 0.15*v.rf()))
		}
		// propagate upward
		for y := 0; y < H-2; y++ {
			r1 := (y + 1) * w
			r2 := (y + 2) * w
			ro := y * w
			for x := 0; x < w; x++ {
				r := v.rnd()
				xs := min(max(x+int(r%3)-1, 0), w-1)
				xl := max(x-1, 0)
				xr := min(x+1, w-1)
				t := (v.heat[r1+xl] + v.heat[r1+xs] + v.heat[r1+xr] + v.heat[r2+x]) * 0.25
				t -= cool * float32((r>>8)&255) / 255
				if t < 0 {
					t = 0
				}
				v.heat[ro+x] = t
			}
		}
	}

	// Sparks: a burst per kick, a trickle with loudness, at most a few hundred.
	fw, fh := float64(w), float64(h)
	emit := 0
	if a.kicks != v.fireK {
		v.fireK = a.kicks
		emit = 4 + w/8
	}
	if rate := a.level * a.live * fw * 0.6 * dt; v.rf() < rate-math.Floor(rate) {
		emit += int(rate) + 1
	} else {
		emit += int(rate)
	}
	for ; emit > 0 && len(v.sparks) < 400; emit-- {
		v.sparks = append(v.sparks, vizSpark{
			x:    fw/2 + (v.rf()-0.5)*fw*0.7,
			y:    fh - 1 - v.rf()*fh*0.25,
			vx:   (v.rf() - 0.5) * fw * 0.08,
			vy:   -(0.35 + 0.55*v.rf()) * fh,
			life: 0.5 + 0.8*v.rf(),
		})
	}
	for i := 0; i < len(v.sparks); {
		p := &v.sparks[i]
		p.life -= dt
		p.x += p.vx * dt
		p.y += p.vy * dt
		p.vy += fh * 0.25 * dt // air slows them
		p.vx += (v.rf() - 0.5) * fw * 0.1 * dt
		if p.life <= 0 || p.y < 0 || p.x < 0 || p.x >= fw {
			v.sparks[i] = v.sparks[len(v.sparks)-1]
			v.sparks = v.sparks[:len(v.sparks)-1]
			continue
		}
		i++
	}

	for y := 0; y < h; y++ {
		ro := y * w
		for x := 0; x < w; x++ {
			t := v.heat[ro+x]
			var g rune
			var c uint8
			switch {
			case t < 0.06:
				continue
			case t < 0.14:
				g, c = '░', vcRed
			case t < 0.24:
				g, c = '▒', vcRed
			case t < 0.36:
				g, c = '▓', vcRed
			case t < 0.48:
				g, c = '▓', vcBRed
			case t < 0.6:
				g, c = '▓', vcYellow
			case t < 0.74:
				g, c = '█', vcYellow
			case t < 0.9:
				g, c = '█', vcBYellow
			default:
				g, c = '█', vcBWhite
			}
			v.glyph[ro+x], v.col[ro+x] = g, c
		}
	}
	for _, p := range v.sparks {
		x, y := int(p.x), int(p.y)
		if y < 0 || y >= h || v.heat[y*w+x] > 0.36 {
			continue // hidden in the flames
		}
		g, c := '·', uint8(vcRed)
		switch {
		case p.life > 0.8:
			g, c = '•', vcBYellow
		case p.life > 0.4:
			c = vcYellow
		}
		v.glyph[y*w+x], v.col[y*w+x] = g, c
	}
}
