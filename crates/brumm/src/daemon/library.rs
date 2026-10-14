//! The library cache: everything the players browse — the library lists,
//! each playlist's and album's tracks — in memory and on disk, so lists
//! open at once and the network only refreshes them.
//!
//! Keys are a list name (ipc::LIST_PLAYLISTS, …) or an Item's key().

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config;
use crate::error::Result;
use crate::music::{Item, Track};

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Data {
    #[serde(deserialize_with = "crate::music::nullable")]
    items: HashMap<String, Vec<Item>>,
    #[serde(deserialize_with = "crate::music::nullable")]
    tracks: HashMap<String, Vec<Track>>,
}

pub struct Library {
    data: Mutex<Data>,
    path: Mutex<PathBuf>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            data: Mutex::new(Data::default()),
            path: Mutex::new(path()),
        }
    }
}

fn path() -> PathBuf {
    config::session_cache_dir().join("library.json")
}

impl Library {
    pub fn namespace(&self) -> PathBuf {
        self.path
            .lock()
            .unwrap()
            .parent()
            .expect("library parent")
            .to_owned()
    }
    pub fn clear(&self) {
        *self.data.lock().unwrap() = Data::default();
        *self.path.lock().unwrap() = path();
    }
    pub fn reload(&self) {
        let loaded = Self::load();
        *self.data.lock().unwrap() = loaded.data.into_inner().unwrap();
        *self.path.lock().unwrap() = loaded.path.into_inner().unwrap();
    }
    pub fn load() -> Library {
        let path = path();
        let data = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Library {
            data: Mutex::new(data),
            path: Mutex::new(path),
        }
    }

    pub fn save(&self) -> Result<()> {
        let b = serde_json::to_vec(&*self.data.lock().unwrap())?;
        let path = self.path.lock().unwrap();
        std::fs::create_dir_all(path.parent().expect("library parent"))?;
        config::write_private(&path, &b)?;
        Ok(())
    }

    pub fn items(&self, key: &str) -> Option<Vec<Item>> {
        self.data.lock().unwrap().items.get(key).cloned()
    }

    pub fn tracks(&self, key: &str) -> Option<Vec<Track>> {
        self.data.lock().unwrap().tracks.get(key).cloned()
    }

    pub fn set_items(&self, key: &str, v: Vec<Item>) {
        self.data.lock().unwrap().items.insert(key.to_string(), v);
    }

    pub fn set_tracks(&self, key: &str, v: Vec<Track>) {
        self.data.lock().unwrap().tracks.insert(key.to_string(), v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::testenv;
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn a_cache_keeps_its_account_path_after_selection_changes() {
        let Some(_env) = testenv::isolated() else {
            return;
        };
        let mut cfg = config::Config {
            provider: "apple".into(),
            user_token: "account-a".into(),
            ..Default::default()
        };
        cfg.save().unwrap();
        let old = Library::load();
        old.set_items(
            "albums",
            vec![Item {
                id: "only-a".into(),
                ..Default::default()
            }],
        );
        cfg.user_token = "account-b".into();
        cfg.save().unwrap();
        old.save().unwrap();
        assert!(
            Library::load().items("albums").is_none(),
            "late writes must not enter account B"
        );
        cfg.user_token = "account-a".into();
        cfg.save().unwrap();
        assert_eq!(Library::load().items("albums").unwrap()[0].id, "only-a");
    }

    #[test]
    fn cache_persists_privately() {
        let Some(_env) = testenv::isolated() else {
            return;
        };
        let l = Library::load();
        assert!(l.items("playlists").is_none());
        l.set_items(
            "playlists",
            vec![Item {
                kind: "playlist".into(),
                id: "p.1".into(),
                name: "Mix".into(),
                ..Default::default()
            }],
        );
        l.set_tracks(
            "lib:album:l.1",
            vec![Track {
                id: "i.1".into(),
                title: "Song".into(),
                ..Default::default()
            }],
        );
        l.save().unwrap();
        assert_eq!(std::fs::metadata(path()).unwrap().mode() & 0o777, 0o600);
        let again = Library::load();
        assert_eq!(again.items("playlists").unwrap()[0].name, "Mix");
        assert_eq!(again.tracks("lib:album:l.1").unwrap()[0].title, "Song");
        // A cache written by the Go daemon reads the same.
        std::fs::write(
            path(),
            r#"{"items":{"albums":[{"kind":"album","id":"l.9","name":"Blue"}]},"tracks":null}"#,
        )
        .unwrap();
        assert_eq!(Library::load().items("albums").unwrap()[0].id, "l.9");
    }
}
