package daemon

import (
	"log"
	"os"
	"syscall"
	"time"

	"github.com/chriopter/brumm/internal/update"
)

// ExitUpdated is the exit status that tells systemd to start the daemon
// again (RestartForceExitStatus in the unit): the program was replaced.
const ExitUpdated = 75

// autoUpdate installs new releases, checking a while after start and then
// daily. Only the installed program updates itself, never a dev build.
func (d *Daemon) autoUpdate() {
	if !update.Managed(d.version) {
		return
	}
	check := func() {
		tag, installed, err := update.Update(d.version)
		switch {
		case err != nil:
			log.Printf("update check: %v", err)
		case installed:
			log.Printf("installed %s; restarting when idle", tag)
		}
	}
	t := time.NewTimer(10 * time.Minute)
	for {
		select {
		case <-d.quit:
			return
		case <-t.C:
			check()
			t.Reset(24 * time.Hour)
		}
	}
}

// fileID identifies the file behind a path, so a replaced program is
// noticed even when the new one has the same size and time.
func fileID(path string) (dev, ino uint64, ok bool) {
	var st syscall.Stat_t
	if syscall.Stat(path, &st) != nil {
		return 0, 0, false
	}
	return uint64(st.Dev), st.Ino, true
}

// watchSelf restarts the daemon into a replaced program — by an update or
// by bin/setup — at a moment nobody hears: while paused, or right as a
// song ends, in which case the new daemon carries on playing.
func (d *Daemon) watchSelf() {
	self, err := os.Executable()
	if err != nil {
		return
	}
	dev, ino, ok := fileID(self)
	if !ok {
		return
	}
	t := time.NewTicker(15 * time.Second)
	defer t.Stop()
	for {
		select {
		case <-d.quit:
			return
		case <-t.C:
		}
		if d2, i2, ok := fileID(self); !ok || (d2 == dev && i2 == ino) {
			continue
		}
		// Replaced. Wait for a quiet moment.
		lastID := d.currentID()
		for {
			select {
			case <-d.quit:
				return
			case <-time.After(time.Second):
			}
			playing, id := d.playing(), d.currentID()
			if !playing || id != lastID {
				d.restartForUpdate(playing)
				return
			}
		}
	}
}

func (d *Daemon) currentID() string {
	d.mu.Lock()
	defer d.mu.Unlock()
	return d.state.ID
}

// restartForUpdate saves where playback is — marked to continue if it was
// playing — and exits so systemd starts the new program.
func (d *Daemon) restartForUpdate(playing bool) {
	d.mu.Lock()
	if r := d.resume; r != nil {
		r.Autoplay = playing
		r.save()
	}
	if d.eng != nil {
		d.eng.Close()
	}
	d.mu.Unlock()
	if d.mpris != nil {
		d.mpris.Close()
	}
	log.Printf("restarting into the updated program")
	os.Exit(ExitUpdated)
}
