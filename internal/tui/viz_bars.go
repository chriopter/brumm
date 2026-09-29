package tui

import "math"

// ─── 1. bars ───────────────────────────────────────────────────────────────
// Classic Winamp spectrum: thick bars with 1-cell gaps, eighth-block tops,
// LED coloring by height and peak caps that hold, then fall with gravity.
// A freshly pushed cap is white; a kick lights every bar's tip.
func (v *visualizer) drawBars(dt float64) {
	w, h := v.w, v.h
	bw := 1
	switch {
	case w >= 120:
		bw = 3
	case w >= 60:
		bw = 2
	}
	nb := max(1, (w+1)/(bw+1))
	off := (w - (nb*(bw+1) - 1)) / 2
	v.bands = vizResample(v.bands, v.au.sm, nb)
	pk := v.barPk.update(v.bands, dt, 0.4)
	flash := v.au.beat > 0.4
	h8 := h * 8
	for i, b := range v.bands {
		x0 := off + i*(bw+1)
		e := int(b*float64(h8) + 0.5)
		tip := (e+7)/8 - 1 // row of the bar's top cell, -1 when empty
		for row := 0; row <= tip && row < h; row++ {
			g := '█'
			if fill := e - row*8; fill < 8 {
				g = vizEighths[fill]
			}
			c := meterColor((float64(row) + 0.5) / float64(h))
			if flash && row == tip {
				c = vcBWhite
			}
			for dx := 0; dx < bw; dx++ {
				v.set(x0+dx, h-1-row, g, c)
			}
		}
		if pk[i] <= 0.015 {
			continue
		}
		// The cap floats with half-cell precision, resting on the bar.
		p8 := min(int(pk[i]*float64(h8)), h8-1)
		pr, g := p8/8, '▁'
		if p8%8 >= 4 {
			g = '▔'
		}
		if pr <= tip {
			pr, g = tip+1, '▁'
		}
		if pr >= h {
			continue
		}
		c := vizBright(meterColor((float64(pr) + 0.5) / float64(h)))
		if v.barPk.hold[i] > 0 {
			c = vcBWhite
		}
		for dx := 0; dx < bw; dx++ {
			v.set(x0+dx, h-1-pr, g, c)
		}
	}
}

// ─── 7. led ────────────────────────────────────────────────────────────────
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

// ─── 6. waterfall ──────────────────────────────────────────────────────────
// Mirrored spectrogram: bass in the middle, highs at both edges. The newest
// row enters at the top at 20 rows a second and history flows down, fading
// with age; every kick leaves a dotted line that rides down with it.
var vizWfGlyph = [9]rune{' ', '·', '░', '░', '▒', '▒', '▓', '▓', '█'}
var vizWfCol = [9]uint8{0, vcBlue, vcBlue, vcCyan, vcCyan, vcBCyan, vcBCyan, vcBWhite, vcBWhite}

func (v *visualizer) drawWaterfall(style int, dt float64) {
	w, h := v.w, v.h
	if v.wfW != w || v.wfH != h {
		v.wf = make([]uint8, w*h)
		v.wfW, v.wfH, v.wfHead = w, h, 0
	}
	v.bands = vizMirror(v.bands, v.au.sm, w)
	for s := v.ticks(style, dt, 20, 4); s > 0; s-- {
		kick := v.au.kicks != v.wfK
		v.wfK = v.au.kicks
		v.wfHead = (v.wfHead + 1) % h
		row := v.wf[v.wfHead*w : (v.wfHead+1)*w]
		for x, b := range v.bands {
			l := int((0.55*b+0.45*math.Sqrt(b))*9.4 - 0.5)
			if kick {
				l = max(l+1, 1)
			}
			row[x] = uint8(min(max(l, 0), 8))
		}
	}
	for y := 0; y < h; y++ {
		src := ((v.wfHead-y)%h + h) % h
		row := v.wf[src*w : (src+1)*w]
		fade := y * 3 / h
		ro := y * w
		for x, l := range row {
			k := int(l) - fade
			if k <= 0 {
				continue
			}
			v.glyph[ro+x], v.col[ro+x] = vizWfGlyph[k], vizWfCol[k]
		}
	}
}
