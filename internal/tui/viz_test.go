package tui

import (
	"fmt"
	"math"
	"os"
	"regexp"
	"strconv"
	"strings"
	"testing"
	"time"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
)

func vizData(kind string, n, frame int) (spec, wave []float64) {
	switch kind {
	case "nil":
		return nil, nil
	case "silent":
		return make([]float64, n), make([]float64, 256)
	}
	var r uint64 = uint64(frame)*2654435761 + 7
	next := func() float64 {
		r ^= r << 13
		r ^= r >> 7
		r ^= r << 17
		return float64(r>>11) / (1 << 53)
	}
	spec = make([]float64, n)
	beat := math.Exp(-float64(frame%12) * 0.25) // kick every 12 frames
	for i := range spec {
		// music-ish: smooth, rolling off toward the highs, bass kicks
		u := float64(i) / float64(n)
		x := (1 - u*0.7) * (0.45 + 0.25*math.Sin(float64(frame)*0.21+u*9) + 0.15*math.Sin(float64(frame)*0.07-u*23))
		if u < 0.2 {
			x += beat * 0.5 * (1 - u*5)
		}
		spec[i] = math.Min(1, math.Max(0, x+0.06*next()))
	}
	if kind == "random" {
		wave = make([]float64, 512)
		for i := range wave {
			wave[i] = math.Sin(float64(i)*0.07+float64(frame)*0.3)*0.6 + (next()-0.5)*0.3
		}
	}
	return spec, wave // "specOnly": wave nil, scope must synthesize
}

// vizTick makes v's clock advance 40 ms (one 25 fps frame) per render.
func vizTick(v *visualizer) {
	t := time.Unix(0, 0)
	v.now = func() time.Time {
		t = t.Add(40 * time.Millisecond)
		return t
	}
}

func TestVizRender(t *testing.T) {
	sizes := [][2]int{{80, 24}, {237, 68}, {300, 90}, {20, 6}, {1, 1}, {2, 1}, {1, 5}, {3, 2}, {0, 3}}
	for style := range vizNames {
		for _, sz := range sizes {
			for _, kind := range []string{"nil", "silent", "random", "specOnly"} {
				var v visualizer
				vizTick(&v)
				for f := 0; f < 60; f++ {
					spec, wave := vizData(kind, 48, f)
					playing := f < 30 || f >= 50 // pause, then resume
					lines := v.render(style, spec, wave, sz[0], sz[1], f, playing)
					if len(lines) != sz[1] {
						t.Fatalf("%s %v %s: %d lines, want %d", vizNames[style], sz, kind, len(lines), sz[1])
					}
					for i, l := range lines {
						if got := lipgloss.Width(l); got != sz[0] {
							t.Fatalf("%s %v %s line %d: width %d, want %d", vizNames[style], sz, kind, i, got, sz[0])
						}
						if strings.Contains(l, "\x1b[") && !strings.HasSuffix(l, "\x1b[0m") {
							t.Fatalf("%s line %d: unbalanced reset", vizNames[style], i)
						}
					}
				}
			}
		}
	}
	if len(vizNames) != vizCount {
		t.Fatalf("vizNames has %d entries, want %d", len(vizNames), vizCount)
	}
}

// Only the theme's 16 colors, foreground and background: no 256-color or
// truecolor sequences, and every SGR a plain color, default or reset.
func TestVizThemeColors(t *testing.T) {
	sgr := regexp.MustCompile(`\x1b\[([0-9;]*)m`)
	for style := range vizNames {
		var v visualizer
		vizTick(&v)
		for f := 0; f < 60; f++ {
			spec, wave := vizData("random", 48, f)
			for _, l := range v.render(style, spec, wave, 100, 30, f, f < 45) {
				for _, m := range sgr.FindAllStringSubmatch(l, -1) {
					for _, p := range strings.Split(m[1], ";") {
						n, err := strconv.Atoi(p)
						ok := err == nil && (n == 0 || n == 39 || n == 49 ||
							(n >= 30 && n <= 37) || (n >= 90 && n <= 97) ||
							(n >= 40 && n <= 47) || (n >= 100 && n <= 107))
						if !ok {
							t.Fatalf("%s: SGR %q is not a theme color", vizNames[style], m[0])
						}
					}
				}
				if strings.Contains(ansi.Strip(l), "\x1b") {
					t.Fatalf("%s: stray escape", vizNames[style])
				}
			}
		}
	}
}

// A dissolve mixes two styles block by block: well formed all the way,
// the old style at 0, the new one at 1, both in between.
func TestVizMix(t *testing.T) {
	var v, b visualizer
	vizTick(&v)
	vizTick(&b)
	sizes := [][2]int{{80, 24}, {1, 1}, {7, 3}, {200, 60}}
	for f := 0; f < 80; f++ {
		sz := sizes[(f/20)%len(sizes)]
		spec, wave := vizData("random", 48, f)
		from, to := f%vizCount, (f*5+3)%vizCount
		lines := v.renderMix(from, to, float64(f%20)/19, spec, wave, sz[0], sz[1], true)
		if len(lines) != sz[1] {
			t.Fatalf("mix %d lines, want %d", len(lines), sz[1])
		}
		for i, l := range lines {
			if got := lipgloss.Width(l); got != sz[0] {
				t.Fatalf("mix %v line %d: width %d", sz, i, got)
			}
		}
	}
	// the ends: all old, all new (fresh visualizers see the same clock)
	spec, wave := vizData("random", 48, 1)
	mix0 := v.renderMix(vzScope, vzLED, 0, spec, wave, 60, 20, true)
	full := strings.Count(strings.Join(b.render(vzLED, spec, wave, 60, 20, 0, true), ""), "▆")
	if strings.Contains(strings.Join(mix0, ""), "▆") {
		t.Fatalf("mix at 0 shows the incoming LED style")
	}
	mix1 := v.renderMix(vzScope, vzLED, 1, spec, wave, 60, 20, true)
	if n := strings.Count(strings.Join(mix1, ""), "▆"); n != full {
		t.Fatalf("mix at 1 shows %d LED segments, want %d", n, full)
	}
	half := strings.Join(v.renderMix(vzScope, vzLED, 0.5, spec, wave, 60, 20, true), "")
	if n := strings.Count(half, "▆"); n == 0 || n == full {
		t.Fatalf("mix at 0.5 shows %d of %d LED segments", n, full)
	}
}

// Unchanged rows come back as the very same string, not a new encoding.
func TestVizRowReuse(t *testing.T) {
	var v visualizer
	vizTick(&v)
	a := v.render(vzLED, nil, nil, 40, 10, 0, false)
	b := v.render(vzLED, nil, nil, 40, 10, 1, false)
	same := 0
	for i := range a {
		if a[i] == b[i] {
			same++
		}
	}
	if same == 0 {
		t.Fatalf("no row reused")
	}
}

// Drawn at 120 fps with the spectrum arriving at 30 and a kick every half
// second, the full-screen styles must move evenly: about the same number
// of cells changing every frame, no screen-wide jolt on the beat. (The old
// plasma, whose speed and palette lurched on every kick, scored cv 0.62
// with single frames repainting 72% of the screen.)
func TestVizSmooth(t *testing.T) {
	for _, style := range []int{vzPlasma, vzFire, vzLava} {
		var v visualizer
		tm := time.Unix(0, 0)
		v.now = func() time.Time { tm = tm.Add(time.Second / 120); return tm }
		const w, h = 120, 40
		var prev []rune
		var pc, pb []uint8
		var ch []float64
		for f := 0; f < 600; f++ {
			spec, wave := vizData("random", 48, f/4)
			v.render(style, spec, wave, w, h, f, true)
			if f > 60 {
				n := 0
				for j := range prev {
					if prev[j] != v.glyph[j] || pc[j] != v.col[j] || pb[j] != v.bg[j] {
						n++
					}
				}
				ch = append(ch, float64(n))
			}
			prev = append(prev[:0], v.glyph...)
			pc = append(pc[:0], v.col...)
			pb = append(pb[:0], v.bg...)
		}
		var sum, sq, mx float64
		for _, x := range ch {
			sum += x
			sq += x * x
			mx = max(mx, x)
		}
		m := sum / float64(len(ch))
		cv := math.Sqrt(sq/float64(len(ch))-m*m) / m
		// lava's picture is mostly still, so its kick swells stand out in
		// cv; what no style may do is repaint most of the screen at once
		if (cv > 0.35 && style != vzLava) || mx > 0.3*w*h {
			t.Errorf("%s: changed cells/frame mean %.0f cv %.2f max %.0f: jerky", vizNames[style], m, cv, mx)
		}
	}
}

// Beyond its output strings a frame allocates nothing.
func TestVizAllocs(t *testing.T) {
	const w, h = 200, 60
	var specs, waves [24][]float64
	for i := range specs {
		specs[i], waves[i] = vizData("random", 48, i)
	}
	for style := range vizNames {
		var v visualizer
		vizTick(&v)
		f := 0
		frame := func() {
			v.render(style, specs[f%24], waves[f%24], w, h, f, true)
			f++
		}
		for range 200 { // warm up: canvases, particles, drops
			frame()
		}
		// at most h strings and the slice holding them
		if n := testing.AllocsPerRun(50, frame); n > h+1 {
			t.Errorf("%s: %.0f allocations a frame, want ≤ %d", vizNames[style], n, h+1)
		}
	}
}

// Resizing mid-run, switching styles back and forth (a dissolve draws two
// per frame) and a stalled clock must all keep the frame well formed.
func TestVizResizeAndSwitch(t *testing.T) {
	var v visualizer
	vizTick(&v)
	sizes := [][2]int{{80, 24}, {1, 1}, {300, 90}, {20, 6}, {5, 40}}
	for f := 0; f < 200; f++ {
		sz := sizes[(f/7)%len(sizes)]
		spec, wave := vizData("random", 48, f)
		for _, style := range []int{f % vizCount, (f/3 + 4) % vizCount} {
			lines := v.render(style, spec, wave, sz[0], sz[1], f, f%50 < 40)
			if len(lines) != sz[1] {
				t.Fatalf("frame %d %s: %d lines, want %d", f, vizNames[style], len(lines), sz[1])
			}
			for i, l := range lines {
				if got := lipgloss.Width(l); got != sz[0] {
					t.Fatalf("frame %d %s %v line %d: width %d", f, vizNames[style], sz, i, got)
				}
			}
		}
	}
}

// Paused, every style must keep moving (breathing), not freeze.
func TestVizIdleBreathes(t *testing.T) {
	for style := range vizNames {
		var v visualizer
		vizTick(&v)
		var first string
		changed := false
		for f := 0; f < 80; f++ {
			lines := v.render(style, nil, nil, 60, 20, f, false)
			s := strings.Join(lines, "\n")
			if f == 20 {
				first = s
			} else if f > 20 && s != first {
				changed = true
			}
		}
		if !changed {
			t.Errorf("%s: paused screen is static", vizNames[style])
		}
	}
}

// The kick detector must catch the synthetic kick every 12 frames and
// nothing while paused.
func TestVizKicks(t *testing.T) {
	var v visualizer
	vizTick(&v)
	for f := 0; f < 120; f++ {
		spec, wave := vizData("random", 48, f)
		v.render(0, spec, wave, 40, 10, f, true)
	}
	if k := v.au.kicks; k < 6 || k > 12 {
		t.Fatalf("%d kicks in 120 frames, want about 10", k)
	}
	k := v.au.kicks
	for f := 0; f < 60; f++ {
		spec, wave := vizData("random", 48, f)
		v.render(0, spec, wave, 40, 10, f, false)
	}
	if v.au.kicks != k {
		t.Fatalf("kicks counted while paused")
	}
}

// VIZDUMP=1 go test -run TestVizDump -v prints every style as plain text
// (VIZDUMP=color in color, VIZONLY=name one style, VIZPAUSE=1 paused).
func TestVizDump(t *testing.T) {
	if os.Getenv("VIZDUMP") == "" {
		t.Skip("set VIZDUMP=1")
	}
	only := os.Getenv("VIZONLY") // one style by name
	for style := range vizNames {
		if only != "" && only != vizNames[style] {
			continue
		}
		var v visualizer
		vizTick(&v)
		var lines []string
		for f := 0; f < 60; f++ {
			spec, wave := vizData("random", 48, f)
			lines = v.render(style, spec, wave, 100, 24, f, f < 40 || os.Getenv("VIZPAUSE") == "")
		}
		t.Logf("── %s", vizNames[style])
		for _, l := range lines {
			if os.Getenv("VIZDUMP") == "color" {
				os.Stdout.WriteString(l + "\n")
			} else {
				t.Log("|" + ansi.Strip(l) + "|")
			}
		}
	}
}

// BenchmarkViz renders every style at 240×70 and 200×60, the clock
// advancing one 25 fps frame per render so the simulations do real work.
func BenchmarkViz(b *testing.B) {
	var specs, waves [24][]float64
	for i := range specs {
		specs[i], waves[i] = vizData("random", 48, i)
	}
	for _, sz := range [][2]int{{240, 70}, {200, 60}} {
		for style, name := range vizNames {
			b.Run(fmt.Sprintf("%s/%dx%d", name, sz[0], sz[1]), func(b *testing.B) {
				var v visualizer
				vizTick(&v)
				b.ReportAllocs()
				for i := 0; b.Loop(); i++ {
					v.render(style, specs[i%24], waves[i%24], sz[0], sz[1], i, true)
				}
			})
		}
	}
}
