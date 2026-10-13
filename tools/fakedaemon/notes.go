package main

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// BRUMM_FAKE_UPDATE optionally previews public release notes. This helper
// has no dependency on the production updater or the user's account.
func notesSince(current string) (string, []string, error) {
	client := &http.Client{Timeout: 10 * time.Second}
	response, err := client.Get("https://api.github.com/repos/chriopter/brumm/releases?per_page=50")
	if err != nil {
		return "", nil, err
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		return "", nil, fmt.Errorf("github: %s", response.Status)
	}
	var releases []struct {
		Tag        string `json:"tag_name"`
		Body       string `json:"body"`
		Draft      bool   `json:"draft"`
		Prerelease bool   `json:"prerelease"`
	}
	if err := json.NewDecoder(io.LimitReader(response.Body, 4<<20)).Decode(&releases); err != nil {
		return "", nil, err
	}
	latest := ""
	sections := map[string][]string{}
	for _, release := range releases {
		if release.Draft || release.Prerelease {
			continue
		}
		if newer(release.Tag, latest) {
			latest = release.Tag
		}
		if !newer(release.Tag, current) {
			continue
		}
		section := ""
		for _, line := range strings.Split(release.Body, "\n") {
			line = strings.TrimSpace(line)
			if strings.HasPrefix(line, "## ") {
				section = strings.TrimPrefix(line, "## ")
			} else if strings.HasPrefix(line, "- ") {
				sections[section] = append(sections[section], strings.TrimPrefix(line, "- "))
			}
		}
	}
	if latest == "" {
		return "", nil, fmt.Errorf("github: no stable release")
	}
	var notes []string
	for _, section := range []string{"New", "Changed", "Faster", "Fixed"} {
		if points := sections[section]; len(points) > 0 {
			notes = append(notes, "## "+section)
			notes = append(notes, points...)
		}
	}
	return latest, notes, nil
}

func newer(a, b string) bool {
	parts := func(version string) [3]int {
		var out [3]int
		for i, part := range strings.Split(strings.TrimPrefix(version, "v"), ".") {
			if i == len(out) {
				break
			}
			out[i], _ = strconv.Atoi(part)
		}
		return out
	}
	x, y := parts(a), parts(b)
	for i := range x {
		if x[i] != y[i] {
			return x[i] > y[i]
		}
	}
	return false
}
