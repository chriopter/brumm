//! The library cache: everything the players browse — the library lists,
//! each playlist's and album's tracks — in memory and on disk, so lists
//! open at once and the network only refreshes them.
//!
//! Keys are a list name (ipc::LIST_PLAYLISTS, …) or an Item's key().
//! Each music service has a cache of its own.

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
    /// Counts switches and clears, so a write meant for the data before
    /// one never lands in the data after it.
    #[serde(skip)]
    generation: u64,
}

#[derive(Default)]
pub struct Library {
    data: Mutex<Data>,
    /// Whose cache this is: a music service's id.
    provider: Mutex<String>,
}

fn path(provider: &str) -> PathBuf {
    config::service_cache("library", provider)
}

fn read(provider: &str) -> Data {
    std::fs::read(path(provider))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

impl Library {
    pub fn load(provider: &str) -> Library {
        Library {
            data: Mutex::new(read(provider)),
            provider: Mutex::new(provider.to_string()),
        }
    }

    /// Becomes another service's cache. The service stays locked until its
    /// data is in, so a save never writes one service's data as the other's.
    pub fn switch(&self, provider: &str) {
        let mut p = self.provider.lock().unwrap();
        self.replace(read(provider));
        *p = provider.to_string();
    }

    /// Empties the cache: another account of the same service.
    pub fn clear(&self) {
        self.replace(Data::default());
    }

    fn replace(&self, mut data: Data) {
        let mut d = self.data.lock().unwrap();
        data.generation = d.generation + 1;
        *d = data;
    }

    /// Which data this is: read it before checking that the service is
    /// still current, then write with set_*_if.
    pub fn generation(&self) -> u64 {
        self.data.lock().unwrap().generation
    }

    /// set_items, unless the data was switched or cleared since generation.
    pub fn set_items_if(&self, generation: u64, key: &str, v: Vec<Item>) {
        let mut d = self.data.lock().unwrap();
        if d.generation == generation {
            d.items.insert(key.to_string(), v);
        }
    }

    /// set_tracks, unless the data was switched or cleared since generation.
    pub fn set_tracks_if(&self, generation: u64, key: &str, v: Vec<Track>) {
        let mut d = self.data.lock().unwrap();
        if d.generation == generation {
            d.tracks.insert(key.to_string(), v);
        }
    }

    pub fn save(&self) -> Result<()> {
        let p = self.provider.lock().unwrap(); // held until written: see switch
        let b = serde_json::to_vec(&*self.data.lock().unwrap())?;
        std::fs::create_dir_all(config::cache_dir())?;
        config::write_private(&path(&p), &b)?;
        Ok(())
    }

    pub fn items(&self, key: &str) -> Option<Vec<Item>> {
        self.data.lock().unwrap().items.get(key).cloned()
    }

    pub fn tracks(&self, key: &str) -> Option<Vec<Track>> {
        self.data.lock().unwrap().tracks.get(key).cloned()
    }

    /// Unguarded: the daemon writes with set_items_if.
    #[cfg(test)]
    pub fn set_items(&self, key: &str, v: Vec<Item>) {
        self.data.lock().unwrap().items.insert(key.to_string(), v);
    }

    /// Unguarded: the daemon writes with set_tracks_if.
    #[cfg(test)]
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
        let l = Library::load("");
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
        assert_eq!(
            std::fs::metadata(config::cache_dir().join("library.json"))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
        let again = Library::load("apple");
        assert_eq!(again.items("playlists").unwrap()[0].name, "Mix");
        assert_eq!(again.tracks("lib:album:l.1").unwrap()[0].title, "Song");
        // Spotify's cache is its own; switching back finds Apple's again.
        again.switch("spotify");
        assert!(again.items("playlists").is_none());
        again.set_items("albums", vec![]);
        again.save().unwrap();
        assert!(config::cache_dir().join("library-spotify.json").exists());
        again.switch("apple");
        assert_eq!(again.items("playlists").unwrap()[0].name, "Mix");
        // A cache written by the Go daemon reads the same.
        std::fs::write(
            path(""),
            r#"{"items":{"albums":[{"kind":"album","id":"l.9","name":"Blue"}]},"tracks":null}"#,
        )
        .unwrap();
        assert_eq!(Library::load("").items("albums").unwrap()[0].id, "l.9");
        // A write meant for the data before a switch does not land after it.
        let l = Library::load("apple");
        let g = l.generation();
        l.switch("spotify");
        l.set_items_if(g, "playlists", vec![Item::default()]);
        assert!(
            l.items("playlists").is_none(),
            "Apple's write in Spotify's cache"
        );
        l.set_items_if(l.generation(), "playlists", vec![Item::default()]);
        assert!(l.items("playlists").is_some());
    }
}
