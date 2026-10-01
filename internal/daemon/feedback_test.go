package daemon

import (
	"net/url"
	"strings"
	"testing"
	"time"
)

// The issue is named for the version and the time, and carries what was
// written above what runs here.
func TestFeedback(t *testing.T) {
	title := feedbackTitle("v1.2.3", time.Date(2026, 10, 1, 9, 30, 0, 0, time.UTC))
	if title != "brumm v1.2.3 – 2026-10-01 09:30" {
		t.Fatalf("title %q", title)
	}
	body := feedbackBody("  the cover is blank\n", "v1.2.3", "gui")
	if !strings.HasPrefix(body, "the cover is blank\n\n---\nbrumm v1.2.3 · gui · ") {
		t.Fatalf("body %q", body)
	}
	u, err := url.Parse(feedbackURL(title, body))
	if err != nil || u.Host != "github.com" || u.Path != "/chriopter/brumm/issues/new" ||
		u.Query().Get("title") != title || u.Query().Get("body") != body {
		t.Fatalf("url %v %v", u, err)
	}
	if p := agentPrompt("no sound", "v1.2.3", "tui"); !strings.Contains(p, "no sound") || !strings.Contains(p, "journalctl --user -u brumm") {
		t.Fatalf("prompt %q", p)
	}
	if _, err := sendFeedback("  ", "v1", "tui", false); err == nil {
		t.Fatal("sent nothing")
	}
}
