package launch

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"time"
)

type previewWindow struct {
	Address   string   `json:"address"`
	Class     string   `json:"class"`
	Title     string   `json:"title"`
	Mapped    bool     `json:"mapped"`
	Floating  bool     `json:"floating"`
	Grouped   []string `json:"grouped"`
	Workspace struct {
		ID int `json:"id"`
	} `json:"workspace"`
}

func previewHypr(args ...string) ([]byte, error) {
	cmd := exec.Command("hyprctl", args...)
	cmd.Env = os.Environ()
	for i, entry := range cmd.Env {
		if strings.HasPrefix(entry, "XDG_RUNTIME_DIR=") {
			cmd.Env[i] = "XDG_RUNTIME_DIR=" + os.Getenv("BRUMM_DESKTOP_RUNTIME")
		}
	}
	out, err := cmd.CombinedOutput()
	if err != nil {
		return out, fmt.Errorf("hyprctl: %w: %s", err, out)
	}
	return out, nil
}
func previewWindows() ([]previewWindow, error) {
	out, err := previewHypr("clients", "-j")
	if err != nil {
		return nil, err
	}
	var ws []previewWindow
	err = json.Unmarshal(out, &ws)
	return ws, err
}
func isPreviewFace(w previewWindow, ui string) bool {
	if ui == GUI {
		return w.Class == "brumm" && (w.Title == "brumm · API prototype · sample data" || w.Title == "brumm · API preview · live account")
	}
	return w.Class == "brumm-prototype"
}
func previewSource(ws []previewWindow, ui string) *previewWindow {
	for _, w := range ws {
		if w.Mapped && w.Workspace.ID == 4 && !isPreviewFace(w, ui) && (isPreviewFace(w, GUI) || isPreviewFace(w, TUI)) && !w.Floating && len(w.Grouped) == 0 {
			copy := w
			return &copy
		}
	}
	return nil
}
func awaitPreview(ws []previewWindow, ui string) (previewWindow, error) {
	seen := map[string]bool{}
	for _, w := range ws {
		seen[w.Address] = true
	}
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		current, err := previewWindows()
		if err != nil {
			return previewWindow{}, err
		}
		for _, w := range current {
			if w.Mapped && !seen[w.Address] && isPreviewFace(w, ui) {
				return w, nil
			}
		}
		time.Sleep(50 * time.Millisecond)
	}
	return previewWindow{}, fmt.Errorf("the prototype window did not appear; keeping the current player")
}
func handoffPreview(old, next previewWindow) error {
	// Stage off-screen, then insert at the old focused leaf and remove the
	// old window in the same compositor batch. The old process exits normally
	// after launch returns, on the hidden workspace rather than in the layout.
	var active previewWindow
	if out, err := previewHypr("activewindow", "-j"); err == nil {
		_ = json.Unmarshal(out, &active)
	}
	selector := func(w previewWindow) string { return strconv.Quote("address:" + w.Address) }
	focusAfter := next
	if active.Address != "" && active.Address != old.Address {
		focusAfter = active
	}
	batch := []string{
		"dispatch hl.dsp.focus({ window = " + selector(old) + " })",
		"dispatch hl.dsp.window.move({ window = " + selector(next) + ", workspace = \"4\", follow = false })",
		"dispatch hl.dsp.window.move({ window = " + selector(old) + ", workspace = \"special:brumm-handoff\", follow = false })",
		"dispatch hl.dsp.focus({ window = " + selector(focusAfter) + " })",
	}
	_, err := previewHypr("--batch", strings.Join(batch, "; "))
	return err
}
