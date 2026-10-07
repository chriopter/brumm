package tui

import (
	"strings"

	tea "charm.land/bubbletea/v2"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/ipc"
)

type cardHit struct {
	r     rect
	index int
}

func (m *Model) discovering() bool {
	v := m.cur()
	if m.playerView {
		return false
	}
	if len(v.rows) == 0 {
		return false
	}
	if v.item != nil && (v.item.Kind == apple.KindAlbum || v.item.Kind == apple.KindPlaylist) {
		return false
	}
	if v.item != nil && v.item.Kind == apple.KindArtist {
		return true
	}
	if m.section != secExplore {
		return false
	}
	for _, r := range v.rows {
		if r.track != nil {
			return false
		}
	}
	return true
}

// Shelves fill the window; each card retains its original row identity.
func (m *Model) discoveryRows(w, h, x, y int) []string {
	v := m.cur()
	cols := max(2, w/32)
	cw := (w - (cols-1)*3) / cols
	var lines []string
	positions := make(map[int]rect)
	var pending []int
	flush := func() {
		if len(pending) == 0 {
			return
		}
		var titles, subtitles []string
		for col, i := range pending {
			r := v.rows[i]
			title, sub := "", ""
			if r.item != nil {
				title, sub = r.item.Name, r.item.Artist
				if sub == "" {
					sub = "Explore ›"
				}
			}
			if r.track != nil {
				title, sub = r.track.Title, r.track.Artist
			}
			style := sMusic
			mark := "  "
			if i == v.sel {
				style = sHere.Bold(true)
				mark = "› "
			}
			titles = append(titles, fit(style.Render(mark+title), cw))
			subtitles = append(subtitles, fit(sDim.Render("  "+sub), cw))
			positions[i] = rect{x + col*(cw+3), len(lines), x + col*(cw+3) + cw, len(lines) + 2}
		}
		lines = append(lines, strings.Join(titles, "   "), strings.Join(subtitles, "   "), "")
		pending = nil
	}
	for i, r := range v.rows {
		if !r.selectable() {
			flush()
			text := r.head
			if text == "" {
				text = r.note
			}
			if text != "" {
				lines = append(lines, sBold.Render(text), "")
			}
		} else {
			pending = append(pending, i)
			if len(pending) == cols {
				flush()
			}
		}
	}
	flush()
	selected := positions[v.sel]
	v.off = max(0, min(v.off, max(0, len(lines)-h)))
	if selected.y0 < v.off {
		v.off = selected.y0
	}
	if selected.y1 > v.off+h {
		v.off = selected.y1 - h
	}
	v.off = max(0, v.off)
	for i, r := range positions {
		if r.y0 >= v.off && r.y1 <= v.off+h {
			r.y0, r.y1 = y+r.y0-v.off, y+r.y1-v.off
			m.geo.cards = append(m.geo.cards, cardHit{r: r, index: i})
		}
	}
	end := min(len(lines), v.off+h)
	result := lines[min(v.off, end):end]
	for i := range result {
		result[i] = fit(result[i], w)
	}
	return result
}

// Explore's initial links arrive before artwork requests. Each group is an
// independent command, so one slow chart never blocks another or navigation.
type exploreLoad struct {
	page   apple.ExplorePage
	groups [3][]apple.Shelf
}
type exploreGroupMsg struct {
	v       *view
	load    *exploreLoad
	group   int
	shelves []apple.Shelf
}

func (m *Model) loadExploreVisuals(v *view, reply ipc.Message) tea.Cmd {
	if m.client == nil || reply.Explorer == nil {
		return nil
	}
	load := &exploreLoad{page: *reply.Explorer}
	v.explore = load
	client := m.client
	cmds := []tea.Cmd{func() tea.Msg {
		home, err := client.Do(ipc.Request{Cmd: ipc.CmdHome})
		var shelves []apple.Shelf
		if err == nil {
			for _, sh := range home.Shelves {
				if sh.Title == "Your music" || sh.Title == "Browse" {
					continue
				}
				var items []apple.Item
				for _, it := range sh.Items {
					if it.Artwork != "" {
						items = append(items, it)
					}
				}
				if len(items) > 0 {
					sh.Items = items[:min(6, len(items))]
					sh.Tracks = nil
					shelves = append(shelves, sh)
				}
				if len(shelves) == 2 {
					break
				}
			}
		}
		return exploreGroupMsg{v: v, load: load, group: 0, shelves: shelves}
	}}
	base := ""
	for _, it := range load.page.Items {
		if strings.HasSuffix(it.Route, "/genres") {
			base = strings.TrimSuffix(it.Route, "/genres")
			break
		}
	}
	if base != "" {
		for i, types := range []string{"albums,playlists", "music-videos"} {
			cmds = append(cmds, func() tea.Msg {
				data, err := client.Do(ipc.Request{Cmd: ipc.CmdExplore, Query: base + "/charts?types=" + types + "&limit=6"})
				var shelves []apple.Shelf
				if err == nil && data.Explorer != nil {
					for _, sh := range data.Explorer.Shelves {
						sh.Items = sh.Items[:min(6, len(sh.Items))]
						sh.Tracks = sh.Tracks[:min(6, len(sh.Tracks))]
						shelves = append(shelves, sh)
					}
					if len(data.Explorer.Items) > 0 {
						title := "Charts"
						if i == 1 {
							title = "Music videos"
						}
						shelves = append(shelves, apple.Shelf{Title: title, Items: data.Explorer.Items[:min(6, len(data.Explorer.Items))]})
					}
				}
				return exploreGroupMsg{v: v, load: load, group: i + 1, shelves: shelves}
			})
		}
	}
	return tea.Batch(cmds...)
}

func mergeExploreGroup(msg exploreGroupMsg) {
	v := msg.v
	var selected row
	if v.sel >= 0 && v.sel < len(v.rows) {
		selected = v.rows[v.sel]
	}
	off := v.off
	msg.load.groups[msg.group] = msg.shelves
	page := msg.load.page
	page.Items = nil
	page.Info = ""
	page.Shelves = nil
	for _, group := range msg.load.groups {
		page.Shelves = append(page.Shelves, group...)
	}
	page.Shelves = append(page.Shelves, apple.Shelf{Title: "Browse", Items: msg.load.page.Items})
	fill(v, ipc.Message{Explorer: &page})
	for i, r := range v.rows {
		if selected.item != nil && r.item != nil && selected.item.Key() == r.item.Key() || selected.track != nil && r.track != nil && selected.track.ID == r.track.ID {
			v.sel = i
			break
		}
	}
	v.off = off
}
