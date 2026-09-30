package tui

import (
	"errors"
	"os"
	"testing"

	"github.com/chriopter/brumm/internal/update"
)

// Tests never read or write the user's options, split or covers, and
// never switch the real bar widget.
func TestMain(m *testing.M) {
	hasOmarchy = func() bool { return false }
	canMusicKeys = func() bool { return false }
	update.Omarchy = func(...string) ([]byte, error) { return nil, errors.New("no omarchy in tests") }
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
