//! Runs a provider-supplied playback page in headless Chrome. The page is
//! served on a loopback port at a secret path (a secure context, which EME
//! needs), and driven over the DevTools pipe.

mod cdp;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;

use cdp::Cdp;
pub use cdp::CdpError;

use crate::chrome;
use crate::error::{Error, Result, err};
use crate::httpd;
use crate::ipc::Player;
use crate::music::Track;
use crate::proc;

/// How long Chrome may take to come up.
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(30);

pub struct Engine {
    cdp: Cdp,
    /// The playback page.
    session: String,
    pid: u32,
    /// True once Chrome has exited.
    exited: watch::Receiver<bool>,
    server: JoinHandle<()>,
    profile: PathBuf,
    closed: Mutex<bool>,
}

/// The current song and what follows it.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Queue {
    pub pos: i64,
    pub items: Vec<Track>,
}

/// JSON for inside a page's <script>: no character can end the script.
pub fn script_json(v: &Value) -> String {
    serde_json::to_string(v)
        .unwrap_or_default()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// A JavaScript string literal of s.
fn js(s: &str) -> String {
    script_json(&Value::String(s.to_string()))
}

/// A JavaScript number literal of f.
fn num(f: f64) -> String {
    if f.is_finite() {
        format!("{f}")
    } else {
        "0".into()
    }
}

fn ids_js(ids: &[String]) -> String {
    script_json(&json!(ids))
}

/// Where Chrome's throwaway profiles go: the user's runtime directory,
/// private to the user and cleared at logout.
fn profile_root() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => std::env::temp_dir(),
    }
}

/// Removes the profiles a killed daemon left behind. Call it before
/// starting Chrome.
pub fn sweep_profiles() {
    let Ok(entries) = std::fs::read_dir(profile_root()) else {
        return;
    };
    for e in entries.flatten() {
        if e.file_name().to_string_lossy().starts_with("brumm-chrome-") {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// n random bytes as hex.
pub fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    // The kernel's random source; without it nothing here is safe to run.
    getrandom::fill(&mut b).expect("random bytes from the kernel");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A fresh private directory under root named prefix + random.
pub fn private_temp_dir(root: &Path, prefix: &str) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    for _ in 0..10 {
        let dir = root.join(format!("{prefix}{}", random_hex(8)));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("no free temporary name"))
}

#[cfg(test)]
fn page_html(developer_token: &str, user_token: &str, version: &str) -> String {
    use crate::provider::session::Definition;
    crate::provider::apple::session::Apple
        .playback(
            &crate::config::Config {
                developer_token: developer_token.into(),
                user_token: user_token.into(),
                ..Default::default()
            },
            version,
        )
        .unwrap()
        .html
}

#[cfg(test)]
const PAGE: &str = include_str!("../provider/apple/player.html");

/// Serves html at path to requests addressed to host, nothing else.
async fn serve_page(ln: TcpListener, host: String, path: String, html: Arc<String>) {
    loop {
        let Ok((mut s, _)) = ln.accept().await else {
            return;
        };
        let (host, path, html) = (host.clone(), path.clone(), html.clone());
        tokio::spawn(async move {
            let Some(r) = httpd::read_request(&mut s, 0).await else {
                return;
            };
            if r.path != path || r.headers.get("host").map(String::as_str) != Some(host.as_str()) {
                httpd::respond(
                    &mut s,
                    404,
                    &[("Content-Type", "text/plain; charset=utf-8")],
                    b"404 page not found\n",
                )
                .await;
                return;
            }
            let headers = [
                ("Content-Type", "text/html; charset=utf-8"),
                ("Cache-Control", "no-store"),
                ("Referrer-Policy", "no-referrer"),
            ];
            httpd::respond(&mut s, 200, &headers, html.as_bytes()).await;
        });
    }
}

impl Engine {
    /// Launches Chrome on the playback page. Chrome dies with the daemon
    /// (PR_SET_PDEATHSIG), so a killed daemon leaves no orphaned browser.
    pub async fn start(page: crate::provider::session::PlaybackPage) -> Result<Arc<Engine>> {
        let html = Arc::new(page.html);
        let ln = TcpListener::bind("127.0.0.1:0").await?;
        // The page carries private provider credentials. It is served only
        // at an unguessable path and only to requests addressed to this
        // loopback port, so other local users cannot read it.
        let page_path = format!("/{}", random_hex(16));
        let host = ln.local_addr()?.to_string();
        let server = tokio::spawn(serve_page(ln, host.clone(), page_path.clone(), html));

        let profile = match private_temp_dir(&profile_root(), "brumm-chrome-") {
            Ok(p) => p,
            Err(e) => {
                server.abort();
                return Err(e.into());
            }
        };
        let fail = |e: Error, profile: &Path| {
            server.abort();
            let _ = std::fs::remove_dir_all(profile);
            e
        };

        // Chrome talks DevTools over two pipes (fd 3 in, fd 4 out) and opens
        // no debugging port another local user could connect to.
        let pipes = std::io::pipe().and_then(|a| std::io::pipe().map(|b| (a, b)));
        let ((to_chrome, our_w), (our_r, from_chrome)) = match pipes {
            Ok(p) => p,
            Err(e) => return Err(fail(e.into(), &profile)),
        };
        let mut cmd = std::process::Command::new(chrome::path());
        cmd.args(chrome::args())
            .arg("--remote-debugging-pipe")
            .arg(format!("--user-data-dir={}", profile.display()))
            .args([
                "--no-first-run",
                "--no-default-browser-check",
                "about:blank",
            ])
            .stdin(std::process::Stdio::null());
        if let Some(runtime) = std::env::var_os("BRUMM_DESKTOP_RUNTIME").filter(|r| !r.is_empty()) {
            let runtime = PathBuf::from(runtime);
            if std::env::var_os("PULSE_SERVER").is_none_or(|v| v.is_empty()) {
                cmd.env(
                    "PULSE_SERVER",
                    format!("unix:{}", runtime.join("pulse/native").display()),
                );
            }
            if std::env::var_os("PIPEWIRE_RUNTIME_DIR").is_none_or(|v| v.is_empty()) {
                cmd.env("PIPEWIRE_RUNTIME_DIR", &runtime);
            }
        }
        let mut child = match proc::spawn_tied(cmd, to_chrome.into(), from_chrome.into()) {
            Ok(c) => c,
            Err(e) => return Err(fail(err(format!("chrome: {e}")), &profile)),
        };
        let pid = child.id();
        let (exited_tx, exited) = watch::channel(false);
        std::thread::spawn(move || {
            let _ = child.wait();
            let _ = exited_tx.send(true);
        });
        let w = tokio::net::unix::pipe::Sender::from_owned_fd(our_w.into());
        let r = tokio::net::unix::pipe::Receiver::from_owned_fd(our_r.into());
        let (w, r) = match (w, r) {
            (Ok(w), Ok(r)) => (w, r),
            (Err(e), _) | (_, Err(e)) => {
                proc::kill(pid);
                return Err(fail(e.into(), &profile));
            }
        };
        let mut e = Engine {
            cdp: Cdp::new(w, r),
            session: String::new(),
            pid,
            exited,
            server,
            profile,
            closed: Mutex::new(false),
        };
        match e.open(&format!("http://{host}{page_path}")).await {
            Ok(()) => Ok(Arc::new(e)),
            Err(er) => {
                e.close().await;
                Err(er)
            }
        }
    }

    /// Attaches to Chrome's tab and loads url in it, bounded so a Chrome
    /// that never comes up or a dead network cannot hang startup.
    async fn open(&mut self, url: &str) -> Result<()> {
        let deadline = tokio::time::Instant::now() + LAUNCH_TIMEOUT;
        let left = || deadline.saturating_duration_since(tokio::time::Instant::now());
        let c = |e: CdpError| err(e.to_string());
        let targets = self
            .cdp
            .call(left(), "", "Target.getTargets", None)
            .await
            .map_err(c)?;
        let mut id = targets
            .get("targetInfos")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|t| t.get("type").and_then(Value::as_str) == Some("page"))
            .and_then(|t| t.get("targetId").and_then(Value::as_str))
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            let created = self
                .cdp
                .call(
                    left(),
                    "",
                    "Target.createTarget",
                    Some(json!({"url": "about:blank"})),
                )
                .await
                .map_err(c)?;
            id = created
                .get("targetId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
        }
        let attached = self
            .cdp
            .call(
                left(),
                "",
                "Target.attachToTarget",
                Some(json!({"targetId": id, "flatten": true})),
            )
            .await
            .map_err(c)?;
        self.session = attached
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let nav = self
            .cdp
            .call(
                left(),
                &self.session,
                "Page.navigate",
                Some(json!({"url": url})),
            )
            .await
            .map_err(c)?;
        if let Some(t) = nav
            .get("errorText")
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
        {
            return Err(err(format!("chrome: loading the player: {t}")));
        }
        Ok(())
    }

    /// Runs js in the page and returns its value, within limit; with
    /// await_promise it waits for a promise the expression returns.
    async fn eval_wait(
        &self,
        js: &str,
        limit: Duration,
        await_promise: bool,
    ) -> Result<Value, CdpError> {
        let params =
            json!({"expression": js, "returnByValue": true, "awaitPromise": await_promise});
        let r = self
            .cdp
            .call(limit, &self.session, "Runtime.evaluate", Some(params))
            .await?;
        if let Some(x) = r.get("exceptionDetails") {
            let desc = x
                .get("exception")
                .and_then(|e| e.get("description"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let msg = if desc.is_empty() {
                x.get("text").and_then(Value::as_str).unwrap_or("")
            } else {
                desc
            };
            return Err(CdpError::Remote(format!("page: {msg}")));
        }
        match r.get("result").and_then(|v| v.get("value")) {
            Some(v) => Ok(v.clone()),
            None => {
                let typ = r
                    .get("result")
                    .and_then(|v| v.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                Err(CdpError::Remote(format!("page: {typ} result")))
            }
        }
    }

    async fn eval(&self, js: &str) -> Result<Value> {
        self.eval_wait(js, Duration::from_secs(3), false)
            .await
            .map_err(|e| err(e.to_string()))
    }

    /// Runs a command without awaiting any promise it returns.
    async fn call(&self, js: &str) -> Result<()> {
        self.eval(&format!("{js}; true")).await.map(|_| ())
    }

    pub async fn state(&self) -> Result<Player> {
        let v = self.eval(r#"window.brumm ? brumm.state() : "{}""#).await?;
        Ok(serde_json::from_str(v.as_str().unwrap_or("{}"))?)
    }

    /// state, as a future that holds the engine itself.
    pub fn state_owned(
        self: &Arc<Self>,
    ) -> impl std::future::Future<Output = Result<Player>> + Send + 'static {
        let e = self.clone();
        async move { e.state().await }
    }

    /// An engine without Chrome behind it, for the daemon's tests: every
    /// call fails as with a Chrome that went away.
    #[cfg(test)]
    pub fn stand_in() -> Arc<Engine> {
        let (w, _) = tokio::io::duplex(64);
        let (_, r) = tokio::io::duplex(64);
        let (_, exited) = watch::channel(true);
        Arc::new(Engine {
            cdp: Cdp::new(w, r),
            session: String::new(),
            pid: 0,
            exited,
            server: tokio::spawn(async {}),
            profile: std::env::temp_dir().join("brumm-stand-in-engine-never-created"),
            closed: Mutex::new(true),
        })
    }

    /// The player's state once it is newer than rev — the revision of the
    /// last answer, 0 to ask at once — or after heartbeat at the latest.
    /// The page answers on its own events, so calling it in a loop follows
    /// the player without polling it. A page that has not answered well
    /// after the heartbeat is hung: a Timeout.
    pub async fn watch(&self, rev: i64, heartbeat: Duration) -> Result<(Player, i64), CdpError> {
        let js = format!(
            r#"window.brumm ? brumm.wait({rev}, {}) : new Promise((r) => setTimeout(() => r("{{}}"), 300))"#,
            heartbeat.as_millis()
        );
        let v = self
            .eval_wait(&js, heartbeat + Duration::from_secs(15), true)
            .await?;
        let raw = v.as_str().unwrap_or("{}");
        let st: Player = serde_json::from_str(raw).map_err(|e| CdpError::Remote(e.to_string()))?;
        let rev = serde_json::from_str::<Value>(raw)
            .ok()
            .and_then(|v| v.get("rev").and_then(Value::as_i64))
            .unwrap_or(rev);
        Ok((st, rev))
    }

    /// n log-spaced frequency bands, 0–255 each.
    pub async fn spectrum(&self, n: i64) -> Result<Vec<i64>> {
        let v = self
            .eval(&format!(
                "window.brumm && brumm.spectrum ? brumm.spectrum({n}) : []"
            ))
            .await?;
        Ok(serde_json::from_value(v)?)
    }

    /// n waveform samples, -100…100.
    pub async fn wave(&self, n: i64) -> Result<Vec<i64>> {
        let v = self
            .eval(&format!(
                "window.brumm && brumm.wave ? brumm.wave({n}) : []"
            ))
            .await?;
        Ok(serde_json::from_value(v)?)
    }

    /// Plays a song's 30-second clip over a paused queue.
    pub async fn preview(&self, id: &str) -> Result<()> {
        self.call(&format!("brumm.preview({})", js(id))).await
    }

    pub async fn stop_preview(&self) -> Result<()> {
        self.call("brumm.stopPreview()").await
    }

    /// Queues songs by id and starts at start_id, start_at seconds in;
    /// source names the list they came from so players can mark it.
    pub async fn play_ids(
        &self,
        ids: &[String],
        start_id: &str,
        source: &str,
        start_at: f64,
    ) -> Result<()> {
        self.call(&format!(
            "brumm.playIds({}, {}, {}, {})",
            ids_js(ids),
            js(start_id),
            js(source),
            num(start_at)
        ))
        .await
    }

    /// Plays a radio station: endless, chosen by Apple.
    pub async fn play_station(&self, id: &str, source: &str) -> Result<()> {
        self.call(&format!("brumm.playStation({}, {})", js(id), js(source)))
            .await
    }

    /// Looks songs up in the page ahead of a play, so starting one of them
    /// skips the round trip to Apple.
    pub async fn warm(&self, ids: &[String]) -> Result<()> {
        self.call(&format!("brumm.warm({})", ids_js(ids))).await
    }

    /// Lets similar music play on when the queue runs out.
    pub async fn set_autoplay(&self, on: bool) -> Result<()> {
        self.call(&format!("brumm.autoplay({on})")).await
    }

    pub async fn queue(&self, n: i64) -> Result<Queue> {
        let v = self
            .eval(&format!(
                r#"window.brumm && brumm.queue ? brumm.queue({n}) : "{{}}""#
            ))
            .await?;
        Ok(serde_json::from_str(v.as_str().unwrap_or("{}"))?)
    }

    /// Plays the queue item at index i.
    pub async fn jump(&self, i: i64) -> Result<()> {
        self.call(&format!("brumm.jump({i})")).await
    }

    /// Adds songs right after the current one (next) or at the end.
    pub async fn enqueue(&self, ids: &[String], next: bool) -> Result<()> {
        self.call(&format!("brumm.enqueue({}, {next})", ids_js(ids)))
            .await
    }

    pub async fn toggle(&self) -> Result<()> {
        self.call("brumm.toggle()").await
    }
    pub async fn play(&self) -> Result<()> {
        self.call("brumm.play()").await
    }
    pub async fn pause(&self) -> Result<()> {
        self.call("brumm.pause()").await
    }
    pub async fn next(&self) -> Result<()> {
        self.call("brumm.next()").await
    }
    pub async fn prev(&self) -> Result<()> {
        self.call("brumm.prev()").await
    }
    pub async fn seek(&self, sec: f64) -> Result<()> {
        self.call(&format!("brumm.seek({})", num(sec))).await
    }
    pub async fn set_volume(&self, v: f64) -> Result<()> {
        self.call(&format!("brumm.volume({})", num(v))).await
    }
    pub async fn refresh_session(
        &self,
        page: crate::provider::session::PlaybackPage,
    ) -> Result<()> {
        let call = page
            .refresh
            .ok_or_else(|| err("session requires a player restart"))?;
        let args = call
            .args
            .iter()
            .map(script_json)
            .collect::<Vec<_>>()
            .join(",");
        self.call(&format!("{}({args})", call.method)).await
    }
    pub async fn set_shuffle(&self, on: bool) -> Result<()> {
        self.call(&format!("brumm.shuffle({on})")).await
    }
    pub async fn set_repeat(&self, mode: i64) -> Result<()> {
        self.call(&format!("brumm.repeat({mode})")).await
    }

    /// Whether Chrome is still there.
    pub fn alive(&self) -> bool {
        !*self.exited.borrow() && self.cdp.alive()
    }

    /// Ends Chrome — asking first, killing it if it does not exit within a
    /// few seconds — waits for it so its profile is not still being
    /// written, and removes the profile. Only the first call does it.
    pub async fn close(&self) {
        let mut closed = self.closed.lock().await;
        if *closed {
            return;
        }
        *closed = true;
        let _ = self
            .cdp
            .call(Duration::from_secs(2), "", "Browser.close", None)
            .await;
        let mut exited = self.exited.clone();
        if tokio::time::timeout(Duration::from_secs(5), exited.wait_for(|e| *e))
            .await
            .is_err()
        {
            if !*self.exited.borrow() {
                proc::kill(self.pid);
            }
            let _ = exited.wait_for(|e| *e).await;
        }
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.profile);
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.server.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every brumm.<name>(…) the engine calls exists on the playback page:
    /// a missing one fails silently in Chrome.
    #[test]
    fn page_defines_every_call() {
        let src = include_str!("mod.rs");
        let mut called = std::collections::BTreeSet::new();
        for part in src.split("brumm.").skip(1) {
            let name: String = part
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect();
            if part[name.len()..].starts_with('(') && !name.is_empty() {
                called.insert(name);
            }
        }
        assert!(
            called.len() >= 10,
            "found only {} calls; the scan is off",
            called.len()
        );
        for name in called {
            let defined = PAGE.lines().any(|l| {
                let t = l.trim_start();
                t.starts_with(&format!("{name}(")) || t.starts_with(&format!("async {name}("))
            });
            assert!(defined, "player.html does not define brumm.{name}");
        }
        assert!(
            PAGE.contains("__BRUMM_CFG__"),
            "player.html lost its config placeholder"
        );
    }

    /// The queue's songs carry every field the player reads from them:
    /// without their covers, nothing comes up next.
    #[test]
    fn queue_carries_every_field() {
        let i = PAGE.find("  queue(n) {").unwrap();
        let body = &PAGE[i..i + PAGE[i..].find("\n  },").unwrap()];
        for f in [
            "id", "title", "artist", "album", "duration", "artwork", "number",
        ] {
            assert!(
                body.contains(&format!(" {f}:")) || body.contains(&format!("{{ {f}:")),
                "queue leaves out {f}"
            );
        }
    }

    #[test]
    fn config_cannot_break_out_of_the_script() {
        let html = page_html("dev</script><script>alert(1)", "user\u{2028}", "v1&");
        assert!(!html.contains("</script><script>alert"));
        let escaped = format!("\"developerToken\":\"dev{}u003c/script", '\\');
        assert!(html.contains(&escaped), "{html}");
        assert!(!html.contains("__BRUMM_CFG__"));
    }

    #[tokio::test]
    async fn page_is_served_only_at_its_path_and_host() {
        let ln = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = ln.local_addr().unwrap().to_string();
        tokio::spawn(serve_page(
            ln,
            host.clone(),
            "/secret".into(),
            Arc::new("<html>".into()),
        ));
        let get = |path: &str, h: &str| {
            let (host, path, h) = (host.clone(), path.to_string(), h.to_string());
            async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut s = tokio::net::TcpStream::connect(&host).await.unwrap();
                s.write_all(format!("GET {path} HTTP/1.1\r\nHost: {h}\r\n\r\n").as_bytes())
                    .await
                    .unwrap();
                let mut out = String::new();
                s.read_to_string(&mut out).await.unwrap();
                out
            }
        };
        let ok = get("/secret", &host).await;
        assert!(
            ok.starts_with("HTTP/1.1 200") && ok.contains("no-store") && ok.ends_with("<html>"),
            "{ok}"
        );
        assert!(get("/other", &host).await.starts_with("HTTP/1.1 404"));
        assert!(
            get("/secret", "evil.example")
                .await
                .starts_with("HTTP/1.1 404"),
            "a rebinding host got the page"
        );
    }
}
