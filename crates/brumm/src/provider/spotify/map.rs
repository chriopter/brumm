//! Spotify's objects as brumm's documents.

use serde_json::Value;

use crate::music::{Detail, Item, KIND_ALBUM, KIND_ARTIST, KIND_PLAYLIST, KIND_STATION, Track};

pub(super) fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn i(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}

/// The widest image: Spotify lists them widest first, but does not
/// promise it.
pub(super) fn artwork(v: &Value) -> String {
    v.get("images")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .max_by_key(|im| im.get("width").and_then(Value::as_i64).unwrap_or(0))
        .map(|im| s(im, "url").to_string())
        .unwrap_or_default()
}

/// Its artists' names, as one line.
fn artists(v: &Value) -> String {
    v.get("artists")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|a| s(a, "name"))
        .filter(|n| !n.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn share(v: &Value) -> String {
    v.get("external_urls")
        .map(|u| s(u, "spotify").to_string())
        .unwrap_or_default()
}

/// A description as plain text: Spotify escapes it as HTML.
fn plain(t: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in t.chars() {
        match c {
            '<' => tag = true,
            '>' if tag => tag = false,
            c if !tag => out.push(c),
            _ => {}
        }
    }
    htmlize::unescape(out).trim().to_string()
}

/// A playable song; None for local files, episodes and removed tracks.
/// Album tracks come without their album: album and art fill it in.
pub(super) fn track(t: &Value, album: Option<&Item>) -> Option<Track> {
    if t.is_null()
        || t.get("is_local").and_then(Value::as_bool) == Some(true)
        || !matches!(s(t, "type"), "" | "track")
    {
        return None;
    }
    let id = s(t, "id");
    if id.is_empty() {
        return None;
    }
    let (album_name, art) = match t.get("album") {
        Some(a) if a.is_object() => (s(a, "name").to_string(), artwork(a)),
        _ => album
            .map(|a| (a.name.clone(), a.artwork.clone()))
            .unwrap_or_default(),
    };
    let mut details = Vec::new();
    if t.get("explicit").and_then(Value::as_bool) == Some(true) {
        details.push(Detail {
            label: "Explicit".into(),
            value: "yes".into(),
        });
    }
    Some(Track {
        id: id.to_string(),
        title: s(t, "name").to_string(),
        artist: artists(t),
        album: album_name,
        duration: i(t, "duration_ms") as f64 / 1000.0,
        artwork: art,
        number: i(t, "track_number"),
        details,
        favorite: false,
    })
}

pub(super) fn album(a: &Value, catalog: bool) -> Option<Item> {
    let id = s(a, "id");
    if id.is_empty() {
        return None;
    }
    let mut info = Vec::new();
    if let Some(y) = s(a, "release_date").get(..4) {
        info.push(y.to_string());
    }
    match s(a, "album_type") {
        "single" => info.push("Single".into()),
        "compilation" => info.push("Compilation".into()),
        _ => {}
    }
    match i(a, "total_tracks") {
        0 => {}
        1 => info.push("1 song".into()),
        n => info.push(format!("{n} songs")),
    }
    Some(Item {
        kind: KIND_ALBUM.into(),
        id: id.to_string(),
        name: s(a, "name").to_string(),
        artist: artists(a),
        catalog,
        artwork: artwork(a),
        info: info.join(" · "),
        url: share(a),
        ..Default::default()
    })
}

pub(super) fn artist(a: &Value, catalog: bool) -> Option<Item> {
    let id = s(a, "id");
    if id.is_empty() {
        return None;
    }
    Some(Item {
        kind: KIND_ARTIST.into(),
        id: id.to_string(),
        name: s(a, "name").to_string(),
        catalog,
        artwork: artwork(a),
        url: share(a),
        ..Default::default()
    })
}

/// A playlist the user owns or collaborates on opens to its songs. Spotify
/// lists no other playlist's songs to an app in development mode, so those
/// are stations here: they play as a whole and do not open.
pub(super) fn playlist(p: &Value, me: &str, catalog: bool) -> Option<Item> {
    let id = s(p, "id");
    if id.is_empty() {
        return None;
    }
    let owner = p.get("owner").unwrap_or(&Value::Null);
    let mine = !me.is_empty() && s(owner, "id") == me;
    let shared = p.get("collaborative").and_then(Value::as_bool) == Some(true);
    let by = match s(owner, "display_name") {
        "" => s(owner, "id"),
        n => n,
    };
    // `items` replaced `tracks` in 2026; either carries the count.
    let count = ["items", "tracks"].iter().find_map(|k| {
        p.get(*k)
            .and_then(|v| v.get("total"))
            .and_then(Value::as_i64)
    });
    let mut it = Item {
        name: s(p, "name").to_string(),
        artist: if mine { String::new() } else { by.to_string() },
        catalog,
        artwork: artwork(p),
        note: plain(s(p, "description")),
        url: share(p),
        ..Default::default()
    };
    if mine || shared {
        it.kind = KIND_PLAYLIST.into();
        it.id = id.to_string();
        it.editable = mine || shared;
        it.info = match count {
            Some(1) => "1 song".into(),
            Some(n) => format!("{n} songs"),
            None => String::new(),
        };
    } else {
        it.kind = KIND_STATION.into();
        it.id = format!("spotify:playlist:{id}");
        it.info = "Playlist: plays as a whole".into();
    }
    Some(it)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tracks_skip_what_cannot_play() {
        let t = json!({"type":"track","id":"t1","name":"Song","duration_ms":61000,"track_number":3,
            "artists":[{"name":"A"},{"name":"B"}],"album":{"name":"Al","images":[
            {"url":"small","width":64},{"url":"big","width":640}]},"explicit":true});
        let tr = track(&t, None).unwrap();
        assert_eq!(
            (
                tr.id.as_str(),
                tr.artist.as_str(),
                tr.album.as_str(),
                tr.artwork.as_str()
            ),
            ("t1", "A, B", "Al", "big")
        );
        assert_eq!((tr.duration, tr.number), (61.0, 3));
        assert!(track(&json!({"type":"track","id":null,"is_local":true}), None).is_none());
        assert!(track(&json!({"type":"episode","id":"e"}), None).is_none());
        assert!(track(&Value::Null, None).is_none());
        // An album's own tracks take the album from the item.
        let al = Item {
            name: "Blue".into(),
            artwork: "cover".into(),
            ..Default::default()
        };
        let tr = track(&json!({"id":"t2","name":"x"}), Some(&al)).unwrap();
        assert_eq!((tr.album.as_str(), tr.artwork.as_str()), ("Blue", "cover"));
    }

    #[test]
    fn only_own_playlists_open() {
        let own = json!({"id":"p1","name":"Mine","owner":{"id":"me"},"items":{"total":2}});
        let it = playlist(&own, "me", false).unwrap();
        assert_eq!((it.kind.as_str(), it.id.as_str()), (KIND_PLAYLIST, "p1"));
        assert!(it.editable);
        assert_eq!(it.info, "2 songs");
        let shared = json!({"id":"p2","name":"Ours","owner":{"id":"you"},"collaborative":true});
        assert_eq!(playlist(&shared, "me", false).unwrap().kind, KIND_PLAYLIST);
        let theirs = json!({"id":"p3","name":"Hits","owner":{"id":"spotify","display_name":"Spotify"},
            "description":"Best &amp; <a href=\"x\">newest</a>"});
        let it = playlist(&theirs, "me", true).unwrap();
        assert_eq!(
            (it.kind.as_str(), it.id.as_str(), it.artist.as_str()),
            (KIND_STATION, "spotify:playlist:p3", "Spotify")
        );
        assert_eq!(it.note, "Best & newest");
    }

    #[test]
    fn albums_say_year_kind_and_size() {
        let a = json!({"id":"a1","name":"EP","release_date":"2019-04-01","album_type":"single",
            "total_tracks":4,"artists":[{"name":"X"}],"external_urls":{"spotify":"https://open.spotify.com/album/a1"}});
        let it = album(&a, true).unwrap();
        assert_eq!(it.info, "2019 · Single · 4 songs");
        assert_eq!(it.url, "https://open.spotify.com/album/a1");
        assert!(it.catalog);
    }
}
