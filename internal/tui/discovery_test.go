package tui

import (
	"fmt"
	"testing"

	"github.com/chriopter/brumm/internal/apple"
)

func TestDiscoveryLayoutAndTargets(t *testing.T) {
	for _, size := range [][2]int{{80, 24}, {120, 35}, {200, 55}} {
		m := playingModel(size[0], size[1])
		m.section = secExplore
		v := m.cur()
		v.loaded = true
		v.rows = []row{{head: "Your music"}}
		for i := 0; i < 30; i++ {
			v.rows = append(v.rows, row{item: &apple.Item{Kind: apple.KindAlbum, ID: fmt.Sprint(i), Name: fmt.Sprintf("Album %d", i)}})
			if i == 8 {
				v.rows = append(v.rows, row{head: "Recommendations"})
			}
		}
		v.sel = 1
		widths(t, m)
		if m.geo.divider != (rect{}) {
			t.Fatal("discovery still has a split stage")
		}
		if m.geo.play == (rect{}) || m.geo.queue == (rect{}) {
			t.Fatal("player controls missing")
		}
		if len(m.geo.cards) < 2 {
			t.Fatal("no card targets")
		}
		for _, hit := range m.geo.cards {
			if hit.index >= len(v.rows) || !v.rows[hit.index].selectable() {
				t.Fatal("card target is not a resource")
			}
			if hit.r.y1 > m.geo.play.y0 {
				t.Fatal("card overlaps player")
			}
		}
		v.sel = len(v.rows) - 1
		widths(t, m)
		visible := false
		for _, hit := range m.geo.cards {
			if hit.index == v.sel {
				visible = true
			}
		}
		if !visible {
			t.Fatal("selected card did not scroll into view")
		}
	}
}

func TestDiscoveryKeepsAlbumTracksAsList(t *testing.T) {
	m := playingModel(120, 35)
	v := m.cur()
	v.item = &apple.Item{Kind: apple.KindAlbum}
	v.rows = []row{{track: &apple.Track{ID: "1", Title: "Song"}}}
	if m.discovering() {
		t.Fatal("album tracks became discovery cards")
	}
}

func TestHomeKeepsQuickStartList(t *testing.T) {
	m := playingModel(120, 35)
	m.cur().rows = []row{{head: "Recently played"}, {item: &apple.Item{Kind: apple.KindAlbum, Name: "Album"}}}
	if m.discovering() {
		t.Fatal("Home became the Explore grid")
	}
	widths(t, m)
	if m.geo.divider == (rect{}) {
		t.Fatal("Home lost its usual stage")
	}
}

func TestExploreGroupsKeepNavigationAndSelection(t *testing.T) {
	m := playingModel(120, 35)
	m.section = secExplore
	v := m.cur()
	link := apple.Item{Kind: apple.KindShelf, ID: "genres", Name: "Genres", Route: "/v1/catalog/de/genres"}
	load := &exploreLoad{page: apple.ExplorePage{Title: "Explore", Items: []apple.Item{link}}}
	v.explore = load
	v.rows = []row{{item: &link}}
	v.loaded = true
	msg := exploreGroupMsg{v: v, load: load, group: 2, shelves: []apple.Shelf{{Title: "Videos", Items: []apple.Item{{Kind: apple.KindVideo, ID: "video", Name: "Video"}}}}}
	mergeExploreGroup(msg)
	if v.rows[v.sel].item == nil || v.rows[v.sel].item.ID != "genres" {
		t.Fatal("late artwork changed selection")
	}
	// Navigating away does not get undone by a remaining group.
	m.section = secHome
	m.Update(exploreGroupMsg{v: v, load: load, group: 0, shelves: []apple.Shelf{{Title: "Recent", Items: []apple.Item{{Kind: apple.KindAlbum, ID: "album", Name: "Album"}}}}})
	if m.section != secHome {
		t.Fatal("late group changed section")
	}
	if v.rows[v.sel].item == nil || v.rows[v.sel].item.ID != "genres" {
		t.Fatal("second group changed selection")
	}
	before := len(v.rows)
	m.Update(exploreGroupMsg{v: v, load: &exploreLoad{}, group: 1, shelves: []apple.Shelf{{Title: "Stale"}}})
	if len(v.rows) != before {
		t.Fatal("stale group replaced newer load")
	}
}
