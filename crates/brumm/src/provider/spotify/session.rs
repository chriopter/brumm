//! Spotify's adapter uses the same account and playback boundary as other services.
use super::{Client, Session, login};
use crate::{
    config::Config,
    error::{Error, Result, err},
    provider::{Api, Reply, session::*},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub struct Spotify;
impl Definition for Spotify {
    fn id(&self) -> &'static str {
        "spotify"
    }
    fn navigation(&self) -> Vec<NavigationEntry> {
        let mut entries = vec![NavigationEntry::new(
            "home",
            "Home",
            "home",
            Destination::Home,
        )];
        for (id, label) in [
            ("playlists", "Playlists"),
            ("albums", "Albums"),
            ("artists", "Artists"),
            ("songs", "Songs"),
        ] {
            entries.push(NavigationEntry::new(
                id,
                label,
                id,
                Destination::Library { list: id.into() },
            ));
        }
        entries
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            collection_kinds: ["song", "album", "playlist"].map(str::to_owned).to_vec(),
            rating_kinds: vec!["song".into()],
            ratings: true,
            collection_write: true,
            playlist_write: true,
            ..Default::default()
        }
    }
    fn signed_in(&self, cfg: &Config) -> bool {
        !cfg.spotify_client_id.is_empty()
            && !cfg.spotify_refresh_token.is_empty()
            && !cfg.spotify_user.is_empty()
    }
    fn validate(&self, _cfg: &Config) -> Result<()> {
        Ok(())
    }
    fn expiry(&self, _cfg: &Config) -> Option<i64> {
        None
    }
    fn account_key(&self, cfg: &Config) -> String {
        Sha256::digest(cfg.spotify_user.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
    fn api(&self, cfg: &Config) -> Result<Arc<dyn Api>> {
        Ok(Arc::new(Client::new(Arc::new(Session::new(cfg)))))
    }
    fn playback(&self, cfg: &Config, version: &str) -> Result<PlaybackPage> {
        Ok(PlaybackPage {
            html: include_str!("player.html").replace(
                "__BRUMM_CFG__",
                &crate::engine::script_json(&json!({"version":version})),
            ),
            // Reauthentication replaces the SDK device as well as its token.
            refresh: None,
            resources: PlaybackResources {
                assets: vec![],
                bridge: Some(Arc::new(Bridge {
                    account: cfg.spotify_user.clone(),
                    client_id: cfg.spotify_client_id.clone(),
                    session: Arc::new(Session::new(cfg)),
                })),
            },
        })
    }
    fn authenticate(&self, report: AuthReporter) -> Reply<'_, Result<()>> {
        Box::pin(login::run(report))
    }
}
struct Bridge {
    account: String,
    client_id: String,
    session: Arc<Session>,
}
impl Bridge {
    fn current(&self) -> Result<()> {
        let cfg = Config::load()?;
        if cfg.effective_provider() != "spotify"
            || cfg.spotify_user != self.account
            || cfg.spotify_client_id != self.client_id
        {
            return Err(Error::Unauthorized);
        }
        Ok(())
    }
}
impl PlaybackBridge for Bridge {
    fn request<'a>(&'a self, route: &'a str, body: Value) -> Reply<'a, Result<Value>> {
        Box::pin(async move {
            self.current()?;
            let value = match route {
                "credentials" => {
                    let token = match body["rejected"].as_str() {
                        Some(stale) => self.session.renew(stale).await?,
                        None => self.session.token().await?,
                    };
                    json!({"accessToken":token})
                }
                "tracks" => {
                    let ids = body["ids"]
                        .as_array()
                        .filter(|ids| ids.len() <= 20)
                        .ok_or_else(|| err("at most 20 track IDs are required"))?;
                    let client = Client::new(self.session.clone());
                    let ids = ids
                        .iter()
                        .map(|id| id.as_str().ok_or_else(|| err("invalid track ID")))
                        .collect::<Result<Vec<_>>>()?;
                    for id in &ids {
                        super::checked(id)?;
                    }
                    let client = Arc::new(client);
                    let mut jobs = tokio::task::JoinSet::new();
                    for id in ids {
                        let (client, id) = (client.clone(), id.to_owned());
                        jobs.spawn(async move { client.song(&id).await });
                    }
                    let mut tracks = Vec::new();
                    while let Some(result) = jobs.join_next().await {
                        match result.map_err(|e| err(e.to_string()))? {
                            Ok(value) => {
                                if let Some(track) = super::map::track(&value, None) {
                                    tracks.push(track);
                                }
                            }
                            Err(Error::Unauthorized) => return Err(Error::Unauthorized),
                            Err(_) => {}
                        }
                    }
                    json!(tracks)
                }
                _ => return Err(Error::Unsupported),
            };
            self.current()?;
            Ok(value)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spotify_navigation_capabilities_and_accounts_are_isolated() {
        let mut cfg = Config {
            provider: "spotify".into(),
            spotify_client_id: "client".into(),
            spotify_user: "alice".into(),
            spotify_refresh_token: "private-refresh".into(),
            ..Default::default()
        };
        assert!(Spotify.signed_in(&cfg));
        let path = crate::config::session_cache_dir_for(&cfg);
        cfg.spotify_refresh_token = "rotated-refresh".into();
        assert_eq!(path, crate::config::session_cache_dir_for(&cfg));
        cfg.spotify_user = "bob".into();
        assert_ne!(path, crate::config::session_cache_dir_for(&cfg));
        let info = crate::provider::registry::selection(&cfg);
        assert_eq!(info.selected, "spotify");
        assert!(
            !info
                .navigation
                .iter()
                .any(|entry| matches!(entry.id.as_str(), "explore" | "radio"))
        );
        assert!(info.capabilities.ratings && info.capabilities.playlist_write);
        assert!(
            !info.capabilities.previews
                && !info.capabilities.stations
                && !info.capabilities.dislike
        );
        assert!(!serde_json::to_string(&info).unwrap().contains("refresh"));
        let page = Spotify.playback(&cfg, "test").unwrap();
        assert!(!page.html.contains("rotated-refresh"));
        assert!(page.refresh.is_none());
    }
    #[tokio::test]
    async fn old_playback_bridges_reject_after_switching_services() {
        let Some(_env) = crate::config::testenv::isolated() else {
            return;
        };
        let cfg = Config {
            provider: "spotify".into(),
            spotify_client_id: "client".into(),
            spotify_user: "alice".into(),
            spotify_refresh_token: "refresh".into(),
            ..Default::default()
        };
        cfg.save().unwrap();
        let bridge = Spotify
            .playback(&cfg, "test")
            .unwrap()
            .resources
            .bridge
            .unwrap();
        assert_eq!(
            bridge.request("unknown", json!({})).await,
            Err(Error::Unsupported)
        );
        assert!(
            bridge
                .request("tracks", json!({"ids":["../invalid"]}))
                .await
                .is_err()
        );
        Config::update(|cfg| {
            cfg.provider = "apple".into();
            Ok(())
        })
        .unwrap();
        assert_eq!(
            bridge.request("credentials", json!({})).await,
            Err(Error::Unauthorized)
        );
    }
}
