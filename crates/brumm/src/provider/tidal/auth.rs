//! The daemon owns the Tidal session; users authorize directly with Tidal.
use crate::{
    config::Config,
    error::{Error, Result, err},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::Mutex;

pub(crate) static SESSION: Mutex<()> = Mutex::const_new(());

#[cfg(test)]
static AUTH_ORIGIN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub(super) fn oauth_url(operation: &str) -> String {
    #[cfg(test)]
    if let Some(origin) = AUTH_ORIGIN.lock().unwrap().as_ref() {
        return format!("{origin}/v1/oauth2/{operation}");
    }
    format!("https://auth.tidal.com/v1/oauth2/{operation}")
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Session {
    pub flow: String,
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub scopes: Vec<String>,
    pub user_id: String,
    pub country: String,
}
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TidalSession")
            .field("user_id", &self.user_id)
            .finish_non_exhaustive()
    }
}
impl Session {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
    pub fn client_id(&self) -> &str {
        if self.client_id.is_empty() {
            super::device::client_id()
        } else {
            &self.client_id
        }
    }
    pub fn requested_scopes(&self) -> &str {
        super::device::SCOPES
    }
    pub fn signed_in(&self) -> bool {
        !self.user_id.is_empty()
            && (!self.refresh_token.is_empty() || !self.access_token.is_empty())
    }
}

pub(super) fn http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}
async fn exchange(fields: &[(&str, &str)], session: &Session) -> Result<Value> {
    let mut request = http()?.post(oauth_url("token"));
    {
        if session.client_id() != super::device::client_id() {
            return Err(err(
                "Tidal device session belongs to a different application",
            ));
        }
        request = request.basic_auth(session.client_id(), Some(super::device::client_secret()));
    }
    let response = request.form(fields).send().await?;
    let status = response.status();
    if !status.is_success() {
        return if status.as_u16() == 400 || status.as_u16() == 401 {
            Err(Error::Unauthorized)
        } else {
            Err(err(format!("Tidal authorization failed ({status})")))
        };
    }
    Ok(response.json().await?)
}
fn update(session: &mut Session, value: &Value) -> Result<()> {
    session.access_token = value["access_token"]
        .as_str()
        .or_else(|| value["accessToken"].as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| err("Tidal returned no access token"))?
        .into();
    if let Some(token) = value["refresh_token"]
        .as_str()
        .or_else(|| value["refreshToken"].as_str())
        .filter(|s| !s.is_empty())
    {
        session.refresh_token = token.into();
    }
    session.expires_at = chrono::Utc::now().timestamp()
        + value["expires_in"]
            .as_i64()
            .or_else(|| value["expiresIn"].as_i64())
            .unwrap_or(3600)
            .clamp(0, 86400 * 365);
    if let Some(scopes) = value["scope"].as_str() {
        session.scopes = scopes.split_whitespace().map(str::to_owned).collect();
    }
    Ok(())
}
/// Refresh at most once per expired token, even when REST and playback request it together.
/// A forced retry only refreshes if the rejected token is still current.
pub async fn credentials(account: &str, rejected: Option<&str>) -> Result<Session> {
    // A rotated refresh token must still be persisted to its owning saved record
    // if the playback page closes while its network request is in flight.
    let account = account.to_owned();
    let rejected = rejected.map(str::to_owned);
    tokio::spawn(async move { credentials_inner(&account, rejected.as_deref()).await })
        .await
        .map_err(|_| err("Tidal credential refresh was interrupted"))?
}

async fn credentials_inner(account: &str, rejected: Option<&str>) -> Result<Session> {
    let _guard = SESSION.lock().await;
    let mut cfg = Config::load()?;
    let session = &mut cfg.tidal;
    if cfg.provider != "tidal" || session.user_id != account || !session.signed_in() {
        return Err(Error::Unauthorized);
    }
    let force = rejected.is_some_and(|token| token == session.access_token);
    if !force && session.expires_at > chrono::Utc::now().timestamp() + 60 {
        return Ok(session.clone());
    }
    if session.refresh_token.is_empty() {
        return Err(Error::Unauthorized);
    }
    let value = exchange(
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &session.refresh_token),
            ("client_id", session.client_id()),
            ("scope", session.requested_scopes()),
        ],
        session,
    )
    .await?;
    update(session, &value)?;
    let result = session.clone();
    Config::update(|current| {
        if current.tidal.user_id != account {
            return Err(Error::Unauthorized);
        }
        current.tidal = result.clone();
        Ok(())
    })?;
    if Config::load()?.effective_provider() != "tidal" {
        return Err(Error::Unauthorized);
    }
    Ok(result)
}

pub fn sdk_credentials(session: &Session) -> Value {
    json!({"clientId": session.client_id(), "userId": session.user_id,
        "token": session.access_token, "expires": session.expires_at * 1000,
        "requestedScopes": session.requested_scopes().split_whitespace().collect::<Vec<_>>(), "grantedScopes": session.scopes})
}

pub async fn run(report: crate::provider::session::AuthReporter) -> Result<()> {
    let cfg = Config::load()?;
    if cfg.effective_provider() != "tidal" {
        return Err(err("music provider changed before sign-in"));
    }
    let progress = report.clone();
    device_login(
        cfg,
        move |text| progress(crate::provider::session::AuthEvent::Progress(text.into())),
        move |prompt| {
            report(crate::provider::session::AuthEvent::Prompt(
                crate::provider::session::AuthPrompt {
                    kind: "device".into(),
                    url: prompt.url.clone(),
                    code: Some(prompt.code),
                    expires_at: prompt.expires_at,
                },
            ));
            let _ = std::process::Command::new("xdg-open")
                .arg(&prompt.url)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        },
    )
    .await
}

async fn finish_login(mut cfg: Config, token: &Value) -> Result<()> {
    let scalar = |value: &Value| {
        value
            .as_str()
            .map(str::to_owned)
            .or_else(|| value.as_u64().map(|v| v.to_string()))
    };
    let identity = [
        &token["user_id"],
        &token["userId"],
        &token["user"]["user_id"],
        &token["user"]["userId"],
    ]
    .into_iter()
    .find_map(scalar);
    if let Some(identity) = identity {
        cfg.tidal.user_id = identity;
    }
    cfg.tidal.country = token["user"]["countryCode"]
        .as_str()
        .or_else(|| token["user"]["country"].as_str())
        .or_else(|| token["countryCode"].as_str())
        .or_else(|| token["country"].as_str())
        .unwrap_or("")
        .into();
    let response = http()?
        .get("https://openapi.tidal.com/v2/users/me")
        .bearer_auth(&cfg.tidal.access_token)
        .header("Accept", "application/vnd.api+json")
        .send()
        .await;
    if let Ok(response) = response {
        if response.status().is_success() {
            let user: Value = response.json().await?;
            if let Some(id) = scalar(&user["data"]["id"]) {
                cfg.tidal.user_id = id;
            }
            if let Some(country) = user["data"]["attributes"]["country"]
                .as_str()
                .or_else(|| user["data"]["attributes"]["countryCode"].as_str())
            {
                cfg.tidal.country = country.into();
            }
        } else if cfg.tidal.user_id.is_empty() {
            return Err(err(
                "Tidal authorized, but could not read the account identity",
            ));
        }
    }
    if cfg.tidal.user_id.is_empty() {
        return Err(err("Tidal returned no account identity"));
    }
    if cfg.tidal.country.len() != 2 || !cfg.tidal.country.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(err("Tidal returned no account country; connect again"));
    }
    cfg.tidal.country.make_ascii_uppercase();
    Config::update(|current| {
        if current.effective_provider() != "tidal" {
            return Err(err("music provider changed during sign-in"));
        }
        current.tidal = cfg.tidal;
        Ok(())
    })
}

async fn device_login(
    mut cfg: Config,
    status: impl Fn(&str),
    prompt: impl Fn(super::device::Prompt),
) -> Result<()> {
    let authorization = super::device::start().await?;
    status(&format!(
        "Link Tidal at {} with code {}",
        authorization.prompt.url, authorization.prompt.code
    ));
    prompt(authorization.prompt.clone());
    let mut interval = authorization.interval;
    let deadline = tokio::time::Instant::now()
        + Duration::from_secs(
            (authorization.prompt.expires_at - chrono::Utc::now().timestamp()).max(0) as u64,
        );
    loop {
        if tokio::time::Instant::now() + interval >= deadline {
            return Err(err(
                "Tidal linking code expired; connect again for a new code",
            ));
        }
        tokio::time::sleep(interval).await;
        let attempt = tokio::time::timeout_at(deadline, super::device::poll(&authorization))
            .await
            .map_err(|_| err("Tidal linking code expired"))??;
        match attempt {
            super::device::Poll::Pending => {}
            super::device::Poll::SlowDown => interval += Duration::from_secs(5),
            super::device::Poll::Transient => {
                interval = (interval + Duration::from_secs(2))
                    .min(Duration::from_secs(60))
                    .max(interval)
            }
            super::device::Poll::Denied => return Err(err("Tidal device linking was declined")),
            super::device::Poll::Expired => {
                return Err(err(
                    "Tidal linking code expired; connect again for a new code",
                ));
            }
            super::device::Poll::Token(value) => {
                let _guard = SESSION.lock().await;
                let current = Config::load()?;
                if current.effective_provider() != "tidal" {
                    return Err(err("music provider changed during sign-in"));
                }
                cfg.tidal.flow = "device".into();
                cfg.tidal.client_id = super::device::client_id().into();
                cfg.tidal.access_token.clear();
                cfg.tidal.refresh_token.clear();
                cfg.tidal.user_id.clear();
                cfg.tidal.scopes.clear();
                update(&mut cfg.tidal, &value)?;
                if cfg.tidal.scopes.is_empty() {
                    cfg.tidal.scopes = super::device::SCOPES
                        .split_whitespace()
                        .map(str::to_owned)
                        .collect();
                }
                return finish_login(cfg, &value).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refresh_preserves_a_missing_rotated_refresh_token_and_redacts_credentials() {
        let mut session = Session {
            refresh_token: "private-refresh".into(),
            scopes: vec!["r_usr".into()],
            ..Default::default()
        };
        update(
            &mut session,
            &json!({"access_token":"private-access","expires_in":120}),
        )
        .unwrap();
        assert_eq!(session.refresh_token, "private-refresh");
        assert_eq!(session.scopes, ["r_usr"]);
        assert!(!format!("{session:?}").contains("private-"));
    }
    async fn refresh_fixture() -> tokio::net::TcpListener {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        *AUTH_ORIGIN.lock().unwrap() = Some(format!("http://{}", listener.local_addr().unwrap()));
        Config {
            provider: "tidal".into(),
            user_token: "retained-apple-session".into(),
            tidal: Session {
                user_id: "fixture-user".into(),
                country: "GB".into(),
                access_token: "old-access".into(),
                refresh_token: "old-refresh".into(),
                ..Default::default()
            },
            ..Default::default()
        }
        .save()
        .unwrap();
        listener
    }

    #[tokio::test]
    async fn rest_and_sdk_refresh_once_and_retain_the_rotated_token() {
        let Some(_env) = crate::config::testenv::isolated() else {
            return;
        };
        let listener = refresh_fixture().await;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = crate::httpd::read_request(&mut stream, 4096).await.unwrap();
            assert!(
                String::from_utf8(request.body)
                    .unwrap()
                    .contains("refresh_token=old-refresh")
            );
            crate::httpd::respond(&mut stream,200,&[("Content-Type","application/json")],br#"{"access_token":"rotated-access","refresh_token":"rotated-refresh","expires_in":3600}"#).await;
        });
        let (a, b) = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(
                credentials("fixture-user", None),
                credentials("fixture-user", None)
            )
        })
        .await
        .unwrap();
        assert_eq!(a.unwrap().access_token, "rotated-access");
        assert_eq!(b.unwrap().access_token, "rotated-access");
        let cfg = Config::load().unwrap();
        assert_eq!(cfg.tidal.refresh_token, "rotated-refresh");
        assert_eq!(cfg.user_token, "retained-apple-session");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn refresh_after_switch_saves_only_its_own_credentials() {
        let Some(_env) = crate::config::testenv::isolated() else {
            return;
        };
        let listener = refresh_fixture().await;
        let (received, receiving) = tokio::sync::oneshot::channel();
        let (resume, resuming) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            crate::httpd::read_request(&mut stream, 4096).await.unwrap();
            received.send(()).unwrap();
            resuming.await.unwrap();
            crate::httpd::respond(&mut stream,200,&[("Content-Type","application/json")],br#"{"access_token":"rotated-access","refresh_token":"rotated-refresh","expires_in":3600}"#).await;
        });
        let refresh = tokio::spawn(async { credentials("fixture-user", None).await });
        receiving.await.unwrap();
        Config::update(|cfg| {
            cfg.provider = "apple".into();
            Ok(())
        })
        .unwrap();
        resume.send(()).unwrap();
        assert_eq!(refresh.await.unwrap().unwrap_err(), Error::Unauthorized);
        let cfg = Config::load().unwrap();
        assert_eq!(cfg.provider, "apple");
        assert_eq!(cfg.user_token, "retained-apple-session");
        assert_eq!(cfg.tidal.refresh_token, "rotated-refresh");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn cancelling_device_login_never_persists_credentials() {
        let Some(_env) = crate::config::testenv::isolated() else {
            return;
        };
        let listener = refresh_fixture().await;
        Config::update(|cfg| {
            cfg.tidal = Session::default();
            Ok(())
        })
        .unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = crate::httpd::read_request(&mut stream, 4096).await.unwrap();
            assert_eq!(request.path, "/v1/oauth2/device_authorization");
            crate::httpd::respond(&mut stream,200,&[("Content-Type","application/json")],br#"{"device_code":"private-device-code","user_code":"PUBLIC","verification_uri":"https://link.tidal.com","expires_in":300,"interval":1}"#).await;
        });
        let (shown, showing) = tokio::sync::oneshot::channel();
        let shown = std::sync::Arc::new(std::sync::Mutex::new(Some(shown)));
        let task = tokio::spawn(run(std::sync::Arc::new(move |event| {
            if let crate::provider::session::AuthEvent::Prompt(prompt) = event
                && let Some(shown) = shown.lock().unwrap().take()
            {
                let _ = shown.send(prompt);
            }
        })));
        let prompt = tokio::time::timeout(Duration::from_secs(3), showing)
            .await
            .unwrap()
            .unwrap();
        assert!(
            !serde_json::to_string(&prompt)
                .unwrap()
                .contains("private-device-code")
        );
        task.abort();
        let _ = task.await;
        assert!(Config::load().unwrap().tidal.access_token.is_empty());
        server.await.unwrap();
    }
}
