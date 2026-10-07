package tui

import (
	"encoding/json"
	"os"
	"regexp"
	"strconv"
	"testing"
)

// Section numbers are exchanged in CmdPlace when g hands off a view.
// Their meaning must stay identical across the separately built players.
func TestSectionOrderMatchesGUI(t *testing.T) {
	data, err := os.ReadFile("../../gui/qml/Store.qml")
	if err != nil {
		t.Fatal(err)
	}
	names := regexp.MustCompile(`sectionNames:\s*(\[[^\n]+\])`).FindSubmatch(data)
	if len(names) != 2 {
		t.Fatal("missing GUI section names")
	}
	var gui []string
	if err = json.Unmarshal(names[1], &gui); err != nil {
		t.Fatal(err)
	}
	if len(gui) != len(sectionNames) {
		t.Fatal("section count differs")
	}
	for i, name := range sectionNames {
		if gui[i] != name {
			t.Fatalf("section %d: TUI %s, GUI %s", i, name, gui[i])
		}
	}
	for name, index := range map[string]section{"secHome": secHome, "secExplore": secExplore, "secPlaylists": secPlaylists, "secAlbums": secAlbums, "secArtists": secArtists, "secSongs": secSongs, "secRadio": secRadio, "secQueue": secQueue, "secSearch": secSearch} {
		match := regexp.MustCompile(name + `:\s*(\d+)`).FindSubmatch(data)
		if len(match) != 2 || string(match[1]) != strconv.Itoa(int(index)) {
			t.Fatalf("%s number differs", name)
		}
	}
}
