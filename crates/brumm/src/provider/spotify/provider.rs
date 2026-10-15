//! Maps the Spotify client onto the common provider contracts.

use std::collections::{BTreeMap, HashSet};

use reqwest::Method;
use serde_json::{Value, json};

use super::{Client, checked, link, map, query, uri};
use crate::error::{Error, Result, err};
use crate::music::{DISLIKE, Item, KIND_ALBUM, KIND_PLAYLIST, LOVE, Ref, Shelf, Track};
use crate::provider::{Api, Catalog, Library, Listing, Reply};

/// Pages of 50, Spotify's most for library lists.
const PAGE: &str = "50";
/// Playlists of up to 10 000 songs, libraries of up to 20 000.
const MAX_PAGES: usize = 400;
/// GET /me/library/contains and PUT /me/library take at most 40 URIs.
const URIS_PER_CALL: usize = 40;

/// Whether a rating's kind names a song.
fn is_song(kind: &str) -> bool {
    matches!(kind, "" | "song" | "songs" | "track")
}

fn tracks_of(v: &[Value], key: Option<&str>, album: Option<&Item>) -> Vec<Track> {
    v.iter()
        .filter_map(|e| match key {
            // Playlist entries hold `item` (2026) or `track`.
            Some(k) => e.get(k).or_else(|| e.get("track")),
            None => Some(e),
        })
        .filter_map(|t| map::track(t, album))
        .collect()
}

impl Client {
    async fn search_all(&self, term: &str) -> Result<Vec<Shelf>> {
        let doc = self
            .get(&format!(
                "/v1/search?{}",
                query(&[
                    ("q", term),
                    ("type", "track,album,artist,playlist"),
                    ("limit", "10"),
                ])
            ))
            .await?;
        let me = self.me().await.unwrap_or_default();
        let list = |k: &str| -> Vec<Value> {
            doc.get(k)
                .and_then(|v| v.get("items"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        let mut songs = Shelf::titled("Songs");
        songs.tracks = tracks_of(&list("tracks"), None, None);
        let mut artists = Shelf::titled("Artists");
        artists.items = list("artists")
            .iter()
            .filter_map(|a| map::artist(a, true))
            .collect();
        let mut albums = Shelf::titled("Albums");
        albums.items = list("albums")
            .iter()
            .filter_map(|a| map::album(a, true))
            .collect();
        let mut playlists = Shelf::titled("Playlists");
        playlists.items = list("playlists")
            .iter()
            .filter_map(|p| map::playlist(p, &me, true))
            .collect();
        Ok([songs, artists, albums, playlists]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect())
    }

    async fn item_tracks(&self, it: &Item) -> Listing<Track> {
        let id = match checked(&it.id) {
            Ok(id) => id,
            Err(e) => {
                // Stations and the like play as a whole.
                return (
                    Vec::new(),
                    Some(if it.id.starts_with("spotify:") {
                        Error::Unsupported
                    } else {
                        e
                    }),
                );
            }
        };
        match it.kind.as_str() {
            KIND_ALBUM => {
                let path = format!("/v1/albums/{id}/tracks?limit={PAGE}");
                let (v, e) = self.pages(&path, None, MAX_PAGES).await;
                // An album opened from a song carries no cover: ask once.
                let mut al = it.clone();
                if al.artwork.is_empty()
                    && let Ok(doc) = self.get(&format!("/v1/albums/{id}")).await
                {
                    al.artwork = map::artwork(&doc);
                    if al.name.is_empty() {
                        al.name = map::s(&doc, "name").to_string();
                    }
                }
                (tracks_of(&v, None, Some(&al)), e)
            }
            KIND_PLAYLIST => {
                let path = format!(
                    "/v1/playlists/{id}/items?{}",
                    query(&[("limit", PAGE), ("additional_types", "track")])
                );
                let (v, e) = self.pages(&path, None, MAX_PAGES).await;
                (tracks_of(&v, Some("item"), None), e)
            }
            _ => (Vec::new(), Some(Error::Unsupported)),
        }
    }

    pub(super) async fn song(&self, id: &str) -> Result<Value> {
        self.get(&format!("/v1/tracks/{}", checked(id)?)).await
    }

    async fn album_of_song(&self, id: &str) -> Result<Item> {
        let t = self.song(id).await?;
        t.get("album")
            .and_then(|a| map::album(a, true))
            .ok_or_else(|| err("spotify: this song has no album"))
    }

    async fn artist_of_song(&self, id: &str) -> Result<Item> {
        let t = self.song(id).await?;
        t.get("artists")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(|a| map::artist(a, true))
            .ok_or_else(|| err("spotify: this song has no artist"))
    }

    async fn resolve_link(&self, link: &str) -> Result<(Item, String)> {
        let (kind, id) = link::parse(link).ok_or_else(|| err("not a Spotify link"))?;
        match kind {
            "track" => Ok((self.album_of_song(&id).await?, id)),
            "album" => {
                let doc = self.get(&format!("/v1/albums/{id}")).await?;
                Ok((
                    map::album(&doc, true).ok_or_else(|| err("spotify: no album"))?,
                    String::new(),
                ))
            }
            "artist" => {
                let doc = self.get(&format!("/v1/artists/{id}")).await?;
                Ok((
                    map::artist(&doc, true).ok_or_else(|| err("spotify: no artist"))?,
                    String::new(),
                ))
            }
            _ => {
                let fields = "id,name,owner(id,display_name),collaborative,description,images,external_urls,items(total)";
                let doc = self
                    .get(&format!(
                        "/v1/playlists/{id}?{}",
                        query(&[("fields", fields)])
                    ))
                    .await?;
                let me = self.me().await.unwrap_or_default();
                Ok((
                    map::playlist(&doc, &me, true).ok_or_else(|| err("spotify: no playlist"))?,
                    String::new(),
                ))
            }
        }
    }

    async fn my_playlists(&self) -> Listing<Item> {
        let me = match self.me().await {
            Ok(me) => me,
            Err(e) => return (Vec::new(), Some(e)),
        };
        let (v, e) = self
            .pages(&format!("/v1/me/playlists?limit={PAGE}"), None, MAX_PAGES)
            .await;
        (
            v.iter()
                .filter_map(|p| map::playlist(p, &me, false))
                .collect(),
            e,
        )
    }

    async fn my_albums(&self) -> Listing<Item> {
        let (v, e) = self
            .pages(&format!("/v1/me/albums?limit={PAGE}"), None, MAX_PAGES)
            .await;
        (
            v.iter()
                .filter_map(|s| s.get("album"))
                .filter_map(|a| map::album(a, false))
                .collect(),
            e,
        )
    }

    async fn my_artists(&self) -> Listing<Item> {
        let path = format!("/v1/me/following?type=artist&limit={PAGE}");
        let (v, e) = self.pages(&path, Some("artists"), MAX_PAGES).await;
        (v.iter().filter_map(|a| map::artist(a, false)).collect(), e)
    }

    async fn my_songs(&self) -> Listing<Track> {
        let (v, e) = self
            .pages(&format!("/v1/me/tracks?limit={PAGE}"), None, MAX_PAGES)
            .await;
        (tracks_of(&v, Some("track"), None), e)
    }

    /// Home: what played lately, and the user's most played songs and
    /// artists. A shelf that fails is left out unless all do.
    async fn home_shelves(&self) -> Result<Vec<Shelf>> {
        let (recent, songs, artists) = tokio::join!(
            self.get("/v1/me/player/recently-played?limit=50"),
            self.get("/v1/me/top/tracks?limit=30&time_range=short_term"),
            self.get("/v1/me/top/artists?limit=20&time_range=medium_term"),
        );
        let mut out = Vec::new();
        let mut first_err = None;
        let items = |v: &Value| {
            v.get("items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        match recent {
            Ok(doc) => {
                let mut s = Shelf::titled("Recently played");
                let mut seen = HashSet::new();
                s.tracks = tracks_of(&items(&doc), Some("track"), None)
                    .into_iter()
                    .filter(|t| seen.insert(t.id.clone()))
                    .collect();
                out.push(s);
            }
            Err(e) => first_err = first_err.or(Some(e)),
        }
        match songs {
            Ok(doc) => {
                let mut s = Shelf::titled("Your top songs");
                s.tracks = tracks_of(&items(&doc), None, None);
                out.push(s);
            }
            Err(e) => first_err = first_err.or(Some(e)),
        }
        match artists {
            Ok(doc) => {
                let mut s = Shelf::titled("Your top artists");
                s.items = items(&doc)
                    .iter()
                    .filter_map(|a| map::artist(a, true))
                    .collect();
                out.push(s);
            }
            Err(e) => first_err = first_err.or(Some(e)),
        }
        match first_err {
            Some(Error::Unauthorized) => Err(Error::Unauthorized),
            Some(e) if out.is_empty() => Err(e),
            _ => Ok(out.into_iter().filter(|s| !s.is_empty()).collect()),
        }
    }

    /// An artist's albums, then singles and EPs. Spotify no longer gives
    /// apps an artist's top songs or similar artists.
    async fn artist_shelves(&self, it: &Item) -> Result<Vec<Shelf>> {
        let id = checked(&it.id)?;
        let mut out = Vec::new();
        for (group, title) in [
            ("album", "Albums"),
            ("single", "Singles & EPs"),
            ("compilation", "Compilations"),
        ] {
            let path = format!("/v1/artists/{id}/albums?include_groups={group}&limit=10");
            let (v, e) = self.pages(&path, None, 5).await;
            if let Some(Error::Unauthorized) = e {
                return Err(Error::Unauthorized);
            }
            if v.is_empty()
                && let Some(e) = e
                && out.is_empty()
                && group == "album"
            {
                return Err(e);
            }
            let mut s = Shelf::titled(title);
            s.items = v.iter().filter_map(|a| map::album(a, true)).collect();
            out.push(s);
        }
        Ok(out.into_iter().filter(|s| !s.is_empty()).collect())
    }

    /// Which songs of refs are saved: a saved (liked) song is loved. Albums
    /// and playlists in the library are saved too, but not loved: Spotify
    /// has no love for them.
    async fn saved(&self, refs: &[Ref]) -> Result<BTreeMap<String, i64>> {
        let mut out = BTreeMap::new();
        let known: Vec<(String, String)> = refs
            .iter()
            .filter(|r| is_song(&r.kind))
            .filter_map(|r| uri(&r.kind, &r.id).ok().map(|u| (u, r.id.clone())))
            .collect();
        for chunk in known.chunks(URIS_PER_CALL) {
            let uris: Vec<&str> = chunk.iter().map(|(u, _)| u.as_str()).collect();
            let doc = self
                .get(&format!(
                    "/v1/me/library/contains?{}",
                    query(&[("uris", &uris.join(","))])
                ))
                .await?;
            let flags = doc.as_array().cloned().unwrap_or_default();
            for ((_, id), f) in chunk.iter().zip(flags) {
                if f.as_bool() == Some(true) {
                    out.insert(id.clone(), LOVE);
                }
            }
        }
        Ok(out)
    }

    /// Saves or removes URIs, 40 at a time.
    async fn library(&self, method: Method, uris: &[String]) -> Result<()> {
        for chunk in uris.chunks(URIS_PER_CALL) {
            self.send(
                method.clone(),
                &format!("/v1/me/library?{}", query(&[("uris", &chunk.join(","))])),
                None,
            )
            .await?;
        }
        Ok(())
    }

    async fn set_rating(&self, r: &Ref, value: i64) -> Result<()> {
        if !is_song(&r.kind) {
            return Err(Error::Unsupported); // the i key adds an album or playlist to the library
        }
        let u = uri(&r.kind, &r.id)?;
        match value {
            LOVE => self.library(Method::PUT, &[u]).await,
            0 => self.library(Method::DELETE, &[u]).await,
            DISLIKE => Err(Error::Unsupported),
            _ => Err(err("spotify: unknown rating")),
        }
    }

    async fn add_all(&self, kind: &str, ids: &[String]) -> Result<()> {
        let uris = ids
            .iter()
            .map(|id| uri(kind, id))
            .collect::<Result<Vec<_>>>()?;
        self.library(Method::PUT, &uris).await
    }

    async fn add_songs(&self, playlist: &str, songs: &[String]) -> Result<()> {
        let id = checked(playlist)?;
        let uris = songs
            .iter()
            .map(|s| uri("song", s))
            .collect::<Result<Vec<_>>>()?;
        for chunk in uris.chunks(100) {
            self.send(
                Method::POST,
                &format!("/v1/playlists/{id}/items"),
                Some(json!({ "uris": chunk })),
            )
            .await?;
        }
        Ok(())
    }

    async fn new_playlist(&self, name: &str, songs: &[String]) -> Result<Item> {
        let me = self.me().await?;
        let doc = self
            .send(
                Method::POST,
                "/v1/me/playlists",
                // Without a description, Spotify shows "null" under the name.
                Some(json!({"name": name, "public": false, "description": ""})),
            )
            .await?
            .unwrap_or_default();
        let mut it =
            map::playlist(&doc, &me, false).ok_or_else(|| err("spotify: no playlist made"))?;
        if !songs.is_empty() {
            self.add_songs(&it.id, songs).await?;
            it.info = match songs.len() {
                1 => "1 song".into(),
                n => format!("{n} songs"),
            };
        }
        Ok(it)
    }
}

impl Catalog for Client {
    fn search<'a>(&'a self, term: &'a str) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(self.search_all(term))
    }
    fn tracks<'a>(&'a self, item: &'a Item) -> Reply<'a, Listing<Track>> {
        Box::pin(self.item_tracks(item))
    }
    fn album_of<'a>(&'a self, song: &'a str) -> Reply<'a, Result<Item>> {
        Box::pin(self.album_of_song(song))
    }
    fn artist_of<'a>(&'a self, song: &'a str) -> Reply<'a, Result<Item>> {
        Box::pin(self.artist_of_song(song))
    }
    fn link<'a>(&'a self, song: &'a str) -> Reply<'a, Result<String>> {
        Box::pin(async move { Ok(format!("https://open.spotify.com/track/{}", checked(song)?)) })
    }
    fn resolve<'a>(&'a self, link: &'a str) -> Reply<'a, Result<(Item, String)>> {
        Box::pin(self.resolve_link(link))
    }
}

impl Library for Client {
    fn playlists<'a>(&'a self) -> Reply<'a, Listing<Item>> {
        Box::pin(self.my_playlists())
    }
    fn albums<'a>(&'a self) -> Reply<'a, Listing<Item>> {
        Box::pin(self.my_albums())
    }
    fn artists<'a>(&'a self) -> Reply<'a, Listing<Item>> {
        Box::pin(self.my_artists())
    }
    fn songs<'a>(&'a self) -> Reply<'a, Listing<Track>> {
        Box::pin(self.my_songs())
    }
}

impl Api for Client {
    fn recently_played<'a>(&'a self) -> Reply<'a, Result<Vec<Track>>> {
        Box::pin(async {
            let doc = self.get("/v1/me/player/recently-played?limit=50").await?;
            let items = doc
                .get("items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(tracks_of(&items, Some("track"), None))
        })
    }
    fn home<'a>(&'a self) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(self.home_shelves())
    }
    fn artist<'a>(&'a self, item: &'a Item) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(self.artist_shelves(item))
    }
    fn ratings<'a>(&'a self, refs: &'a [Ref]) -> Reply<'a, Result<BTreeMap<String, i64>>> {
        Box::pin(self.saved(refs))
    }
    fn rate<'a>(&'a self, resource: &'a Ref, value: i64) -> Reply<'a, Result<()>> {
        Box::pin(self.set_rating(resource, value))
    }
    fn add_to_library<'a>(&'a self, kind: &'a str, ids: &'a [String]) -> Reply<'a, Result<()>> {
        Box::pin(self.add_all(kind, ids))
    }
    fn create_playlist<'a>(
        &'a self,
        name: &'a str,
        songs: &'a [String],
    ) -> Reply<'a, Result<Item>> {
        Box::pin(self.new_playlist(name, songs))
    }
    fn add_to_playlist<'a>(&'a self, id: &'a str, songs: &'a [String]) -> Reply<'a, Result<()>> {
        Box::pin(self.add_songs(id, songs))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testserver::fake_api;
    use super::*;
    use crate::music::{KIND_ARTIST, KIND_STATION};

    const T1: &str = "1111111111111111111111";
    const T2: &str = "2222222222222222222222";
    const AL: &str = "aaaaaaaaaaaaaaaaaaaaaa";
    const PL: &str = "pppppppppppppppppppppp";

    fn song(id: &str) -> String {
        format!(
            r#"{{"type":"track","id":"{id}","name":"Song {id}","duration_ms":1000,"artists":[{{"id":"{AL}","name":"Ar"}}],"album":{{"id":"{AL}","name":"Al","images":[]}}}}"#
        )
    }

    #[tokio::test]
    async fn library_lists_map_and_split_playlists() {
        let (c, seen) = fake_api(vec![
            (
                "/v1/me/playlists",
                r#"{"items":[{"id":"p1","name":"Mine","owner":{"id":"me"}},
                    {"id":"p2","name":"Theirs","owner":{"id":"x"}}],"next":null}"#,
            ),
            (
                "/v1/me/tracks",
                &format!(
                    r#"{{"items":[{{"track":{}}},{{"track":null}}],"next":null}}"#,
                    song(T1)
                ),
            ),
            (
                "/v1/me/following",
                r#"{"artists":{"items":[{"id":"a1","name":"Ar"}],"next":null}}"#,
            ),
            (
                "/v1/me/albums",
                r#"{"items":[{"album":{"id":"b1","name":"Al"}}],"next":null}"#,
            ),
        ])
        .await;
        let (pl, e) = c.playlists().await;
        assert_eq!(e, None);
        assert_eq!(
            pl.iter().map(|p| p.kind.as_str()).collect::<Vec<_>>(),
            [KIND_PLAYLIST, KIND_STATION]
        );
        let (songs, e) = c.songs().await;
        assert_eq!((songs.len(), e), (1, None));
        assert_eq!(c.artists().await.0[0].kind, KIND_ARTIST);
        assert!(!c.albums().await.0[0].catalog);
        let limits: Vec<String> = seen
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.target.clone())
            .collect();
        assert!(
            limits.iter().any(|t| t == "/v1/me/playlists?limit=50"),
            "{limits:?}"
        );
    }

    #[tokio::test]
    async fn playlists_and_albums_open_to_their_songs() {
        let (c, seen) = fake_api(vec![
            (
                &format!("/v1/playlists/{PL}/items"),
                &format!(
                    r#"{{"items":[{{"is_local":false,"item":{}}},{{"is_local":true,"item":{{"id":null}}}},
                    {{"item":{{"type":"episode","id":"e"}}}}],"next":null}}"#,
                    song(T1)
                ),
            ),
            (
                &format!("/v1/albums/{AL}/tracks"),
                &format!(r#"{{"items":[{{"id":"{T2}","name":"Two","track_number":2}}],"next":null}}"#),
            ),
            (&format!("/v1/albums/{AL}"), r#"{"name":"Al","images":[{"url":"cover","width":640}]}"#),
        ])
        .await;
        let pl = Item {
            kind: KIND_PLAYLIST.into(),
            id: PL.into(),
            ..Default::default()
        };
        let (t, e) = c.tracks(&pl).await;
        assert_eq!((t.len(), e), (1, None));
        let al = Item {
            kind: KIND_ALBUM.into(),
            id: AL.into(),
            ..Default::default()
        };
        let (t, _) = c.tracks(&al).await;
        assert_eq!(
            (t[0].album.as_str(), t[0].artwork.as_str(), t[0].number),
            ("Al", "cover", 2)
        );
        let station = Item {
            kind: KIND_STATION.into(),
            id: format!("spotify:playlist:{PL}"),
            ..Default::default()
        };
        assert_eq!(c.tracks(&station).await.1, Some(Error::Unsupported));
        let bad = Item {
            kind: KIND_PLAYLIST.into(),
            id: "../me".into(),
            ..Default::default()
        };
        let n = seen.lock().unwrap().len();
        assert!(c.tracks(&bad).await.1.is_some());
        assert_eq!(seen.lock().unwrap().len(), n, "a bad handle made a request");
    }

    #[tokio::test]
    async fn search_shelves_by_kind() {
        let (c, seen) = fake_api(vec![(
            "/v1/search",
            &format!(
                r#"{{"tracks":{{"items":[{}]}},"albums":{{"items":[{{"id":"{AL}","name":"Al"}}]}},
                "artists":{{"items":[]}},"playlists":{{"items":[null,{{"id":"{PL}","name":"P","owner":{{"id":"me"}}}}]}}}}"#,
                song(T1)
            ),
        )])
        .await;
        let shelves = c.search("blue moon").await.unwrap();
        assert_eq!(
            shelves.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(),
            ["Songs", "Albums", "Playlists"]
        );
        let target = seen.lock().unwrap()[0].target.clone();
        assert!(
            target.contains("q=blue+moon") && target.contains("limit=10"),
            "{target}"
        );
    }

    #[tokio::test]
    async fn love_is_saving_and_dislike_is_unsupported() {
        let (c, seen) = fake_api(vec![
            ("/v1/me/library/contains", "[true,false]"),
            ("/v1/me/library", ""),
        ])
        .await;
        let refs = [
            Ref {
                kind: "song".into(),
                id: T1.into(),
            },
            Ref {
                kind: "album".into(),
                id: AL.into(),
            },
            Ref {
                kind: "song".into(),
                id: "bad".into(),
            },
        ];
        let r = c.ratings(&refs).await.unwrap();
        assert_eq!(r.get(T1), Some(&LOVE));
        assert_eq!(r.len(), 1);
        c.rate(&refs[0], LOVE).await.unwrap();
        c.rate(&refs[0], 0).await.unwrap();
        assert_eq!(c.rate(&refs[0], DISLIKE).await, Err(Error::Unsupported));
        assert_eq!(c.rate(&refs[1], LOVE).await, Err(Error::Unsupported));
        let seen = seen.lock().unwrap();
        assert!(
            seen[0]
                .target
                .ends_with(&format!("uris=spotify%3Atrack%3A{T1}")),
            "only songs are asked about: {}",
            seen[0].target
        );
        assert_eq!(seen[1].method, "PUT");
        assert_eq!(seen[2].method, "DELETE");
    }

    #[tokio::test]
    async fn new_playlists_take_their_songs() {
        let (c, seen) = fake_api(vec![
            (
                "/v1/me/playlists",
                &format!(r#"{{"id":"{PL}","name":"New","owner":{{"id":"me"}}}}"#),
            ),
            (
                &format!("/v1/playlists/{PL}/items"),
                r#"{"snapshot_id":"s"}"#,
            ),
        ])
        .await;
        let it = c
            .create_playlist("New", &[T1.into(), T2.into()])
            .await
            .unwrap();
        assert!(it.editable && it.kind == KIND_PLAYLIST);
        let made = seen.lock().unwrap()[1].clone(); // after /v1/me
        let body: Value = serde_json::from_slice(&made.body).unwrap();
        assert_eq!(
            body["description"], "",
            "no description reads \"null\" on Spotify"
        );
        let add = seen
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.target.ends_with("/items"))
            .cloned()
            .unwrap();
        let body: Value = serde_json::from_slice(&add.body).unwrap();
        assert_eq!(body["uris"][1], format!("spotify:track:{T2}"));
        assert!(c.add_to_playlist("x/../../me", &[T1.into()]).await.is_err());
    }

    #[tokio::test]
    async fn links_resolve_to_what_opens() {
        let (c, _) = fake_api(vec![
            (&format!("/v1/tracks/{T1}"), &song(T1)),
            (
                &format!("/v1/playlists/{PL}"),
                &format!(r#"{{"id":"{PL}","name":"Hits","owner":{{"id":"spotify"}}}}"#),
            ),
        ])
        .await;
        let (it, song) = c
            .resolve(&format!("https://open.spotify.com/track/{T1}?si=x"))
            .await
            .unwrap();
        assert_eq!(
            (it.kind.as_str(), it.id.as_str(), song.as_str()),
            (KIND_ALBUM, AL, T1)
        );
        let (it, _) = c.resolve(&format!("spotify:playlist:{PL}")).await.unwrap();
        assert_eq!(it.kind, KIND_STATION);
        assert!(
            c.resolve("https://music.apple.com/de/album/x/1")
                .await
                .is_err()
        );
        assert_eq!(
            c.link(T1).await.unwrap(),
            format!("https://open.spotify.com/track/{T1}")
        );
    }

    #[tokio::test]
    async fn home_keeps_what_answers() {
        let (c, _) = fake_api(vec![
            (
                "/v1/me/player/recently-played",
                &format!(r#"{{"items":[{{"track":{0}}},{{"track":{0}}}]}}"#, song(T1)),
            ),
            ("/v1/me/top/tracks", "403 {}"),
            ("/v1/me/top/artists", r#"{"items":[{"id":"a","name":"A"}]}"#),
        ])
        .await;
        let home = c.home().await.unwrap();
        assert_eq!(home[0].tracks.len(), 1, "played twice, shown once");
        assert_eq!(home.len(), 2);
    }
    #[tokio::test]
    async fn listening_history_preserves_provider_order() {
        let (c, _) = fake_api(vec![(
            "/v1/me/player/recently-played",
            &format!(
                r#"{{"items":[{{"track":{}}},{{"track":{}}}]}}"#,
                song(T2),
                song(T1)
            ),
        )])
        .await;
        let history = c.recently_played().await.unwrap();
        assert_eq!(history[0].id, T2);
        assert_eq!(history[1].id, T1);
    }
}
