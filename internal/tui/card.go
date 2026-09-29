package tui

import (
	"fmt"
	"image"
	"math"
	"strings"
	"time"

	"github.com/charmbracelet/x/ansi"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/art"
)

// The selected row shows as a card laid on the stage's cover: the playing
// cover dims behind it, and the card sits in its lower right corner with a
// margin of background around it and its name under it. One picture with
// another on top, not two side by side. Scrolling through songs previews
// their covers the same way. Nothing playing, the card is the stage. Once
// the list rests for cardFor, the card goes and the playing cover is back.

// card is what a selected row shows on the stage.
type card struct {
	art, kind, name, sub string
}

// cardFor is how long a card stays after the selection last moved.
const cardFor = 4 * time.Second

// cardLeft is how long the selected row's card has left on the stage.
func (m *Model) cardLeft() time.Duration {
	return cardFor - time.Duration(m.frame-m.cur().selAt)*frameEvery
}

// selectedCard is the card of the selected row, if it has one worth
// showing: not the song playing, not a cover already on the stage, and
// only a while after the selection moved — or for good when nothing plays.
func (m *Model) selectedCard() *card {
	v := m.cur()
	if m.opts.ReduceMotion || v.sel >= len(v.rows) {
		return nil
	}
	if m.state.Title != "" && m.cardLeft() <= 0 {
		return nil
	}
	var c card
	switch r := v.rows[v.sel]; {
	case r.track != nil:
		if r.track.ID == m.state.ID || r.track.Artwork == "" {
			return nil
		}
		c = card{r.track.Artwork, "song", r.track.Title, r.track.Artist}
	case r.item != nil:
		it := r.item
		switch it.Kind {
		case apple.KindAlbum, apple.KindPlaylist, apple.KindArtist, apple.KindStation:
		default:
			return nil
		}
		c = card{it.Artwork, it.Kind, it.Name, it.Info}
		if c.sub == "" {
			c.sub = it.Artist
		}
	default:
		return nil
	}
	if c.art != "" && c.art == m.coverURL && m.state.Title != "" {
		return nil // the stage shows it already
	}
	return &c
}

// cardLines draws a card's cover at size: through kitty graphics, from
// the rendered cache, or as a placeholder while it is being drawn.
func (m *Model) cardLines(c *card, size art.Size) []string {
	if m.opts.cover() == coverOriginal && m.covers[c.art] != nil {
		if lines, ok := m.kittyLines(c.art, size); ok {
			return lines
		}
	}
	m.cardSize = size
	key := m.thumbKey(c.art, size)
	if lines, ok := m.thumbs[key]; ok {
		return lines
	}
	if c.art != "" {
		m.thumbWant = thumbReq{key, c.art, size}
	}
	icon := kindIcon(c.kind)
	if c.kind == "song" {
		icon = icSong
	}
	return placeholder(size, icon)
}

func (m *Model) thumbKey(art string, size art.Size) string {
	return fmt.Sprintf("%s:%s@%dx%d", m.opts.cover(), art, size.Width, size.Height)
}

// nextThumb is a card worth drawing ahead: the rows around the selection,
// nearest first, whose cover is loaded and card not drawn yet; else an
// up-next cover.
func (m *Model) nextThumb() thumbReq {
	v, size := m.cur(), m.cardSize
	if size.Width == 0 || m.opts.ReduceMotion {
		return m.nextThumbWant()
	}
	for _, d := range []int{1, -1, 2, -2, 3, 4, 5, -3} {
		i := v.sel + d
		if i < 0 || i >= len(v.rows) {
			continue
		}
		url := ""
		if t := v.rows[i].track; t != nil {
			url = t.Artwork
		} else if it := v.rows[i].item; it != nil {
			url = it.Artwork
		}
		if url == "" || m.covers[url] == nil {
			continue
		}
		if key := m.thumbKey(url, size); m.thumbs[key] == nil {
			return thumbReq{key, url, size}
		}
	}
	return m.nextThumbWant() // then the covers coming up
}

// caption is a card's one line: its name, then artist or facts.
func (c *card) caption() string {
	s := sBold.Render(c.name)
	if c.sub != "" {
		s += sDim.Render("  " + c.sub)
	}
	return s
}

// stageCover is the stage's cover at size, with the selected card on it.
func (m *Model) stageCover(size art.Size) []string {
	c := m.selectedCard()
	w2 := int(math.Round(float64(size.Width) * 0.62))
	h2 := int(math.Round(float64(size.Height) * 0.62))
	if c == nil || h2 < 4 || size.Height-h2 < 3 {
		return m.coverLines(size, false)
	}
	m.cardShown = true
	base := m.coverLines(size, true)
	over := m.cardLines(c, art.Size{Width: w2, Height: h2})
	x0, y0 := size.Width-1-w2, size.Height-1-h2 // the card; one cell of margin left, above and right
	out := make([]string, len(base))
	for y, l := range base {
		var mid string
		switch {
		case y < y0-1:
			out[y] = l
			continue
		case y == y0-1:
			mid = strings.Repeat(" ", w2+2)
		case y < y0+h2:
			mid = " " + over[y-y0] + " "
		default:
			mid = " " + fit(c.caption(), w2) + " "
		}
		out[y] = ansi.Cut(l, 0, x0-1) + "\x1b[m" + mid
	}
	return out
}

// dimmed is a darker copy of a cover, for behind a card.
func dimmed(img image.Image) *image.RGBA {
	b := img.Bounds()
	out := image.NewRGBA(image.Rect(0, 0, b.Dx(), b.Dy()))
	for y := range b.Dy() {
		for x := range b.Dx() {
			r, g, bl, _ := img.At(b.Min.X+x, b.Min.Y+y).RGBA()
			o := out.PixOffset(x, y)
			out.Pix[o], out.Pix[o+1], out.Pix[o+2], out.Pix[o+3] = uint8(r>>8*2/5), uint8(g>>8*2/5), uint8(bl>>8*2/5), 255
		}
	}
	return out
}

// idleCard is the stage when nothing plays: the selected card, full size.
func (m *Model) idleCard(colW, coverH int) []stageLine {
	c := m.selectedCard()
	if c == nil || coverH < 6 {
		return nil
	}
	w := min(colW, int(math.Round(float64(coverH)*m.cellAspect)))
	var out []stageLine
	for _, l := range m.cardLines(c, art.Size{Width: w, Height: coverH}) {
		out = append(out, stageLine{text: l, center: true, width: w})
	}
	out = append(out, stageLine{}, stageLine{text: sBold.Render(c.name), center: true})
	if c.sub != "" {
		out = append(out, stageLine{text: sDim.Render(c.sub), center: true})
	}
	return out
}
