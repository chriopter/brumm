// Package gui opens brumm as a window: brumm-gui, a Qt Quick program
// installed next to brumm (gui/ in the source), talking to the same
// daemon as the terminal player.
package gui

import (
	"os"
	"syscall"

	"github.com/chriopter/brumm/internal/launch"
)

// Run starts the daemon if needed and becomes the window. The window
// itself brings an open one to the front instead of opening another.
func Run() error {
	p, err := launch.GUIPath()
	if err != nil {
		return err
	}
	c, err := launch.Connect()
	if err != nil {
		return err
	}
	c.Close()
	return syscall.Exec(p, []string{launch.GUIName}, os.Environ())
}
