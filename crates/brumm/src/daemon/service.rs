//! The composition point for the selected music service: its API client,
//! its playback page and, for Spotify, the session whose tokens the daemon
//! keeps fresh in that page.
use crate::config::Config;
use crate::engine::Page;
use crate::error::{Error, Result};
use crate::provider::{Api, apple, spotify};
use std::sync::Arc;

pub(super) struct Service {
    pub api: Arc<dyn Api>,
    pub session: Option<Arc<spotify::Session>>,
}

pub(super) fn new_service(cfg: &Config) -> Result<Service> {
    match cfg.effective_provider() {
        "apple" => Ok(Service {
            api: Arc::new(apple::Client::new(cfg.developer(), &cfg.user_token)),
            session: None,
        }),
        "spotify" => {
            let s = Arc::new(spotify::Session::new(
                &cfg.spotify_client_id,
                &cfg.spotify_refresh_token,
            ));
            Ok(Service {
                api: Arc::new(spotify::Client::new(s.clone())),
                session: Some(s),
            })
        }
        _ => Err(Error::Unsupported),
    }
}

impl Service {
    /// The token the playback page signs in with now.
    pub async fn token(&self, cfg: &Config) -> Result<String> {
        match &self.session {
            Some(s) => s.token().await,
            None => Ok(cfg.user_token.clone()),
        }
    }

    /// The playback page, with a current token.
    pub async fn page(&self, cfg: &Config) -> Result<Page> {
        match &self.session {
            Some(s) => Ok(Page::Spotify {
                access_token: s.token().await?,
            }),
            None => Ok(Page::Apple {
                developer_token: cfg.developer().to_string(),
                user_token: cfg.user_token.clone(),
            }),
        }
    }
}
