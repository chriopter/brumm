package daemon

import "testing"

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
