package tui

import (
	"encoding/json"
	"testing"

	"github.com/chriopter/brumm/internal/apple"
	"github.com/chriopter/brumm/internal/ipc"
)

// A place left by one player opens the other right there: the section,
// the album opened in it, the song selected once its rows come; and it
// is taken instead of going to what plays.
func TestPlaceRoundTrip(t *testing.T) {
	a := newModel(nil, ipc.State{Status: ipc.StatusReady})
	a.section = secAlbums
	fill(a.cur(), ipc.Message{Items: []apple.Item{{Kind: apple.KindAlbum, ID: "l.1", Name: "Discovery"}}})
	album := &apple.Item{Kind: apple.KindAlbum, ID: "l.1", Name: "Discovery"}
	v := &view{title: "Discovery", key: album.Key(), item: album}
	a.stacks[secAlbums] = append(a.stacks[secAlbums], v)
	songs := ipc.Message{Tracks: []apple.Track{{ID: "s1", Title: "One More Time"}, {ID: "s2", Title: "Aerodynamic"}}}
	fill(v, songs)
	v.sel = 1
	a.query = "daft punk"
	b, err := json.Marshal(a.here())
	if err != nil {
		t.Fatal(err)
	}

	m := newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.state.Source, m.placing = "lib:playlist:p.1", true
	if m.maybeResume() != nil || m.resumed {
		t.Fatal("resumed before the place answered")
	}
	var p *place
	if err := json.Unmarshal(b, &p); err != nil {
		t.Fatal(err)
	}
	m.gotPlace(placeMsg{p})
	if m.section != secAlbums || len(m.stack()) != 2 || m.cur().key != album.Key() || !m.resumed || m.placing {
		t.Fatalf("opened section %d at %q", m.section, m.cur().key)
	}
	if m.query != "daft punk" || m.stacks[secSearch][0].key != "search:daft punk" {
		t.Fatalf("search %q at %q", m.query, m.stacks[secSearch][0].key)
	}
	m.Update(loadedMsg{v: m.cur(), reply: songs})
	if m.cur().sel != 1 {
		t.Fatalf("selected row %d, want 1", m.cur().sel)
	}

	// With no place left, brumm starts as ever.
	m = newModel(nil, ipc.State{Status: ipc.StatusReady})
	m.placing = true
	m.gotPlace(placeMsg{})
	if m.placing || m.section != secHome {
		t.Fatal("no place: not as ever")
	}
}
