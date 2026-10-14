//! Spotify sign-in: OAuth with PKCE, through the user's own Spotify app.
//!
//! The loopback page (at a secret path) asks for the app's Client ID and
//! gets back Spotify's authorize address, carrying a PKCE challenge and a
//! random state. Spotify sends the browser back to the fixed REDIRECT with
//! a code, which brumm trades for tokens. Only a callback carrying this
//! sign-in's state counts, and only the page itself may name the app.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};

use super::{ADDR, open};
use crate::config::{self, Config};
use crate::engine::{random_hex, script_json};
use crate::error::{Error, Result, err};
use crate::httpd;
use crate::provider::spotify::{ACCOUNTS_BASE, API_BASE, SCOPES, grant};

const PAGE: &str = include_str!("spotify.html");

/// Where Spotify sends the browser back: the app's redirect URI must be
/// exactly this.
pub const CALLBACK: &str = "/spotify/callback";
pub const REDIRECT: &str = "http://127.0.0.1:7777/spotify/callback";

/// Making the app takes a while the first time.
const WAIT: Duration = Duration::from_secs(15 * 60);

const TEXT: [(&str, &str); 1] = [("Content-Type", "text/plain; charset=utf-8")];

fn b64(b: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(b)
}

/// A PKCE verifier: 64 random bytes as 86 URL-safe characters.
fn verifier() -> String {
    let mut b = [0u8; 64];
    getrandom::fill(&mut b).expect("random bytes from the kernel");
    b64(&b)
}

/// The verifier's S256 challenge.
fn challenge(verifier: &str) -> String {
    b64(&Sha256::digest(verifier.as_bytes()))
}

/// A Client ID as Spotify's dashboard shows it: 32 hex digits.
fn valid_client(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn param(query: &str, key: &str) -> String {
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

/// One sign-in's secrets.
struct Flow {
    secret: String,
    /// The page's origin: only it may name the app.
    origin: String,
    state: String,
    challenge: String,
    /// The Client ID saved from a sign-in before, offered again.
    saved: String,
    /// The Client ID the page sent.
    client: Mutex<String>,
}

/// What the browser brought back.
enum Got {
    /// A code, and where to say how trading it went.
    Code(String, oneshot::Sender<std::result::Result<(), String>>),
    /// Spotify's reason for not signing in: the user declined, say.
    Refused(String),
}

impl Flow {
    fn new(origin: &str, saved: &str, verifier: &str) -> Flow {
        Flow {
            secret: random_hex(16),
            origin: origin.to_string(),
            state: random_hex(16),
            challenge: challenge(verifier),
            saved: saved.to_string(),
            client: Mutex::new(String::new()),
        }
    }

    fn page(&self) -> String {
        let cfg = json!({"clientId": self.saved, "redirect": REDIRECT});
        PAGE.replace("__BRUMM_CFG__", &script_json(&cfg))
    }

    fn authorize(&self, client: &str) -> String {
        let q = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs([
                ("client_id", client),
                ("response_type", "code"),
                ("redirect_uri", REDIRECT),
                ("code_challenge_method", "S256"),
                ("code_challenge", &self.challenge),
                ("state", &self.state),
                ("scope", SCOPES),
            ])
            .finish();
        format!("{ACCOUNTS_BASE}/authorize?{q}")
    }
}

/// A small page for the end of the sign-in.
fn said(msg: &str) -> String {
    let msg = msg
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>brumm · Spotify</title>\
         <style>:root{{color-scheme:light dark}}body{{margin:0;min-height:100vh;display:grid;place-items:center;\
         font:16px/1.5 ui-sans-serif,system-ui,sans-serif}}main{{text-align:center;padding:24px}}\
         .bear{{font:40px/1 ui-monospace,monospace;color:#1db954}}</style></head>\
         <body><main><div class=\"bear\">ʕ•ᴥ•ʔ</div><p>{msg}</p></main></body></html>"
    )
}

async fn html(s: &mut TcpStream, body: &str) {
    let headers = [
        ("Content-Type", "text/html; charset=utf-8"),
        ("Cache-Control", "no-store"),
        ("Referrer-Policy", "no-referrer"),
    ];
    httpd::respond(s, 200, &headers, body.as_bytes()).await;
}

/// Answers one request: the page at /secret/, the Client ID at
/// /secret/client, and Spotify's callback.
async fn answer(s: &mut TcpStream, f: &Flow, tx: &mpsc::Sender<Got>) {
    let Some(r) = httpd::read_request(s, 4 << 10).await else {
        return;
    };
    if r.path == format!("/{}/", f.secret) {
        if r.method != "GET" && r.method != "HEAD" {
            return httpd::respond(s, 405, &TEXT, b"method not allowed\n").await;
        }
        return html(s, &f.page()).await;
    }
    if r.path == format!("/{}/client", f.secret) {
        if r.method != "POST" {
            return httpd::respond(s, 405, &TEXT, b"method not allowed\n").await;
        }
        let header = |k: &str| r.headers.get(k).map(String::as_str).unwrap_or("");
        if header("origin") != f.origin || header("content-type") != "application/json" {
            return httpd::respond(s, 403, &TEXT, b"forbidden\n").await;
        }
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct Body {
            client_id: String,
        }
        let b: Body = serde_json::from_slice(&r.body).unwrap_or_default();
        if !valid_client(&b.client_id) {
            return httpd::respond(s, 400, &TEXT, b"not a client id\n").await;
        }
        *f.client.lock().unwrap_or_else(|e| e.into_inner()) = b.client_id.clone();
        let body = json!({"url": f.authorize(&b.client_id)}).to_string();
        return httpd::respond(
            s,
            200,
            &[
                ("Content-Type", "application/json"),
                ("Cache-Control", "no-store"),
            ],
            body.as_bytes(),
        )
        .await;
    }
    if r.path == CALLBACK && r.method == "GET" {
        let state = param(&r.query, "state");
        if state.is_empty() || state != f.state {
            return httpd::respond(s, 403, &TEXT, b"forbidden\n").await;
        }
        let refused = param(&r.query, "error");
        if !refused.is_empty() {
            let _ = tx.send(Got::Refused(refused.clone())).await;
            return html(
                s,
                &said(&format!(
                    "Spotify did not sign you in ({refused}). You can close this tab."
                )),
            )
            .await;
        }
        let code = param(&r.query, "code");
        if code.is_empty() {
            return httpd::respond(s, 400, &TEXT, b"missing code\n").await;
        }
        let (done, outcome) = oneshot::channel();
        if tx.send(Got::Code(code, done)).await.is_err() {
            return html(s, &said("This sign-in is over. Start a new one in brumm.")).await;
        }
        let msg = match tokio::time::timeout(Duration::from_secs(30), outcome).await {
            Ok(Ok(Ok(()))) => {
                "Signed in to Spotify. You can close this tab and go back to brumm.".to_string()
            }
            Ok(Ok(Err(e))) => format!("Sign-in failed: {e}"),
            _ => "Sign-in failed: brumm did not answer.".to_string(),
        };
        return html(s, &said(&msg)).await;
    }
    httpd::respond(s, 404, &TEXT, b"404 page not found\n").await;
}

/// Trades a code for tokens and asks who signed in: the refresh token and
/// the account id.
async fn finish(
    http: &reqwest::Client,
    accounts: &str,
    api: &str,
    client: &str,
    code: &str,
    verifier: &str,
) -> Result<(String, String)> {
    let g = grant(
        http,
        accounts,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT),
            ("client_id", client),
            ("code_verifier", verifier),
        ],
    )
    .await
    .map_err(|e| match e {
        Error::Unauthorized => err("Spotify did not take the sign-in code"),
        e => e,
    })?;
    if g.refresh_token.is_empty() {
        return Err(err("Spotify sent no refresh token"));
    }
    let resp = http
        .get(format!("{api}/v1/me"))
        .bearer_auth(&g.access_token)
        .send()
        .await?;
    let status = resp.status();
    let me: Value = serde_json::from_slice(&resp.bytes().await?).unwrap_or_default();
    if !status.is_success() {
        let why = me
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("");
        // In development mode, an account the app does not list.
        return Err(err(format!(
            "Spotify refused this account ({status} {why}); add it under User Management in your app"
        )));
    }
    match me.get("id").and_then(Value::as_str) {
        Some(id) if !id.is_empty() => Ok((g.refresh_token, id.to_string())),
        _ => Err(err("Spotify did not say who signed in")),
    }
}

/// Saves the session. Another account starts with empty caches.
fn save(client: &str, refresh: &str, user: &str) -> Result<()> {
    let mut cfg = Config::load()?;
    if !cfg.spotify_user.is_empty() && cfg.spotify_user != user {
        for name in config::SERVICE_CACHES {
            let _ = std::fs::remove_file(config::service_cache(name, "spotify"));
        }
    }
    cfg.provider = "spotify".into();
    cfg.spotify_client_id = client.to_string();
    cfg.spotify_refresh_token = refresh.to_string();
    cfg.spotify_user = user.to_string();
    cfg.save()
}

/// Opens the sign-in page and waits until Spotify sends the browser back
/// with a code that works (fifteen minutes at most). A code that fails
/// leaves the page open for another try.
pub(super) async fn run(cfg: Config, status: &impl Fn(&str)) -> Result<()> {
    let ln = TcpListener::bind(ADDR)
        .await
        .map_err(|e| err(format!("login: {e} (is another login open?)")))?;
    let verifier = verifier();
    let flow = Arc::new(Flow::new(
        &format!("http://{ADDR}"),
        &cfg.spotify_client_id,
        &verifier,
    ));
    let (tx, mut rx) = mpsc::channel::<Got>(4);
    let server = {
        let flow = flow.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = ln.accept().await else {
                    return;
                };
                let (flow, tx) = (flow.clone(), tx.clone());
                tokio::spawn(async move { answer(&mut s, &flow, &tx).await });
            }
        })
    };
    let url = format!("{}/{}/", flow.origin, flow.secret);
    status("opening Spotify sign-in in your browser…");
    open(&url, status);
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_default();
    let deadline = tokio::time::Instant::now() + WAIT;
    let r = loop {
        let got = match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(g)) => g,
            _ => break Err(err("login timed out")),
        };
        match got {
            Got::Refused(why) => break Err(err(format!("spotify sign-in: {why}"))),
            Got::Code(code, done) => {
                let client = flow
                    .client
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                let r =
                    match finish(&http, ACCOUNTS_BASE, API_BASE, &client, &code, &verifier).await {
                        Ok((refresh, user)) => save(&client, &refresh, &user),
                        Err(e) => Err(e),
                    };
                let _ = done.send(r.as_ref().map(|_| ()).map_err(|e| e.to_string()));
                if r.is_ok() {
                    break r;
                }
            }
        }
    };
    server.abort();
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::testserver::{Seen, serve, split};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn pkce_follows_rfc_7636() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let v = verifier();
        assert_eq!(v.len(), 86);
        assert!(
            v.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
        assert!(valid_client("0123456789abcdef0123456789ABCDEF"));
        assert!(!valid_client("0123456789abcdef"));
        assert!(!valid_client("0123456789abcdef0123456789abcdeg"));
    }

    #[test]
    fn the_page_config_is_a_safe_script() {
        let f = Flow::new("http://x", "</script><script>alert(1)", "v");
        let page = f.page();
        assert!(!page.contains("</script><script>alert"));
        assert!(!page.contains("__BRUMM_CFG__"));
        assert!(page.contains("127.0.0.1:7777/spotify/callback"));
    }

    #[tokio::test]
    async fn only_the_page_names_the_app_and_only_spotify_calls_back() {
        let ln = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = ln.local_addr().unwrap();
        let origin = format!("http://{addr}");
        let flow = Arc::new(Flow::new(&origin, "", "verifier"));
        let (tx, mut rx) = mpsc::channel::<Got>(4);
        {
            let flow = flow.clone();
            tokio::spawn(async move {
                loop {
                    let (mut s, _) = ln.accept().await.unwrap();
                    let (flow, tx) = (flow.clone(), tx.clone());
                    tokio::spawn(async move { answer(&mut s, &flow, &tx).await });
                }
            });
        }
        // The waiting sign-in: takes the code.
        tokio::spawn(async move {
            while let Some(g) = rx.recv().await {
                if let Got::Code(code, done) = g {
                    let _ = done.send(if code == "good" {
                        Ok(())
                    } else {
                        Err("bad code".into())
                    });
                }
            }
        });
        let ask = |req: String| async move {
            let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
            s.write_all(req.as_bytes()).await.unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).await.unwrap();
            out
        };
        let post = |origin: &str, body: &str| {
            format!(
                "POST /{}/client HTTP/1.1\r\nHost: x\r\nOrigin: {origin}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                flow.secret,
                body.len()
            )
        };
        let page = ask(format!("GET /{}/ HTTP/1.1\r\nHost: x\r\n\r\n", flow.secret)).await;
        assert!(page.starts_with("HTTP/1.1 200") && page.contains("Client ID"));
        let id = r#"{"clientId":"0123456789abcdef0123456789abcdef"}"#;
        assert!(
            ask(post("http://evil.example", id))
                .await
                .starts_with("HTTP/1.1 403")
        );
        assert!(
            ask(post(&origin, r#"{"clientId":"nope"}"#))
                .await
                .starts_with("HTTP/1.1 400")
        );
        let ok = ask(post(&origin, id)).await;
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        let url = ok.split("\r\n\r\n").nth(1).unwrap();
        let url: Value = serde_json::from_str(url).unwrap();
        let url = url["url"].as_str().unwrap();
        assert!(url.starts_with("https://accounts.spotify.com/authorize?"));
        for part in [
            "client_id=0123456789abcdef0123456789abcdef",
            "code_challenge_method=S256",
            &format!("state={}", flow.state),
            "redirect_uri=http%3A%2F%2F127.0.0.1%3A7777%2Fspotify%2Fcallback",
            "scope=streaming+",
        ] {
            assert!(url.contains(part), "{part} in {url}");
        }
        let callback = |q: String| format!("GET {CALLBACK}?{q} HTTP/1.1\r\nHost: x\r\n\r\n");
        assert!(
            ask(callback("code=good&state=guess".into()))
                .await
                .starts_with("HTTP/1.1 403")
        );
        assert!(
            ask(callback("code=good".into()))
                .await
                .starts_with("HTTP/1.1 403")
        );
        let bad = ask(callback(format!("code=bad&state={}", flow.state))).await;
        assert!(bad.contains("Sign-in failed: bad code"), "{bad}");
        let good = ask(callback(format!("code=good&state={}", flow.state))).await;
        assert!(good.contains("Signed in to Spotify"), "{good}");
        assert!(
            ask("GET /other HTTP/1.1\r\nHost: x\r\n\r\n".into())
                .await
                .starts_with("HTTP/1.1 404")
        );
    }

    #[tokio::test]
    async fn a_code_becomes_a_refresh_token_and_an_account() {
        let (base, seen) = serve(Arc::new(|s: &Seen| {
            let (path, _) = split(&s.target);
            match path.as_str() {
                "/api/token" => (
                    200,
                    r#"{"access_token":"a","expires_in":3600,"refresh_token":"r"}"#.into(),
                ),
                "/v1/me"
                    if s.headers.get("authorization").map(String::as_str) == Some("Bearer a") =>
                {
                    (200, r#"{"id":"alice"}"#.into())
                }
                _ => (403, r#"{"error":{"message":"User not registered"}}"#.into()),
            }
        }))
        .await;
        let http = reqwest::Client::new();
        let got = finish(&http, &base, &base, "cid", "code", "ver")
            .await
            .unwrap();
        assert_eq!(got, ("r".to_string(), "alice".to_string()));
        let token = seen.lock().unwrap()[0].clone();
        let body = String::from_utf8_lossy(&token.body).to_string();
        for part in [
            "grant_type=authorization_code",
            "code=code",
            "code_verifier=ver",
            "client_id=cid",
        ] {
            assert!(body.contains(part), "{part} in {body}");
        }
    }
}
