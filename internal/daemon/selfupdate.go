package daemon

import (
	"fmt"
	"log"
	"os"
	"syscall"
	"time"

	"github.com/chriopter/brumm/internal/update"
)

// ExitUpdated is the exit status that tells systemd to start the daemon
// again (RestartForceExitStatus in the unit): the program was replaced.
const ExitUpdated = 75

// checkUpdates looks for a newer release right after start, daily, and
// whenever a client opens (checkNow), and offers it to clients. It installs on its own only when this
// build's Apple Music access is about to run out, so playback never simply
// stops. Development builds never update.
func (d *Daemon) checkUpdates() {
	if !update.Managed(d.version) {
		return
	}
	t := time.NewTimer(5 * time.Second)
	for {
		select {
		case <-d.quit:
			return
		case <-t.C:
		case <-d.checkNow:
		}
		t.Reset(24 * time.Hour)
		tag, err := update.Latest()
		if err != nil {
			log.Printf("update check: %v", err)
			continue
		}
		if !update.Newer(tag, d.version) {
			continue
		}
		d.mu.Lock()
		d.update = tag
		urgent := d.expiresIn > 0 && d.expiresIn < 30
		d.mu.Unlock()
		if urgent {
			if err := d.installUpdate(); err != nil {
				log.Printf("update: %v", err)
			}
		}
	}
}

// installUpdate installs the newest release; the daemon then restarts into
// it at the next quiet moment (watchSelf).
func (d *Daemon) installUpdate() error {
	if !update.Managed(d.version) {
		return fmt.Errorf("this brumm (%s) was built from source; update it with bin/update", d.version)
	}
	tag, installed, err := update.Update(d.version)
	if err != nil {
		return err
	}
	if installed {
		log.Printf("installed %s; restarting when idle", tag)
		d.mu.Lock()
		d.update = ""
		d.mu.Unlock()
	}
	return nil
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
