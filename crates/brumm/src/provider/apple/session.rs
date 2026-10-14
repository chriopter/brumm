//! Apple-only session setup. The daemon knows neither MusicKit nor token fields.
use crate::provider::{Api, Reply, session::*};
use crate::{
    config::Config,
    error::{Result, err},
    music::{Item, KIND_SHELF},
};
use serde_json::{Value, json};
use std::sync::Arc;

pub struct Apple;
impl Definition for Apple {
    fn id(&self) -> &'static str {
        "apple"
    }
    fn navigation(&self) -> Vec<NavigationEntry> {
        let mut entries = vec![
            NavigationEntry::new("home", "Home", "home", Destination::Home),
            NavigationEntry::new(
                "explore",
                "Explore",
                "explore",
                Destination::Page {
                    route: "app:discover".into(),
                },
            ),
        ];
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
        entries.push(NavigationEntry::new(
            "radio",
            "Radio",
            "radio",
            Destination::Container {
                item: Box::new(Item {
                    kind: KIND_SHELF.into(),
                    id: "radio".into(),
                    name: "Radio".into(),
                    catalog: true,
                    ..Default::default()
                }),
            },
        ));
        entries
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            collection_kinds: ["song", "album", "playlist"].map(str::to_owned).to_vec(),
            rating_kinds: ["song", "album", "playlist", "station"]
                .map(str::to_owned)
                .to_vec(),
            favorite_kinds: ["song", "album", "artist", "playlist"]
                .map(str::to_owned)
                .to_vec(),
            ratings: true,
            dislike: true,
            favorites: true,
            collection_write: true,
            playlist_write: true,
            folders: true,
            stations: true,
            previews: true,
            autoplay: true,
        }
    }
    fn signed_in(&self, cfg: &Config) -> bool {
        !cfg.user_token.is_empty()
    }
    fn validate(&self, cfg: &Config) -> Result<()> {
        if cfg.developer().is_empty() {
            Err(err("no Apple Music developer token in this build"))
        } else {
            Ok(())
        }
    }
    fn expiry(&self, cfg: &Config) -> Option<i64> {
        cfg.expires()
    }
    fn account_key(&self, cfg: &Config) -> String {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(cfg.user_token.as_bytes()))
    }
    fn api(&self, cfg: &Config) -> Result<Arc<dyn Api>> {
        Ok(Arc::new(super::Client::new(
            cfg.developer(),
            &cfg.user_token,
        )))
    }
    fn playback(&self, cfg: &Config, version: &str) -> Result<PlaybackPage> {
        self.validate(cfg)?;
        let settings = json!({"developerToken": cfg.developer(), "userToken": cfg.user_token, "version": version});
        Ok(PlaybackPage {
            html: include_str!("player.html")
                .replace("__BRUMM_CFG__", &crate::engine::script_json(&settings)),
            refresh: Some(ScriptCall {
                method: "brumm.setToken",
                args: vec![Value::String(cfg.user_token.clone())],
            }),
        })
    }
    fn authenticate(&self, report: AuthReporter) -> Reply<'_, Result<()>> {
        Box::pin(super::auth::run(report))
    }
}
