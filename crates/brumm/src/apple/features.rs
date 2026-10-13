//! Named pages (app:…) resolved to public API requests, and the few edits
//! Apple allows: new playlists and folders, favorites.

use std::collections::BTreeMap;

use reqwest::Method;
use serde_json::json;

use super::browse::feature_link;
use super::text::{dedupe, encode_query, query_escape};
use super::{
    Client, ExplorePage, KIND_ALBUM, KIND_ARTIST, KIND_FOLDER, KIND_PLAYLIST, Ref, Resource, item,
    mixed,
};
use crate::error::{Result, err};

impl Client {
    /// Resolves named pages to public API requests. The same pages are
    /// consumed by both players; only actions need a player's own form.
    pub async fn feature(&self, route: &str) -> Result<ExplorePage> {
        let mut route = route.to_string();
        match route.as_str() {
            "app:discover" => {
                return Ok(ExplorePage {
                    title: "Explore".into(),
                    items: vec![
                        feature_link("Genres", &self.catalog_path("/genres").await),
                        feature_link("Radio genres", &self.catalog_path("/station-genres").await),
                        feature_link("Charts", "app:charts"),
                        feature_link(
                            "Music videos",
                            &self.catalog_path("/charts?types=music-videos").await,
                        ),
                        feature_link("Recommendations", "/v1/me/recommendations"),
                        feature_link("Catalog lookup", "app:lookup"),
                        feature_link("Countries and languages", "/v1/storefronts"),
                    ],
                    info: "Search also finds activities, curators and record labels.".into(),
                    ..Default::default()
                });
            }
            "app:charts" => {
                return Ok(ExplorePage {
                    title: "Charts".into(),
                    items: vec![
                        feature_link(
                            "Top songs, albums and playlists",
                            &self
                                .catalog_path("/charts?types=songs,albums,playlists")
                                .await,
                        ),
                        feature_link(
                            "City charts",
                            &self
                                .catalog_path("/charts?types=playlists&with=cityCharts")
                                .await,
                        ),
                        feature_link(
                            "Global charts",
                            &self
                                .catalog_path("/charts?types=playlists&with=dailyGlobalTopCharts")
                                .await,
                        ),
                        feature_link(
                            "Music videos",
                            &self.catalog_path("/charts?types=music-videos").await,
                        ),
                    ],
                    ..Default::default()
                });
            }
            "app:favorites:songs"
            | "app:favorites:albums"
            | "app:favorites:artists"
            | "app:favorites:playlists" => {
                let typ = route.trim_start_matches("app:favorites:");
                let (rs, e) = self
                    .all(
                        &format!("/v1/me/library/{typ}?limit=100&extend=inFavorites"),
                        300,
                    )
                    .await;
                if let Some(e) = e {
                    return Err(e);
                }
                let selected: Vec<Resource> = rs
                    .into_iter()
                    .filter(|r| r.attributes.get("inFavorites") == Some(&json!(true)))
                    .collect();
                let (items, tracks) = mixed(&selected);
                return Ok(ExplorePage {
                    title: format!("Favorite {typ}"),
                    items,
                    tracks,
                    info: "Shows favorites reported by Apple Music for your library.".into(),
                    ..Default::default()
                });
            }
            "app:favorites" => {
                return Ok(ExplorePage {
                    title: "Favorites".into(),
                    items: vec![
                        feature_link("Songs", "app:favorites:songs"),
                        feature_link("Albums", "app:favorites:albums"),
                        feature_link("Artists", "app:favorites:artists"),
                        feature_link("Playlists", "app:favorites:playlists"),
                    ],
                    ..Default::default()
                });
            }
            "app:replay" => route =
                "/v1/me/music-summaries?filter[year]=latest&views=top-artists,top-albums,top-songs"
                    .into(),
            "app:folders" => {
                let root = self
                    .explore("/v1/me/library/playlist-folders?filter[identity]=playlistsroot")
                    .await?;
                match root.items.first() {
                    None => {
                        return Ok(ExplorePage {
                            title: "Playlist folders".into(),
                            info: "Apple Music has not provided a folder root for this account. Your playlists are available in Playlists.".into(),
                            items: vec![feature_link("New folder…", "action:create:folder"), feature_link("New playlist…", "action:create:playlist")],
                            ..Default::default()
                        });
                    }
                    Some(first) => route = first.route.clone(),
                }
            }
            "app:lookup" => {
                return Ok(ExplorePage {
                    title: "Catalog lookup".into(),
                    items: vec![
                        feature_link("Find a song by ISRC", "action:lookup:isrc"),
                        feature_link("Find an album by UPC", "action:lookup:upc"),
                        feature_link(
                            "Find equivalent songs by catalog ID",
                            "action:lookup:equivalent",
                        ),
                        feature_link("Look up songs by catalog IDs", "action:lookup:songs"),
                        feature_link("Look up albums by catalog IDs", "action:lookup:albums"),
                        feature_link(
                            "Look up videos by catalog IDs",
                            "action:lookup:music-videos",
                        ),
                        feature_link("Mixed IDs (songs:ID; albums:ID)", "action:lookup:batch"),
                    ],
                    ..Default::default()
                });
            }
            _ => {
                if let Some(country) = route.strip_prefix("app:country:") {
                    let b = country.as_bytes();
                    if b.len() != 2 || !b.iter().all(u8::is_ascii_lowercase) {
                        return Err(err("invalid country"));
                    }
                    return Ok(ExplorePage {
                        title: country.to_uppercase(),
                        items: vec![
                            feature_link(
                                "Charts",
                                &format!(
                                    "/v1/catalog/{country}/charts?types=songs,albums,playlists"
                                ),
                            ),
                            feature_link("Genres", &format!("/v1/catalog/{country}/genres")),
                            feature_link("Languages", &format!("/v1/storefronts/{country}")),
                        ],
                        ..Default::default()
                    });
                }
                if let Some(genre) = route.strip_prefix("app:genre:") {
                    let path = self
                        .catalog_path("/charts?types=songs,albums,playlists&genre=")
                        .await
                        + &query_escape(genre);
                    return self.explore(&path).await;
                }
                if let Some(rest) = route.strip_prefix("app:lookup:") {
                    let Some((kind, ids)) =
                        rest.split_once(':').filter(|(_, v)| !v.trim().is_empty())
                    else {
                        return Err(err("enter an identifier"));
                    };
                    let one = |k: &str, v: &str| encode_query(&[(k.to_string(), v.to_string())]);
                    route = match kind {
                        "isrc" => self.catalog_path("/songs?").await + &one("filter[isrc]", ids),
                        "upc" => self.catalog_path("/albums?").await + &one("filter[upc]", ids),
                        "batch" => {
                            let mut q = BTreeMap::new();
                            for group in ids.split(';') {
                                let Some((typ, list)) = group
                                    .trim()
                                    .split_once(':')
                                    .filter(|(_, l)| !l.trim().is_empty())
                                else {
                                    return Err(err("use songs:ID; albums:ID"));
                                };
                                if !matches!(
                                    typ,
                                    "songs"
                                        | "albums"
                                        | "artists"
                                        | "playlists"
                                        | "music-videos"
                                        | "stations"
                                        | "activities"
                                        | "curators"
                                        | "apple-curators"
                                        | "record-labels"
                                ) {
                                    return Err(err("unsupported resource type"));
                                }
                                q.insert(format!("ids[{typ}]"), list.trim().to_string());
                            }
                            let pairs: Vec<(String, String)> = q.into_iter().collect();
                            self.catalog_path("").await + "?" + &encode_query(&pairs)
                        }
                        "songs" | "albums" | "music-videos" => {
                            self.catalog_path(&format!("/{kind}?")).await + &one("ids", ids)
                        }
                        "equivalent" => {
                            self.catalog_path("/songs?").await + &one("filter[equivalents]", ids)
                        }
                        _ => return Err(err("unknown lookup")),
                    };
                }
            }
        }
        // Asked every time: the storefront also sets the catalog language.
        route = route.replace("{storefront}", &self.storefront().await);
        let mut page = self.explore(&route).await?;
        if page.items.len() + page.tracks.len() + page.shelves.len() == 0 {
            page.info = "No results available for this account or country.".into();
        }
        if route.contains("/playlist-folders/") {
            page.items
                .push(feature_link("New folder…", "action:create:folder"));
            page.items
                .push(feature_link("New playlist…", "action:create:playlist"));
        }
        Ok(page)
    }

    /// Creates an empty playlist or folder, optionally inside an existing
    /// folder. It never emulates unsupported editing operations.
    pub async fn create_container(
        &self,
        kind: &str,
        name: &str,
        parent: &str,
    ) -> Result<super::Item> {
        if name.trim().is_empty() {
            return Err(err("enter a name"));
        }
        let typ = match kind {
            "folder" => "playlist-folders",
            "playlist" => "playlists",
            _ => return Err(err("unsupported container")),
        };
        let mut body = json!({"attributes": {"name": name.trim()}});
        if !parent.is_empty() && parent != "p.playlistsroot" {
            body["relationships"] =
                json!({"parent": {"data": [{"id": parent, "type": "library-playlist-folders"}]}});
        }
        let doc = self
            .send(Method::POST, &format!("/v1/me/library/{typ}"), Some(body))
            .await?;
        let data: Vec<Resource> = doc
            .and_then(|d| serde_json::from_value(d.get("data")?.clone()).ok())
            .unwrap_or_default();
        let Some(r) = data.first() else {
            return Err(err("Apple Music did not return the created resource"));
        };
        Ok(item(
            r,
            if kind == "folder" {
                KIND_FOLDER
            } else {
                KIND_PLAYLIST
            },
            false,
        ))
    }

    /// Submits songs, albums, artists or playlists to Apple Music favorites.
    pub async fn favorite(&self, refs: &[Ref]) -> Result<()> {
        if refs.is_empty() {
            return Err(err("select a resource"));
        }
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for r in refs {
            let kind = if r.kind.is_empty() || r.kind == "track" {
                "song"
            } else {
                r.kind.as_str()
            };
            if !matches!(kind, "song" | KIND_ALBUM | KIND_ARTIST | KIND_PLAYLIST) {
                return Err(err("this resource cannot be favorited"));
            }
            let mut typ = format!("{kind}s");
            if ["i.", "l.", "p.", "r."].iter().any(|p| r.id.starts_with(p)) {
                typ = format!("library-{typ}");
            }
            groups.entry(typ).or_default().push(r.id.clone());
        }
        let pairs: Vec<(String, String)> = groups
            .into_iter()
            .map(|(t, ids)| (format!("ids[{t}]"), dedupe(ids).join(",")))
            .collect();
        self.send(
            Method::POST,
            &format!("/v1/me/favorites?{}", encode_query(&pairs)),
            None,
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::testserver::{Seen, fake_api, serve, split};
    use super::*;

    #[tokio::test]
    async fn lookups_use_encoded_documented_filters() {
        let (c, asked) = fake_api(vec![("/v1/catalog/de/songs", r#"{"data":[]}"#)]).await;
        for route in [
            "app:lookup:isrc:US123&ids=bad",
            "app:lookup:equivalent:123,456",
        ] {
            c.feature(route).await.unwrap();
        }
        let asked = asked.lock().unwrap();
        assert!(
            asked[1]
                .target
                .contains("filter%5Bisrc%5D=US123%26ids%3Dbad"),
            "{}",
            asked[1].target
        );
        assert!(
            asked[2]
                .target
                .contains("filter%5Bequivalents%5D=123%2C456"),
            "{}",
            asked[2].target
        );
    }

    #[tokio::test]
    async fn folders_resolve_the_meta_root_and_offer_creates() {
        let (c, _) = fake_api(vec![
            (
                "/v1/me/library/playlist-folders#playlistsroot",
                r#"{"data":[],"meta":{"filters":{"identity":{"playlistsroot":[{"id":"p.root","type":"library-playlist-folders","href":"/v1/me/library/playlist-folders/p.root"}]}}}}"#,
            ),
            (
                "/v1/me/library/playlist-folders/p.root/children",
                r#"{"data":[{"id":"p.child","type":"library-playlist-folders","href":"/v1/me/library/playlist-folders/p.child","attributes":{"name":"Child"}}]}"#,
            ),
        ])
        .await;
        let p = c.feature("app:folders").await.unwrap();
        assert_eq!(p.items.len(), 3, "{p:?}");
        assert_eq!(
            p.items[0].route,
            "/v1/me/library/playlist-folders/p.child/children"
        );
        assert_eq!(p.items[1].route, "action:create:folder");
    }

    #[tokio::test]
    async fn replay_unwraps_playable_period_songs() {
        let (c, _) = fake_api(vec![(
            "/v1/me/music-summaries",
            r#"{"data":[{"id":"replay","type":"music-summaries","attributes":{"year":2025},"views":{"top-songs":{"data":[{"id":"summary","type":"song-period-summaries","relationships":{"song":{"data":[{"id":"123","type":"songs","attributes":{"name":"Song"}}]}}}]}}}]}"#,
        )])
        .await;
        let p = c.feature("app:replay").await.unwrap();
        assert!(p.items.is_empty());
        assert_eq!(p.shelves.len(), 1);
        assert_eq!(p.shelves[0].tracks[0].id, "123");
        assert_eq!(p.title, "");
    }

    #[tokio::test]
    async fn create_folder_has_a_typed_parent_and_favorite_is_query_only() {
        let (base, seen) = serve(Arc::new(|s: &Seen| {
            assert_eq!(s.headers.get("music-user-token").map(String::as_str), Some("user"));
            assert_eq!(s.method, "POST");
            let (path, q) = split(&s.target);
            match path.as_str() {
                "/v1/me/library/playlist-folders" => {
                    let b: serde_json::Value = serde_json::from_slice(&s.body).unwrap();
                    assert_eq!(b["attributes"]["name"], "Folder");
                    assert_eq!(b["relationships"]["parent"]["data"][0]["type"], "library-playlist-folders");
                    assert_eq!(b["relationships"]["parent"]["data"][0]["id"], "p.root");
                    (201, r#"{"data":[{"id":"p.new","type":"library-playlist-folders","attributes":{"name":"Folder"}}]}"#.into())
                }
                "/v1/me/favorites" => {
                    assert_eq!(q.get("ids[songs]").map(String::as_str), Some("123"));
                    assert_eq!(q.get("ids[library-songs]").map(String::as_str), Some("i.456"));
                    (202, String::new())
                }
                _ => (404, "{}".into()),
            }
        }))
        .await;
        let c = Client::with_base(&base, "dev", "user");
        let f = c
            .create_container("folder", "Folder", "p.root")
            .await
            .unwrap();
        assert_eq!((f.id.as_str(), f.kind.as_str()), ("p.new", KIND_FOLDER));
        c.favorite(&[
            Ref {
                kind: "song".into(),
                id: "123".into(),
            },
            Ref {
                kind: "song".into(),
                id: "i.456".into(),
            },
        ])
        .await
        .unwrap();
        assert_eq!(seen.lock().unwrap().len(), 2);
        assert!(
            c.favorite(&[Ref {
                kind: "station".into(),
                id: "ra.1".into()
            }])
            .await
            .is_err()
        );
    }
}
