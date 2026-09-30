package update

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The keys go to brumm with a file and one line, and back with neither.
func TestMusicKeys(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("PATH", "") // no hyprctl
	if err := SetMusicKeys(true); err == nil || CanMusicKeys() {
		t.Fatal("set keys without Omarchy's bindings")
	}
	mine := "-- my keys\no.bind(\"SUPER + X\", nil, \"x\")\n"
	os.MkdirAll(filepath.Join(dir, "hypr"), 0o755)
	os.WriteFile(bindingsFile(), []byte(mine), 0o644)
	for range 2 { // twice: no second line
		if err := SetMusicKeys(true); err != nil {
			t.Fatal(err)
		}
	}
	b, _ := os.ReadFile(bindingsFile())
	if !MusicKeysOn() || strings.Count(string(b), keysLine) != 1 || !strings.HasPrefix(string(b), mine) {
		t.Fatalf("on: %q", b)
	}
	if err := SetMusicKeys(false); err != nil {
		t.Fatal(err)
	}
	b, _ = os.ReadFile(bindingsFile())
	if MusicKeysOn() || string(b) != mine {
		t.Fatalf("off: %q", b)
	}
	if _, err := os.Stat(keysFile()); err == nil {
		t.Fatal("brumm.lua left behind")
	}
}
