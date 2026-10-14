//! The last queue and song, kept across restarts so brumm opens on it —
//! paused, one press of play away.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config;
use crate::engine::Engine;
use crate::error::Result;
use crate::ipc::Player;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Resume {
    pub ids: Vec<String>,
    /// A station plays instead of ids.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub station: String,
    pub source: String,
    pub id: String,
    pub pos: f64,
    pub dur: f64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork: String,
    /// Continues playback after a restart for an update, which happens
    /// between songs while music plays.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub autoplay: bool,
    #[serde(skip)]
    pub(super) saved_at: Option<Instant>,
}

fn path() -> PathBuf {
    config::session_cache_dir().join("resume.json")
}

impl Resume {
    pub fn load() -> Option<Resume> {
        let b = std::fs::read(path()).ok()?;
        let r: Resume = serde_json::from_slice(&b).ok()?;
        r.playable().then_some(r)
    }

    pub fn save(&mut self) {
        self.saved_at = Some(Instant::now());
        let Ok(b) = serde_json::to_vec(self) else {
            return;
        };
        if std::fs::create_dir_all(config::session_cache_dir()).is_ok() {
            let _ = config::write_private(&path(), &b);
        }
    }

    /// Follows the player: the song, where it is, what it looks like.
    /// Writes to disk when the song changes and every few seconds.
    pub fn track(&mut self, es: &Player) {
        if es.id.is_empty() || es.preview.is_some() {
            return;
        }
        let changed = es.id != self.id;
        let moved = es.pos != self.pos;
        self.id.clone_from(&es.id);
        self.pos = es.pos;
        self.dur = es.dur;
        self.title.clone_from(&es.title);
        self.artist.clone_from(&es.artist);
        self.album.clone_from(&es.album);
        self.artwork.clone_from(&es.artwork);
        if !es.source.is_empty() {
            self.source.clone_from(&es.source);
        }
        // Paused: nothing to write.
        let stale = self
            .saved_at
            .is_none_or(|t| t.elapsed() > Duration::from_secs(5));
        if changed || (moved && stale) {
            self.save();
        }
    }

    pub fn playable(&self) -> bool {
        !self.station.is_empty() || (!self.id.is_empty() && !self.ids.is_empty())
    }

    /// Starts what was playing: the station, or the queue at the song and
    /// second it stopped.
    pub async fn play(&self, eng: &Engine) -> Result<()> {
        if !self.station.is_empty() {
            return eng.play_station(&self.station, &self.source).await;
        }
        eng.play_ids(&self.ids, &self.id, &self.source, self.pos)
            .await
    }

    /// The saved song on an idle player.
    pub fn overlay(&self, mut es: Player) -> Player {
        es.id.clone_from(&self.id);
        es.source.clone_from(&self.source);
        es.pos = self.pos;
        es.dur = self.dur;
        es.title.clone_from(&self.title);
        es.artist.clone_from(&self.artist);
        es.album.clone_from(&self.album);
        es.artwork.clone_from(&self.artwork);
        es
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::testenv;

    #[test]
    fn resume_follows_and_restores() {
        let Some(_env) = testenv::isolated() else {
            return;
        };
        assert!(Resume::load().is_none());
        let mut r = Resume {
            ids: vec!["a".into(), "b".into()],
            source: "lib:album:l.1".into(),
            ..Default::default()
        };
        r.track(&Player {
            id: "b".into(),
            title: "B".into(),
            pos: 42.0,
            dur: 200.0,
            ..Default::default()
        });
        let back = Resume::load().expect("saved on a song change");
        assert_eq!(
            (back.id.as_str(), back.pos, back.title.as_str()),
            ("b", 42.0, "B")
        );
        // A preview is not the song.
        r.track(&Player {
            id: "c".into(),
            preview: Some(Default::default()),
            ..Default::default()
        });
        assert_eq!(Resume::load().unwrap().id, "b");
        let shown = back.overlay(Player {
            ready: true,
            ..Default::default()
        });
        assert!(shown.ready && shown.title == "B" && shown.pos == 42.0);
        // Unplayable leftovers are not resumed.
        std::fs::write(path(), r#"{"ids":[],"id":"x"}"#).unwrap();
        assert!(Resume::load().is_none());
    }
}
