package tui

import (
	"math"
	"os"
	"strings"
	"testing"

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

func TestVizRender(t *testing.T) {
	sizes := [][2]int{{80, 24}, {237, 68}, {20, 6}, {1, 1}, {3, 2}, {0, 3}}
	for style := range vizNames {
		for _, sz := range sizes {
			for _, kind := range []string{"nil", "silent", "random", "specOnly"} {
				var v visualizer
				for f := 0; f < 40; f++ {
					spec, wave := vizData(kind, 64, f)
					playing := f < 30
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

// VIZDUMP=1 go test -run TestVizDump -v prints every style as plain text.
func TestVizDump(t *testing.T) {
	if os.Getenv("VIZDUMP") == "" {
		t.Skip("set VIZDUMP=1")
	}
	for style := range vizNames {
		var v visualizer
		var lines []string
		for f := 0; f < 60; f++ {
			spec, wave := vizData("random", 64, f)
			lines = v.render(style, spec, wave, 100, 24, f, true)
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

func BenchmarkViz(b *testing.B) {
	spec, wave := vizData("random", 64, 1)
	for style, name := range vizNames {
		b.Run(name, func(b *testing.B) {
			var v visualizer
			b.ReportAllocs()
			for i := 0; i < b.N; i++ {
				v.render(style, spec, wave, 240, 70, i, true)
			}
		})
	}
}
