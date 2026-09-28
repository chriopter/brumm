// Package art fetches album covers and renders them as dithered sextant
// pixel art for the terminal.
package art

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"image"
	_ "image/jpeg" // Apple serves covers as JPEG
	_ "image/png"
	"io"
	"net/http"
	"os"
	"path/filepath"
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

// Cached returns a cover from the disk cache, downloading it on a miss.
// Covers never change at an address, so entries never expire.
func Cached(ctx context.Context, client *http.Client, dir, url string) (image.Image, error) {
	sum := sha256.Sum256([]byte(url))
	path := filepath.Join(dir, hex.EncodeToString(sum[:12]))
	if b, err := os.ReadFile(path); err == nil {
		if img, err := decode(b); err == nil {
			return img, nil
		}
	}
	b, err := download(ctx, client, url)
	if err != nil {
		return nil, err
	}
	img, err := decode(b)
	if err != nil {
		return nil, err
	}
	if os.MkdirAll(dir, 0o755) == nil {
		tmp, err := os.CreateTemp(dir, ".cover-*")
		if err == nil {
			_, werr := tmp.Write(b)
			tmp.Close()
			if werr == nil {
				_ = os.Rename(tmp.Name(), path)
			} else {
				os.Remove(tmp.Name())
			}
		}
	}
	return img, nil
}

// Fetch downloads and decodes an image, refusing anything oversized before
// the full decode allocates it.
func Fetch(ctx context.Context, client *http.Client, url string) (image.Image, error) {
	b, err := download(ctx, client, url)
	if err != nil {
		return nil, err
	}
	return decode(b)
}

func download(ctx context.Context, client *http.Client, url string) ([]byte, error) {
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
	return buf, nil
}

func decode(buf []byte) (image.Image, error) {
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
