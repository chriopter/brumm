package tui

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"syscall"
	"time"

	tea "charm.land/bubbletea/v2"

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
	_, err = tea.NewProgram(New(client, *r.State)).Run()
	return err
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
