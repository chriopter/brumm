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
	"image/draw"
	_ "image/jpeg" // Apple serves covers as JPEG
	_ "image/png"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"time"
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
// Covers never change at an address; Prune bounds the cache by dropping the
// least recently used.
func Cached(ctx context.Context, client *http.Client, dir, url string) (image.Image, error) {
	path := cachePath(dir, url)
	if b, err := os.ReadFile(path); err == nil {
		if img, err := decode(b); err == nil {
			now := time.Now()
			_ = os.Chtimes(path, now, now) // recently used: pruned last
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
	if os.MkdirAll(dir, 0o700) == nil {
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

func cachePath(dir, url string) string {
	sum := sha256.Sum256([]byte(url))
	return filepath.Join(dir, hex.EncodeToString(sum[:12]))
}

// Prune deletes the least recently used covers in dir until the rest fit
// in max bytes.
func Prune(dir string, max int64) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return
	}
	type file struct {
		path string
		size int64
		used time.Time
	}
	var files []file
	var total int64
	for _, e := range entries {
		info, err := e.Info()
		if err != nil || !info.Mode().IsRegular() {
			continue
		}
		files = append(files, file{filepath.Join(dir, e.Name()), info.Size(), info.ModTime()})
		total += info.Size()
	}
	sort.Slice(files, func(i, j int) bool { return files[i].used.Before(files[j].used) })
	for _, f := range files {
		if total <= max {
			break
		}
		if os.Remove(f.path) == nil {
			total -= f.size
		}
	}
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
	if err != nil {
		return nil, err
	}
	return shrink(img, keepPixels), nil
}

// keepPixels bounds a kept cover's longer side. Apple serves 600 px; the
// largest cover brumm draws needs ~2 pixels per cell across, so 320 keeps
// full detail at a fraction of the memory, and as RGBA the dithering reads
// it directly.
const keepPixels = 320

// shrink box-filters img so its longer side is at most n, as RGBA.
func shrink(img image.Image, n int) *image.RGBA {
	b := img.Bounds()
	src, ok := img.(*image.RGBA)
	if !ok {
		src = image.NewRGBA(b)
		draw.Draw(src, b, img, b.Min, draw.Src)
	}
	sw, sh := b.Dx(), b.Dy()
	if sw <= n && sh <= n {
		return src
	}
	dw, dh := n, sh*n/sw
	if sh > sw {
		dw, dh = sw*n/sh, n
	}
	dw, dh = max(dw, 1), max(dh, 1)
	dst := image.NewRGBA(image.Rect(0, 0, dw, dh))
	for y := range dh {
		y0, y1 := y*sh/dh, max((y+1)*sh/dh, y*sh/dh+1)
		for x := range dw {
			x0, x1 := x*sw/dw, max((x+1)*sw/dw, x*sw/dw+1)
			var r, g, bl, a, cnt int
			for sy := y0; sy < y1; sy++ {
				row := src.Pix[sy*src.Stride:]
				for sx := x0; sx < x1; sx++ {
					p := row[sx*4 : sx*4+4]
					r, g, bl, a = r+int(p[0]), g+int(p[1]), bl+int(p[2]), a+int(p[3])
					cnt++
				}
			}
			o := dst.PixOffset(x, y)
			dst.Pix[o], dst.Pix[o+1], dst.Pix[o+2], dst.Pix[o+3] = uint8(r/cnt), uint8(g/cnt), uint8(bl/cnt), uint8(a/cnt)
		}
	}
	return dst
}
