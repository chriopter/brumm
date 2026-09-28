// Package art fetches album covers and renders them as dithered sextant
// pixel art for the terminal.
package art

import (
	"bytes"
	"context"
	"fmt"
	"image"
	_ "image/jpeg" // Apple serves covers as JPEG
	_ "image/png"
	"io"
	"net/http"
)

// Size is a render target in terminal cells.
type Size struct {
	Width  int
	Height int
}

const (
	maxBytes  = 5 << 20
	maxPixels = 16_000_000
)

// Fetch downloads and decodes an image, refusing anything oversized before
// the full decode allocates it.
func Fetch(ctx context.Context, client *http.Client, url string) (image.Image, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("cover: %s", resp.Status)
	}
	buf, err := io.ReadAll(io.LimitReader(resp.Body, maxBytes+1))
	if err != nil {
		return nil, err
	}
	if len(buf) > maxBytes {
		return nil, fmt.Errorf("cover: larger than %d bytes", maxBytes)
	}
	cfg, _, err := image.DecodeConfig(bytes.NewReader(buf))
	if err != nil {
		return nil, err
	}
	if cfg.Width <= 0 || cfg.Height <= 0 || cfg.Width > maxPixels/cfg.Height {
		return nil, fmt.Errorf("cover: bad dimensions %dx%d", cfg.Width, cfg.Height)
	}
	img, _, err := image.Decode(bytes.NewReader(buf))
	return img, err
}

func hexColor(r, g, b uint8) string {
	return fmt.Sprintf("#%02x%02x%02x", r, g, b)
}
