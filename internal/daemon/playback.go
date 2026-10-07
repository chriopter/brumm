package daemon

import (
	"log"
	"time"

	"github.com/chriopter/brumm/internal/engine"
	"github.com/chriopter/brumm/internal/ipc"
)

const playConfirmWithin = 15 * time.Second

func (d *Daemon) cancelPlayCheck() uint64 {
	d.mu.Lock()
	defer d.mu.Unlock()
	d.playCheck++
	return d.playCheck
}

func (d *Daemon) checkingPlayback(eng *engine.Engine, seq uint64) bool {
	d.mu.Lock()
	defer d.mu.Unlock()
	return d.eng == eng && d.playCheck == seq
}

// Check the actual player before toggling: the daemon's last report can be
// stale. Only a request to resume needs a playback confirmation.
func (d *Daemon) togglePlayback(eng *engine.Engine) error {
	seq := d.cancelPlayCheck()
	es, err := eng.State()
	if err != nil {
		return err
	}
	d.apply(es)
	if d.resumeIfIdle(eng) {
		go d.verifyPlayback(eng, seq)
		return nil
	}
	if err := eng.Toggle(); err != nil {
		return err
	}
	if !es.Playing && es.Preview == nil {
		go d.verifyPlayback(eng, seq)
	}
	return nil
}

// A successful command only means JavaScript accepted it. Confirm that
// playback started, even if the event stream stopped reporting changes.
// One failed attempt gets one player restart; a deliberate pause cancels it.
func (d *Daemon) verifyPlayback(eng *engine.Engine, seq uint64) {
	d.confirmPlayback(eng, seq, playConfirmWithin, time.Second, eng.State, func() {
		log.Printf("playback did not start within %s; restarting player and restoring the queue", playConfirmWithin)
		go eng.Close()
		d.setStatus(ipc.StatusStarting, "playback did not start; restarting player")
		go d.boot()
	})
}

func (d *Daemon) confirmPlayback(eng *engine.Engine, seq uint64, timeout, interval time.Duration,
	state func() (engine.State, error), recover func()) {
	deadline := time.Now().Add(timeout)
	t := time.NewTicker(interval)
	defer t.Stop()
	for {
		select {
		case <-d.quit:
			return
		case <-t.C:
		}
		if !d.checkingPlayback(eng, seq) {
			return
		}
		es, err := state()
		if !d.checkingPlayback(eng, seq) {
			return
		}
		if err == nil {
			d.apply(es)
			if es.Playing || es.NeedsAuth || es.Err != "" {
				return
			}
		}
		if time.Now().Before(deadline) {
			continue
		}
		if d.claimPlaybackRecovery(eng, seq) {
			recover()
		}
		return
	}
}

func (d *Daemon) claimPlaybackRecovery(eng *engine.Engine, seq uint64) bool {
	d.mu.Lock()
	defer d.mu.Unlock()
	if d.eng != eng || d.playCheck != seq || d.state.Playing || d.resume == nil || !d.resume.playable() {
		return false
	}
	d.resume.Autoplay = true
	d.eng = nil // the existing watch must not restart the same player again
	return true
}
