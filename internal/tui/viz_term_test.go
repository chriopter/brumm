package tui

import (
	"bytes"
	"fmt"
	"os"
	"strings"
	"testing"
	"time"

	uv "github.com/charmbracelet/ultraviolet"
)

// vizPipe replays what bubbletea's renderer does with a fullscreen frame:
// parse the styled string into a cell buffer, diff it against the last
// frame and write the changes, so the benchmark sees the real cost of a
// style downstream: the parse, the diff and the bytes the terminal must eat.
type vizPipe struct {
	out  bytes.Buffer
	cell uv.ScreenBuffer
	scr  *uv.TerminalRenderer
}

func newVizPipe(w, h int) *vizPipe {
	p := &vizPipe{cell: uv.NewScreenBuffer(w, h)}
	p.scr = uv.NewTerminalRenderer(&p.out, []string{"TERM=xterm-256color"})
	p.scr.SetFullscreen(true)
	p.scr.SetRelativeCursor(false)
	p.scr.Resize(w, h)
	return p
}

// frame pushes one frame through and returns the bytes it wrote.
func (p *vizPipe) frame(lines []string) int {
	p.out.Reset()
	p.cell.Clear()
	uv.NewStyledString(strings.Join(lines, "\n")).Draw(p.cell, p.cell.Bounds())
	p.scr.Render(p.cell.RenderBuffer)
	_ = p.scr.Flush()
	return p.out.Len()
}

// BenchmarkVizTerm is the whole trip at 200×60, drawn at 120 fps with the
// spectrum arriving at 30: the style's render plus bubbletea's parse, diff
// and write, reporting the bytes handed over and those the terminal gets.
func BenchmarkVizTerm(b *testing.B) {
	var specs, waves [24][]float64
	for i := range specs {
		specs[i], waves[i] = vizData("random", 48, i)
	}
	tick120 := func(v *visualizer) {
		t := time.Unix(0, 0)
		v.now = func() time.Time {
			t = t.Add(time.Second / 120)
			return t
		}
	}
	const w, h = 200, 60
	for style, name := range vizNames {
		b.Run(fmt.Sprintf("%s/%dx%d", name, w, h), func(b *testing.B) {
			var v visualizer
			tick120(&v)
			p := newVizPipe(w, h)
			var n, frames, chg, out int
			pg := make([]rune, w*h)
			pc := make([]uint8, w*h)
			for i := 0; b.Loop(); i++ {
				lines := v.render(style, specs[i/4%24], waves[i/4%24], w, h, i, true)
				for _, l := range lines {
					out += len(l)
				}
				n += p.frame(lines)
				for j := range pg {
					if pg[j] != v.glyph[j] || pc[j] != v.col[j] {
						chg++
					}
				}
				copy(pg, v.glyph)
				copy(pc, v.col)
				frames++
			}
			b.ReportMetric(float64(out)/float64(frames), "out-B/frame")
			b.ReportMetric(float64(n)/float64(frames), "term-B/frame")
			b.ReportMetric(float64(chg)/float64(frames), "chg/frame")
		})
	}
}

func vizIndex(name string) int {
	for i, n := range vizNames {
		if n == name {
			return i
		}
	}
	return 0
}

// VIZOUT=file go test -run TestVizFrames writes a few colored frames of
// every style (VIZONLY=name for one) to file, for looking at.
func TestVizFrames(t *testing.T) {
	path := os.Getenv("VIZOUT")
	if path == "" {
		t.Skip("set VIZOUT")
	}
	only := os.Getenv("VIZONLY")
	w, h := 120, 34
	var sb strings.Builder
	for style, name := range vizNames {
		if only != "" && only != name {
			continue
		}
		var v visualizer
		vizTick(&v)
		for f := 0; f <= 90; f++ {
			spec, wave := vizData("random", 48, f)
			lines := v.render(style, spec, wave, w, h, f, f < 80 || os.Getenv("VIZPAUSE") == "")
			if f == 45 || f == 90 {
				fmt.Fprintf(&sb, "@@ %s frame %d\n", name, f)
				sb.WriteString(strings.Join(lines, "\n") + "\n")
			}
		}
	}
	if err := os.WriteFile(path, []byte(sb.String()), 0o644); err != nil {
		t.Fatal(err)
	}
}

// BenchmarkVizMix is the same dissolve done on the grids by renderMix:
// both styles drawn, mixed and encoded once.
func BenchmarkVizMix(b *testing.B) {
	const w, h = 200, 60
	spec, wave := vizData("random", 48, 3)
	var v visualizer
	vizTick(&v)
	for b.Loop() {
		v.renderMix(vizIndex("plasma"), vizIndex("scope"), 0.5, spec, wave, w, h, true)
	}
}
