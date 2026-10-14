//! The service boundary for navigation, authentication and playback setup.
//! Credentials remain in provider code; only public presentation crosses IPC.
use super::{Api, Reply};
use crate::{config::Config, error::Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Destination {
    Home,
    Page { route: String },
    Library { list: String },
    Container { item: Box<crate::music::Item> },
    Search,
    Queue,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NavigationEntry {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub destination: Destination,
}
impl NavigationEntry {
    pub fn new(id: &str, label: &str, icon: &str, destination: Destination) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: icon.into(),
            destination,
        }
    }
}

/// Brumm utilities follow the provider's ordered entries.
pub fn navigation(mut entries: Vec<NavigationEntry>) -> Vec<NavigationEntry> {
    entries.push(NavigationEntry::new(
        "search",
        "Search",
        "search",
        Destination::Search,
    ));
    entries.push(NavigationEntry::new(
        "queue",
        "Queue",
        "queue",
        Destination::Queue,
    ));
    entries
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Capabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preference: Option<Preference>,
    pub collection_kinds: Vec<String>,
    pub rating_kinds: Vec<String>,
    pub favorite_kinds: Vec<String>,
    pub ratings: bool,
    pub dislike: bool,
    pub favorites: bool,
    pub collection_write: bool,
    pub playlist_write: bool,
    pub folders: bool,
    pub stations: bool,
    pub previews: bool,
    pub autoplay: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Preference {
    pub label: String,
    pub remove_label: String,
    pub collection_equivalent: bool,
    pub removable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuthPrompt {
    pub kind: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub expires_at: i64,
}
#[derive(Clone, Debug)]
pub enum AuthEvent {
    Progress(String),
    Prompt(AuthPrompt),
}
pub type AuthReporter = Arc<dyn Fn(AuthEvent) + Send + Sync>;

/// Private playback configuration. Never serialized or printed via Debug.
pub struct PlaybackPage {
    pub html: String,
    pub refresh: Option<ScriptCall>,
    pub resources: PlaybackResources,
}

#[derive(Clone, Default)]
pub struct PlaybackResources {
    pub assets: Vec<PlaybackAsset>,
    pub bridge: Option<Arc<dyn PlaybackBridge>>,
}

#[derive(Clone)]
pub struct PlaybackAsset {
    pub name: &'static str,
    pub content_type: &'static str,
    pub body: &'static [u8],
}

/// Private SDK operations. Providers implement a finite route set and capture
/// their own account; these responses never go to the frontend socket.
pub trait PlaybackBridge: Send + Sync {
    fn request<'a>(
        &'a self,
        route: &'a str,
        body: serde_json::Value,
    ) -> Reply<'a, Result<serde_json::Value>>;
}
pub struct ScriptCall {
    pub method: &'static str,
    pub args: Vec<serde_json::Value>,
}

pub trait Definition: Send + Sync {
    fn cache_max_age(&self) -> Option<std::time::Duration> {
        None
    }
    fn id(&self) -> &'static str;
    fn navigation(&self) -> Vec<NavigationEntry>;
    fn capabilities(&self) -> Capabilities;
    fn signed_in(&self, cfg: &Config) -> bool;
    fn validate(&self, cfg: &Config) -> Result<()>;
    fn expiry(&self, cfg: &Config) -> Option<i64>;
    fn account_key(&self, cfg: &Config) -> String;
    fn api(&self, cfg: &Config) -> Result<Arc<dyn Api>>;
    fn playback(&self, cfg: &Config, version: &str) -> Result<PlaybackPage>;
    fn authenticate(&self, report: AuthReporter) -> Reply<'_, Result<()>>;
}
