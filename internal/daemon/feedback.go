package daemon

import (
	"net/url"
	"os"
	"os/exec"
	"strings"
)

// Feedback is a new issue on GitHub, opened in the browser with what runs
// here already written down; the user adds what happened and sends it.

const issuesURL = "https://github.com/chriopter/brumm/issues/new"

// feedbackURL is the new-issue page, filled in for this brumm and the face
// (tui, gui) it was asked from.
func feedbackURL(version, face string) string {
	if face == "" {
		face = "?"
	}
	body := "**What happened?**\n\n\n**What did you expect?**\n\n\n---\n" +
		"brumm " + version + " · " + face + " · " + system()
	return issuesURL + "?" + url.Values{"body": {body}}.Encode()
}

// system names what brumm runs on: the distribution and its version, the
// desktop.
func system() string {
	name := "Linux"
	if b, err := os.ReadFile("/etc/os-release"); err == nil {
		for _, l := range strings.Split(string(b), "\n") {
			if v, ok := strings.CutPrefix(l, "PRETTY_NAME="); ok {
				name = strings.Trim(v, `"`)
			}
		}
	}
	if b, err := os.ReadFile("/usr/share/omarchy/version"); err == nil {
		name += " " + strings.TrimSpace(string(b))
	}
	if d := os.Getenv("XDG_CURRENT_DESKTOP"); d != "" {
		name += " · " + d
	}
	return name
}

// openURL opens the browser on u.
func openURL(u string) error { return exec.Command("xdg-open", u).Start() }
