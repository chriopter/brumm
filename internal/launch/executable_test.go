package launch

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestReplacementIgnoresBuildTimestamp(t *testing.T) {
	dir := t.TempDir()
	running, installed := filepath.Join(dir, "running"), filepath.Join(dir, "installed")
	if err := os.WriteFile(running, []byte("old"), 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.Link(running, installed); err != nil {
		t.Fatal(err)
	}
	if replaced(running, installed) {
		t.Fatal("same inode reported as replaced")
	}
	fresh := filepath.Join(dir, "new")
	if err := os.WriteFile(fresh, []byte("new"), 0755); err != nil {
		t.Fatal(err)
	}
	older := time.Unix(1, 0)
	if err := os.Chtimes(fresh, older, older); err != nil {
		t.Fatal(err)
	}
	if err := os.Rename(fresh, installed); err != nil {
		t.Fatal(err)
	}
	if !replaced(running, installed) {
		t.Fatal("replacement with an older build timestamp missed")
	}
	if err := os.Remove(installed); err != nil {
		t.Fatal(err)
	}
	if replaced(running, installed) {
		t.Fatal("missing executable should not trigger restart")
	}
}
