package tui

import (
	"image"
	"path/filepath"
	"strconv"
	"strings"

	tea "charm.land/bubbletea/v2"

	"github.com/chriopter/brumm/internal/art"
	"github.com/chriopter/brumm/internal/config"
)

// drawCover renders img at size in a cover style; original is drawn by
// kittyLines, and smooth stands in for it until the image is there.
func drawCover(style string, img image.Image, size art.Size) []string {
	if style == coverPixel {
		return art.RenderDithered(img, size)
	}
	return art.RenderSmooth(img, size)
}

// placeholder fills a cover's footprint while it loads.
func placeholder(size art.Size, icon string) []string {
	out := make([]string, size.Height)
	for i := range out {
		l := strings.Repeat("░", size.Width)
		if icon != "" && i == size.Height/2 && size.Width > 4 {
			l = strings.Repeat("░", size.Width/2-1) + " " + icon + " " + strings.Repeat("░", size.Width-size.Width/2-2)
		}
		out[i] = sDim.Render(l)
	}
	return out
}

// A cover shown through kitty graphics: sent to the terminal once per
// address and size, then drawn as placeholder cells.
type kittyImage struct {
	id    int
	ready bool // sent; until then the cover draws smooth
	used  int  // for evicting the least recently drawn
}

type kittyReq struct {
	key, url string
	size     art.Size
}

type kittyMsg struct {
	key, seq string
}

// Terminals keep images until told otherwise; brumm keeps at most this many.
const kittyKeep = 24

// kittyLines draws a cover through kitty graphics, or reports false while
// it is not in the terminal yet (it is then requested).
func (m *Model) kittyLines(url string, size art.Size) ([]string, bool) {
	key := url + "@" + sizeKey(size)
	if k := m.kitty[key]; k != nil {
		m.kittyClock++
		k.used = m.kittyClock
		if k.ready {
			return art.KittyLines(k.id, size), true
		}
		return nil, false
	}
	for _, w := range m.kittyWant {
		if w.key == key {
			return nil, false
		}
	}
	m.kittyWant = append(m.kittyWant, kittyReq{key, url, size})
	return nil, false
}

func sizeKey(s art.Size) string { return strconv.Itoa(s.Width) + "x" + strconv.Itoa(s.Height) }

// kittySend uploads the first wanted cover off the event loop, one at a
// time, freeing the least recently drawn one when too many are kept.
func (m *Model) kittySend() tea.Cmd {
	if m.kittyBusy || len(m.kittyWant) == 0 {
		return nil
	}
	w := m.kittyWant[0]
	m.kittyWant = m.kittyWant[1:]
	if m.kitty[w.key] != nil {
		return m.kittySend()
	}
	var cmds []tea.Cmd
	used := map[int]bool{}
	for len(m.kitty) >= kittyKeep {
		oldest := ""
		for k, v := range m.kitty {
			if oldest == "" || v.used < m.kitty[oldest].used {
				oldest = k
			}
		}
		cmds = append(cmds, tea.Raw(art.KittyDelete(m.kitty[oldest].id)))
		delete(m.kitty, oldest)
	}
	for _, v := range m.kitty {
		used[v.id] = true
	}
	id := 0
	for i := range 240 { // ids 16–255, see art.KittySend
		c := 16 + (m.kittyNext+i)%240
		if !used[c] {
			id = c
			break
		}
	}
	m.kittyNext = id - 16 + 1
	m.kittyClock++
	m.kitty[w.key] = &kittyImage{id: id, used: m.kittyClock}
	m.kittyBusy = true
	fallback := m.covers[w.url]
	cmds = append(cmds, func() tea.Msg {
		img, err := art.Original(filepath.Join(config.CacheDir(), "covers"), w.url)
		if err != nil {
			img = fallback
		}
		if img == nil {
			return kittyMsg{key: w.key}
		}
		seq, _ := art.KittySend(id, img, w.size)
		return kittyMsg{key: w.key, seq: seq}
	})
	return tea.Batch(cmds...)
}

// kittySent marks a cover as shown and passes its upload to the terminal.
func (m *Model) kittySent(msg kittyMsg) tea.Cmd {
	m.kittyBusy = false
	k := m.kitty[msg.key]
	if k == nil {
		return m.kittySend()
	}
	if msg.seq == "" {
		k.ready = false // no image: stay smooth, and do not ask again
		return m.kittySend()
	}
	k.ready = true
	return tea.Batch(tea.Raw(msg.seq), m.kittySend())
}

// kittyCleanup frees every image brumm sent, for when it quits.
func (m *Model) kittyCleanup() string {
	var sb strings.Builder
	for _, k := range m.kitty {
		sb.WriteString(art.KittyDelete(k.id))
	}
	return sb.String()
}
