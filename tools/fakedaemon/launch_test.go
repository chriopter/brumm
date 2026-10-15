package main

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestPreviewSwitchDoesNotWaitForNewFrontendToClose(t *testing.T) {
	dir := t.TempDir()
	binary := filepath.Join(dir, "brumm")
	release := filepath.Join(dir, "release")
	started := filepath.Join(dir, "started")
	done := filepath.Join(dir, "done")
	t.Setenv("BRUMM_TEST_RELEASE", release)
	t.Setenv("BRUMM_TEST_STARTED", started)
	t.Setenv("BRUMM_TEST_DONE", done)
	script := `#!/bin/sh
printf '%s\n' "$@" > "$BRUMM_TEST_STARTED"
while [ ! -f "$BRUMM_TEST_RELEASE" ]; do /bin/sleep 0.01; done
: > "$BRUMM_TEST_DONE"
`
	if err := os.WriteFile(binary, []byte(script), 0700); err != nil {
		t.Fatal(err)
	}
	defer os.WriteFile(release, nil, 0600)
	result := make(chan error, 1)
	go func() { result <- startPreview(binary, "gui") }()
	select {
	case err := <-result:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(time.Second):
		t.Fatal("switch waited for the GUI to close")
	}
	waitFile := func(path string) {
		t.Helper()
		deadline := time.Now().Add(time.Second)
		for time.Now().Before(deadline) {
			if _, err := os.Stat(path); err == nil {
				return
			}
			time.Sleep(10 * time.Millisecond)
		}
		t.Fatalf("%s did not appear", filepath.Base(path))
	}
	waitFile(started)
	args, _ := os.ReadFile(started)
	if string(args) != "open\ngui\n" {
		t.Fatalf("wrong launch: %q", args)
	}
	if err := os.WriteFile(release, nil, 0600); err != nil {
		t.Fatal(err)
	}
	waitFile(done)
}
