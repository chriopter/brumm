// brumm is an Apple Music player for Omarchy.
//
//	brumm          open the player (starts the background daemon if needed)
//	brumm daemon   run the background daemon (usually via systemd)
//	brumm login    sign in to Apple Music in the browser, then open the player
//	brumm update   install the newest release now and offer to restart into it
//	brumm setup D  install from an unpacked release D (used by install.sh)
package main

import (
	"context"
	"fmt"
	"os"
	"strings"

	"golang.org/x/sys/unix"

	"github.com/chriopter/brumm/internal/daemon"
	"github.com/chriopter/brumm/internal/ipc"
	"github.com/chriopter/brumm/internal/login"
	"github.com/chriopter/brumm/internal/tui"
	"github.com/chriopter/brumm/internal/update"
)

var version = "dev"

func main() {
	var err error
	cmd := ""
	if len(os.Args) > 1 {
		cmd = os.Args[1]
	}
	switch cmd {
	case "":
		err = tui.Run()
	case "daemon":
		err = daemon.Run(version)
	case "login":
		err = login.Run(context.Background(), func(s string) { fmt.Println(s) })
		if err == nil {
			fmt.Println("signed in")
			if c, dialErr := ipc.Dial(); dialErr == nil {
				_, _ = c.Do(ipc.Request{Cmd: ipc.CmdReload})
				c.Close()
			}
			// Signed in at a terminal: go straight to the music.
			if isTerminal(os.Stdin) && isTerminal(os.Stdout) {
				err = tui.Run()
			}
		}
	case "setup":
		from := "."
		if len(os.Args) > 2 {
			from = os.Args[2]
		}
		if err = update.Install(from); err == nil {
			fmt.Println("installed brumm to", update.BinPath())
		}
	case "update":
		if !update.Managed(version) {
			err = fmt.Errorf("this brumm (%s) was not installed from a release; use bin/update in the source checkout", version)
			break
		}
		var tag string
		var installed bool
		if tag, installed, err = update.Update(version); err == nil {
			if installed {
				fmt.Println("updated to", tag)
				restartOffer()
			} else {
				fmt.Println("brumm", version, "is current")
			}
		}
	case "version", "--version":
		fmt.Println("brumm", version)
	default:
		fmt.Fprintln(os.Stderr, "usage: brumm [daemon | login | update | setup DIR | version]")
		os.Exit(2)
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "brumm:", err)
		os.Exit(1)
	}
}

func isTerminal(f *os.File) bool {
	_, err := unix.IoctlGetTermios(int(f.Fd()), unix.TCGETS)
	return err == nil
}

// restartOffer asks, at a terminal, to restart the running player into the
// update now; it plays on where it was. Otherwise it waits for a pause.
func restartOffer() {
	later := "brumm restarts into it when playback is paused or a song ends"
	c, err := ipc.Dial()
	if err != nil {
		return // nothing running: the next start is the new one
	}
	defer c.Close()
	if !isTerminal(os.Stdin) || !isTerminal(os.Stdout) {
		fmt.Println(later)
		return
	}
	fmt.Print("restart now? the music goes on where it is [Y/n] ")
	var answer string
	_, _ = fmt.Scanln(&answer)
	if a := strings.ToLower(strings.TrimSpace(answer)); a != "" && a != "y" && a != "yes" && a != "j" && a != "ja" {
		fmt.Println(later)
		return
	}
	if _, err := c.Do(ipc.Request{Cmd: ipc.CmdUpdate, Value: 3}); err != nil {
		fmt.Println("could not restart:", err, "—", later)
		return
	}
	fmt.Println("restarting")
}
