package launch

import (
	"fmt"
	"os"
	"os/exec"
	"strconv"
	"strings"
)

// The preview shares its fixture socket across both faces and stays on
// the user's chosen testing workspace, including switches with g.
func openPrototype(ui, self string) error {
	args := []string{"env"}
	for _, key := range []string{"XDG_RUNTIME_DIR", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "WAYLAND_DISPLAY", "BRUMM_DESKTOP_RUNTIME", "BRUMM_PROTOTYPE", "BRUMM_PREVIEW", "PULSE_SERVER", "PIPEWIRE_RUNTIME_DIR"} {
		args = append(args, key+"="+os.Getenv(key))
	}
	if ui == GUI {
		path, err := GUIPath()
		if err != nil {
			return err
		}
		args = append(args, path)
	} else {
		terminal, err := exec.LookPath("foot")
		if err != nil {
			return fmt.Errorf("prototype terminal: %w", err)
		}
		args = append(args, terminal, "--app-id=brumm-prototype", "--title=brumm prototype · TUI", self, "--tui")
	}
	if os.Getenv("HYPRLAND_INSTANCE_SIGNATURE") == "" {
		return detach(exec.Command(args[0], args[1:]...), "prototype.log")
	}
	quoted := make([]string, len(args))
	for i, arg := range args {
		quoted[i] = "'" + strings.ReplaceAll(arg, "'", "'\\''") + "'"
	}
	windows, err := previewWindows()
	if err != nil {
		return err
	}
	source := previewSource(windows, ui)
	workspace := "4 silent"
	if source != nil {
		workspace = "special:brumm-handoff silent"
	}
	expr := "hl.dsp.exec_cmd(" + strconv.Quote("exec "+strings.Join(quoted, " ")) + ", { workspace = " + strconv.Quote(workspace) + ", no_initial_focus = true })"
	if _, err := previewHypr("dispatch", expr); err != nil {
		return err
	}
	next, err := awaitPreview(windows, ui)
	if err != nil {
		return err
	}
	if source != nil {
		return handoffPreview(*source, next)
	}
	return nil
}
