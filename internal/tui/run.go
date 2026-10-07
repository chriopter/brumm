package tui

import (
	"os"
	"path/filepath"
	"runtime/pprof"
	"strconv"
	"strings"
	"syscall"
	"time"

	tea "charm.land/bubbletea/v2"
	"golang.org/x/sys/unix"

	"github.com/chriopter/brumm/internal/art"
	"github.com/chriopter/brumm/internal/config"
	"github.com/chriopter/brumm/internal/ipc"
	"github.com/chriopter/brumm/internal/launch"
)

// Run connects to the daemon — starting it if needed — and runs the TUI.
// Quitting the TUI leaves the music playing.
func Run() error {
	// BRUMM_CPUPROFILE=file profiles the session until it quits, for go
	// tool pprof: how bin/bench's runs are looked into.
	if p := os.Getenv("BRUMM_CPUPROFILE"); p != "" {
		if f, err := os.Create(p); err == nil {
			_ = pprof.StartCPUProfile(f)
			defer pprof.StopCPUProfile()
		}
	}
	client, err := launch.Connect()
	if err != nil {
		return err
	}
	defer client.Close()
	go art.Prune(filepath.Join(config.CacheDir(), "covers"), 200<<20)
	r, err := client.Do(ipc.Request{Cmd: ipc.CmdSubscribe, Bands: bands, FPS: meterFPS})
	if err != nil {
		return err
	}
	final, err := tea.NewProgram(newModel(client, *r.State), tea.WithFPS(maxDrawFPS)).Run()
	if err != nil {
		return err
	}
	if m, ok := final.(*Model); ok {
		os.Stdout.WriteString(m.kittyCleanup() + m.pointerReset())
	}
	if m, ok := final.(*Model); ok && m.reexec {
		// The binary changed under us (an update): continue in the new one.
		self, err := launch.Executable()
		if err != nil {
			return err
		}
		if m.client != nil {
			m.client.Close()
		}
		return syscall.Exec(self, os.Args, os.Environ())
	}
	return nil
}

// binaryUpdated reports whether the executable was replaced since start.
func binaryUpdated() bool {
	return launch.BinaryUpdated()
}

type binaryCheckMsg struct{}

func checkBinary() tea.Cmd {
	return tea.Tick(time.Second, func(time.Time) tea.Msg { return binaryCheckMsg{} })
}

func splitPath() string { return filepath.Join(config.CacheDir(), "split") }

// loadSplit is the list's remembered share of the width, half by default.
func loadSplit() float64 {
	b, err := os.ReadFile(splitPath())
	if err != nil {
		return 0.5
	}
	f, err := strconv.ParseFloat(strings.TrimSpace(string(b)), 64)
	if err != nil || f < 0.1 || f > 0.95 {
		return 0.5
	}
	return f
}

func saveSplit(f float64) {
	_ = os.MkdirAll(config.CacheDir(), 0o700)
	_ = os.WriteFile(splitPath(), []byte(strconv.FormatFloat(f, 'f', 3, 64)), 0o600)
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
