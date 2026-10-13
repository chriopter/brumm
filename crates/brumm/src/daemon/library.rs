//! The library cache: everything the players browse — the library lists,
//! each playlist's and album's tracks — in memory and on disk, so lists
//! open at once and the network only refreshes them.
//!
//! Keys are a list name (ipc::LIST_PLAYLISTS, …) or an Item's key().

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::apple::{Item, Track};
use crate::config;
use crate::error::Result;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Data {
    #[serde(deserialize_with = "crate::apple::nullable")]
    items: HashMap<String, Vec<Item>>,
    #[serde(deserialize_with = "crate::apple::nullable")]
    tracks: HashMap<String, Vec<Track>>,
}

#[derive(Default)]
pub struct Library {
    data: Mutex<Data>,
}

fn path() -> PathBuf {
    config::cache_dir().join("library.json")
}

impl Library {
    pub fn load() -> Library {
        let data = std::fs::read(path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Library {
            data: Mutex::new(data),
        }
    }

    pub fn save(&self) -> Result<()> {
        let b = serde_json::to_vec(&*self.data.lock().unwrap())?;
        std::fs::create_dir_all(config::cache_dir())?;
        config::write_private(&path(), &b)?;
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
