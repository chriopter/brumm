//! Provider discovery for first-run selection. Only implemented services can
//! be selected; adding an entry is separate from enabling its adapter.
use crate::config::Config;
use crate::error::{Error, Result, err};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub available: bool,
    /// What the players hide for this service: its API offers no way to
    /// do it. Names: explore, radio, preview, dislike, favorites, folders,
    /// videos, visualizer (no audio to analyze: it breathes as if paused),
    /// and love-items (only songs can be loved).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lacks: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Selection {
    pub selected: String,
    #[serde(rename = "needsChoice")]
    pub needs_choice: bool,
    pub choices: Vec<ProviderInfo>,
}
/// What Spotify's Web API leaves out for apps in development mode:
/// recommendations, previews and browse pages are gone, and it has no
/// dislike, favorites, folders or videos, and loves songs only. Its player
/// plays in a frame of its own, out of reach of the visualizer.
pub const SPOTIFY_LACKS: &[&str] = &[
    "explore",
    "radio",
    "preview",
    "dislike",
    "favorites",
    "folders",
    "videos",
    "visualizer",
    "love-items",
];

pub fn choices() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "apple".into(),
            name: "Apple Music".into(),
            available: true,
            ..Default::default()
        },
        ProviderInfo {
            id: "spotify".into(),
            name: "Spotify".into(),
            available: true,
            lacks: SPOTIFY_LACKS.iter().map(|s| s.to_string()).collect(),
        },
        ProviderInfo {
            id: "tidal".into(),
            name: "Tidal".into(),
            available: false,
            ..Default::default()
        },
    ]
}

/// A service's display name; Apple Music's for none.
pub fn name(id: &str) -> &'static str {
    match id {
        "spotify" => "Spotify",
        "tidal" => "Tidal",
        _ => "Apple Music",
    }
}

/// What the players hide for the service.
pub fn lacks(id: &str) -> Vec<String> {
    choices()
        .into_iter()
        .find(|p| p.id == id)
        .map(|p| p.lacks)
        .unwrap_or_default()
}
pub fn validate(id: &str) -> Result<()> {
    match choices().into_iter().find(|p| p.id == id) {
        Some(p) if p.available => Ok(()),
        Some(_) => Err(Error::Unsupported),
        None => Err(err("unknown music provider")),
    }
}
pub fn selection(cfg: &Config) -> Selection {
    Selection {
        selected: cfg.effective_provider().into(),
        needs_choice: cfg.needs_provider_choice(),
        choices: choices(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_users_choose_and_existing_sessions_keep_apple() {
        assert!(selection(&Config::default()).needs_choice);
        let cfg = Config {
            user_token: "session".into(),
            ..Default::default()
        };
        assert_eq!(selection(&cfg).selected, "apple");
        assert!(!selection(&cfg).needs_choice);
        assert!(validate("apple").is_ok());
        assert!(validate("spotify").is_ok());
        assert_eq!(validate("tidal"), Err(Error::Unsupported));
        assert!(validate("unknown").is_err());
    }

    #[test]
    fn spotify_sessions_and_capabilities() {
        let cfg = Config {
            provider: "spotify".into(),
            spotify_client_id: "id".into(),
            ..Default::default()
        };
        assert_eq!(selection(&cfg).selected, "spotify");
        assert_eq!(cfg.service_name(), "Spotify");
        assert_eq!(cfg.signed_in(), Ok(false), "no refresh token yet");
        let cfg = Config {
            spotify_refresh_token: "r".into(),
            ..cfg
        };
        assert_eq!(cfg.signed_in(), Ok(true));
        assert!(lacks("spotify").iter().any(|l| l == "radio"));
        assert!(lacks("apple").is_empty());
        assert_eq!(name(""), "Apple Music");
    }
}
