// brumm is an Apple Music player for Omarchy.
//
//	brumm          open the player (starts the background daemon if needed)
//	brumm daemon   run the background daemon (usually via systemd)
//	brumm login    sign in to Apple Music in the browser
//	brumm update   install the newest release now (it also updates itself daily)
//	brumm setup D  install from an unpacked release D (used by install.sh)
package main

import (
	"context"
	"fmt"
	"os"

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
				fmt.Println("updated to", tag, "— brumm restarts into it when playback is paused or a song ends")
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
