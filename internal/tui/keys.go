package tui

import (
	"sort"
	"strings"

	"charm.land/lipgloss/v2"

	"github.com/chriopter/brumm/internal/apple"
)

// Keytips: the keys drawn on the things they act on — tabs, the
// transport, the artist and album, the selected row — when the options
// say so. ? shows the full list either way.

// showTips is whether keytips are drawn now. The key list and the
// fullscreen visualizer carry their own hints; the menus do not hide them,
// so switching them on in the options shows at once.
func (m *Model) showTips() bool {
	return m.opts.AlwaysTips && !m.full && !m.help
}

// tip styles a key badge.
func (m *Model) tip(k string) string { return sKey.Render(k) }

// badges lays key badges out on a line of w cells: each at its column,
// dropped where it would run into the one before or off the edge.
type badge struct {
	col int
	key string
}

func (m *Model) badges(w int, bs []badge) string {
	sort.Slice(bs, func(i, j int) bool { return bs[i].col < bs[j].col })
	var sb strings.Builder
	at := 0
	for _, b := range bs {
		kw := lipgloss.Width(b.key)
		if b.col < at || b.col+kw > w {
			continue
		}
		sb.WriteString(strings.Repeat(" ", b.col-at))
		sb.WriteString(m.tip(b.key))
		at = b.col + kw + 1
		if at > w {
			at = w
		}
		if b.col+kw < w {
			sb.WriteString(" ")
		}
	}
	return sb.String() + strings.Repeat(" ", max(0, w-at))
}

// rowTips are the keys for the selected row, most useful first, for the
// list's bottom border.
func (m *Model) rowTips(v *view) string {
	if v.sel >= len(v.rows) || !v.rows[v.sel].selectable() {
		return ""
	}
	r := v.rows[v.sel]
	var keys [][2]string
	preview := "space"
	if !m.releases {
		preview = "O"
	}
	switch {
	case v.key == "queue:":
		keys = [][2]string{{"enter", "play"}, {"a A", "album artist"}, {"*", "love"}}
	case r.track != nil:
		keys = [][2]string{{"enter", "play"}, {preview, "preview"}, {"z Z", "queue"}, {"a A", "album artist"},
			{"R", "radio"}, {"* d", "rate"}, {"P", "playlist"}, {"y", "link"}}
		if !strings.HasPrefix(r.track.ID, "i.") {
			keys = append(keys[:6], append([][2]string{{"i", "library"}}, keys[6:]...)...)
		}
	default:
		switch r.item.Kind {
		case apple.KindAlbum, apple.KindPlaylist:
			keys = [][2]string{{"enter", "open"}, {"z Z", "queue"}, {"* d", "rate"}, {"P", "playlist"}}
			if r.item.Catalog {
				keys = append(keys, [2]string{"i", "library"})
			}
		case apple.KindArtist:
			keys = [][2]string{{"enter", "open"}, {"R", "radio"}}
		case apple.KindStation:
			keys = [][2]string{{"enter", "play"}, {"* d", "rate"}}
		case apple.KindTerm:
			keys = [][2]string{{"enter", "search"}}
		default:
			keys = [][2]string{{"enter", "open"}}
		}
	}
	parts := make([]string, len(keys))
	for i, k := range keys {
		parts[i] = m.tip(k[0]) + " " + sDim.Render(k[1])
	}
	return strings.Join(parts, sDim.Render("  "))
}
