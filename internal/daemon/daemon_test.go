package daemon

import (
	"testing"

	"github.com/chriopter/brumm/internal/config"
	"github.com/chriopter/brumm/internal/ipc"
)

func TestWants(t *testing.T) {
	d := &Daemon{conns: map[*conn]bool{}}
	if b, w, fps := d.wants(); b != 0 || w != 0 || fps < 1 {
		t.Fatalf("no clients: %d %d %d", b, w, fps)
	}
	small := &conn{bands: 48, fps: 15}
	blurred := &conn{fps: 25} // asks for nothing: its rate must not count
	d.conns[small], d.conns[blurred] = true, true
	if b, w, fps := d.wants(); b != 48 || w != 0 || fps != 15 {
		t.Fatalf("one meter: %d %d %d", b, w, fps)
	}
	d.conns[&conn{bands: 48, wave: 128, fps: 25}] = true
	if b, w, fps := d.wants(); b != 48 || w != 128 || fps != 25 {
		t.Fatalf("with the visualizer: %d %d %d", b, w, fps)
	}
}

// A player changes some options; the rest stay, and every subscribed
// player hears of it.
func TestOptions(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	o := config.Options{Cover: "smooth", NoAutoplay: true}
	if err := o.Save(); err != nil {
		t.Fatal(err)
	}
	d := &Daemon{conns: map[*conn]bool{}}
	var reply ipc.Message
	if err := d.options(map[string]any{"start": "gui", "no_autoplay": false}, &reply); err != nil {
		t.Fatal(err)
	}
	got := config.LoadOptions()
	if got.Start != "gui" || got.NoAutoplay || got.Cover != "smooth" || reply.Options == nil || *reply.Options != got {
		t.Fatalf("saved %+v, answered %+v", got, reply.Options)
	}
	if err := d.options(map[string]any{"viz_fps": "fast"}, &reply); err == nil {
		t.Fatal("took a word for a number")
	}
	if err := d.resolve("https://example.com/x", &reply); err == nil {
		t.Fatal("resolved a link elsewhere")
	}
}

// A player leaves its place; the next one asks and finds it.
func TestPlace(t *testing.T) {
	d := &Daemon{}
	var reply ipc.Message
	d.place([]byte(`{"section":2}`), &reply)
	reply = ipc.Message{}
	d.place(nil, &reply)
	if string(reply.Place) != `{"section":2}` {
		t.Fatalf("place %s", reply.Place)
	}
}
