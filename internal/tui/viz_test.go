package tui

import (
	"fmt"
	"math"
	"os"
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
