use crate::music::{Detail, Item, Track};
use serde_json::Value;
use std::collections::HashMap;
#[derive(Clone)]
pub(super) struct Document {
    resources: HashMap<(String, String), Value>,
}
fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or("").into()
}
impl Document {
    pub fn data(value: &Value) -> Vec<&Value> {
        match &value["data"] {
            Value::Array(a) => a.iter().collect(),
            Value::Object(_) => vec![&value["data"]],
            _ => vec![],
        }
    }
    pub fn new(value: &Value) -> Self {
        let mut resources = HashMap::new();
        for resource in Self::data(value)
            .into_iter()
            .chain(value["included"].as_array().into_iter().flatten())
        {
            resources.insert(
                (text(resource, "type"), text(resource, "id")),
                resource.clone(),
            );
        }
        Self { resources }
    }
    pub fn expand<'a>(&'a self, resource: &'a Value) -> &'a Value {
        self.resources
            .get(&(text(resource, "type"), text(resource, "id")))
            .unwrap_or(resource)
    }
    pub fn extend(&mut self, value: &Value) {
        self.resources.extend(Self::new(value).resources);
    }
    pub fn related<'a>(&'a self, resource: &'a Value, name: &str) -> Vec<&'a Value> {
        Self::data(&resource["relationships"][name])
            .into_iter()
            .map(|r| self.expand(r))
            .collect()
    }
    fn artists(&self, resource: &Value) -> String {
        self.related(resource, "artists")
            .iter()
            .map(|r| text(&r["attributes"], "name"))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    }
    fn artwork(&self, resource: &Value) -> String {
        for relation in ["coverArt", "profileArt"] {
            for artwork in self.related(resource, relation) {
                if let Some(files) = artwork["attributes"]["files"].as_array()
                    && let Some(file) = files
                        .iter()
                        .filter(|f| f["href"].as_str().is_some())
                        .min_by_key(|f| (f["meta"]["width"].as_i64().unwrap_or(640) - 640).abs())
                {
                    return text(file, "href");
                }
            }
        }
        String::new()
    }
    pub fn track(&self, resource: &Value) -> Option<Track> {
        let resource = self.expand(resource);
        let a = &resource["attributes"];
        let title = text(a, "title");
        let id = text(resource, "id");
        if title.is_empty() || id.is_empty() {
            return None;
        }
        let album = self.related(resource, "albums").first().copied();
        let mut details = Vec::new();
        if a["explicit"] == true {
            details.push(Detail {
                label: "Content".into(),
                value: "Explicit".into(),
            });
        }
        Some(Track {
            id,
            title,
            artist: self.artists(resource),
            album: album
                .map(|v| text(&v["attributes"], "title"))
                .unwrap_or_default(),
            artwork: album.map(|v| self.artwork(v)).unwrap_or_default(),
            duration: duration(&text(a, "duration")),
            number: a["trackNumber"].as_i64().unwrap_or(0),
            details,
            ..Default::default()
        })
    }
    pub fn item(&self, resource: &Value, account: &str) -> Option<Item> {
        let resource = self.expand(resource);
        let a = &resource["attributes"];
        let kind = match resource["type"].as_str()? {
            "albums" => "album",
            "artists" => "artist",
            "playlists" => "playlist",
            _ => return None,
        };
        let name = if kind == "album" {
            text(a, "title")
        } else {
            text(a, "name")
        };
        let id = text(resource, "id");
        if name.is_empty() || id.is_empty() {
            return None;
        }
        let editable = kind == "playlist"
            && self
                .related(resource, "owners")
                .iter()
                .any(|r| r["id"] == account);
        Some(Item {
            kind: kind.into(),
            id: id.clone(),
            name,
            artist: self.artists(resource),
            catalog: true,
            artwork: self.artwork(resource),
            note: text(a, "description"),
            info: text(a, "releaseDate"),
            editable,
            url: format!("https://tidal.com/browse/{kind}/{id}"),
            ..Default::default()
        })
    }
}
/// Tidal durations are ISO 8601, including fractional seconds.
fn duration(value: &str) -> f64 {
    let Some(value) = value.strip_prefix("PT") else {
        return 0.0;
    };
    let mut total = 0.0;
    let mut number = String::new();
    for ch in value.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            number.push(ch);
        } else {
            let Ok(n) = number.parse::<f64>() else {
                return 0.0;
            };
            total += n * match ch {
                'H' => 3600.0,
                'M' => 60.0,
                'S' => 1.0,
                _ => return 0.0,
            };
            number.clear();
        }
    }
    if total.is_finite() && number.is_empty() {
        total
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn expands_relationships_and_preserves_track_identity() {
        let value = json!({"data":[{"type":"tracks","id":"1"}],"included":[
            {"type":"tracks","id":"1","attributes":{"title":"Song","duration":"PT1H2M3.5S","explicit":true},"relationships":{"artists":{"data":[{"type":"artists","id":"1"}]},"albums":{"data":[{"type":"albums","id":"a"}]}}},
            {"type":"artists","id":"1","attributes":{"name":"Artist"}},
            {"type":"albums","id":"a","attributes":{"title":"Album"},"relationships":{"coverArt":{"data":[{"type":"artworks","id":"art"}]}}},
            {"type":"artworks","id":"art","attributes":{"files":[{"href":"https://example.org/cover","meta":{"width":640}}]}}
        ]});
        let doc = Document::new(&value);
        let track = doc.track(&value["data"][0]).unwrap();
        assert_eq!(track.id, "1");
        assert_eq!(track.artist, "Artist");
        assert_eq!(track.album, "Album");
        assert_eq!(track.artwork, "https://example.org/cover");
        assert_eq!(track.duration, 3723.5);
        assert_eq!(track.details[0].value, "Explicit");
    }
    #[test]
    fn ownership_is_explicit_and_duration_rejects_malformed_input() {
        let value = json!({"data":{"type":"playlists","id":"p","attributes":{"name":"My Playlist"},"relationships":{"owners":{"data":[{"type":"users","id":"owner"}]}}}});
        let doc = Document::new(&value);
        assert!(doc.item(&value["data"], "owner").unwrap().editable);
        assert!(!doc.item(&value["data"], "someone-else").unwrap().editable);
        for input in ["PTfooS", "PT3", "P2D", "PTNaNS"] {
            assert_eq!(duration(input), 0.0);
        }
    }
}
