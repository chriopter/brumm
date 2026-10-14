//! What the players browse, as the daemon hands it to them.

use serde::{Deserialize, Deserializer, Serialize};

// Kinds of containers.
pub const KIND_PLAYLIST: &str = "playlist";
pub const KIND_ALBUM: &str = "album";
pub const KIND_ARTIST: &str = "artist";
/// Plays, never opens.
pub const KIND_STATION: &str = "station";
/// A page of shelves: charts, a recommendation.
pub const KIND_SHELF: &str = "shelf";
/// A search suggestion.
pub const KIND_TERM: &str = "term";
pub const KIND_VIDEO: &str = "music-videos";
pub const KIND_FOLDER: &str = "playlist-folders";

// Ratings.
pub const LOVE: i64 = 1;
pub const DISLIKE: i64 = -1;

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero(i: &i64) -> bool {
    *i == 0
}

/// An API attribute kept as it is, so both players show the same facts
/// without a field for each.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Detail {
    pub label: String,
    pub value: String,
}

/// A container the user can open: a playlist, an album or an artist.
/// Catalog items (from search) carry `catalog` so their children come from
/// the catalog instead of the library.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Item {
    pub kind: String,
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub artist: String,
    #[serde(skip_serializing_if = "is_false")]
    pub catalog: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub artwork: String,
    /// A one-line summary: year, label, song count, audio quality.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub info: String,
    /// An editorial blurb or a playlist's description.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// A playlist songs can be added to.
    #[serde(skip_serializing_if = "is_false")]
    pub editable: bool,
    /// An opaque provider navigation handle.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub route: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<Detail>,
    #[serde(skip_serializing_if = "is_false")]
    pub favorite: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(rename = "previewUrl", skip_serializing_if = "String::is_empty")]
    pub preview_url: String,
}

impl Item {
    /// Identifies an item across kinds and sources.
    pub fn key(&self) -> String {
        format!(
            "{}:{}:{}",
            if self.catalog { "cat" } else { "lib" },
            self.kind,
            self.id
        )
    }
}

/// One playable song. The active provider interprets its opaque ID.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Seconds.
    pub duration: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub artwork: String,
    /// Position on its album.
    #[serde(skip_serializing_if = "is_zero")]
    pub number: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<Detail>,
    #[serde(skip_serializing_if = "is_false")]
    pub favorite: bool,
}

/// A titled row of a page: Home, an artist, the charts, a search.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Shelf {
    /// Opaque handle for opening the complete row.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub route: String,
    pub title: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Item>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<Track>,
}

impl Shelf {
    pub fn titled(title: &str) -> Shelf {
        Shelf {
            title: title.to_string(),
            ..Default::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.tracks.is_empty()
    }
}

/// One page of the explorer: a resource's facts, its shelves, and where
/// the list goes on.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExplorePage {
    /// Optional artwork rows to load on Explore, supplied by the provider.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub visuals: Vec<Item>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub info: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<Detail>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shelves: Vec<Shelf>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Item>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<Track>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub next: String,
}

/// Names a rated thing: a kind ("song", or an Item kind) and an id.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Ref {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub kind: String,
    pub id: String,
}

/// A null where a value belongs is the empty value, as Go's decoder left it.
pub(crate) fn nullable<'de, D: Deserializer<'de>, T: Default + Deserialize<'de>>(
    d: D,
) -> std::result::Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}
