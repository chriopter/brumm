// Package launch starts brumm's pieces: the daemon behind the player, and
// the player itself as the terminal UI or the window (GUI).
package launch

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"syscall"
	"time"

	"github.com/chriopter/brumm/internal/config"
	"github.com/chriopter/brumm/internal/ipc"
)

// The two faces of the player.
const (
	TUI = "tui"
	GUI = "gui"
)

// Default is the face plain `brumm` opens, as picked in the options.
func Default() string {
	if config.LoadOptions().Start == GUI {
		return GUI
	}
	return TUI
}

// GUIName is the window's program, installed next to brumm.
const GUIName = "brumm-gui"

// GUIPath finds the window: next to this brumm, else on the path.
func GUIPath() (string, error) {
	if self, err := os.Executable(); err == nil {
		p := filepath.Join(filepath.Dir(self), GUIName)
		if fi, err := os.Stat(p); err == nil && !fi.IsDir() && fi.Mode()&0o111 != 0 {
			return p, nil
		}
	}
	if p, err := exec.LookPath(GUIName); err == nil {
		return p, nil
	}
	return "", errors.New("brumm's window (brumm-gui) is not installed: brumm update brings it; from source, bin/setup builds it (needs qt6-declarative)")
}

// Open starts the player as ui in a process of its own, apart from this
// one: the window, or the terminal UI in a new terminal.
func Open(ui string) error {
	if ui == "" {
		ui = Default()
	}
	self, err := os.Executable()
	if err != nil {
		return err
	}
	if ui == GUI {
		if _, err := GUIPath(); err != nil {
			return err // say so here, before this player goes
		}
		return detach(exec.Command(self, "--gui"), "gui.log")
	}
	// Omarchy's launcher styles the terminal, names it as the desktop
	// entry does, and focuses one already open; elsewhere any terminal the
	// desktop knows.
	for _, c := range [][]string{
		{"omarchy-launch-or-focus-tui", self, "--tui"},
		{"xdg-terminal-exec", "-e", self, "--tui"},
		{os.Getenv("TERMINAL"), "-e", self, "--tui"},
	} {
		if c[0] == "" {
			continue
		}
		if _, err := exec.LookPath(c[0]); err == nil {
			return detach(exec.Command(c[0], c[1:]...), "tui.log")
		}
	}
	return errors.New("no terminal to open brumm in")
}

// detach runs cmd in a session of its own, so it outlives this process,
// its output in the cache's log.
func detach(cmd *exec.Cmd, log string) error {
	if err := os.MkdirAll(config.CacheDir(), 0o700); err != nil {
		return err
	}
	f, err := os.OpenFile(filepath.Join(config.CacheDir(), log), os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		return err
	}
	defer f.Close()
	cmd.Stdout, cmd.Stderr = f, f
	cmd.SysProcAttr = &syscall.SysProcAttr{Setsid: true}
	if err := cmd.Start(); err != nil {
		return err
	}
	return cmd.Process.Release()
}

// Connect connects to the daemon, starting it if needed.
func Connect() (*ipc.Client, error) {
	if c, err := ipc.Dial(); err == nil {
		return c, nil
	}
	if err := startDaemon(); err != nil {
		return nil, err
	}
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		if c, err := ipc.Dial(); err == nil {
			return c, nil
		}
		time.Sleep(100 * time.Millisecond)
	}
	return nil, errors.New("the brumm daemon did not start; see `journalctl --user -u brumm`")
}

// startDaemon prefers the systemd user unit, so the daemon is supervised
// and logs to the journal; without it, it forks a detached daemon.
func startDaemon() error {
	if exec.Command("systemctl", "--user", "start", "brumm.service").Run() == nil {
		return nil
	}
	self, err := os.Executable()
	if err != nil {
		return err
	}
	return detach(exec.Command(self, "daemon"), "daemon.log")
}
