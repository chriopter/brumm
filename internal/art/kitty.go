package art

import (
	"bytes"
	"encoding/base64"
	"fmt"
	"image"
	"image/png"
	"os"
	"strings"

	"github.com/charmbracelet/x/ansi/kitty"
)

// The kitty graphics protocol shows the cover at the terminal's full
// resolution. It is placed with Unicode placeholders: the image is sent
// once, and the cells that show it are ordinary text — a private-use
// character whose foreground color names the image and whose diacritics
// name the row and column — so the TUI's renderer moves, clips and redraws
// it like any other text. kitty and Ghostty support it.

// KittySupported guesses from the environment whether the terminal speaks
// the kitty graphics protocol with Unicode placeholders.
func KittySupported() bool {
	switch {
	case os.Getenv("KITTY_WINDOW_ID") != "", os.Getenv("TERM") == "xterm-kitty",
		os.Getenv("TERM_PROGRAM") == "ghostty", os.Getenv("GHOSTTY_RESOURCES_DIR") != "":
		return true
	}
	return false
}

// KittySend is the escape sequence that uploads img as image id, fitted
// into a box of size cells. Ids are 16–255, so the 256-color foreground
// that names them survives any color handling.
func KittySend(id int, img image.Image, size Size) (string, error) {
	var buf bytes.Buffer
	if err := png.Encode(&buf, img); err != nil {
		return "", err
	}
	data := base64.StdEncoding.EncodeToString(buf.Bytes())
	var sb strings.Builder
	const chunk = 4096
	for i := 0; i < len(data) || i == 0; i += chunk {
		part := data[i:min(i+chunk, len(data))]
		more := 0
		if i+chunk < len(data) {
			more = 1
		}
		if i == 0 {
			// a=T with U=1: transmit and make a virtual placement for the
			// placeholders; q=2: no replies to swallow.
			fmt.Fprintf(&sb, "\x1b_Ga=T,U=1,f=100,i=%d,c=%d,r=%d,q=2,m=%d;%s\x1b\\", id, size.Width, size.Height, more, part)
		} else {
			fmt.Fprintf(&sb, "\x1b_Gm=%d;%s\x1b\\", more, part)
		}
	}
	return sb.String(), nil
}

// KittyDelete frees image id in the terminal.
func KittyDelete(id int) string { return fmt.Sprintf("\x1b_Ga=d,d=I,i=%d,q=2\x1b\\", id) }

// KittyLines are the placeholder cells that show image id in a box of size.
func KittyLines(id int, size Size) []string {
	lines := make([]string, size.Height)
	for r := range size.Height {
		var sb strings.Builder
		fmt.Fprintf(&sb, "\x1b[38;5;%dm", id)
		for c := range size.Width {
			sb.WriteRune(kitty.Placeholder)
			sb.WriteRune(kitty.Diacritic(r))
			sb.WriteRune(kitty.Diacritic(c))
		}
		sb.WriteString("\x1b[m")
		lines[r] = sb.String()
	}
	return lines
}

// Original returns a cover at its full size, from the disk cache that
// Cached fills.
func Original(dir, url string) (image.Image, error) {
	b, err := os.ReadFile(cachePath(dir, url))
	if err != nil {
		return nil, err
	}
	cfg, _, err := image.DecodeConfig(bytes.NewReader(b))
	if err != nil {
		return nil, err
	}
	if cfg.Width <= 0 || cfg.Height <= 0 || cfg.Width > maxPixels/cfg.Height {
		return nil, fmt.Errorf("cover: bad dimensions %dx%d", cfg.Width, cfg.Height)
	}
	img, _, err := image.Decode(bytes.NewReader(b))
	return img, err
}
