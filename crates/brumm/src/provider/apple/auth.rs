//! Signs the user in to Apple Music in their own browser and stores the
//! resulting Music User Token.
//!
//! MusicKit's authorize() needs a visible window for Apple's OAuth popup
//! (in headless Chrome it hangs on an invisible one), so the login runs on
//! a loopback page opened in the default browser, which posts the token
//! back.

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::engine::{random_hex, script_json};
use crate::error::{Result, err};
use crate::httpd;

const PAGE: &str = include_str!("login.html");

/// A fixed port keeps the page's origin stable between logins.
const ADDR: &str = "127.0.0.1:7777";

/// The page, with the developer token as a JavaScript string.
fn page(developer_token: &str) -> String {
    PAGE.replace(
        "{{.DeveloperToken}}",
        &script_json(&Value::String(developer_token.to_string())),
    )
}

/// Answers one request: the page at /secret/, the token at
/// /secret/token. Says the token when one came.
async fn answer(
    s: &mut tokio::net::TcpStream,
    secret: &str,
    origin: &str,
    html: &str,
) -> Option<String> {
    let r = httpd::read_request(s, 64 << 10).await?;
    let (page_path, token_path) = (format!("/{secret}/"), format!("/{secret}/token"));
    let text = [("Content-Type", "text/plain; charset=utf-8")];
    if r.path == page_path {
        if r.method != "GET" && r.method != "HEAD" {
            httpd::respond(s, 405, &text, b"method not allowed\n").await;
            return None;
        }
        httpd::respond(
            s,
            200,
            &[
                ("Content-Type", "text/html; charset=utf-8"),
                ("Cache-Control", "no-store"),
            ],
            html.as_bytes(),
        )
        .await;
        return None;
    }
    if r.path != token_path {
        httpd::respond(s, 404, &text, b"404 page not found\n").await;
        return None;
    }
    if r.method != "POST" {
        httpd::respond(s, 405, &text, b"method not allowed\n").await;
        return None;
    }
    // Only this page may post: nothing else — a local process, another web
    // page — can plant a token of its own.
    let header = |k: &str| r.headers.get(k).map(String::as_str).unwrap_or("");
    if header("origin") != origin || header("content-type") != "application/json" {
        httpd::respond(s, 403, &text, b"forbidden\n").await;
        return None;
    }
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        token: String,
    }
    match serde_json::from_slice::<Body>(&r.body) {
        Ok(b) if !b.token.is_empty() => {
            httpd::respond(s, 204, &[], b"").await;
            Some(b.token)
        }
        _ => {
            httpd::respond(s, 400, &text, b"missing token\n").await;
            None
        }
    }
}

/// Opens the login page and waits until a token arrives (five minutes at
/// most). The token is saved to the config.
pub async fn run(report: crate::provider::session::AuthReporter) -> Result<()> {
    let mut cfg = Config::load()?;
    let status = |text: &str| report(crate::provider::session::AuthEvent::Progress(text.into()));
    if cfg.developer().is_empty() {
        return Err(err("no Apple Music developer token in this build"));
    }
    let ln = TcpListener::bind(ADDR)
        .await
        .map_err(|e| err(format!("login: {e} (is another login open?)")))?;
    // A secret per login: the page lives at /<secret>/ and posts the token
    // back under it. The origin must be this page's.
    let secret = random_hex(16);
    let origin = format!("http://{ADDR}");
    let html = page(cfg.developer());
    let (tx, mut rx) = mpsc::channel::<String>(1);
    let server = {
        let (secret, origin) = (secret.clone(), origin.clone());
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = ln.accept().await else {
                    return;
                };
                let (secret, origin, html, tx) =
                    (secret.clone(), origin.clone(), html.clone(), tx.clone());
                tokio::spawn(async move {
                    if let Some(t) = answer(&mut s, &secret, &origin, &html).await {
                        let _ = tx.try_send(t);
                    }
                });
            }
        })
    };
    // Aborting authentication closes its private listener rather than leaving port 7777 busy.
    struct StopServer(tokio::task::JoinHandle<()>);
    impl Drop for StopServer {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let server = StopServer(server);
    let url = format!("{origin}/{secret}/");
    report(crate::provider::session::AuthEvent::Prompt(
        crate::provider::session::AuthPrompt {
            kind: "browser".into(),
            url: url.clone(),
            code: None,
            expires_at: chrono::Utc::now().timestamp() + 300,
        },
    ));
    status("opening Apple Music sign-in in your browser…");
    if std::process::Command::new("xdg-open")
        .arg(&url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_err()
    {
        status(&format!("open {url} to sign in"));
    }
    let got = tokio::time::timeout(Duration::from_secs(5 * 60), rx.recv()).await;
    drop(server);
    match got {
        Ok(Some(tok)) => {
            // Retain options/configuration changed while the browser was open.
            cfg = Config::load()?;
            if cfg.effective_provider() != "apple" {
                return Err(err("provider changed during sign-in"));
            }
            cfg.user_token = tok;
            cfg.save()
        }
        _ => Err(err("login timed out")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn cancelling_authentication_releases_the_listener_without_saving_credentials() {
        let Some(_env) = crate::config::testenv::isolated() else {
            return;
        };
        Config {
            provider: "apple".into(),
            developer_token: "fictional".into(),
            ..Default::default()
        }
        .save()
        .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let task = tokio::spawn(run(std::sync::Arc::new(move |event| {
            if let crate::provider::session::AuthEvent::Prompt(prompt) = event {
                let _ = tx.try_send(prompt);
            }
        })));
        let prompt = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(prompt.kind, "browser");
        assert!(prompt.code.is_none());
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        // Abort-on-drop must complete before another login can bind the same port.
        let listener = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(listener) = TcpListener::bind(ADDR).await {
                    break listener;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        drop(listener);
        assert!(Config::load().unwrap().user_token.is_empty());
    }

    #[test]
    fn the_token_is_a_safe_javascript_string() {
        let html = page("abc\"</script>");
        let b = '\\';
        assert!(
            html.contains(&format!(
                "developerToken: \"abc{b}\"{b}u003c/script{b}u003e\""
            )),
            "{html}"
        );
        assert!(!html.contains("abc\"</script>"));
        assert!(!html.contains("{{.DeveloperToken}}"));
    }

    #[tokio::test]
    async fn only_the_page_may_post_a_token() {
        let ln = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = ln.local_addr().unwrap();
        let origin = format!("http://{addr}");
        let o = origin.clone();
        let server = tokio::spawn(async move {
            let mut got = Vec::new();
            for _ in 0..5 {
                let (mut s, _) = ln.accept().await.unwrap();
                got.push(answer(&mut s, "sec", &o, "<html>").await);
            }
            got
        });
        let ask = |req: String| async move {
            let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
            s.write_all(req.as_bytes()).await.unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).await.unwrap();
            out
        };
        let post = |origin: &str, ctype: &str, body: &str| {
            format!(
                "POST /sec/token HTTP/1.1\r\nHost: x\r\nOrigin: {origin}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
        };
        assert!(
            ask("GET /sec/ HTTP/1.1\r\nHost: x\r\n\r\n".into())
                .await
                .starts_with("HTTP/1.1 200")
        );
        assert!(
            ask("GET /other/ HTTP/1.1\r\nHost: x\r\n\r\n".into())
                .await
                .starts_with("HTTP/1.1 404")
        );
        assert!(
            ask(post(
                "http://evil.example",
                "application/json",
                r#"{"token":"t"}"#
            ))
            .await
            .starts_with("HTTP/1.1 403")
        );
        assert!(
            ask(post(&origin, "application/json", r#"{}"#))
                .await
                .starts_with("HTTP/1.1 400")
        );
        assert!(
            ask(post(&origin, "application/json", r#"{"token":"good"}"#))
                .await
                .starts_with("HTTP/1.1 204")
        );
        let got = server.await.unwrap();
        assert_eq!(got, vec![None, None, None, None, Some("good".into())]);
    }
}
