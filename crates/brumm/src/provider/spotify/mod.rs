//! A small Spotify Web API client: the user's library, the catalog, and
//! what Spotify lets an app in development mode do with them. Since 2026
//! that leaves out batch lookups, browse pages, recommendations, previews
//! and the songs of playlists the user neither owns nor shares.

mod auth;
mod link;
mod map;
mod provider;

use std::sync::Arc;
use std::time::Duration;

use reqwest::{Method, StatusCode};
use serde_json::Value;

use crate::error::{Error, Result, err};
use crate::provider::Listing;

pub(crate) use auth::grant;
pub use auth::{ACCOUNTS_BASE, Session};

pub const API_BASE: &str = "https://api.spotify.com";

/// The scopes brumm asks for: playback in the page, the library to read
/// and change, playlists, and what Home shows.
pub const SCOPES: &str = "streaming user-read-email user-read-private \
    user-read-playback-state user-modify-playback-state \
    user-library-read user-library-modify \
    playlist-read-private playlist-read-collaborative \
    playlist-modify-private playlist-modify-public \
    user-follow-read user-top-read user-read-recently-played";

/// Spotify's IDs: 22 characters of base62. Checked before any handle
/// from a player goes into a request.
pub fn valid_id(id: &str) -> bool {
    id.len() == 22 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn checked(id: &str) -> Result<&str> {
    if valid_id(id) {
        Ok(id)
    } else {
        Err(err("not a Spotify id"))
    }
}

/// The URI of a brumm kind and id: songs, albums, playlists, artists;
/// stations already are URIs.
pub fn uri(kind: &str, id: &str) -> Result<String> {
    if let Some(rest) = id.strip_prefix("spotify:playlist:") {
        return Ok(format!("spotify:playlist:{}", checked(rest)?));
    }
    let kind = match kind {
        "" | "song" | "songs" | "track" => "track",
        "album" | "albums" => "album",
        "playlist" | "playlists" => "playlist",
        "artist" | "artists" => "artist",
        other => return Err(err(format!("spotify has no {other}"))),
    };
    Ok(format!("spotify:{kind}:{}", checked(id)?))
}

/// A query string of pairs.
fn query(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

pub struct Client {
    /// The API's address; a test server's in tests.
    base: String,
    session: Arc<Session>,
    http: reqwest::Client,
    /// The signed-in user's id, for telling own playlists from others.
    me: tokio::sync::OnceCell<String>,
}

/// What Spotify says went wrong, as a player shows it.
fn refusal(status: StatusCode, body: &[u8], retry: Option<u64>) -> Error {
    if status == StatusCode::UNAUTHORIZED {
        return Error::Unauthorized;
    }
    let e = serde_json::from_slice::<Value>(body).unwrap_or_default();
    let e = e.get("error").cloned().unwrap_or_default();
    let (msg, reason) = (map::s(&e, "message"), map::s(&e, "reason"));
    if status == StatusCode::TOO_MANY_REQUESTS {
        if reason == "QUOTA_EXCEEDED" {
            return err("spotify: your app's request quota is used up for now");
        }
        return err(match retry {
            Some(s) => format!("spotify: too many requests; try again in {s}s"),
            None => "spotify: too many requests; try again shortly".into(),
        });
    }
    if reason == "PREMIUM_REQUIRED" {
        return err("spotify: this needs Spotify Premium");
    }
    if msg.is_empty() {
        err(format!("spotify: {status}"))
    } else {
        err(format!("spotify: {msg}"))
    }
}

impl Client {
    pub fn new(session: Arc<Session>) -> Client {
        Client::with_base(API_BASE, session)
    }

    pub(crate) fn with_base(base: &str, session: Arc<Session>) -> Client {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Client {
            base: base.to_string(),
            session,
            http,
            me: Default::default(),
        }
    }

    /// A path on the API; a `next` link only when it points back at the
    /// API, so the token never goes anywhere else.
    fn url(&self, path: &str) -> Result<String> {
        if path.starts_with('/') {
            return Ok(format!("{}{path}", self.base));
        }
        for base in [self.base.as_str(), API_BASE] {
            if let Some(rest) = path.strip_prefix(base)
                && rest.starts_with('/')
            {
                return Ok(format!("{}{rest}", self.base));
            }
        }
        Err(err("spotify: a link off the API"))
    }

    /// Makes a request with an optional JSON body and decodes the answer,
    /// when there is one. A refused token is renewed and the request made
    /// once more: it may have run out while the laptop slept.
    pub(crate) async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Option<Value>> {
        let token = self.session.token().await?;
        match self
            .send_with(&token, method.clone(), path, body.as_ref())
            .await
        {
            Err(Error::Unauthorized) => {
                let fresh = self.session.renew(&token).await?;
                self.send_with(&fresh, method, path, body.as_ref()).await
            }
            r => r,
        }
    }

    async fn send_with(
        &self,
        token: &str,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Option<Value>> {
        let mut rb = self
            .http
            .request(method.clone(), self.url(path)?)
            .bearer_auth(token);
        if let Some(b) = body {
            rb = rb
                .header("Content-Type", "application/json")
                .body(serde_json::to_vec(b)?);
        } else if method != Method::GET {
            rb = rb.header("Content-Length", "0");
        }
        let resp = rb.send().await?;
        let status = resp.status();
        let retry = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());
        let b = resp.bytes().await?;
        if !status.is_success() {
            return Err(refusal(status, &b, retry));
        }
        if b.is_empty() {
            return Ok(None);
        }
        match serde_json::from_slice(&b) {
            Ok(v) => Ok(Some(v)),
            // Changes answer with an empty or plain body.
            Err(_) if method != Method::GET => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub(crate) async fn get(&self, path: &str) -> Result<Value> {
        Ok(self
            .send(Method::GET, path, None)
            .await?
            .unwrap_or_default())
    }

    /// Follows `next` links from path, at most max pages. Pages hold their
    /// list in `items`, under key when given (the followed artists). A
    /// listing that stops early keeps what came and says Partial.
    pub(crate) async fn pages(&self, path: &str, key: Option<&str>, max: usize) -> Listing<Value> {
        let mut out = Vec::new();
        let mut next = path.to_string();
        let mut n = 0;
        while !next.is_empty() && n < max {
            n += 1;
            let doc = match self.get(&next).await {
                Ok(d) => d,
                Err(e) => {
                    let e = if !out.is_empty() && e != Error::Unauthorized {
                        Error::Partial
                    } else {
                        e
                    };
                    return (out, Some(e));
                }
            };
            let page = match key {
                Some(k) => doc.get(k).cloned().unwrap_or_default(),
                None => doc,
            };
            if let Some(items) = page.get("items").and_then(Value::as_array) {
                out.extend(items.iter().cloned());
            }
            next = map::s(&page, "next").to_string();
        }
        if !next.is_empty() {
            return (out, Some(Error::Partial));
        }
        (out, None)
    }

    /// The signed-in user's id, asked once.
    pub(crate) async fn me(&self) -> Result<String> {
        self.me
            .get_or_try_init(|| async {
                let doc = self.get("/v1/me").await?;
                match map::s(&doc, "id") {
                    "" => Err(err("spotify: no user")),
                    id => Ok(id.to_string()),
                }
            })
            .await
            .cloned()
    }
}

#[cfg(test)]
pub(crate) mod testserver {
    //! A stand-in for the Spotify Web API: canned JSON by path, for the
    //! user "me".
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};

    pub use crate::provider::testserver::{Seen, serve, split};

    /// Answers each path (query left out) from docs; /v1/me is "me".
    /// A doc beginning with a status and a space ("429 {…}") answers with
    /// that status; "{base}" in a doc is the server's address.
    pub async fn fake_api(docs: Vec<(&str, &str)>) -> (super::Client, Arc<Mutex<Vec<Seen>>>) {
        let docs: HashMap<String, String> = docs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let at = Arc::new(OnceLock::<String>::new());
        let here = at.clone();
        let (base, seen) = serve(Arc::new(move |s: &Seen| {
            let (path, _) = split(&s.target);
            if path == "/v1/me" {
                return (200, r#"{"id":"me","display_name":"Me"}"#.into());
            }
            match docs.get(&path) {
                Some(d) => {
                    let d = d.replace("{base}", here.get().map(String::as_str).unwrap_or(""));
                    match d.split_once(' ') {
                        Some((code, rest)) if code.len() == 3 && code.parse::<u16>().is_ok() => {
                            (code.parse().unwrap(), rest.to_string())
                        }
                        _ => (200, d),
                    }
                }
                None => (
                    404,
                    r#"{"error":{"status":404,"message":"Not found."}}"#.into(),
                ),
            }
        }))
        .await;
        let _ = at.set(base.clone());
        let c = super::Client::with_base(&base, Arc::new(super::Session::fixed("token")));
        (c, seen)
    }
}

#[cfg(test)]
mod tests {
    use super::testserver::fake_api;
    use super::*;

    #[test]
    fn uris_check_their_ids() {
        let id = "4uLU6hMCjMI75M1A2tKUQC";
        assert_eq!(uri("song", id).unwrap(), format!("spotify:track:{id}"));
        assert_eq!(uri("albums", id).unwrap(), format!("spotify:album:{id}"));
        assert_eq!(
            uri("station", &format!("spotify:playlist:{id}")).unwrap(),
            format!("spotify:playlist:{id}")
        );
        assert!(uri("song", "../me").is_err());
        assert!(uri("station", "spotify:playlist:x/../../me").is_err());
        assert!(uri("music-videos", id).is_err());
    }

    #[tokio::test]
    async fn errors_say_what_spotify_says() {
        let (c, seen) = fake_api(vec![
            ("/v1/unauthorized", "401 {}"),
            (
                "/v1/quota",
                r#"429 {"error":{"status":429,"message":"Too many requests","reason":"QUOTA_EXCEEDED"}}"#,
            ),
            (
                "/v1/premium",
                r#"403 {"error":{"status":403,"message":"Player command failed: Premium required","reason":"PREMIUM_REQUIRED"}}"#,
            ),
        ])
        .await;
        assert_eq!(c.get("/v1/unauthorized").await, Err(Error::Unauthorized));
        assert!(
            c.get("/v1/quota")
                .await
                .unwrap_err()
                .to_string()
                .contains("quota")
        );
        assert!(
            c.get("/v1/premium")
                .await
                .unwrap_err()
                .to_string()
                .contains("Premium")
        );
        let first = seen.lock().unwrap()[0].clone();
        assert_eq!(first.headers.get("authorization").unwrap(), "Bearer token");
        // A next link elsewhere is never followed with the token.
        assert!(c.get("https://evil.example/v1/me").await.is_err());
        assert_eq!(seen.lock().unwrap().len(), 3);
    }

    /// A token the laptop's sleep outlived: Spotify refuses it, the
    /// request is made again with a renewed one.
    #[tokio::test]
    async fn a_refused_token_is_renewed_once() {
        let (base, seen) = testserver::serve(std::sync::Arc::new(|s: &testserver::Seen| {
            let auth = s.headers.get("authorization").cloned().unwrap_or_default();
            match testserver::split(&s.target).0.as_str() {
                "/api/token" => (200, r#"{"access_token":"fresh","expires_in":3600}"#.into()),
                _ if auth == "Bearer fresh" => (200, r#"{"id":"me"}"#.into()),
                _ => (401, "{}".into()),
            }
        }))
        .await;
        let c = Client::with_base(&base, Arc::new(Session::stale(&base, "slept")));
        assert_eq!(c.me().await.unwrap(), "me");
        let targets: Vec<String> = seen
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.target.clone())
            .collect();
        assert_eq!(targets, ["/v1/me", "/api/token", "/v1/me"]);
        // A token refused even when fresh means signing in again.
        let (base, _) =
            testserver::serve(std::sync::Arc::new(
                |s: &testserver::Seen| match testserver::split(&s.target).0.as_str() {
                    "/api/token" => (200, r#"{"access_token":"fresh","expires_in":3600}"#.into()),
                    _ => (401, "{}".into()),
                },
            ))
            .await;
        let c = Client::with_base(&base, Arc::new(Session::stale(&base, "slept")));
        assert_eq!(c.get("/v1/me").await, Err(Error::Unauthorized));
    }

    #[tokio::test]
    async fn pages_follow_next_and_say_partial() {
        let (c, seen) = fake_api(vec![
            (
                "/v1/me/tracks",
                r#"{"items":[1,2],"next":"{base}/v1/page2?offset=2"}"#,
            ),
            // Spotify's own address is this server in tests.
            (
                "/v1/page2",
                r#"{"items":[3],"next":"https://api.spotify.com/v1/page3"}"#,
            ),
            ("/v1/page3", r#"{"items":[4],"next":"{base}/v1/gone"}"#),
        ])
        .await;
        let (v, e) = c.pages("/v1/me/tracks", None, 10).await;
        assert_eq!((v.len(), e), (4, Some(Error::Partial)));
        assert_eq!(seen.lock().unwrap()[2].target, "/v1/page3");
        let (v, e) = c.pages("/v1/me/tracks", None, 1).await;
        assert_eq!(
            (v.len(), e),
            (2, Some(Error::Partial)),
            "stopped at max pages"
        );
        let (c, _) = fake_api(vec![(
            "/v1/me/following",
            r#"{"artists":{"items":[{"id":"a"}],"next":null}}"#,
        )])
        .await;
        let (v, e) = c.pages("/v1/me/following", Some("artists"), 10).await;
        assert_eq!((v.len(), e), (1, None));
    }
}
