// brumm is an Apple Music player for Omarchy.
//
//	brumm          open the player (starts the background daemon if needed)
//	brumm daemon   run the background daemon (usually via systemd)
//	brumm login    sign in to Apple Music in the browser
package main

import (
	"context"
	"fmt"
	"os"

	"github.com/chriopter/brumm/internal/daemon"
	"github.com/chriopter/brumm/internal/ipc"
	"github.com/chriopter/brumm/internal/login"
	"github.com/chriopter/brumm/internal/tui"
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
	case "version", "--version":
		fmt.Println("brumm", version)
	default:
		fmt.Fprintln(os.Stderr, "usage: brumm [daemon | login | version]")
		os.Exit(2)
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "brumm:", err)
		os.Exit(1)
	}
}
