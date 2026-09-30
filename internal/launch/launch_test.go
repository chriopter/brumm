package launch

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/chriopter/brumm/internal/config"
)

// The window is found on the path, and missing it is said before anything
// starts.
func TestGUIPath(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("PATH", dir)
	if _, err := GUIPath(); err == nil {
		t.Fatal("found a window that is not there")
	}
	if err := Open(GUI); err == nil {
		t.Fatal("opened a window that is not there")
	}
	p := filepath.Join(dir, GUIName)
	if err := os.WriteFile(p, []byte("#!/bin/sh\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	if got, err := GUIPath(); err != nil || got != p {
		t.Fatalf("GUIPath = %q, %v", got, err)
	}
}

// Plain brumm opens what the options say, the terminal player by default.
func TestDefault(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	if Default() != TUI {
		t.Fatal("default is not the terminal player")
	}
	o := config.LoadOptions()
	o.Start = GUI
	if err := o.Save(); err != nil {
		t.Fatal(err)
	}
	if Default() != GUI {
		t.Fatal("gui not picked up")
	}
}
