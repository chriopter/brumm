package engine

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"sync"
)

// cdp speaks the Chrome DevTools Protocol over the pipe Chrome opens with
// --remote-debugging-pipe: it reads commands on fd 3 and answers on fd 4,
// each message JSON ended by a NUL byte. Unlike a debugging port, a pipe
// is reachable only by the two processes holding its ends.
type cdp struct {
	w    *os.File // our end of Chrome's fd 3
	r    *os.File // our end of Chrome's fd 4
	wmu  sync.Mutex
	mu   sync.Mutex
	next int64
	wait map[int64]chan cdpReply
	gone chan struct{} // closed when the pipe from Chrome ends
}

type cdpReply struct {
	Result json.RawMessage `json:"result"`
	Error  *struct {
		Code    int    `json:"code"`
		Message string `json:"message"`
	} `json:"error"`
}

func newCDP(w, r *os.File) *cdp {
	c := &cdp{w: w, r: r, wait: map[int64]chan cdpReply{}, gone: make(chan struct{})}
	go c.read()
	return c
}

// read routes replies to their callers. Events (messages without an id)
// are not needed and dropped.
func (c *cdp) read() {
	defer func() {
		c.mu.Lock()
		close(c.gone)
		c.wait = nil
		c.mu.Unlock()
	}()
	br := bufio.NewReaderSize(c.r, 64<<10)
	for {
		msg, err := br.ReadBytes(0)
		if err != nil {
			return
		}
		var m struct {
			ID int64 `json:"id"`
			cdpReply
		}
		if json.Unmarshal(msg[:len(msg)-1], &m) != nil || m.ID == 0 {
			continue
		}
		c.mu.Lock()
		ch := c.wait[m.ID]
		delete(c.wait, m.ID)
		c.mu.Unlock()
		if ch != nil {
			ch <- m.cdpReply
		}
	}
}

// call sends method with params to the browser, or to a page when session
// is set, and decodes the reply's result into out (if non-nil).
func (c *cdp) call(ctx context.Context, session, method string, params, out any) error {
	c.mu.Lock()
	if c.wait == nil {
		c.mu.Unlock()
		return errors.New("chrome: connection closed")
	}
	c.next++
	id := c.next
	ch := make(chan cdpReply, 1)
	c.wait[id] = ch
	c.mu.Unlock()
	drop := func() {
		c.mu.Lock()
		if c.wait != nil {
			delete(c.wait, id)
		}
		c.mu.Unlock()
	}

	msg, err := json.Marshal(struct {
		ID        int64  `json:"id"`
		Method    string `json:"method"`
		Params    any    `json:"params,omitempty"`
		SessionID string `json:"sessionId,omitempty"`
	}{id, method, params, session})
	if err != nil {
		drop()
		return err
	}
	// A write only blocks when Chrome stops reading; the deadline keeps a
	// hung browser from holding every caller.
	c.wmu.Lock()
	if d, ok := ctx.Deadline(); ok {
		_ = c.w.SetWriteDeadline(d)
	}
	_, err = c.w.Write(append(msg, 0))
	c.wmu.Unlock()
	if err != nil {
		drop()
		return fmt.Errorf("chrome: %w", err)
	}

	select {
	case r := <-ch:
		if r.Error != nil {
			return fmt.Errorf("chrome: %s: %s", method, r.Error.Message)
		}
		if out != nil {
			return json.Unmarshal(r.Result, out)
		}
		return nil
	case <-c.gone:
		return errors.New("chrome: connection closed")
	case <-ctx.Done():
		drop()
		return fmt.Errorf("chrome: %s: %w", method, ctx.Err())
	}
}

func (c *cdp) close() {
	_ = c.w.Close()
	_ = c.r.Close()
}
