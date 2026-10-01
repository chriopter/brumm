package daemon

import (
	"errors"
	"net/url"
	"os"
	"os/exec"
	"strings"
	"time"

	"github.com/chriopter/brumm/internal/update"
)

// Feedback is an issue on GitHub, written in the player: the browser opens
// on it, filled in, or Omarchy's coding agent looks into it first.

const (
	issuesRepo = "chriopter/brumm"
	issuesURL  = "https://github.com/" + issuesRepo + "/issues/new"
)

// feedbackTitle names the issue: this brumm, now.
func feedbackTitle(version string, at time.Time) string {
	return "brumm " + version + " – " + at.Format("2006-01-02 15:04")
}

// feedbackBody is what the user wrote, and under it what runs here.
func feedbackBody(text, version, face string) string {
	if face == "" {
		face = "?"
	}
	return strings.TrimSpace(text) + "\n\n---\nbrumm " + version + " · " + face + " · " + system()
}

// feedbackURL is the new-issue page with title and body filled in.
func feedbackURL(title, body string) string {
	return issuesURL + "?" + url.Values{"title": {title}, "body": {body}}.Encode()
}

// sendFeedback opens the browser on the new issue, filled in, to be sent
// there with one click. With agent set, and an Omarchy agent chosen, the
// agent gets it instead: it looks into the logs and files the issue.
// It answers how it went: "agent" or "browser".
func sendFeedback(text, version, face string, agent bool) (how string, err error) {
	if strings.TrimSpace(text) == "" {
		return "", errors.New("feedback: nothing written")
	}
	if agent && update.HasAgent() {
		cmd := exec.Command("omarchy-agent", "--prompt", agentPrompt(text, version, face))
		if err := cmd.Start(); err == nil {
			go func() { _ = cmd.Wait() }()
			return "agent", nil
		}
	}
	return "browser", openURL(feedbackURL(feedbackTitle(version, time.Now()), feedbackBody(text, version, face)))
}

// agentPrompt asks the agent to look into what the user reports and file it.
func agentPrompt(text, version, face string) string {
	return `A user of brumm (an Apple Music player for Omarchy, https://github.com/` + issuesRepo + `) reports:

  ` + strings.TrimSpace(text) + `

Running: brumm ` + version + ` · ` + face + ` · ` + system() + `

Look into it: is it a bug or a feature request? For a bug, check the logs
(journalctl --user -u brumm -n 200, and ~/.cache/brumm/*.log) for what went
wrong. Then write a short, clear GitHub issue titled "` + feedbackTitle(version, time.Now()) + `: <summary>"
with the report, what you found and the versions above; leave out anything
private (tokens, account names, paths in the home directory beyond ~/.cache/brumm).
Show it to the user, and once they agree create it in ` + issuesRepo + `
(gh issue create -R ` + issuesRepo + `, or open ` + issuesURL + ` in the
browser filled in if gh is not signed in) and open it in the browser.`
}

// lastLine is the last line of out that says something.
func lastLine(out []byte) string {
	lines := strings.Split(strings.TrimSpace(string(out)), "\n")
	return strings.TrimSpace(lines[len(lines)-1])
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
