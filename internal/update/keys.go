package update

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// Omarchy opens its music apps with SUPER+SHIFT+M (Spotify) and
// SUPER+SHIFT+ALT+M (a terminal player). brumm can take both over: its
// window and its terminal player. It keeps the change to a file of its own,
// loaded by one marked line in the user's bindings; switching it off takes
// both away, and Omarchy's keys are back.

func hyprDir() string {
	if d := os.Getenv("XDG_CONFIG_HOME"); d != "" {
		return filepath.Join(d, "hypr")
	}
	return filepath.Join(home(), ".config", "hypr")
}

func keysFile() string     { return filepath.Join(hyprDir(), "brumm.lua") }
func bindingsFile() string { return filepath.Join(hyprDir(), "bindings.lua") }

// keysLine loads brumm's keys from the user's bindings.
const keysLine = `pcall(require, "hypr.brumm") -- brumm's music keys: brumm keys on|off`

const keysLua = `-- Written by brumm (options → Music Keys, or brumm keys on|off); brumm
-- removes it again when switched off, and Omarchy's music keys are back.
hl.unbind("SUPER + SHIFT + M")
hl.unbind("SUPER + SHIFT + ALT + M")
o.bind("SUPER + SHIFT + M", "brumm", "brumm open gui")
o.bind("SUPER + SHIFT + ALT + M", "brumm in the terminal", "brumm open tui")
`

// CanMusicKeys says whether this Hyprland is set up the Omarchy way, with
// the user's bindings in bindings.lua.
func CanMusicKeys() bool {
	_, err := os.Stat(bindingsFile())
	return err == nil
}

// MusicKeysOn says whether brumm has the music keys now.
func MusicKeysOn() bool {
	b, err := os.ReadFile(bindingsFile())
	if err != nil {
		return false
	}
	_, err = os.Stat(keysFile())
	return err == nil && strings.Contains(string(b), keysLine)
}

// SetMusicKeys gives the music keys to brumm, or back to Omarchy, and has
// Hyprland read its config again.
func SetMusicKeys(on bool) error {
	if !CanMusicKeys() {
		return errors.New("music keys: no ~/.config/hypr/bindings.lua (Omarchy's Hyprland config)")
	}
	b, err := os.ReadFile(bindingsFile())
	if err != nil {
		return err
	}
	text := string(b)
	has := strings.Contains(text, keysLine)
	switch {
	case on:
		if err := os.WriteFile(keysFile(), []byte(keysLua), 0o644); err != nil {
			return err
		}
		if !has {
			if !strings.HasSuffix(text, "\n") && text != "" {
				text += "\n"
			}
			text += "\n" + keysLine + "\n"
		}
	default:
		_ = os.Remove(keysFile())
		if has {
			text = strings.Replace(text, "\n"+keysLine+"\n", "", 1) // as it was added
			text = strings.Replace(text, keysLine+"\n", "", 1)
		}
	}
	if text != string(b) {
		if err := os.WriteFile(bindingsFile(), []byte(text), 0o644); err != nil {
			return err
		}
	}
	if _, err := exec.LookPath("hyprctl"); err == nil {
		_ = exec.Command("hyprctl", "reload").Run()
	}
	return nil
}
