package art

import (
	"fmt"
	"image"
	"strings"
)

// RenderSmooth draws the cover in true color with half blocks: each cell
// is two square pixels, the upper one as the glyph and the lower one as the
// background. No palette, no dithering — the cover as it is, only coarse.
func RenderSmooth(img image.Image, size Size) []string {
	if img == nil || size.Width <= 0 || size.Height <= 0 || img.Bounds().Empty() {
		return nil
	}
	gw, gh := size.Width, size.Height*2
	px := sampleGrid(img, gw, gh)
	lines := make([]string, size.Height)
	for row := range size.Height {
		var sb strings.Builder
		for col := range size.Width {
			t, b := u8(px[2*row*gw+col]), u8(px[(2*row+1)*gw+col])
			fmt.Fprintf(&sb, "\x1b[38;2;%d;%d;%d;48;2;%d;%d;%dm▀", t[0], t[1], t[2], b[0], b[1], b[2])
		}
		sb.WriteString("\x1b[m")
		lines[row] = sb.String()
	}
	return lines
}
