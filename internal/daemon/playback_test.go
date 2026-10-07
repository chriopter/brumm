package daemon

import (
	"testing"
	"time"

	"github.com/chriopter/brumm/internal/engine"
	"github.com/chriopter/brumm/internal/ipc"
)

func playbackTestDaemon() (*Daemon, *engine.Engine) {
	eng := &engine.Engine{}
	return &Daemon{eng: eng, conns: map[*conn]bool{}, quit: make(chan struct{}), nudge: make(chan struct{}, 1),
		resume: &resume{ID: "song", IDs: []string{"song"}, Pos: 59}}, eng
}

func TestFailedResumeRestartsOnceAndPreservesQueue(t *testing.T) {
	d, eng := playbackTestDaemon()
	seq := d.cancelPlayCheck()
	restarts := 0
	state := func() (engine.State, error) { return engine.State{Ready: true}, nil }
	recover := func() { restarts++ }
	d.confirmPlayback(eng, seq, 5*time.Millisecond, time.Millisecond, state, recover)
	if restarts != 1 || d.eng != nil || !d.resume.Autoplay || d.resume.ID != "song" || d.resume.Pos != 59 {
		t.Fatalf("recovery: %d restarts, engine %p, resume %+v", restarts, d.eng, d.resume)
	}
	d.confirmPlayback(eng, seq, 5*time.Millisecond, time.Millisecond, state, recover)
	if restarts != 1 {
		t.Fatal("recovery restarted the same player twice")
	}
}

func TestPlaybackCheckRepairsStaleReport(t *testing.T) {
	d, eng := playbackTestDaemon()
	d.resume = nil
	seq := d.cancelPlayCheck()
	d.confirmPlayback(eng, seq, time.Second, time.Millisecond, func() (engine.State, error) {
		return engine.State{Ready: true, Playing: true, Title: "Song"}, nil
	}, func() { t.Fatal("healthy playback restarted") })
	if !d.state.Playing || d.state.Title != "Song" {
		t.Fatalf("stale state not repaired: %+v", d.state)
	}
}

func TestPlaybackCheckCancelsOnNewTransportAction(t *testing.T) {
	for _, replace := range []bool{false, true} {
		d, eng := playbackTestDaemon()
		seq := d.cancelPlayCheck()
		d.confirmPlayback(eng, seq, time.Second, time.Millisecond, func() (engine.State, error) {
			if replace {
				d.mu.Lock()
				d.eng = &engine.Engine{}
				d.mu.Unlock()
			} else {
				d.cancelPlayCheck()
			}
			return engine.State{Ready: true}, nil
		}, func() { t.Fatal("cancelled check restarted player") })
		if d.resume.Autoplay {
			t.Fatal("cancelled play request was restored")
		}
	}
}

func TestPlaybackCheckSurfacesPlayerErrors(t *testing.T) {
	for _, auth := range []bool{false, true} {
		d, eng := playbackTestDaemon()
		seq := d.cancelPlayCheck()
		d.confirmPlayback(eng, seq, time.Second, time.Millisecond, func() (engine.State, error) {
			return engine.State{Ready: true, Err: "play: unavailable", NeedsAuth: auth}, nil
		}, func() { t.Fatal("explicit error triggered automatic restart") })
		want := ipc.StatusError
		if auth {
			want = ipc.StatusLoggedOut
		}
		if d.state.Status != want {
			t.Fatalf("status %s, want %s", d.state.Status, want)
		}
	}
}
