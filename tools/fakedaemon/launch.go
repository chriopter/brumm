package main

import (
	"fmt"
	"log"
	"os"
	"os/exec"
)

// A GUI stays alive until it closes. Reply as soon as it starts so the old
// frontend can finish switching instead of waiting for the new one to exit.
func startPreview(binary, ui string) error {
	if ui != "gui" && ui != "tui" {
		return fmt.Errorf("unknown preview UI %q", ui)
	}
	cmd := exec.Command(binary, "open", ui)
	cmd.Stdout, cmd.Stderr = os.Stdout, os.Stderr
	if err := cmd.Start(); err != nil {
		return err
	}
	go func() {
		if err := cmd.Wait(); err != nil {
			log.Printf("preview UI: %v", err)
		}
	}()
	return nil
}
