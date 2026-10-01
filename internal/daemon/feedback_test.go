package daemon

import (
	"net/url"
	"strings"
	"testing"
)

// The issue comes filled in with the version and the face.
func TestFeedbackURL(t *testing.T) {
	u, err := url.Parse(feedbackURL("v1.2.3", "gui"))
	if err != nil || u.Host != "github.com" || u.Path != "/chriopter/brumm/issues/new" {
		t.Fatalf("url %v %v", u, err)
	}
	if body := u.Query().Get("body"); !strings.Contains(body, "brumm v1.2.3 · gui") || !strings.Contains(body, "What happened?") {
		t.Fatalf("body %q", body)
	}
}
