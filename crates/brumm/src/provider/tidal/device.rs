//! Device authorization using the compatibility client used by Untidy.
//! These are first-party application credentials, not registered to Brumm.
//! Encoding preserves the reference representation; it does not protect a secret.
use crate::error::{Result, err};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{sync::LazyLock, time::Duration};

pub const SCOPES: &str = "r_usr w_usr w_sub";
fn decode(parts: &[&str]) -> String {
    let encoded: Vec<u8> = parts
        .iter()
        .flat_map(|part| STANDARD.decode(part).expect("device credential encoding"))
        .collect();
    String::from_utf8(
        STANDARD
            .decode(encoded)
            .expect("device credential encoding"),
    )
    .expect("device credential text")
}
static ID: LazyLock<String> = LazyLock::new(|| decode(&["WmxneVNuaGtiVzUw", "V2xkTE1HbDRWQT09"]));
static SECRET: LazyLock<String> = LazyLock::new(|| {
    decode(&[
        "TVU1dU9VRm1SRUZxZUhKblNrWktZa3RPVjB4bFFY",
        "bExSMVpIYlVsT2RWaFFVRXhJVmxoQmRuaEJaejA9",
    ])
});
pub fn client_id() -> &'static str {
    &ID
}
pub(super) fn client_secret() -> &'static str {
    &SECRET
}

/// Only user-facing linking information may cross IPC. The device code stays private.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    pub url: String,
    pub code: String,
    pub expires_at: i64,
}
pub struct Authorization {
    code: String,
    pub prompt: Prompt,
    pub interval: Duration,
}
fn field<'a>(value: &'a Value, snake: &str, camel: &str) -> Option<&'a str> {
    value[snake].as_str().or_else(|| value[camel].as_str())
}
fn parse(value: &Value) -> Result<Authorization> {
    let code = field(value, "device_code", "deviceCode")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| err("Tidal returned no device authorization code"))?
        .to_string();
    let user_code = field(value, "user_code", "userCode")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| err("Tidal returned no linking code"))?
        .to_string();
    let url = field(
        value,
        "verification_uri_complete",
        "verificationUriComplete",
    )
    .or_else(|| field(value, "verification_uri", "verificationUri"))
    .unwrap_or("link.tidal.com");
    let url = if url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{url}")
    };
    let parsed =
        url::Url::parse(&url).map_err(|_| err("Tidal returned an invalid linking address"))?;
    if !matches!(
        parsed.host_str(),
        Some("link.tidal.com" | "login.tidal.com" | "tidal.com")
    ) || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
    {
        return Err(err("Tidal returned an invalid linking address"));
    }
    let expires = value["expires_in"]
        .as_i64()
        .or_else(|| value["expiresIn"].as_i64())
        .unwrap_or(300)
        .clamp(1, 3600);
    let interval = value["interval"].as_u64().unwrap_or(5).clamp(1, 60);
    Ok(Authorization {
        code,
        prompt: Prompt {
            url,
            code: user_code,
            expires_at: chrono::Utc::now().timestamp() + expires,
        },
        interval: Duration::from_secs(interval),
    })
}
pub async fn start() -> Result<Authorization> {
    let response = super::auth::http()?
        .post(super::auth::oauth_url("device_authorization"))
        .header("Accept", "application/json")
        .form(&[("client_id", client_id()), ("scope", SCOPES)])
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(err(format!(
            "Tidal could not start device linking ({})",
            response.status()
        )));
    }
    parse(&response.json().await?)
}
pub enum Poll {
    Token(Value),
    Pending,
    SlowDown,
    Transient,
    Denied,
    Expired,
}
fn classify(status: u16, value: Value) -> Result<Poll> {
    if (200..300).contains(&status) {
        return Ok(Poll::Token(value));
    }
    if status >= 500 || status == 429 {
        return Ok(Poll::Transient);
    }
    match value["error"].as_str() {
        Some("authorization_pending") => Ok(Poll::Pending),
        Some("slow_down") => Ok(Poll::SlowDown),
        Some("access_denied") => Ok(Poll::Denied),
        Some("expired_token") => Ok(Poll::Expired),
        _ => Err(err(format!("Tidal device authorization failed ({status})"))),
    }
}
pub async fn poll(authorization: &Authorization) -> Result<Poll> {
    let response = super::auth::http()?
        .post(super::auth::oauth_url("token"))
        .basic_auth(client_id(), Some(client_secret()))
        .header("Accept", "application/json")
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", authorization.code.as_str()),
            ("client_id", client_id()),
            ("scope", SCOPES),
        ])
        .send()
        .await;
    let response = match response {
        Ok(r) => r,
        Err(e) if e.is_timeout() || e.is_connect() => return Ok(Poll::Transient),
        Err(e) => return Err(e.into()),
    };
    let status = response.status().as_u16();
    let value = response.json().await.unwrap_or(Value::Null);
    classify(status, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn prompt_accepts_both_wire_formats_and_keeps_device_code_private() {
        for value in [
            json!({"device_code":"private","user_code":"ABCD","verification_uri":"link.tidal.com","expires_in":300,"interval":2}),
            json!({"deviceCode":"private","userCode":"ABCD","verificationUri":"link.tidal.com","expiresIn":300,"interval":2}),
        ] {
            let auth = parse(&value).unwrap();
            assert_eq!(auth.prompt.url, "https://link.tidal.com");
            assert_eq!(auth.interval.as_secs(), 2);
            assert!(
                !serde_json::to_string(&auth.prompt)
                    .unwrap()
                    .contains("private")
            );
        }
        assert!(
            parse(
                &json!({"deviceCode":"private","userCode":"ABCD","verificationUri":"evil.example"})
            )
            .is_err()
        );
    }
    #[test]
    fn polling_distinguishes_pending_slowdown_denial_expiry_and_retry() {
        assert!(matches!(
            classify(400, json!({"error":"authorization_pending"})).unwrap(),
            Poll::Pending
        ));
        assert!(matches!(
            classify(400, json!({"error":"slow_down"})).unwrap(),
            Poll::SlowDown
        ));
        assert!(matches!(
            classify(400, json!({"error":"access_denied"})).unwrap(),
            Poll::Denied
        ));
        assert!(matches!(
            classify(400, json!({"error":"expired_token"})).unwrap(),
            Poll::Expired
        ));
        assert!(matches!(
            classify(503, Value::Null).unwrap(),
            Poll::Transient
        ));
        assert!(classify(401, Value::Null).is_err());
    }
}
