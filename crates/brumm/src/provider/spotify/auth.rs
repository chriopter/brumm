//! Spotify's tokens: an access token good for an hour, and the refresh
//! token that brings the next one. Spotify may hand out a new refresh
//! token with any refresh; it is saved at once, since the old one may stop
//! working.

use std::time::{Duration, SystemTime};

use reqwest::StatusCode;
use serde::Deserialize;

use crate::config::Config;
use crate::error::{Error, Result, err};

pub const ACCOUNTS_BASE: &str = "https://accounts.spotify.com";

/// Refresh this long before the access token runs out.
const EARLY: Duration = Duration::from_secs(60);

/// What the token endpoint answers.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct Grant {
    pub access_token: String,
    pub expires_in: u64,
    pub refresh_token: String,
}

/// Asks the token endpoint for a grant. A refused refresh token or code
/// means signing in again: Unauthorized.
pub(crate) async fn grant(
    http: &reqwest::Client,
    accounts: &str,
    form: &[(&str, &str)],
) -> Result<Grant> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(form)
        .finish();
    let resp = http
        .post(format!("{accounts}/api/token"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await?;
    let status = resp.status();
    let b = resp.bytes().await?;
    if status.is_success() {
        let g: Grant = serde_json::from_slice(&b)?;
        if g.access_token.is_empty() {
            return Err(err("spotify sign-in: no access token"));
        }
        return Ok(g);
    }
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Refusal {
        error: String,
        error_description: String,
    }
    let r: Refusal = serde_json::from_slice(&b).unwrap_or_default();
    // A refused grant, or an app that is gone or changed: sign in again.
    if matches!(r.error.as_str(), "invalid_grant" | "invalid_client")
        || status == StatusCode::UNAUTHORIZED
    {
        return Err(Error::Unauthorized);
    }
    let why = if r.error_description.is_empty() {
        r.error
    } else {
        r.error_description
    };
    Err(err(format!("spotify sign-in: {status} {why}")
        .trim()
        .to_string()))
}

/// How long until a wall-clock time; zero once it has passed.
fn left(until: SystemTime) -> Duration {
    until.duration_since(SystemTime::now()).unwrap_or_default()
}

struct Tokens {
    access: String,
    /// None: never fetched. The wall clock, which counts a suspended
    /// laptop's sleep (Instant does not, on Linux).
    until: Option<SystemTime>,
    refresh: String,
}

type Save = Box<dyn Fn(&str) -> Result<()> + Send + Sync>;

pub struct Session {
    client_id: String,
    accounts: String,
    http: reqwest::Client,
    tokens: tokio::sync::Mutex<Tokens>,
    /// Keeps a new refresh token: in the config, outside tests.
    save: Save,
}

fn save_to_config(refresh: &str) -> Result<()> {
    let mut c = Config::load()?;
    if c.spotify_refresh_token != refresh {
        c.spotify_refresh_token = refresh.to_string();
        c.save()?;
    }
    Ok(())
}

impl Session {
    pub fn new(client_id: &str, refresh: &str) -> Session {
        Session::with(ACCOUNTS_BASE, client_id, refresh, Box::new(save_to_config))
    }

    pub(crate) fn with(accounts: &str, client_id: &str, refresh: &str, save: Save) -> Session {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Session {
            client_id: client_id.to_string(),
            accounts: accounts.to_string(),
            http,
            tokens: tokio::sync::Mutex::new(Tokens {
                access: String::new(),
                until: None,
                refresh: refresh.to_string(),
            }),
            save,
        }
    }

    /// A session holding an access token that stays good, for tests.
    #[cfg(test)]
    pub(crate) fn fixed(access: &str) -> Session {
        let s = Session::with("http://127.0.0.1:9", "client", "", Box::new(|_| Ok(())));
        if let Ok(mut t) = s.tokens.try_lock() {
            t.access = access.to_string();
            t.until = Some(SystemTime::now() + Duration::from_secs(3600));
        }
        s
    }

    /// A session whose access token Spotify will refuse, for tests.
    #[cfg(test)]
    pub(crate) fn stale(accounts: &str, access: &str) -> Session {
        let s = Session::with(accounts, "client", "refresh", Box::new(|_| Ok(())));
        if let Ok(mut t) = s.tokens.try_lock() {
            t.access = access.to_string();
            t.until = Some(SystemTime::now() + Duration::from_secs(3600));
        }
        s
    }

    /// A current access token, refreshed first when it is about to end.
    pub async fn token(&self) -> Result<String> {
        let mut t = self.tokens.lock().await;
        if let Some(until) = t.until
            && !t.access.is_empty()
            && left(until) > EARLY
        {
            return Ok(t.access.clone());
        }
        self.refresh(&mut t).await
    }

    /// A new access token, whatever the old one's time: Spotify refused
    /// `stale`. When another caller renewed it meanwhile, that one is used.
    pub async fn renew(&self, stale: &str) -> Result<String> {
        let mut t = self.tokens.lock().await;
        if !t.access.is_empty() && t.access != stale {
            return Ok(t.access.clone());
        }
        self.refresh(&mut t).await
    }

    /// A new access token now: the playback page's was refused.
    pub async fn refresh_now(&self) -> Result<String> {
        let mut t = self.tokens.lock().await;
        self.refresh(&mut t).await
    }

    /// How long the current access token stays good, less the margin a
    /// refresh keeps; zero when there is none.
    pub async fn remaining(&self) -> Duration {
        let t = self.tokens.lock().await;
        t.until
            .map(|u| left(u).saturating_sub(EARLY))
            .unwrap_or_default()
    }

    async fn refresh(&self, t: &mut Tokens) -> Result<String> {
        if t.refresh.is_empty() {
            return Err(Error::Unauthorized);
        }
        let g = grant(
            &self.http,
            &self.accounts,
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &t.refresh),
                ("client_id", &self.client_id),
            ],
        )
        .await?;
        if !g.refresh_token.is_empty() && g.refresh_token != t.refresh {
            (self.save)(&g.refresh_token)?;
            t.refresh = g.refresh_token;
        }
        t.access = g.access_token;
        t.until = Some(SystemTime::now() + Duration::from_secs(g.expires_in.max(60)));
        Ok(t.access.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::testserver::{Seen, serve, split};
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn refreshes_once_and_keeps_a_rotated_token() {
        let (base, seen) = serve(Arc::new(|s: &Seen| {
            let (_, q) = split(&format!("/?{}", String::from_utf8_lossy(&s.body)));
            match q.get("refresh_token").map(String::as_str) {
                Some("old") => (
                    200,
                    r#"{"access_token":"a1","expires_in":3600,"refresh_token":"new"}"#.into(),
                ),
                _ => (400, r#"{"error":"invalid_grant"}"#.into()),
            }
        }))
        .await;
        let saved = Arc::new(Mutex::new(Vec::<String>::new()));
        let keep = saved.clone();
        let s = Session::with(
            &base,
            "cid",
            "old",
            Box::new(move |t| {
                keep.lock().unwrap().push(t.to_string());
                Ok(())
            }),
        );
        assert_eq!(s.token().await.unwrap(), "a1");
        assert_eq!(
            s.token().await.unwrap(),
            "a1",
            "still good: no second refresh"
        );
        assert_eq!(seen.lock().unwrap().len(), 1);
        let req = seen.lock().unwrap()[0].clone();
        assert_eq!(req.target, "/api/token");
        let body = String::from_utf8_lossy(&req.body).to_string();
        assert!(body.contains("grant_type=refresh_token") && body.contains("client_id=cid"));
        assert_eq!(*saved.lock().unwrap(), ["new"]);
        // The rotated token is the one used next; the server refuses it.
        assert_eq!(s.renew("a1").await, Err(Error::Unauthorized));
    }

    #[tokio::test]
    async fn no_refresh_token_means_signed_out() {
        let s = Session::with("http://127.0.0.1:9", "cid", "", Box::new(|_| Ok(())));
        assert_eq!(s.token().await, Err(Error::Unauthorized));
    }
}
