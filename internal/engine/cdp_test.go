package engine

import (
	"bufio"
	"context"
	"encoding/json"
	"os"
	"testing"
	"time"
)

// fakeChrome answers CDP messages on the far end of the pipes: "Echo"
// returns its params, anything else an error.
func fakeChrome(t *testing.T) (*cdp, func()) {
	t.Helper()
	chromeIn, ourW, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	ourR, chromeOut, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	go func() {
		br := bufio.NewReader(chromeIn)
		for {
			msg, err := br.ReadBytes(0)
			if err != nil {
				return
			}
			var m struct {
				ID     int64           `json:"id"`
				Method string          `json:"method"`
				Params json.RawMessage `json:"params"`
			}
			_ = json.Unmarshal(msg[:len(msg)-1], &m)
			// An event first: the client must skip it.
			_, _ = chromeOut.Write([]byte(`{"method":"Page.loadEventFired","params":{}}` + "\x00"))
			var reply []byte
			if m.Method == "Echo" {
				reply, _ = json.Marshal(map[string]any{"id": m.ID, "result": m.Params})
			} else {
				reply, _ = json.Marshal(map[string]any{"id": m.ID, "error": map[string]any{"code": -32601, "message": "'" + m.Method + "' wasn't found"}})
			}
			_, _ = chromeOut.Write(append(reply, 0))
		}
	}()
	c := newCDP(ourW, ourR)
	return c, func() { chromeOut.Close(); chromeIn.Close() }
}

func TestCDPCall(t *testing.T) {
	c, stop := fakeChrome(t)
	defer c.close()
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	var out struct{ N int }
	if err := c.call(ctx, "", "Echo", map[string]int{"N": 42}, &out); err != nil || out.N != 42 {
		t.Fatalf("echo: %v %+v", err, out)
	}
	if err := c.call(ctx, "s1", "Nope", nil, nil); err == nil {
		t.Fatal("an error reply was not returned")
	}

	stop() // Chrome goes away
	select {
	case <-c.gone:
	case <-time.After(time.Second):
		t.Fatal("closed pipe not noticed")
	}
	if err := c.call(ctx, "", "Echo", nil, nil); err == nil {
		t.Fatal("call on a closed connection succeeded")
	}
}
