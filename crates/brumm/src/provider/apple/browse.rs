//! Pages: Home, the charts, radio, artists, search; ratings and playlists.

use std::collections::{BTreeMap, HashMap, HashSet};

use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};

use super::text::{encode_query, path_escape, query_escape};
use super::{
    Client, Collection, Item, KIND_PLAYLIST, KIND_SHELF, KIND_STATION, KIND_TERM, Ref, Resource,
    Shelf, item, mixed, nullable, tracks,
};
use crate::error::{Error, Result, err};

/// The outcome of jobs run side by side: the first unauthorized error, so
/// a page with one broken shelf still shows the rest; an error only when
/// every job failed.
fn parallel(errs: &[&Result<()>]) -> Result<()> {
    for e in errs {
        if let Err(Error::Unauthorized) = e {
            return Err(Error::Unauthorized);
        }
    }
    if !errs.is_empty() && errs.iter().all(|e| e.is_err()) {
        return Err(errs[0].clone().unwrap_err());
    }
    Ok(())
}

/// A document fetched (or the empty one) and whether fetching it went well.
fn split<T: Default>(r: Result<Option<T>>) -> (T, Result<()>) {
    match r {
        Ok(d) => (d.unwrap_or_default(), Ok(())),
        Err(e) => (T::default(), Err(e)),
    }
}

pub(crate) fn feature_link(name: &str, route: &str) -> Item {
    Item {
        kind: KIND_SHELF.into(),
        id: route.into(),
        name: name.into(),
        route: route.into(),
        catalog: true,
        ..Default::default()
    }
}

/// Where the rating of a song, album, playlist or station lives: library
/// ids ("i.", "l.", "p.") and catalog ids have separate collections.
pub fn rating_path(kind: &str, id: &str) -> String {
    let lib = id.starts_with("i.") || id.starts_with("l.") || id.starts_with("p.");
    let kind = if kind.is_empty() || kind == "track" {
        "song"
    } else {
        kind
    };
    let mut p = String::from("/v1/me/ratings/");
    if lib && kind != KIND_STATION {
        p.push_str("library-");
    }
    p + kind + "s"
}

/// Songs as the playlist endpoints want them.
fn song_refs(ids: &[String]) -> Value {
    Value::Array(
        ids.iter()
            .map(|id| json!({"id": id, "type": if id.starts_with("i.") { "library-songs" } else { "songs" }}))
            .collect(),
    )
}

impl Client {
    /// The start page: what was played, what came in, Apple's picks for
    /// this listener, and a way to the charts. Radio has its own page.
    pub async fn home(&self) -> Result<Vec<Shelf>> {
        let recent = async {
            let r = self.fetch("/v1/me/recent/played?limit=10").await;
            let mut s = Shelf::titled("Recently played");
            s.route = "/v1/me/recent/played".into();
            if let Ok(rs) = &r {
                s.items = mixed(rs).0;
            }
            (s, r.map(|_| ()))
        };
        let songs = async {
            let r = self
                .fetch("/v1/me/recent/played/tracks?limit=20&types=songs,library-songs")
                .await;
            let mut s = Shelf::titled("Recently played songs");
            if let Ok(rs) = &r {
                s.tracks = tracks(rs);
            }
            (s, r.map(|_| ()))
        };
        let rotation = async {
            let r = self.fetch("/v1/me/history/heavy-rotation?limit=10").await;
            let mut s = Shelf::titled("Heavy rotation");
            s.route = "/v1/me/history/heavy-rotation".into();
            if let Ok(rs) = &r {
                s.items = mixed(rs).0;
            }
            (s, r.map(|_| ()))
        };
        let added = async {
            let r = self.fetch("/v1/me/library/recently-added?limit=10").await;
            let mut s = Shelf::titled("Recently added");
            s.route = "/v1/me/library/recently-added".into();
            if let Ok(mut rs) = r.clone() {
                if let Ok(more) = self
                    .fetch("/v1/me/library/recently-added?limit=10&offset=10")
                    .await
                {
                    rs.extend(more);
                }
                s.items = mixed(&rs).0;
            }
            (s, r.map(|_| ()))
        };
        let recs = async {
            match self.recommendations().await {
                Ok(r) => (r, Ok(())),
                Err(e) => (Vec::new(), Err(e)),
            }
        };
        let ((mut recent, e1), (mut songs, e2), (mut rotation, e3), (mut added, e4), (recs, e5)) =
            tokio::join!(recent, songs, rotation, added, recs);
        parallel(&[&e1, &e2, &e3, &e4, &e5])?;
        if !recent.is_empty() {
            recent.items.push(feature_link(
                "More recently played",
                "/v1/me/recent/played?limit=25",
            ));
        }
        if !songs.is_empty() {
            songs.items.push(feature_link(
                "More recently played songs",
                "/v1/me/recent/played/tracks?types=songs,library-songs&limit=25",
            ));
        }
        if !rotation.is_empty() {
            rotation.items.push(feature_link(
                "More heavy rotation",
                "/v1/me/history/heavy-rotation?limit=25",
            ));
        }
        if !added.is_empty() {
            added.items.push(feature_link(
                "More recently added",
                "/v1/me/library/recently-added?limit=25",
            ));
        }
        let browse = Shelf {
            title: "Browse".into(),
            items: vec![Item {
                kind: KIND_SHELF.into(),
                id: "charts".into(),
                name: "Top charts".into(),
                catalog: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let personal = Shelf {
            title: "Your music".into(),
            items: vec![
                feature_link("Favorites", "app:favorites"),
                feature_link("Your Replay", "app:replay"),
                feature_link("Playlist folders", "app:folders"),
            ],
            ..Default::default()
        };
        let mut all = vec![personal, recent, songs, rotation, added];
        all.extend(recs);
        all.push(browse);
        Ok(all.into_iter().filter(|s| !s.is_empty()).collect())
    }

    /// Apple's "for you" groups, each a shelf.
    async fn recommendations(&self) -> Result<Vec<Shelf>> {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Title {
            #[serde(rename = "stringForDisplay", deserialize_with = "nullable")]
            text: String,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Attrs {
            #[serde(deserialize_with = "nullable")]
            title: Title,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Rels {
            #[serde(deserialize_with = "nullable")]
            contents: Collection,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Group {
            #[serde(deserialize_with = "nullable")]
            attributes: Attrs,
            #[serde(deserialize_with = "nullable")]
            relationships: Rels,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Doc {
            #[serde(deserialize_with = "nullable")]
            data: Vec<Group>,
        }
        let doc = self
            .get::<Doc>("/v1/me/recommendations?limit=10")
            .await?
            .unwrap_or_default();
        let mut out = Vec::new();
        for g in doc.data {
            let (items, tracks) = mixed(&g.relationships.contents.data);
            let s = Shelf {
                title: g.attributes.title.text,
                route: "/v1/me/recommendations".into(),
                items,
                tracks,
            };
            if !s.title.is_empty() && !s.is_empty() {
                out.push(s);
            }
        }
        Ok(out)
    }

    /// The storefront's most played songs, albums and playlists.
    pub async fn charts(&self) -> Result<Vec<Shelf>> {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Chart {
            #[serde(deserialize_with = "nullable")]
            name: String,
            #[serde(deserialize_with = "nullable")]
            data: Vec<Resource>,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Doc {
            #[serde(deserialize_with = "nullable")]
            results: HashMap<String, Vec<Chart>>,
        }
        let path = self
            .catalog_path("/charts?types=songs,albums,playlists&limit=25")
            .await;
        let doc = self.get::<Doc>(&path).await?.unwrap_or_default();
        let mut out = Vec::new();
        for typ in ["songs", "albums", "playlists"] {
            for ch in doc.results.get(typ).into_iter().flatten() {
                let (items, tracks) = mixed(&ch.data);
                let s = Shelf {
                    title: ch.name.clone(),
                    items,
                    tracks,
                    ..Default::default()
                };
                if !s.is_empty() {
                    out.push(s);
                }
            }
        }
        Ok(out)
    }

    /// The radio page: your station, Apple's live radio, and the stations
    /// played lately.
    pub async fn radio(&self) -> Result<Vec<Shelf>> {
        let mine_path = self
            .catalog_path("/stations?filter[identity]=personal")
            .await;
        let live_path = self
            .catalog_path("/stations?filter[featured]=apple-music-live-radio")
            .await;
        let shelf = |title: &'static str, path: String| async move {
            let r = self.fetch(&path).await;
            let mut s = Shelf::titled(title);
            if let Ok(rs) = &r {
                s.items = mixed(rs).0;
            }
            (s, r.map(|_| ()))
        };
        let ((mine, e1), (live, e2), (recent, e3)) = tokio::join!(
            shelf("Your station", mine_path),
            shelf("Live radio", live_path),
            shelf(
                "Recently played",
                "/v1/me/recent/radio-stations?limit=25".into()
            )
        );
        parallel(&[&e1, &e2, &e3])?;
        Ok([mine, live, recent]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect())
    }

    /// A shelf item's page.
    pub async fn open(&self, it: &Item) -> Result<Vec<Shelf>> {
        match (it.kind.as_str(), it.id.as_str()) {
            (KIND_SHELF, "charts") => self.charts().await,
            (KIND_SHELF, "radio") => self.radio().await,
            _ => Err(err("nothing to open here")),
        }
    }

    /// The catalog twin of a library song or artist.
    async fn catalog_id(&self, kind: &str, id: &str) -> Result<String> {
        if !id.starts_with("i.") && !id.starts_with("r.") {
            return Ok(id.to_string());
        }
        let rs = self
            .fetch(&format!(
                "/v1/me/library/{kind}/{}/catalog",
                path_escape(id)
            ))
            .await?;
        rs.first()
            .map(|r| r.id.clone())
            .ok_or_else(|| err("not in the Apple Music catalog"))
    }

    /// An artist's page: their station, top songs, releases, and artists
    /// like them; for a library artist, what of theirs is in the library
    /// comes first.
    pub async fn artist(&self, it: &Item) -> Result<Vec<Shelf>> {
        const VIEWS: [(&str, &str); 12] = [
            ("top-songs", "Top songs"),
            ("latest-release", "Latest release"),
            ("full-albums", "Albums"),
            ("singles", "Singles & EPs"),
            ("live-albums", "Live albums"),
            ("compilation-albums", "Compilations"),
            ("appears-on-albums", "Appears on"),
            ("featured-playlists", "Playlists"),
            ("featured-albums", "Featured albums"),
            ("top-music-videos", "Music videos"),
            ("featured-music-videos", "Featured videos"),
            ("similar-artists", "Similar artists"),
        ];
        let mut out = Vec::new();
        let mut id = it.id.clone();
        if !it.catalog {
            let (albums, e) = self.artist_albums(it).await;
            if let Some(e) = e
                && e != Error::Partial
            {
                return Err(e);
            }
            if !albums.is_empty() {
                out.push(Shelf {
                    title: "In your library".into(),
                    items: albums,
                    ..Default::default()
                });
            }
            match self.catalog_id("artists", &it.id).await {
                Ok(c) => id = c,
                Err(_) => return Ok(out), // only in the library: that is all there is
            }
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Rels {
            #[serde(deserialize_with = "nullable")]
            station: Collection,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Artist {
            #[serde(deserialize_with = "nullable")]
            views: BTreeMap<String, Collection>,
            #[serde(deserialize_with = "nullable")]
            relationships: Rels,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Doc {
            #[serde(deserialize_with = "nullable")]
            data: Vec<Artist>,
        }
        let views: Vec<&str> = VIEWS.iter().map(|v| v.0).collect();
        let q = format!(
            "?views={}&include=station&extend=editorialNotes",
            views.join(",")
        );
        let path = self
            .catalog_path(&format!("/artists/{}{q}", path_escape(&id)))
            .await;
        let doc = self.get::<Doc>(&path).await?.unwrap_or_default();
        let Some(a) = doc.data.into_iter().next() else {
            return Ok(out);
        };
        let (st, _) = mixed(&a.relationships.station.data);
        if !st.is_empty() {
            out.insert(
                0,
                Shelf {
                    title: "Radio".into(),
                    items: st,
                    ..Default::default()
                },
            );
        }
        for (view, title) in VIEWS {
            let Some(v) = a.views.get(view) else { continue };
            let mut s = Shelf::titled(title);
            if let Some(t) = v.attributes.get("title").and_then(Value::as_str)
                && !t.is_empty()
            {
                s.title = t.to_string();
            }
            (s.items, s.tracks) = mixed(&v.data);
            if !v.next.is_empty() {
                s.items
                    .push(feature_link(&format!("More {title}"), &v.next));
            }
            if !s.is_empty() {
                out.push(s);
            }
        }
        Ok(out)
    }

    /// The radio station made from a song or an artist.
    pub async fn station(&self, it: &Item, song_id: &str) -> Result<Item> {
        let (kind, id) = if song_id.is_empty() {
            ("artists", it.id.as_str())
        } else {
            ("songs", song_id)
        };
        let id = self.catalog_id(kind, id).await?;
        let path = self
            .catalog_path(&format!("/{kind}/{}/station", path_escape(&id)))
            .await;
        let rs = self.fetch(&path).await?;
        mixed(&rs)
            .0
            .into_iter()
            .next()
            .ok_or_else(|| err("Apple Music has no station for this"))
    }

    /// Looks everywhere at once: suggestions for the words typed, the best
    /// matches, the catalog by kind, and the user's own library.
    pub async fn search(&self, term: &str) -> Result<Vec<Shelf>> {
        let suggest_path = self
            .catalog_path(&format!(
                "/search/suggestions?{}",
                encode_query(&[
                    ("term".into(), term.into()),
                    ("kinds".into(), "terms,topResults".into()),
                    ("limit".into(), "5".into())
                ])
            ))
            .await;
        let hints_path = self
            .catalog_path(&format!(
                "/search/hints?{}",
                encode_query(&[("term".into(), term.into()), ("limit".into(), "5".into())])
            ))
            .await;
        let search_path = self
            .catalog_path(&format!(
                "/search?{}",
                encode_query(&[
                    ("term".into(), term.into()),
                    ("types".into(), "songs,albums,artists,playlists,stations,music-videos,activities,curators,apple-curators,record-labels".into()),
                    ("limit".into(), "15".into()),
                    ("with".into(), "topResults".into()),
                ])
            ))
            .await;
        let lib_path = format!(
            "/v1/me/library/search?{}",
            encode_query(&[
                ("term".into(), term.into()),
                ("limit".into(), "10".into()),
                ("types".into(), "library-songs,library-albums,library-artists,library-playlists,library-music-videos".into()),
            ])
        );
        let suggest = async {
            #[derive(Deserialize, Default)]
            #[serde(default)]
            struct Suggestion {
                #[serde(deserialize_with = "nullable")]
                kind: String,
                content: Option<Resource>,
                #[serde(rename = "searchTerm", deserialize_with = "nullable")]
                term: String,
            }
            #[derive(Deserialize, Default)]
            #[serde(default)]
            struct Results {
                #[serde(deserialize_with = "nullable")]
                suggestions: Vec<Suggestion>,
                #[serde(deserialize_with = "nullable")]
                terms: Vec<String>,
            }
            #[derive(Deserialize, Default)]
            #[serde(default)]
            struct Doc {
                #[serde(deserialize_with = "nullable")]
                results: Results,
            }
            let (doc, r) = split(self.get::<Doc>(&suggest_path).await);
            let mut s = Shelf::titled("Suggestions");
            for sg in doc.results.suggestions {
                if let Some(c) = &sg.content {
                    let (its, ts) = mixed(std::slice::from_ref(c));
                    s.items.extend(its);
                    s.tracks.extend(ts);
                }
                if sg.kind == "terms" && !sg.term.is_empty() && !sg.term.eq_ignore_ascii_case(term)
                {
                    s.items.push(Item {
                        kind: KIND_TERM.into(),
                        id: sg.term.clone(),
                        name: sg.term.clone(),
                        ..Default::default()
                    });
                }
            }
            let (hints, hint_err) = split(self.get::<Doc>(&hints_path).await);
            let mut seen: HashSet<String> = HashSet::from([term.to_lowercase()]);
            for it in &s.items {
                seen.insert(it.name.to_lowercase());
            }
            for hint in hints.results.terms {
                if seen.insert(hint.to_lowercase()) {
                    s.items.push(Item {
                        kind: KIND_TERM.into(),
                        id: hint.clone(),
                        name: hint,
                        ..Default::default()
                    });
                }
            }
            if let Err(Error::Unauthorized) = hint_err {
                return (s, Err(Error::Unauthorized));
            }
            (s, r)
        };
        #[derive(Deserialize, Default, Clone)]
        #[serde(default)]
        struct Page {
            #[serde(deserialize_with = "nullable")]
            next: String,
            #[serde(deserialize_with = "nullable")]
            data: Vec<Resource>,
        }
        #[derive(Deserialize, Default, Clone)]
        #[serde(default)]
        struct Doc {
            #[serde(deserialize_with = "nullable")]
            results: HashMap<String, Page>,
        }
        let top = async {
            let (doc, r) = split(self.get::<Doc>(&search_path).await);
            let mut top = Shelf::titled("Top results");
            if let Some(p) = doc.results.get("top") {
                (top.items, top.tracks) = mixed(&p.data);
            }
            let mut by_kind = Vec::new();
            for (typ, title) in [
                ("songs", "Songs"),
                ("albums", "Albums"),
                ("artists", "Artists"),
                ("playlists", "Playlists"),
                ("stations", "Stations"),
                ("music-videos", "Music videos"),
                ("activities", "Activities"),
                ("curators", "Curators"),
                ("apple-curators", "Apple curators"),
                ("record-labels", "Record labels"),
            ] {
                let mut s = Shelf::titled(title);
                if let Some(p) = doc.results.get(typ) {
                    (s.items, s.tracks) = mixed(&p.data);
                    if !p.next.is_empty() {
                        s.items
                            .push(feature_link(&format!("More {title}"), &p.next));
                    }
                }
                by_kind.push(s);
            }
            ((top, by_kind), r)
        };
        let lib = async {
            let (doc, r) = split(self.get::<Doc>(&lib_path).await);
            let mut s = Shelf::titled("In your library");
            for typ in [
                "library-songs",
                "library-albums",
                "library-artists",
                "library-playlists",
                "library-music-videos",
            ] {
                let Some(p) = doc.results.get(typ) else {
                    continue;
                };
                let (mut its, trs) = mixed(&p.data);
                if !p.next.is_empty() {
                    its.push(feature_link("More library results", &p.next));
                }
                s.items.extend(its);
                s.tracks.extend(trs);
            }
            (s, r)
        };
        let ((suggest, e1), ((top, by_kind), e2), (lib, e3)) = tokio::join!(suggest, top, lib);
        parallel(&[&e1, &e2, &e3])?;
        let mut all = vec![suggest, top, lib];
        all.extend(by_kind);
        Ok(all.into_iter().filter(|s| !s.is_empty()).collect())
    }

    /// The ratings (love 1, dislike -1) of the refs that have one.
    pub async fn ratings(&self, refs: &[Ref]) -> Result<BTreeMap<String, i64>> {
        let mut out = BTreeMap::new();
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for r in refs {
            groups
                .entry(rating_path(&r.kind, &r.id))
                .or_default()
                .push(r.id.clone());
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Attrs {
            value: i64,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Rating {
            #[serde(deserialize_with = "nullable")]
            id: String,
            #[serde(deserialize_with = "nullable")]
            attributes: Attrs,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Doc {
            #[serde(deserialize_with = "nullable")]
            data: Vec<Rating>,
        }
        for (path, list) in groups {
            for chunk in list.chunks(100) {
                let doc = self
                    .get::<Doc>(&format!("{path}?ids={}", query_escape(&chunk.join(","))))
                    .await?;
                for r in doc.unwrap_or_default().data {
                    if r.attributes.value != 0 {
                        out.insert(r.id, r.attributes.value);
                    }
                }
            }
        }
        Ok(out)
    }

    /// Loves (1) or dislikes (-1) something, or clears that (0).
    pub async fn rate(&self, r: &Ref, value: i64) -> Result<()> {
        let path = format!("{}/{}", rating_path(&r.kind, &r.id), path_escape(&r.id));
        if value == 0 {
            self.send(Method::DELETE, &path, None).await?;
        } else {
            self.send(
                Method::PUT,
                &path,
                Some(json!({"type": "rating", "attributes": {"value": value}})),
            )
            .await?;
        }
        Ok(())
    }

    /// Makes a library playlist holding songs. Apple's API can create
    /// playlists and add to them, but not rename, reorder, remove songs or
    /// delete — that stays with the Music app.
    pub async fn create_playlist(&self, name: &str, songs: &[String]) -> Result<Item> {
        let mut body = json!({"attributes": {"name": name}});
        if !songs.is_empty() {
            body["relationships"] = json!({"tracks": {"data": song_refs(songs)}});
        }
        let doc = self
            .send(Method::POST, "/v1/me/library/playlists", Some(body))
            .await?;
        let data: Vec<Resource> = doc
            .and_then(|d| serde_json::from_value(d.get("data")?.clone()).ok())
            .unwrap_or_default();
        match data.first() {
            Some(r) => Ok(item(r, KIND_PLAYLIST, false)),
            None => Ok(Item {
                kind: KIND_PLAYLIST.into(),
                name: name.into(),
                editable: true,
                ..Default::default()
            }),
        }
    }

    /// Appends songs to a library playlist brumm (or another app) may edit.
    pub async fn add_to_playlist(&self, id: &str, songs: &[String]) -> Result<()> {
        let path = format!("/v1/me/library/playlists/{}/tracks", path_escape(id));
        self.send(Method::POST, &path, Some(json!({"data": song_refs(songs)})))
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::KIND_ARTIST;
    use super::super::testserver::fake_api;
    use super::*;

    #[test]
    fn rating_paths() {
        for (kind, id, want) in [
            ("song", "i.abc", "/v1/me/ratings/library-songs"),
            ("", "123", "/v1/me/ratings/songs"),
            ("album", "l.x", "/v1/me/ratings/library-albums"),
            ("album", "144", "/v1/me/ratings/albums"),
            ("playlist", "p.x", "/v1/me/ratings/library-playlists"),
            ("playlist", "pl.x", "/v1/me/ratings/playlists"),
            ("station", "ra.x", "/v1/me/ratings/stations"),
        ] {
            assert_eq!(rating_path(kind, id), want, "{kind} {id}");
        }
    }

    #[tokio::test]
    async fn home_shelves() {
        let (c, _) = fake_api(vec![
            (
                "/v1/me/recent/played",
                r#"{"data":[{"id":"l.1","type":"library-albums","attributes":{"name":"Blue","artistName":"Joni"}},
                {"id":"ra.9","type":"stations","attributes":{"name":"Joni Radio"}},{"id":"mv","type":"music-videos","attributes":{"name":"x"}}]}"#,
            ),
            (
                "/v1/me/recent/played/tracks",
                r#"{"data":[{"id":"1","type":"songs","attributes":{"name":"River","artistName":"Joni","durationInMillis":240000}}]}"#,
            ),
            (
                "/v1/me/recommendations",
                r#"{"data":[{"attributes":{"title":{"stringForDisplay":"Made for You"}},
                "relationships":{"contents":{"data":[{"id":"pl.1","type":"playlists","attributes":{"name":"Favourites Mix","curatorName":"Apple Music","description":{"short":"<b>Your</b> favourites &amp; more"}}}]}}}]}"#,
            ),
        ])
        .await;
        let shelves = c.home().await.unwrap();
        let titles: Vec<&str> = shelves.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(
            titles.join("|"),
            "Your music|Recently played|Recently played songs|Made for You|Browse"
        );
        let its = &shelves[1].items;
        assert_eq!(its.len(), 4, "music videos are kept, and a link to more");
        assert_eq!((its[0].kind.as_str(), its[0].catalog), ("album", false));
        assert_eq!(its[1].kind, KIND_STATION);
        assert_eq!(its[2].kind, "music-videos");
        assert_eq!(shelves[3].items[0].note, "Your favourites & more");
        assert_eq!(shelves[2].tracks[0].duration, 240.0);
    }

    #[tokio::test]
    async fn radio_page() {
        let (c, _) = fake_api(vec![
            ("/v1/catalog/de/stations#personal", r#"{"data":[{"id":"ra.me","type":"stations","attributes":{"name":"My Station"}}]}"#),
            ("/v1/catalog/de/stations#apple-music-live-radio", r#"{"data":[{"id":"ra.1","type":"stations","attributes":{"name":"Apple Music 1","isLive":true}}]}"#),
        ])
        .await;
        let shelves = c
            .open(&Item {
                kind: KIND_SHELF.into(),
                id: "radio".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(shelves.len(), 2);
        assert_eq!(shelves[0].items[0].id, "ra.me");
        assert_eq!(shelves[1].items[0].info, "live");
    }

    #[tokio::test]
    async fn artist_page_without_needless_language() {
        let (c, asked) = fake_api(vec![
            ("/v1/me/library/artists/r.5/albums", r#"{"data":[{"id":"l.2","type":"library-albums","attributes":{"name":"Court and Spark"}}]}"#),
            ("/v1/me/library/artists/r.5/catalog", r#"{"data":[{"id":"55","type":"artists","attributes":{"name":"Joni"}}]}"#),
            (
                "/v1/catalog/de/artists/55",
                r#"{"data":[{"id":"55","views":{
                "top-songs":{"attributes":{"title":"Top-Songs"},"data":[{"id":"7","type":"songs","attributes":{"name":"Both Sides Now"}}]},
                "similar-artists":{"attributes":{"title":""},"data":[{"id":"66","type":"artists","attributes":{"name":"Carole"}}]}},
                "relationships":{"station":{"data":[{"id":"ra.55","type":"stations","attributes":{"name":"Joni Station"}}]}}}]}"#,
            ),
        ])
        .await;
        let shelves = c
            .artist(&Item {
                kind: KIND_ARTIST.into(),
                id: "r.5".into(),
                name: "Joni".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let titles: Vec<&str> = shelves.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(
            titles.join("|"),
            "Radio|In your library|Top-Songs|Similar artists"
        );
        for s in asked.lock().unwrap().iter() {
            assert!(
                !(s.target.starts_with("/v1/catalog/") && s.target.contains("l=")),
                "asked for the storefront's own language: {}",
                s.target
            );
        }
    }

    #[tokio::test]
    async fn search_gathers_shelves() {
        let (c, _) = fake_api(vec![
            (
                "/v1/catalog/de/search",
                r#"{"results":{"songs":{"data":[{"id":"1","type":"songs","attributes":{"name":"S"}}],"next":"/v1/catalog/de/search?offset=15"}}}"#,
            ),
            ("/v1/catalog/de/search/hints", r#"{"results":{"terms":["joni mitchell"]}}"#),
        ])
        .await;
        let shelves = c.search("joni").await.unwrap();
        let titles: Vec<&str> = shelves.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["Suggestions", "Songs"]);
        assert_eq!(shelves[0].items[0].kind, KIND_TERM);
        assert_eq!(shelves[1].items[0].name, "More Songs");
    }
    #[tokio::test]
    async fn listening_history_loads_previously_played_songs() {
        let (c, _) = fake_api(vec![(
            "/v1/me/recent/played/tracks",
            r#"{"data":[{"id":"old","type":"songs","attributes":{"name":"Earlier song","artistName":"Artist","durationInMillis":180000}}]}"#,
        )]).await;
        let history = crate::provider::Api::recently_played(&c).await.unwrap();
        assert_eq!(history[0].id, "old");
        assert_eq!(history[0].title, "Earlier song");
        assert_eq!(history[0].duration, 180.0);
    }
}
