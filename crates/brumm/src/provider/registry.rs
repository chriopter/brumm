//! Provider discovery for first-run selection. Only implemented services can
//! be selected; adding an entry is separate from enabling its adapter.
use crate::config::Config;
use crate::error::{Error, Result, err};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub available: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Selection {
    #[serde(default)]
    pub navigation: Vec<super::session::NavigationEntry>,
    #[serde(default)]
    pub capabilities: super::session::Capabilities,
    #[serde(default, rename = "protocolVersion")]
    pub protocol_version: u32,
    pub selected: String,
    #[serde(rename = "needsChoice")]
    pub needs_choice: bool,
    pub choices: Vec<ProviderInfo>,
}
pub fn choices() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "apple".into(),
            name: "Apple Music".into(),
            available: true,
        },
        ProviderInfo {
            id: "tidal".into(),
            name: "Tidal".into(),
            available: false,
        },
    ]
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
        navigation: definition(cfg.effective_provider())
            .map(|p| super::session::navigation(p.navigation()))
            .unwrap_or_else(|_| super::session::navigation(vec![])),
        capabilities: definition(cfg.effective_provider())
            .map(|p| p.capabilities())
            .unwrap_or_default(),
        protocol_version: 1,
        selected: cfg.effective_provider().into(),
        needs_choice: cfg.needs_provider_choice(),
        choices: choices(),
    }
}

/// The sole composition point for built-in service implementations.
pub fn definition(id: &str) -> Result<&'static dyn super::session::Definition> {
    static APPLE: super::apple::session::Apple = super::apple::session::Apple;
    match id {
        "apple" => Ok(&APPLE),
        _ => Err(Error::Unsupported),
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
        assert_eq!(validate("tidal"), Err(Error::Unsupported));
        assert!(validate("unknown").is_err());
    }
}
