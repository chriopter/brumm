package tui

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"time"

	tea "charm.land/bubbletea/v2"
	"golang.org/x/sys/unix"

	"github.com/chriopter/brumm/internal/config"
	"github.com/chriopter/brumm/internal/ipc"
)

// Run connects to the daemon — starting it if needed — and runs the TUI.
// Quitting the TUI leaves the music playing.
func Run() error {
	client, err := connect()
	if err != nil {
		return err
	}
	defer client.Close()
	r, err := client.Do(ipc.Request{Cmd: ipc.CmdSubscribe, Bands: bands})
	if err != nil {
		return err
	}
	final, err := tea.NewProgram(newModel(client, *r.State)).Run()
	if err != nil {
		return err
	}
	if m, ok := final.(*Model); ok && m.reexec {
		// The binary changed under us (an update): continue in the new one.
		self, err := os.Executable()
		if err != nil {
			return err
		}
		return syscall.Exec(self, os.Args, os.Environ())
	}
	return nil
}

var started = time.Now()

// binaryUpdated reports whether the executable was replaced since start.
func binaryUpdated() bool {
	self, err := os.Executable()
	if err != nil {
		return false
	}
	fi, err := os.Stat(self)
	return err == nil && fi.ModTime().After(started)
}

func splitPath() string { return filepath.Join(config.CacheDir(), "split") }

// loadSplit is the list's remembered share of the width, 2/5 by default.
func loadSplit() float64 {
	b, err := os.ReadFile(splitPath())
	if err != nil {
		return 0.4
	}
	f, err := strconv.ParseFloat(strings.TrimSpace(string(b)), 64)
	if err != nil || f < 0.2 || f > 0.75 {
		return 0.4
	}
	return f
}

func saveSplit(f float64) {
	_ = os.MkdirAll(config.CacheDir(), 0o755)
	_ = os.WriteFile(splitPath(), []byte(strconv.FormatFloat(f, 'f', 3, 64)), 0o644)
}

func connect() (*ipc.Client, error) {
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
	logPath := filepath.Join(config.CacheDir(), "daemon.log")
	if err := os.MkdirAll(config.CacheDir(), 0o755); err != nil {
		return err
	}
	logFile, err := os.OpenFile(logPath, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		return err
	}
	defer logFile.Close()
	cmd := exec.Command(self, "daemon")
	cmd.Stdout, cmd.Stderr = logFile, logFile
	cmd.SysProcAttr = &syscall.SysProcAttr{Setsid: true}
	if err := cmd.Start(); err != nil {
		return err
	}
	return cmd.Process.Release()
}

// cellAspect is a terminal cell's height divided by its width in pixels,
// so the cover can be drawn square. Terminals that do not report pixel
// sizes get the common 2:1.
func cellAspect() float64 {
	ws, err := unix.IoctlGetWinsize(int(os.Stdout.Fd()), unix.TIOCGWINSZ)
	if err != nil || ws.Col == 0 || ws.Row == 0 || ws.Xpixel == 0 || ws.Ypixel == 0 {
		return 2
	}
	a := (float64(ws.Ypixel) / float64(ws.Row)) / (float64(ws.Xpixel) / float64(ws.Col))
	if a < 1.2 || a > 3.5 {
		return 2
	}
	return a
}
