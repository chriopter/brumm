package engine

import (
	"os"
	"regexp"
	"strings"
	"testing"
)

// TestPageDefinesCalls makes sure every brumm.<name>(…) the engine calls
// exists on the playback page — a missing one fails silently in Chrome.
func TestPageDefinesCalls(t *testing.T) {
	src, err := os.ReadFile("engine.go")
	if err != nil {
		t.Fatal(err)
	}
	called := map[string]bool{}
	for _, m := range regexp.MustCompile(`brumm\.([a-zA-Z]+)\(`).FindAllStringSubmatch(string(src), -1) {
		called[m[1]] = true
	}
	if len(called) < 10 {
		t.Fatalf("found only %d calls; the pattern is off", len(called))
	}
	for name := range called {
		def := regexp.MustCompile(`(?m)^\s+(async\s+)?` + name + `\(`)
		if !def.MatchString(page) {
			t.Errorf("player.html does not define brumm.%s", name)
		}
	}
	if !strings.Contains(page, "__BRUMM_CFG__") {
		t.Error("player.html lost its config placeholder")
	}
}

// The queue's songs carry every field the player reads from them: without
// their covers, nothing comes up next.
func TestQueueFields(t *testing.T) {
	i := strings.Index(page, "  queue(n) {")
	body := page[i : i+strings.Index(page[i:], "\n  },")]
	for _, f := range []string{"id", "title", "artist", "album", "duration", "artwork", "number"} {
		if !regexp.MustCompile(`\b` + f + `:`).MatchString(body) {
			t.Errorf("queue leaves out %s", f)
		}
	}
}
