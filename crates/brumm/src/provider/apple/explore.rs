//! The explorer: any public API page as shelves, items and songs, with
//! every attribute kept as a detail so both players show the same facts.

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;
use serde_json::{Map, Value};

use super::text::{dedupe, encode_query, plain};
use super::{
    Client, Collection, ExplorePage, Item, KIND_SHELF, Resource, Shelf, Track, item, kind_of,
    nullable, tracks,
};
use crate::error::{Error, Result, err};

impl Client {
    /// Fetches one page; the players follow `next` on demand. Every
    /// server-provided href is validated before a token-bearing request is
    /// made.
    pub async fn explore(&self, route: &str) -> Result<ExplorePage> {
        valid_route(route)?;
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Meta {
            #[serde(deserialize_with = "nullable")]
            filters: BTreeMap<String, BTreeMap<String, Vec<Resource>>>,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Doc {
            #[serde(deserialize_with = "nullable")]
            data: Vec<Resource>,
            #[serde(deserialize_with = "nullable")]
            next: String,
            #[serde(deserialize_with = "nullable")]
            meta: Meta,
            #[serde(deserialize_with = "nullable")]
            results: BTreeMap<String, Value>,
        }
        let Some(mut doc) = self.get::<Doc>(route).await? else {
            return Err(err("Apple Music has no resource at this route"));
        };
        if doc.data.is_empty() {
            for filter in std::mem::take(&mut doc.meta.filters).into_values() {
                for resources in filter.into_values() {
                    doc.data.extend(resources);
                }
            }
        }
        self.hydrate_sparse(&mut doc.data).await?;
        let mut out = ExplorePage {
            next: doc.next,
            ..Default::default()
        };
        (out.items, out.tracks) = explore_mixed(&doc.data);
        if doc.data.len() == 1 {
            let r = &doc.data[0];
            out.title = attribute_string(&r.attributes, "name");
            out.details = attribute_details(&r.attributes);
            if r.relationships.len() + r.views.len() > 0
                && r.typ != "songs"
                && r.typ != "library-songs"
            {
                out.items.clear();
            }
            for group in [&r.views, &r.relationships] {
                for (name, coll) in group {
                    let (mut items, tracks) = explore_mixed(&coll.data);
                    if !coll.next.is_empty() {
                        items.push(more(&coll.next));
                    }
                    if ((items.is_empty() && tracks.is_empty()) || sparse(&coll.data))
                        && !coll.href.is_empty()
                    {
                        items = vec![Item {
                            kind: KIND_SHELF.into(),
                            id: coll.href.clone(),
                            name: human_label(name),
                            route: coll.href.clone(),
                            catalog: true,
                            ..Default::default()
                        }];
                    }
                    if items.len() + tracks.len() > 0 {
                        out.shelves.push(Shelf {
                            title: collection_title(name, &coll.attributes),
                            items,
                            tracks,
                            ..Default::default()
                        });
                    }
                }
            }
        }
        for (name, raw) in &doc.results {
            let groups: Vec<Collection> =
                match serde_json::from_value::<Vec<Collection>>(raw.clone()) {
                    Ok(g) => g,
                    Err(_) => match serde_json::from_value::<Collection>(raw.clone()) {
                        Ok(single) => vec![single],
                        Err(_) => continue,
                    },
                };
            for group in groups {
                let (mut items, tracks) = explore_mixed(&group.data);
                if !group.next.is_empty() {
                    items.push(more(&group.next));
                }
                let mut title = group.name.clone();
                if title.is_empty() {
                    title = attribute_string(&group.attributes, "name");
                }
                if title.is_empty() {
                    title = human_label(name);
                }
                out.shelves.push(Shelf {
                    title,
                    items,
                    tracks,
                    ..Default::default()
                });
            }
        }
        Ok(out)
    }

    /// Sparse relationships (notably Replay songs) contain only
    /// identifiers. They are hydrated in batches, rather than shown as
    /// blank playable rows.
    async fn hydrate_sparse(&self, rs: &mut [Resource]) -> Result<()> {
        fn collect(rs: &[Resource], groups: &mut BTreeMap<String, Vec<String>>) {
            for r in rs {
                if r.attributes.is_empty()
                    && matches!(
                        r.typ.strip_prefix("library-").unwrap_or(&r.typ),
                        "songs" | "albums" | "artists" | "playlists" | "music-videos" | "stations"
                    )
                {
                    groups.entry(r.typ.clone()).or_default().push(r.id.clone());
                }
                for c in r.relationships.values().chain(r.views.values()) {
                    collect(&c.data, groups);
                }
            }
        }
        fn fill(rs: &mut [Resource], resolved: &HashMap<String, Resource>) {
            for r in rs {
                if r.attributes.is_empty()
                    && let Some(full) = resolved.get(&format!("{}:{}", r.typ, r.id))
                {
                    r.attributes = full.attributes.clone();
                    if r.href.is_empty() {
                        r.href = full.href.clone();
                    }
                }
                for c in r.relationships.values_mut().chain(r.views.values_mut()) {
                    fill(&mut c.data, resolved);
                }
            }
        }
        let mut groups = BTreeMap::new();
        collect(rs, &mut groups);
        if groups.is_empty() {
            return Ok(());
        }
        let mut resolved = HashMap::new();
        for (typ, ids) in groups {
            for part in dedupe(ids).chunks(50) {
                let path = match typ.strip_prefix("library-") {
                    Some(bare) => format!("/v1/me/library/{bare}"),
                    None => self.catalog_path(&format!("/{typ}")).await,
                };
                let q = encode_query(&[("ids".into(), part.join(","))]);
                match self.get::<Collection>(&format!("{path}?{q}")).await {
                    Ok(Some(doc)) => {
                        for r in doc.data {
                            resolved.insert(format!("{}:{}", r.typ, r.id), r);
                        }
                    }
                    Ok(None) => {}
                    Err(Error::Unauthorized) => return Err(Error::Unauthorized),
                    Err(_) => {}
                }
            }
        }
        fill(rs, &resolved);
        Ok(())
    }
}

fn more(next: &str) -> Item {
    Item {
        kind: KIND_SHELF.into(),
        id: next.into(),
        name: "More…".into(),
        route: next.into(),
        catalog: true,
        ..Default::default()
    }
}

/// Checks that a route names a public API resource of this account, and
/// nothing else a token could be sent to.
pub fn valid_route(route: &str) -> Result<()> {
    let bad = || err("invalid public Apple Music route");
    if route.contains('#') || route.starts_with("//") {
        return Err(bad());
    }
    let raw = route.split('?').next().unwrap_or("");
    // A scheme: an absolute address.
    if let Some(i) = raw.find(':')
        && !raw[..i].contains('/')
    {
        return Err(bad());
    }
    let path = percent_decode(raw).ok_or_else(bad)?;
    let allowed = path.starts_with("/v1/catalog/")
        || path.starts_with("/v1/me/")
        || path == "/v1/storefronts"
        || path.starts_with("/v1/storefronts/");
    if path.contains("..") || path.contains('\\') || !allowed {
        return Err(bad());
    }
    Ok(())
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(h, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub(crate) fn explore_mixed(rs: &[Resource]) -> (Vec<Item>, Vec<Track>) {
    let mut items_out = Vec::new();
    let mut tracks_out = Vec::new();
    for r in rs {
        if r.typ.ends_with("-period-summaries") {
            for rel in r.relationships.values() {
                let (its, ts) = explore_mixed(&rel.data);
                items_out.extend(its);
                tracks_out.extend(ts);
            }
            continue;
        }
        let details = attribute_details(&r.attributes);
        if r.typ == "songs" || r.typ == "library-songs" {
            if let Some(mut t) = tracks(std::slice::from_ref(r)).into_iter().next() {
                if t.title.is_empty() {
                    t.title = format!("Unavailable song ({})", r.id);
                }
                t.details = details;
                t.favorite = r
                    .attributes
                    .get("inFavorites")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                tracks_out.push(t);
            }
            continue;
        }
        let (kind, catalog) = kind_of(&r.typ).unwrap_or_else(|| {
            (
                r.typ.strip_prefix("library-").unwrap_or(&r.typ).to_string(),
                !r.typ.starts_with("library-"),
            )
        });
        let mut it = item(r, &kind, catalog);
        it.details = details;
        if it.name.is_empty() {
            it.name = human_label(&r.typ);
            let year = attribute_string(&r.attributes, "year");
            if !year.is_empty() {
                it.name = format!("Replay {year}");
            }
        }
        it.route = r.href.clone();
        if it.route.is_empty() {
            let id = super::text::path_escape(&r.id);
            it.route = if catalog {
                format!("/v1/catalog/{{storefront}}/{}/{id}", r.typ)
            } else {
                format!(
                    "/v1/me/library/{}/{id}",
                    r.typ.strip_prefix("library-").unwrap_or(&r.typ)
                )
            };
        }
        if r.typ == "storefronts" {
            it.route = format!("app:country:{}", r.id);
        }
        if r.typ == "genres" {
            it.route = format!("app:genre:{}", r.id);
        }
        if r.typ == "library-playlist-folders" {
            it.route = format!("{}/children", it.route.trim_end_matches("/children"));
        }
        items_out.push(it);
    }
    (items_out, tracks_out)
}

/// A number as Go's strconv.FormatFloat(f, 'f', -1, 64) writes it.
fn number(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        return i.to_string();
    }
    if let Some(u) = n.as_u64() {
        return u.to_string();
    }
    n.as_f64().map(|f| f.to_string()).unwrap_or_default()
}

/// A value as Go's fmt.Sprint writes it, for list entries.
fn sprint(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => number(n),
        Value::Bool(b) => b.to_string(),
        Value::Null => "<nil>".into(),
        Value::Array(a) => format!("[{}]", a.iter().map(sprint).collect::<Vec<_>>().join(" ")),
        Value::Object(m) => format!(
            "map[{}]",
            m.iter()
                .map(|(k, v)| format!("{k}:{}", sprint(v)))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
}

pub(crate) fn attribute_string(a: &Map<String, Value>, key: &str) -> String {
    match a.get(key) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => number(n),
        Some(v) => serde_json::to_string(v).unwrap_or_default(),
    }
}

pub(crate) fn attribute_details(attrs: &Map<String, Value>) -> Vec<super::Detail> {
    let mut keys: Vec<&String> = attrs.keys().collect();
    keys.sort();
    let mut out = Vec::new();
    for key in keys {
        if key == "artwork" || key == "playParams" || key == "previews" {
            continue;
        }
        let mut val = attribute_string(attrs, key);
        if val.is_empty() {
            continue;
        }
        match &attrs[key] {
            Value::Array(list) => val = list.iter().map(sprint).collect::<Vec<_>>().join(", "),
            Value::Object(nested) => {
                if let Some(Value::String(text)) = nested.get("standard") {
                    val = plain(text);
                }
            }
            _ => {}
        }
        out.push(super::Detail {
            label: human_label(key),
            value: val,
        });
    }
    out
}

pub(crate) fn human_label(s: &str) -> String {
    let label = match s {
        "inFavorites" => "Favorite",
        "hasLyrics" => "Lyrics available",
        "audioVariants" => "Audio variants",
        "composerName" => "Composer",
        "discNumber" => "Disc",
        "contentRating" => "Content rating",
        "genreNames" => "Genres",
        "isrc" => "ISRC",
        "upc" => "UPC",
        "isAppleDigitalMaster" => "Apple Digital Master",
        "music-videos" => "Music videos",
        "library-playlist-folders" => "Playlist folders",
        "station-genres" => "Radio genres",
        "apple-curators" => "Apple curators",
        "record-labels" => "Record labels",
        _ => "",
    };
    if !label.is_empty() {
        return label.to_string();
    }
    let s = s.replace('-', " ");
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev_lower = false;
    for c in s.chars() {
        if prev_lower && c.is_ascii_uppercase() {
            out.push(' ');
        }
        prev_lower = c.is_ascii_lowercase();
        out.push(c);
    }
    let mut chars = out.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().chain(chars).collect(),
        None => out,
    }
}

fn sparse(rs: &[Resource]) -> bool {
    !rs.is_empty()
        && rs
            .iter()
            .all(|r| r.attributes.is_empty() && r.relationships.is_empty() && r.views.is_empty())
}

fn collection_title(name: &str, attrs: &Map<String, Value>) -> String {
    match attrs.get("title").and_then(Value::as_str) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => human_label(name),
    }
}

#[cfg(test)]
mod tests {
    use super::super::testserver::fake_api;
    use super::*;

    #[test]
    fn route_validation() {
        for route in [
            "https://example.com/v1/me/library/songs",
            "//example.com/v1/me/library/songs",
            "/v1/me/../tokens",
            "/v1/me/%2e%2e/tokens",
            "/v1/catalog/de/songs#fragment",
            "/other",
        ] {
            assert!(valid_route(route).is_err(), "accepted {route}");
        }
        for route in [
            "/v1/me/library/playlist-folders?filter[identity]=playlistsroot",
            "/v1/catalog/de/genres",
            "/v1/storefronts",
        ] {
            assert!(valid_route(route).is_ok(), "rejected {route}");
        }
    }

    #[test]
    fn labels() {
        assert_eq!(human_label("inFavorites"), "Favorite");
        assert_eq!(human_label("releaseDateISO"), "Release Date ISO");
        assert_eq!(human_label("top-songs"), "Top songs");
    }

    #[tokio::test]
    async fn explore_keeps_videos_metadata_and_pagination() {
        let (api, _) = fake_api(vec![(
            "/v1/catalog/de/music-videos",
            r#"{"data":[{"id":"v1","type":"music-videos","href":"/v1/catalog/de/music-videos/v1","attributes":{"name":"Video","hasLyrics":true,"composerName":"Composer","audioVariants":["lossless"]}}],"next":"/v1/catalog/de/music-videos?offset=1"}"#,
        )])
        .await;
        let page = api.explore("/v1/catalog/de/music-videos").await.unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].kind, "music-videos");
        assert_eq!(page.items[0].route, "/v1/catalog/de/music-videos/v1");
        assert_eq!(
            page.items[0].details.len(),
            4,
            "{:?}",
            page.items[0].details
        );
        assert_eq!(page.next, "/v1/catalog/de/music-videos?offset=1");
        assert!(api.explore("https://evil.example/v1/me").await.is_err());
    }
}
