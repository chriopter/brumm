//! The Chrome DevTools Protocol over the pipe Chrome opens with
//! --remote-debugging-pipe: it reads commands on fd 3 and answers on fd 4,
//! each message JSON ended by a NUL byte. Unlike a debugging port, a pipe
//! is reachable only by the two processes holding its ends.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{oneshot, watch};
use tokio::time::Instant;

/// Why a call failed.
#[derive(Debug, Clone, PartialEq)]
pub enum CdpError {
    /// No answer in time: Chrome may hang.
    Timeout(String),
    /// The pipe is gone: Chrome exited.
    Closed,
    /// Chrome answered with an error.
    Remote(String),
}

impl std::fmt::Display for CdpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CdpError::Timeout(m) => write!(f, "chrome: {m}: deadline exceeded"),
            CdpError::Closed => f.write_str("chrome: connection closed"),
            CdpError::Remote(m) => write!(f, "chrome: {m}"),
        }
    }
}

type Waiters = Arc<Mutex<Option<HashMap<i64, oneshot::Sender<Value>>>>>;

pub struct Cdp {
    writer: tokio::sync::Mutex<Box<dyn AsyncWrite + Send + Unpin>>,
    next: AtomicI64,
    waiters: Waiters,
    /// True once the pipe from Chrome ended.
    pub gone: watch::Receiver<bool>,
}

impl Cdp {
    /// Talks over w (Chrome's fd 3) and r (its fd 4).
    pub fn new(
        w: impl AsyncWrite + Send + Unpin + 'static,
        r: impl AsyncRead + Send + Unpin + 'static,
    ) -> Cdp {
        let waiters: Waiters = Arc::new(Mutex::new(Some(HashMap::new())));
        let (gone_tx, gone) = watch::channel(false);
        let ws = waiters.clone();
        tokio::spawn(async move {
            read(r, &ws).await;
            ws.lock().unwrap().take(); // whoever waits hears the pipe is gone
            let _ = gone_tx.send(true);
        });
        Cdp {
            writer: tokio::sync::Mutex::new(Box::new(w)),
            next: AtomicI64::new(0),
            waiters,
            gone,
        }
    }

    /// Sends method with params to the browser, or to a page when session
    /// is set, and returns the reply's result. Writing and waiting both
    /// end at the deadline: a hung browser holds no caller for longer.
    pub async fn call(
        &self,
        limit: Duration,
        session: &str,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, CdpError> {
        let deadline = Instant::now() + limit;
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = oneshot::channel();
        match self.waiters.lock().unwrap().as_mut() {
            Some(w) => w.insert(id, tx),
            None => return Err(CdpError::Closed),
        };
        let drop_waiter = || {
            if let Some(w) = self.waiters.lock().unwrap().as_mut() {
                w.remove(&id);
            }
        };
        let mut msg = json!({"id": id, "method": method});
        if let Some(p) = params {
            msg["params"] = p;
        }
        if !session.is_empty() {
            msg["sessionId"] = session.into();
        }
        let mut bytes = serde_json::to_vec(&msg).unwrap_or_default();
        bytes.push(0);
        let write = async {
            let mut w = self.writer.lock().await;
            w.write_all(&bytes).await?;
            w.flush().await
        };
        match tokio::time::timeout_at(deadline, write).await {
            Err(_) => {
                drop_waiter();
                return Err(CdpError::Timeout(method.to_string()));
            }
            Ok(Err(_)) => {
                drop_waiter();
                return Err(CdpError::Closed);
            }
            Ok(Ok(())) => {}
        }
        match tokio::time::timeout_at(deadline, rx).await {
            Err(_) => {
                drop_waiter();
                Err(CdpError::Timeout(method.to_string()))
            }
            Ok(Err(_)) => Err(CdpError::Closed),
            Ok(Ok(reply)) => {
                if let Some(e) = reply.get("error") {
                    let m = e.get("message").and_then(Value::as_str).unwrap_or("error");
                    return Err(CdpError::Remote(format!("{method}: {m}")));
                }
                Ok(reply.get("result").cloned().unwrap_or(Value::Null))
            }
        }
    }

    pub fn alive(&self) -> bool {
        !*self.gone.borrow()
    }
}

/// Routes replies to their callers until the pipe ends. Events (messages
/// without an id) are not needed and dropped.
async fn read(mut r: impl AsyncRead + Unpin, waiters: &Waiters) {
    let mut buf = Vec::with_capacity(64 << 10);
    let mut chunk = vec![0u8; 64 << 10];
    loop {
        let n = match r.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        buf.extend_from_slice(&chunk[..n]);
        let mut start = 0;
        while let Some(i) = buf[start..].iter().position(|&b| b == 0) {
            let msg = &buf[start..start + i];
            start += i + 1;
            let Ok(v) = serde_json::from_slice::<Value>(msg) else {
                continue;
            };
            let Some(id) = v.get("id").and_then(Value::as_i64).filter(|&id| id != 0) else {
                continue;
            };
            let ch = waiters.lock().unwrap().as_mut().and_then(|w| w.remove(&id));
            if let Some(ch) = ch {
                let _ = ch.send(v);
            }
        }
        buf.drain(..start);
        if buf.len() > 256 << 20 {
            return; // no message is this large: the stream is broken
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, BufReader};

    /// A fake Chrome on the far end: "Echo" returns its params after an
    /// event, anything else an error.
    fn fake_chrome() -> (Cdp, tokio::task::JoinHandle<()>) {
        let (ours_w, chrome_r) = tokio::io::duplex(1 << 16);
        let (chrome_w, ours_r) = tokio::io::duplex(1 << 16);
        let task = tokio::spawn(async move {
            let mut r = BufReader::new(chrome_r);
            let mut w = chrome_w;
            loop {
                let mut msg = Vec::new();
                if r.read_until(0, &mut msg).await.unwrap_or(0) == 0 {
                    return;
                }
                msg.pop();
                let m: Value = serde_json::from_slice(&msg).unwrap();
                w.write_all(b"{\"method\":\"Page.loadEventFired\",\"params\":{}}\0")
                    .await
                    .unwrap();
                let reply = if m["method"] == "Echo" {
                    json!({"id": m["id"], "result": m["params"]})
                } else if m["method"] == "Hang" {
                    continue;
                } else {
                    json!({"id": m["id"], "error": {"code": -32601, "message": format!("'{}' wasn't found", m["method"].as_str().unwrap())}})
                };
                let mut b = serde_json::to_vec(&reply).unwrap();
                b.push(0);
                w.write_all(&b).await.unwrap();
            }
        });
        (Cdp::new(ours_w, ours_r), task)
    }

    #[tokio::test]
    async fn calls_errors_timeouts_and_a_closed_pipe() {
        let (c, chrome) = fake_chrome();
        let d = Duration::from_secs(2);
        let out = c.call(d, "", "Echo", Some(json!({"N": 42}))).await.unwrap();
        assert_eq!(out["N"], 42);
        match c.call(d, "s1", "Nope", None).await {
            Err(CdpError::Remote(m)) => assert!(m.contains("wasn't found"), "{m}"),
            other => panic!("an error reply was not returned: {other:?}"),
        }
        let hung = c.call(Duration::from_millis(50), "", "Hang", None).await;
        assert_eq!(hung, Err(CdpError::Timeout("Hang".into())));
        // Concurrent calls each get their own answer.
        let (a, b) = tokio::join!(
            c.call(d, "", "Echo", Some(json!(1))),
            c.call(d, "", "Echo", Some(json!(2)))
        );
        assert_eq!((a.unwrap(), b.unwrap()), (json!(1), json!(2)));

        chrome.abort(); // Chrome goes away
        let mut gone = c.gone.clone();
        tokio::time::timeout(Duration::from_secs(1), gone.wait_for(|g| *g))
            .await
            .expect("closed pipe not noticed")
            .unwrap();
        assert!(!c.alive());
        assert_eq!(c.call(d, "", "Echo", None).await, Err(CdpError::Closed));
    }
}
