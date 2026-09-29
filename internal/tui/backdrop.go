package tui

import (
	"image"
	"image/color"
	"math"
	"strconv"
	"strings"

	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/ipc"
)

// The backdrop: the playing cover's colors, blurred to a glow behind it
// that fades into the terminal's own background toward the stage's edges.
// Cell backgrounds only, so it works with every cover style; the cover
// itself (and a kitty image's placeholder cells) is drawn as it is.
//
// It is worked out once per cover, size and theme background; a frame only
// strings the cells' colors together, reusing whole rows where no text is.

// glowN is the cover's colors as a glowN×glowN grid: coarse on purpose,
// the blur comes from blending between them.
const glowN = 6

type backdrop struct {
	url  string
	bg   color.RGBA
	w, h int
	rgb  [][]uint32 // each cell's color, noGlow where it is the terminal's own
	sgr  [][]string // and the sequence that sets it
	rows []string   // whole rows of it, for rows with nothing on them
	// Pieces already strung, so a frame mostly looks them up: the stage's
	// layout and most of its text stay the same from one frame to the next.
	segs    map[[3]int]string
	painted map[paintKey]string
}

type paintKey struct {
	y, x int
	s    string
}

const pieceKeep = 2048 // pieces kept before starting over

const noGlow = 1 << 24

// stageBackdrop is the backdrop for a stage w×h, nil when there is none:
// switched off, nothing playing, no cover yet, or the terminal has not
// told its background.
func (m *Model) stageBackdrop(w, h int) *backdrop {
	playing := m.state.Title != "" || m.state.Preview != nil
	if m.opts.NoBackdrop || !playing || m.state.Status == ipc.StatusLoggedOut || m.cover == nil || m.termBg == nil || w <= 0 || h <= 0 {
		return nil
	}
	bg := rgba(m.termBg)
	if b := m.backdrop; b != nil && b.url == m.coverURL && b.bg == bg && b.w == w && b.h == h {
		return b
	}
	m.backdrop = newBackdrop(m.cover, m.coverURL, bg, w, h, m.cellAspect)
	return m.backdrop
}

func rgba(c color.Color) color.RGBA {
	r, g, b, _ := c.RGBA()
	return color.RGBA{uint8(r >> 8), uint8(g >> 8), uint8(b >> 8), 255}
}

// glowGrid averages the cover down to glowN×glowN colors, sampling a few
// points in each part rather than every pixel, then softens the grid once.
func glowGrid(img image.Image) [glowN][glowN][3]float64 {
	var g [glowN][glowN][3]float64
	b := img.Bounds()
	const k = 4 // samples per part and axis
	for gy := range glowN {
		for gx := range glowN {
			var sum [3]float64
			for sy := range k {
				for sx := range k {
					x := b.Min.X + ((gx*k+sx)*2+1)*b.Dx()/(2*glowN*k)
					y := b.Min.Y + ((gy*k+sy)*2+1)*b.Dy()/(2*glowN*k)
					r, gg, bl, _ := img.At(x, y).RGBA()
					sum[0] += float64(r >> 8)
					sum[1] += float64(gg >> 8)
					sum[2] += float64(bl >> 8)
				}
			}
			for c := range 3 {
				g[gy][gx][c] = sum[c] / k / k
			}
		}
	}
	var soft [glowN][glowN][3]float64
	for y := range glowN {
		for x := range glowN {
			var sum [3]float64
			n := 0.0
			for dy := -1; dy <= 1; dy++ {
				for dx := -1; dx <= 1; dx++ {
					yy, xx := y+dy, x+dx
					if yy < 0 || yy >= glowN || xx < 0 || xx >= glowN {
						continue
					}
					wt := 1.0
					if dx == 0 && dy == 0 {
						wt = 2
					}
					for c := range 3 {
						sum[c] += g[yy][xx][c] * wt
					}
					n += wt
				}
			}
			for c := range 3 {
				soft[y][x][c] = sum[c] / n
			}
		}
	}
	return soft
}

func newBackdrop(img image.Image, url string, bg color.RGBA, w, h int, aspect float64) *backdrop {
	grid := glowGrid(img)
	// Light themes take less of it: pale tints wash text out sooner.
	lum := (0.2126*float64(bg.R) + 0.7152*float64(bg.G) + 0.0722*float64(bg.B)) / 255
	strength := 0.34 - 0.12*lum
	// The grid spread over the stage as the cover would fill it, square in
	// pixels: cells are aspect times taller than wide.
	pw, ph := float64(w), float64(h)*aspect
	side := max(pw, ph)
	b := &backdrop{url: url, bg: bg, w: w, h: h, rgb: make([][]uint32, h), sgr: make([][]string, h), rows: make([]string, h),
		segs: map[[3]int]string{}, painted: map[paintKey]string{}}
	sgrs := map[uint32]string{}
	for y := range h {
		b.rgb[y], b.sgr[y] = make([]uint32, w), make([]string, w)
		for x := range w {
			// Where in the grid, 0…glowN-1, and how far from the middle.
			u := ((float64(x)+0.5-pw/2)/side + 0.5) * glowN
			v := (((float64(y)+0.5)*aspect-ph/2)/side + 0.5) * glowN
			dx, dy := (float64(x)+0.5)/pw*2-1, (float64(y)+0.5)/ph*aspect*2-1
			fade := 1 - smoothstep(0.3, 1.1, math.Hypot(dx, dy))
			a := strength * fade
			c := uint32(noGlow)
			if a > 0.01 {
				col := bilinear(&grid, u-0.5, v-0.5)
				mix := func(fg float64, bg uint8) uint32 {
					return uint32(math.Round(float64(bg)+(fg-float64(bg))*a)) &^ 1 // even steps: longer runs
				}
				c = mix(col[0], bg.R)<<16 | mix(col[1], bg.G)<<8 | mix(col[2], bg.B)
				if c == (uint32(bg.R)&^1)<<16|(uint32(bg.G)&^1)<<8|(uint32(bg.B)&^1) {
					c = noGlow
				}
			}
			s, ok := sgrs[c]
			if !ok {
				s = "\x1b[49m"
				if c != noGlow {
					s = "\x1b[48;2;" + strconv.Itoa(int(c>>16)) + ";" + strconv.Itoa(int(c>>8&0xff)) + ";" + strconv.Itoa(int(c&0xff)) + "m"
				}
				sgrs[c] = s
			}
			b.rgb[y][x], b.sgr[y][x] = c, s
		}
		b.rows[y] = b.strung(y, 0, w)
	}
	return b
}

func smoothstep(lo, hi, x float64) float64 {
	t := max(0, min(1, (x-lo)/(hi-lo)))
	return t * t * (3 - 2*t)
}

// bilinear reads the grid at a fractional place, held at its edges.
func bilinear(g *[glowN][glowN][3]float64, u, v float64) [3]float64 {
	u, v = max(0, min(glowN-1, u)), max(0, min(glowN-1, v))
	x0, y0 := int(u), int(v)
	x1, y1 := min(glowN-1, x0+1), min(glowN-1, y0+1)
	fx, fy := u-float64(x0), v-float64(y0)
	var out [3]float64
	for c := range 3 {
		top := g[y0][x0][c]*(1-fx) + g[y0][x1][c]*fx
		bot := g[y1][x0][c]*(1-fx) + g[y1][x1][c]*fx
		out[c] = top*(1-fy) + bot*fy
	}
	return out
}

// spaces is n cells of the backdrop from column x of row y; plain spaces
// without one.
func (b *backdrop) spaces(y, x, n int) string {
	if n <= 0 {
		return ""
	}
	if b == nil || y < 0 || y >= b.h {
		return strings.Repeat(" ", n)
	}
	if x == 0 && n == b.w {
		return b.rows[y]
	}
	k := [3]int{y, x, n}
	if s, ok := b.segs[k]; ok {
		return s
	}
	if len(b.segs) > pieceKeep {
		clear(b.segs)
	}
	s := b.strung(y, x, n)
	b.segs[k] = s
	return s
}

func (b *backdrop) strung(y, x, n int) string {
	var sb strings.Builder
	last := uint32(math.MaxUint32)
	for i := x; i < x+n; i++ {
		if i < 0 || i >= b.w {
			if last != noGlow {
				sb.WriteString("\x1b[49m")
				last = noGlow
			}
		} else if c := b.rgb[y][i]; c != last {
			sb.WriteString(b.sgr[y][i])
			last = c
		}
		sb.WriteByte(' ')
	}
	if last != noGlow {
		sb.WriteString("\x1b[49m")
	}
	return sb.String()
}

// paint lays styled text on row y from column x, the backdrop showing
// through wherever the text sets no background of its own.
func (b *backdrop) paint(y, x int, s string) string {
	if b == nil || y < 0 || y >= b.h {
		return s
	}
	k := paintKey{y, x, s}
	if p, ok := b.painted[k]; ok {
		return p
	}
	if len(b.painted) > pieceKeep {
		clear(b.painted)
	}
	p := b.paintNew(y, x, s)
	b.painted[k] = p
	return p
}

func (b *backdrop) paintNew(y, x int, s string) string {
	var sb strings.Builder
	sb.Grow(len(s) + 32)
	last := uint32(math.MaxUint32) // what the terminal has, as far as known
	own := false                   // the text set a background itself
	col := x
	var state byte
	for len(s) > 0 {
		seq, w, n, st := ansi.DecodeSequence(s, state, nil)
		state, s = st, s[n:]
		if w == 0 {
			if strings.HasPrefix(seq, "\x1b[") && strings.HasSuffix(seq, "m") {
				own = sgrOwnBg(seq[2:len(seq)-1], own)
				last = math.MaxUint32 // it may have reset ours
			}
			sb.WriteString(seq)
			continue
		}
		if !own { // a wide character takes its first cell's color
			c, sgr := uint32(noGlow), "\x1b[49m"
			if col >= 0 && col < b.w {
				c, sgr = b.rgb[y][col], b.sgr[y][col]
			}
			if c != last {
				sb.WriteString(sgr)
				last = c
			}
		}
		sb.WriteString(seq)
		col += w
	}
	if !own && last != noGlow {
		sb.WriteString("\x1b[49m")
	}
	return sb.String()
}

// sgrOwnBg says whether the text has a background of its own after an SGR
// with these parameters: a set background or reverse video.
func sgrOwnBg(params string, own bool) bool {
	if params == "" {
		return false
	}
	ps := strings.FieldsFunc(params, func(r rune) bool { return r == ';' || r == ':' })
	for i := 0; i < len(ps); i++ {
		switch p, _ := strconv.Atoi(ps[i]); {
		case p == 0 || p == 49 || p == 27:
			own = false
		case p == 7 || (p >= 40 && p <= 47) || (p >= 100 && p <= 107):
			own = true
		case p == 48:
			own = true
			fallthrough
		case p == 38 || p == 58: // a color: skip its values
			if i+1 < len(ps) && ps[i+1] == "5" {
				i += 2
			} else if i+1 < len(ps) && ps[i+1] == "2" {
				i += 4
			}
		}
	}
	return own
}
