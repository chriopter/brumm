package tui

import (
	"os"
	"testing"
)

// Tests never read or write the user's options, split or covers.
func TestMain(m *testing.M) {
	dir, err := os.MkdirTemp("", "brumm-test-")
	if err != nil {
		panic(err)
	}
	os.Setenv("XDG_CONFIG_HOME", dir)
	os.Setenv("XDG_CACHE_HOME", dir)
	code := m.Run()
	os.RemoveAll(dir)
	os.Exit(code)
}
