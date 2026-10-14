//! Tidal session composition: no service-specific branching in daemon/frontends.
use super::{Client, auth, device};
use crate::{
    config::Config,
    error::{Error, Result, err},
    provider::{Api, Reply, session::*},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub struct Tidal;

impl Definition for Tidal {
    fn cache_max_age(&self) -> Option<std::time::Duration> {
        Some(std::time::Duration::from_secs(86400))
    }
    fn id(&self) -> &'static str {
        "tidal"
    }
    fn navigation(&self) -> Vec<NavigationEntry> {
        let mut entries = vec![
            NavigationEntry::new(
                "home",
                "Home",
                "home",
                Destination::Page {
                    route: "tidal:home".into(),
                },
            ),
            NavigationEntry::new(
                "explore",
                "Explore",
                "explore",
                Destination::Page {
                    route: "tidal:explore".into(),
                },
            ),
        ];
        for (id, label, list, icon) in [
            ("playlists", "Playlists", "playlists", "playlists"),
            ("albums", "Albums", "albums", "albums"),
            ("artists", "Artists", "artists", "artists"),
            ("tracks", "Tracks", "songs", "songs"),
        ] {
            entries.push(NavigationEntry::new(
                id,
                label,
                icon,
                Destination::Library { list: list.into() },
            ));
        }
        entries
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            preference: Some(Preference {
                label: "Favorite".into(),
                remove_label: "Remove from My Collection".into(),
                collection_equivalent: true,
                removable: true,
            }),
            favorites: true,
            favorite_kinds: ["song", "album", "artist", "playlist"]
                .map(str::to_owned)
                .to_vec(),
            playlist_write: true,
            ..Default::default()
        }
    }
    fn signed_in(&self, cfg: &Config) -> bool {
        cfg.tidal.signed_in()
    }
    fn validate(&self, cfg: &Config) -> Result<()> {
        if cfg.tidal.client_id() != device::client_id() {
            return Err(err(
                "This Tidal session uses a different client; connect again",
            ));
        }
        if cfg.tidal.signed_in()
            && (cfg.tidal.country.len() != 2
                || !cfg.tidal.country.chars().all(|c| c.is_ascii_uppercase()))
        {
            return Err(err("Tidal account country is missing; connect again"));
        }
        Ok(())
    }
    fn expiry(&self, _cfg: &Config) -> Option<i64> {
        None
    }
    fn account_key(&self, cfg: &Config) -> String {
        format!("{:x}", Sha256::digest(cfg.tidal.user_id.as_bytes()))
    }
    fn api(&self, cfg: &Config) -> Result<Arc<dyn Api>> {
        Ok(Arc::new(Client::new(&cfg.tidal)?))
    }
    fn playback(&self, cfg: &Config, version: &str) -> Result<PlaybackPage> {
        self.validate(cfg)?;
        let settings = json!({"version": version, "quality": "HIGH", "normalization": "TRACK"});
        Ok(PlaybackPage {
            html: include_str!("player.html")
                .replace("__BRUMM_CFG__", &crate::engine::script_json(&settings)),
            refresh: Some(ScriptCall {
                method: "brumm.setToken",
                args: vec![],
            }),
            resources: PlaybackResources {
                assets: vec![
                    PlaybackAsset {
                        name: "transport.js",
                        content_type: "text/javascript; charset=utf-8",
                        body: include_bytes!("transport.js"),
                    },
                    PlaybackAsset {
                        name: "sdk.js",
                        content_type: "text/javascript; charset=utf-8",
                        body: include_bytes!("player.js"),
                    },
                ],
                bridge: Some(Arc::new(Bridge {
                    client: Client::new(&cfg.tidal)?,
                    account: cfg.tidal.user_id.clone(),
                    media_limit: Arc::new(tokio::sync::Semaphore::new(4)),
                })),
            },
        })
    }
    fn authenticate(&self, report: AuthReporter) -> Reply<'_, Result<()>> {
        Box::pin(auth::run(report))
    }
}

struct Bridge {
    client: Client,
    account: String,
    media_limit: Arc<tokio::sync::Semaphore>,
}
fn media_url(target: &str) -> Result<url::Url> {
    use base64::Engine;
    if target.len() > 16384 {
        return Err(Error::Unsupported);
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(target)
        .map_err(|_| Error::Unsupported)?;
    let raw = std::str::from_utf8(&bytes).map_err(|_| Error::Unsupported)?;
    let url = url::Url::parse(raw).map_err(|_| Error::Unsupported)?;
    if url.scheme() != "https"
        || url.host_str() != Some("sp-ad-fa.audio.tidal.com")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Unsupported);
    }
    Ok(url)
}
impl PlaybackBridge for Bridge {
    fn media<'a>(
        &'a self,
        target: &'a str,
        range: Option<&'a str>,
    ) -> Reply<'a, Result<PlaybackMedia>> {
        Box::pin(async move {
            let url = media_url(target)?;
            let _permit = self
                .media_limit
                .acquire()
                .await
                .map_err(|_| Error::Unsupported)?;
            let owns_session = || -> Result<()> {
                let cfg = Config::load()?;
                if cfg.effective_provider() != "tidal" || cfg.tidal.user_id != self.account {
                    return Err(Error::Unauthorized);
                }
                Ok(())
            };
            owns_session()?;
            let mut request = self.client.http.get(url);
            if let Some(range) = range {
                if !range.starts_with("bytes=")
                    || range.len() > 80
                    || !range[6..].chars().all(|c| c.is_ascii_digit() || c == '-')
                {
                    return Err(Error::Unsupported);
                }
                request = request.header("Range", range);
            }
            // The SDK owns the signed URL, manifest, decoding, DRM and queue.
            // This native transport forwards no Origin, cookie or OAuth token.
            let mut response = request.send().await?;
            if !matches!(response.status().as_u16(), 200 | 206) {
                return Err(err("Tidal media delivery failed"));
            }
            let status = response.status().as_u16();
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_owned();
            let content_range = response
                .headers()
                .get("content-range")
                .and_then(|h| h.to_str().ok())
                .map(str::to_owned);
            const MAX_MEDIA: usize = 32 << 20;
            if response
                .content_length()
                .is_some_and(|n| n > MAX_MEDIA as u64)
            {
                return Err(err("Tidal media response exceeded its limit"));
            }
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                if body.len() + chunk.len() > MAX_MEDIA {
                    return Err(err("Tidal media response exceeded its limit"));
                }
                body.extend_from_slice(&chunk);
            }
            owns_session()?;
            Ok(PlaybackMedia {
                status,
                content_type,
                content_range,
                body,
            })
        })
    }
    fn request<'a>(&'a self, route: &'a str, body: Value) -> Reply<'a, Result<Value>> {
        Box::pin(async move {
            match route {
                "credentials" => auth::credentials(&self.account, body["rejected"].as_str())
                    .await
                    .map(|s| auth::sdk_credentials(&s)),
                "tracks" => {
                    let ids = body["ids"]
                        .as_array()
                        .ok_or_else(|| err("track IDs are required"))?;
                    if ids.len() > 20 {
                        return Err(err("too many tracks"));
                    }
                    let ids = ids
                        .iter()
                        .map(|id| {
                            id.as_str()
                                .filter(|s| !s.is_empty())
                                .map(str::to_owned)
                                .ok_or_else(|| err("invalid track ID"))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    Ok(json!(self.client.player_tracks(&ids).await?))
                }
                _ => Err(Error::Unsupported),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_media_transport_is_restricted_to_the_verified_https_cdn() {
        use base64::Engine;
        let encode = |raw: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
        assert!(
            media_url(&encode(
                "https://sp-ad-fa.audio.tidal.com/media.m4a?token=fictional"
            ))
            .is_ok()
        );
        for raw in [
            "http://sp-ad-fa.audio.tidal.com/a",
            "https://sp-ad-fa.audio.tidal.com:8443/a",
            "https://sp-ad-fa.audio.tidal.com.evil.example/a",
            "https://127.0.0.1/a",
            "https://example.com/a",
            "https://user@sp-ad-fa.audio.tidal.com/a",
            "https://sp-ad-fa.audio.tidal.com/a#fragment",
        ] {
            assert!(media_url(&encode(raw)).is_err(), "accepted {raw}");
        }
        assert!(media_url("not base64!").is_err());
    }
    #[test]
    fn definition_owns_tidal_navigation_and_keeps_credentials_private() {
        let mut cfg = Config {
            provider: "tidal".into(),
            tidal: auth::Session {
                user_id: "user-one".into(),
                country: "GB".into(),
                access_token: "private-access".into(),
                refresh_token: "private-refresh".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let key = Tidal.account_key(&cfg);
        cfg.tidal.access_token = "rotated-private-access".into();
        assert_eq!(key, Tidal.account_key(&cfg));
        let nav = Tidal.navigation();
        assert_eq!(
            nav.iter()
                .map(|entry| entry.label.as_str())
                .collect::<Vec<_>>(),
            [
                "Home",
                "Explore",
                "Playlists",
                "Albums",
                "Artists",
                "Tracks"
            ]
        );
        assert!(matches!(&nav[5].destination, Destination::Library { list } if list == "songs"));
        let descriptor =
            serde_json::to_string(&crate::provider::registry::selection(&cfg)).unwrap();
        assert!(!descriptor.contains("private-"));
        let page = Tidal.playback(&cfg, "fixture").unwrap();
        assert!(!page.html.contains("private-"));
        assert!(page.resources.bridge.is_some());
        let caps = Tidal.capabilities();
        assert!(caps.preference.unwrap().collection_equivalent);
        assert!(!caps.ratings && !caps.collection_write && !caps.stations && !caps.folders);
    }
    #[test]
    fn a_tidal_session_never_imports_legacy_apple_ids() {
        let Some(_env) = crate::config::testenv::isolated() else {
            return;
        };
        std::fs::create_dir_all(crate::config::cache_dir()).unwrap();
        std::fs::write(
            crate::config::cache_dir().join("library.json"),
            br#"{"items":{"albums":[{"id":"apple-only"}]}}"#,
        )
        .unwrap();
        let cfg = Config {
            provider: "tidal".into(),
            tidal: auth::Session {
                user_id: "tidal-user".into(),
                access_token: "fictional".into(),
                country: "GB".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        cfg.save().unwrap();
        crate::config::migrate_session_cache().unwrap();
        assert!(
            !crate::config::session_cache_dir()
                .join("library.json")
                .exists()
        );
        assert!(crate::config::cache_dir().join("library.json").exists());
    }
}
