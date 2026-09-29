package update

import (
	"encoding/json"
	"errors"
	"os"
	"os/exec"
	"strings"
)

// Omarchy runs `omarchy args…` and returns what it printed; a variable, so
// tests never touch the real bar.
var Omarchy = func(args ...string) ([]byte, error) {
	return exec.Command("omarchy", args...).CombinedOutput()
}

// HasOmarchy says whether the omarchy command is here to manage the bar.
func HasOmarchy() bool {
	_, err := exec.LookPath("omarchy")
	return err == nil
}

// PluginLinked says whether the bar widget's files are where Omarchy looks.
func PluginLinked() bool {
	_, err := os.Lstat(pluginLink())
	return err == nil
}

// SetPlugin turns the bar widget on or off.
func SetPlugin(on bool) error {
	verb := "disable"
	if on {
		verb = "enable"
	}
	out, err := Omarchy("plugin", verb, pluginID)
	if err != nil {
		if msg := lastLine(out); msg != "" {
			return errors.New(strings.TrimPrefix(msg, "omarchy-plugin-"+verb+": "))
		}
		return err
	}
	return nil
}

// PluginEnabled asks the Omarchy shell whether the bar widget is on; ok is
// false when it cannot tell (the shell is not running, or does not know it).
func PluginEnabled() (on, ok bool) {
	out, err := Omarchy("plugin", "list", "--json")
	if err != nil {
		return false, false
	}
	var plugins []struct {
		ID      string `json:"id"`
		Enabled bool   `json:"enabled"`
	}
	if json.Unmarshal(out, &plugins) != nil {
		return false, false
	}
	for _, p := range plugins {
		if p.ID == pluginID {
			return p.Enabled, true
		}
	}
	return false, false // not found: the shell has not seen it yet
}

func lastLine(out []byte) string {
	lines := strings.Split(strings.TrimSpace(string(out)), "\n")
	return strings.TrimSpace(lines[len(lines)-1])
}
