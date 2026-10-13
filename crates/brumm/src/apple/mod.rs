//! A small Apple Music REST client: the user's library, the catalog, and
//! what Apple lets a third-party app do with them.

mod browse;
mod explore;
mod features;
mod text;
mod types;

use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use crate::error::{Error, Result, err};

pub use text::encode_query;
use text::{plain, query_escape};
pub use types::*;

const API_BASE: &str = "https://api.music.apple.com";

/// The cover size brumm asks for everywhere, so the same cover always has
/// the same address and caches once.
const ARTWORK_SIZE: &str = "600";

fn artwork_url(a: &Map<String, Value>) -> String {
    match a
        .get("artwork")
        .and_then(|v| v.get("url"))
        .and_then(Value::as_str)
    {
        Some(u) => u.replace("{w}", ARTWORK_SIZE).replace("{h}", ARTWORK_SIZE),
        None => String::new(),
    }
}

/// A null where a value belongs is the empty value, as Go's decoder left it.
pub(crate) fn nullable<'de, D: Deserializer<'de>, T: Default + Deserialize<'de>>(
    d: D,
) -> std::result::Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// One API resource, as listed in a document's data.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct Resource {
    #[serde(deserialize_with = "nullable")]
    pub id: String,
    #[serde(rename = "type", deserialize_with = "nullable")]
    pub typ: String,
    #[serde(deserialize_with = "nullable")]
    pub href: String,
    #[serde(deserialize_with = "nullable")]
    pub attributes: Map<String, Value>,
    #[serde(deserialize_with = "nullable")]
    pub relationships: BTreeMap<String, Collection>,
    #[serde(deserialize_with = "nullable")]
    pub views: BTreeMap<String, Collection>,
}

/// A list of resources: a relationship, a view, a search result.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct Collection {
    #[serde(deserialize_with = "nullable")]
    pub data: Vec<Resource>,
    #[serde(deserialize_with = "nullable")]
    pub href: String,
    #[serde(deserialize_with = "nullable")]
    pub next: String,
    #[serde(deserialize_with = "nullable")]
    pub attributes: Map<String, Value>,
    #[serde(deserialize_with = "nullable")]
    pub name: String,
}

pub(crate) fn attr_str<'a>(a: &'a Map<String, Value>, key: &str) -> &'a str {
    a.get(key).and_then(Value::as_str).unwrap_or("")
}

fn attr_i64(a: &Map<String, Value>, key: &str) -> i64 {
    a.get(key)
        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
        .unwrap_or(0)
}

fn attr_bool(a: &Map<String, Value>, key: &str) -> bool {
    a.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub struct Client {
    /// The API's address; a test server's in tests.
    base: String,
    dev: String,
    user: String,
    http: reqwest::Client,
    /// The storefront and the language for catalog texts.
    sf: tokio::sync::OnceCell<(String, String)>,
    /// The system's locale ($LANG), for the catalog language.
    locale: String,
}

impl Client {
    pub fn new(developer_token: &str, user_token: &str) -> Client {
        Client::with_base(API_BASE, developer_token, user_token)
    }

    pub(crate) fn with_base(base: &str, developer_token: &str, user_token: &str) -> Client {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Client {
            base: base.to_string(),
            dev: developer_token.to_string(),
            user: user_token.to_string(),
            http,
            sf: Default::default(),
            locale: std::env::var("LANG").unwrap_or_default(),
        }
    }

    fn authorized(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        rb.header("Authorization", format!("Bearer {}", self.dev))
            .header("Music-User-Token", &self.user)
    }

    /// Makes a request with an optional JSON body and decodes the answer,
    /// when there is one.
    pub(crate) async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Option<Value>> {
        let mut rb = self.authorized(
            self.http
                .request(method.clone(), format!("{}{}", self.base, path)),
        );
        if let Some(b) = body {
            rb = rb
                .header("Content-Type", "application/json")
                .body(serde_json::to_vec(&b)?);
        }
        let resp = rb.send().await?;
        let status = resp.status();
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(Error::Unauthorized),
            StatusCode::NOT_FOUND if method == Method::DELETE => return Ok(None), // nothing to remove
            s if s.as_u16() >= 300 => return Err(err(format!("apple music: {s}"))),
            StatusCode::NO_CONTENT => return Ok(None),
            _ => {}
        }
        let b = resp.bytes().await?;
        if b.is_empty() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_slice(&b)?))
    }

    /// Fetches one document. Apple answers 404 for an empty library
    /// collection, which is None, not an error.
    pub(crate) async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<Option<T>> {
        let url = format!("{}{}", self.base, self.localize(path));
        let resp = self.authorized(self.http.get(url)).send().await?;
        match resp.status() {
            StatusCode::NOT_FOUND => Ok(None),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(Error::Unauthorized),
            StatusCode::OK => Ok(Some(serde_json::from_slice(&resp.bytes().await?)?)),
            s => Err(err(format!("apple music: {s}"))),
        }
    }

    /// Follows `next` links. They come back as "/v1/me/…", already carrying
    /// the version prefix; joining them onto a base that has it too silently
    /// stopped listings at the first 100 items in vibez. A listing that
    /// stops early keeps what came and says Partial.
    pub(crate) async fn all(&self, path: &str, max_pages: usize) -> (Vec<Resource>, Option<Error>) {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Page {
            #[serde(deserialize_with = "nullable")]
            next: String,
            #[serde(deserialize_with = "nullable")]
            data: Vec<Resource>,
        }
        let mut out = Vec::new();
        let mut path = path.to_string();
        let mut pages = 0;
        while !path.is_empty() && pages < max_pages {
            pages += 1;
            let p = match self.get::<Page>(&path).await {
                Err(e) => {
                    if !out.is_empty() && e != Error::Unauthorized {
                        return (out, Some(Error::Partial)); // keep what we have, but say so
                    }
                    return (out, Some(e));
                }
                Ok(None) => {
                    path.clear();
                    break;
                }
                Ok(Some(p)) => p,
            };
            out.extend(p.data);
            path = if p.next.is_empty() {
                String::new()
            } else {
                text::inherit_page_query(&path, &p.next)
            };
            if !path.is_empty() && !path.starts_with("/v1/") {
                path = format!("/v1/{}", path.trim_start_matches('/'));
            }
        }
        if !path.is_empty() {
            return (out, Some(Error::Partial));
        }
        (out, None)
    }

    /// A plain list of resources; an empty collection is no error.
    pub(crate) async fn fetch(&self, path: &str) -> Result<Vec<Resource>> {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Page {
            #[serde(deserialize_with = "nullable")]
            data: Vec<Resource>,
        }
        Ok(self.get::<Page>(path).await?.unwrap_or_default().data)
    }

    /// The user's country, needed for catalog requests. It also picks the
    /// language for catalog texts: the system's ($LANG), when the
    /// storefront offers it and it is not the storefront's default anyway.
    pub async fn storefront(&self) -> String {
        self.sf
            .get_or_init(|| async {
                #[derive(Deserialize, Default)]
                #[serde(default)]
                struct Attrs {
                    #[serde(rename = "defaultLanguageTag", deserialize_with = "nullable")]
                    default: String,
                    #[serde(rename = "supportedLanguageTags", deserialize_with = "nullable")]
                    supported: Vec<String>,
                }
                #[derive(Deserialize, Default)]
                #[serde(default)]
                struct Sf {
                    #[serde(deserialize_with = "nullable")]
                    id: String,
                    #[serde(deserialize_with = "nullable")]
                    attributes: Attrs,
                }
                #[derive(Deserialize, Default)]
                #[serde(default)]
                struct Doc {
                    #[serde(deserialize_with = "nullable")]
                    data: Vec<Sf>,
                }
                let (mut sf, mut lang) = (String::new(), String::new());
                if let Ok(Some(doc)) = self.get::<Doc>("/v1/me/storefront").await
                    && let Some(s) = doc.data.first()
                {
                    sf = s.id.clone();
                    lang = pick_lang(&self.locale, &s.attributes.default, &s.attributes.supported);
                }
                if sf.is_empty() {
                    sf = "us".into();
                }
                (sf, lang)
            })
            .await
            .0
            .clone()
    }

    /// Adds the language to catalog requests.
    fn localize(&self, path: &str) -> String {
        let lang = self.sf.get().map(|s| s.1.as_str()).unwrap_or("");
        if !path.starts_with("/v1/catalog/") || lang.is_empty() || path.contains("l=") {
            return path.to_string();
        }
        let sep = if path.contains('?') { '&' } else { '?' };
        format!("{path}{sep}l={}", query_escape(lang))
    }

    /// A path under the user's storefront.
    pub(crate) async fn catalog_path(&self, p: &str) -> String {
        format!("/v1/catalog/{}{p}", self.storefront().await)
    }
}

/// Turns a locale like de_DE.UTF-8 into a language tag the storefront
/// supports, matching the language alone if the region differs; "" means
/// the storefront's default.
pub fn pick_lang(locale: &str, def: &str, supported: &[String]) -> String {
    let locale = locale.split('.').next().unwrap_or("");
    let locale = locale.split('@').next().unwrap_or("");
    let tag = locale.replace('_', "-");
    if tag.is_empty() || tag == "C" || tag == "POSIX" {
        return String::new();
    }
    let lang = tag.split('-').next().unwrap_or("");
    let mut best = "";
    for s in supported {
        // The exact tag wins; else the first of the same language.
        if s.eq_ignore_ascii_case(&tag)
            || (best.is_empty() && s.split('-').next().unwrap_or("").eq_ignore_ascii_case(lang))
        {
            best = s;
        }
    }
    if best.eq_ignore_ascii_case(def) {
        return String::new();
    }
    best.to_string()
}

pub(crate) fn items(rs: &[Resource], kind: &str, catalog: bool) -> Vec<Item> {
    rs.iter().map(|r| item(r, kind, catalog)).collect()
}

pub(crate) fn item(r: &Resource, kind: &str, catalog: bool) -> Item {
    let a = &r.attributes;
    let artist = if kind == KIND_PLAYLIST {
        attr_str(a, "curatorName")
    } else {
        attr_str(a, "artistName")
    };
    let mut it = Item {
        kind: kind.to_string(),
        id: r.id.clone(),
        name: attr_str(a, "name").to_string(),
        artist: artist.to_string(),
        catalog,
        artwork: artwork_url(a),
        editable: attr_bool(a, "canEdit"),
        ..Default::default()
    };
    it.details = explore::attribute_details(a);
    it.favorite = attr_bool(a, "inFavorites");
    it.url = explore::attribute_string(a, "url");
    if let Some(p) = a
        .get("previews")
        .and_then(Value::as_array)
        .and_then(|p| p.first())
        .and_then(Value::as_object)
    {
        it.preview_url = explore::attribute_string(p, "url");
    }
    if kind != KIND_ALBUM && kind != KIND_PLAYLIST && kind != KIND_ARTIST && kind != KIND_STATION {
        it.route = r.href.clone();
    }
    let mut info: Vec<String> = Vec::new();
    let date = attr_str(a, "releaseDate");
    if date.len() >= 4 && date.is_char_boundary(4) {
        info.push(date[..4].to_string());
    }
    let label = attr_str(a, "recordLabel");
    if !label.is_empty() {
        info.push(label.to_string());
    }
    let count = attr_i64(a, "trackCount");
    if count > 0 {
        info.push(format!("{count} songs"));
    }
    for t in a
        .get("audioTraits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match t.as_str() {
            Some("lossless" | "hi-res-lossless") => info.push("Lossless".into()),
            Some("atmos" | "spatial") => info.push("Dolby Atmos".into()),
            _ => {}
        }
    }
    if attr_bool(a, "isLive") {
        info.push("live".into());
    }
    it.info = text::dedupe(info).join(" · ");
    for key in ["editorialNotes", "description"] {
        let Some(n) = a.get(key).and_then(Value::as_object) else {
            continue;
        };
        it.note = plain(attr_str(n, "short"));
        if it.note.is_empty() {
            it.note = plain(attr_str(n, "standard"));
        }
        if !it.note.is_empty() {
            break;
        }
    }
    it
}

/// Maps an API resource type to an Item kind and whether it is from the
/// catalog; None for what brumm does not show.
pub(crate) fn kind_of(typ: &str) -> Option<(String, bool)> {
    let catalog = !typ.starts_with("library-");
    let bare = typ.strip_prefix("library-").unwrap_or(typ);
    match bare {
        "albums" => Some((KIND_ALBUM.into(), catalog)),
        "playlists" => Some((KIND_PLAYLIST.into(), catalog)),
        "artists" => Some((KIND_ARTIST.into(), catalog)),
        "stations" => Some((KIND_STATION.into(), true)),
        "music-videos" | "playlist-folders" | "genres" | "station-genres" | "activities"
        | "curators" | "apple-curators" | "record-labels" => Some((bare.to_string(), catalog)),
        _ => None,
    }
}

/// Splits a list of any resources into things to open and songs.
pub(crate) fn mixed(rs: &[Resource]) -> (Vec<Item>, Vec<Track>) {
    let its = rs
        .iter()
        .filter_map(|r| kind_of(&r.typ).map(|(k, c)| item(r, &k, c)))
        .collect();
    (its, tracks(rs))
}

pub(crate) fn tracks(rs: &[Resource]) -> Vec<Track> {
    let mut out = Vec::with_capacity(rs.len());
    for r in rs {
        if !r.typ.is_empty() && r.typ != "songs" && r.typ != "library-songs" {
            continue; // music videos and the like do not play as audio
        }
        let a = &r.attributes;
        let mut id = r.id.clone();
        if let Some(p) = a
            .get("playParams")
            .and_then(|p| p.get("id"))
            .and_then(Value::as_str)
            && !p.is_empty()
        {
            id = p.to_string();
        }
        out.push(Track {
            id,
            title: attr_str(a, "name").to_string(),
            artist: attr_str(a, "artistName").to_string(),
            album: attr_str(a, "albumName").to_string(),
            duration: attr_i64(a, "durationInMillis") as f64 / 1000.0,
            artwork: artwork_url(a),
            number: attr_i64(a, "trackNumber"),
            details: explore::attribute_details(a),
            favorite: attr_bool(a, "inFavorites"),
        });
    }
    out
}

/// What music.apple.com link names: an album or playlist to open, and the
/// song within it, if the link names one.
pub fn parse_link(link: &str) -> Option<(Item, String)> {
    let u = url::Url::parse(link.trim()).ok()?;
    if !u.host_str().unwrap_or("").ends_with("music.apple.com") {
        return None;
    }
    let parts: Vec<&str> = u.path().trim_matches('/').split('/').collect();
    if parts.len() < 3 {
        return None;
    }
    let id = parts[parts.len() - 1].to_string();
    let query = |k: &str| {
        u.query_pairs()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    };
    let named = |kind: &str, name: &str| Item {
        kind: kind.into(),
        id: id.clone(),
        name: name.into(),
        catalog: true,
        ..Default::default()
    };
    match parts[1] {
        "album" => Some((named(KIND_ALBUM, "Album"), query("i"))),
        "playlist" => Some((named(KIND_PLAYLIST, "Playlist"), String::new())),
        "artist" => Some((named(KIND_ARTIST, "Artist"), String::new())),
        "song" => Some((Item::default(), id)),
        _ => None,
    }
}

impl Client {
    // Library listings.

    pub async fn playlists(&self) -> (Vec<Item>, Option<Error>) {
        let (rs, e) = self
            .all("/v1/me/library/playlists?limit=100&extend=inFavorites", 20)
            .await;
        (items(&rs, KIND_PLAYLIST, false), e)
    }

    pub async fn albums(&self) -> (Vec<Item>, Option<Error>) {
        let (rs, e) = self
            .all("/v1/me/library/albums?limit=100&extend=inFavorites", 100)
            .await;
        (items(&rs, KIND_ALBUM, false), e)
    }

    pub async fn artists(&self) -> (Vec<Item>, Option<Error>) {
        let (rs, e) = self
            .all("/v1/me/library/artists?limit=100&extend=inFavorites", 100)
            .await;
        (items(&rs, KIND_ARTIST, false), e)
    }

    pub async fn songs(&self) -> (Vec<Track>, Option<Error>) {
        let (rs, e) = self
            .all("/v1/me/library/songs?limit=100&extend=inFavorites", 300)
            .await;
        (tracks(&rs), e)
    }

    /// A playlist's or an album's songs.
    pub async fn tracks(&self, it: &Item) -> (Vec<Track>, Option<Error>) {
        let id = text::path_escape(&it.id);
        let mut storefront = String::new();
        if it.catalog && !it.route.is_empty() && explore::valid_route(&it.route).is_ok() {
            let path = it.route.split('?').next().unwrap_or("");
            let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
            if parts.len() == 5
                && parts[0] == "v1"
                && parts[1] == "catalog"
                && parts[3] == format!("{}s", it.kind)
                && parts[4] == it.id
            {
                storefront = parts[2].to_string();
            }
        }
        if it.catalog && storefront.is_empty() {
            storefront = self.storefront().await;
        }
        let path = match (it.catalog, it.kind == KIND_PLAYLIST) {
            (true, true) => format!("/v1/catalog/{storefront}/playlists/{id}/tracks?limit=100"),
            (true, false) => format!("/v1/catalog/{storefront}/albums/{id}/tracks?limit=100"),
            (false, true) => format!("/v1/me/library/playlists/{id}/tracks?limit=100"),
            (false, false) => format!("/v1/me/library/albums/{id}/tracks?limit=100"),
        };
        let (rs, e) = self.all(&path, 50).await;
        (tracks(&rs), e)
    }

    /// An artist's albums.
    pub async fn artist_albums(&self, it: &Item) -> (Vec<Item>, Option<Error>) {
        let id = text::path_escape(&it.id);
        let path = if it.catalog {
            format!(
                "/v1/catalog/{}/artists/{id}/albums?limit=100",
                self.storefront().await
            )
        } else {
            format!("/v1/me/library/artists/{id}/albums?limit=100")
        };
        let (rs, e) = self.all(&path, 10).await;
        (items(&rs, KIND_ALBUM, it.catalog), e)
    }

    /// The first resource of a song's relationship, in the library for
    /// library songs, in the catalog otherwise.
    async fn song_relation(
        &self,
        song_id: &str,
        rel: &str,
        kind: &str,
        none: &str,
    ) -> Result<Item> {
        let id = text::path_escape(song_id);
        let catalog = !song_id.starts_with("i.");
        let path = if catalog {
            format!("/v1/catalog/{}/songs/{id}/{rel}", self.storefront().await)
        } else {
            format!("/v1/me/library/songs/{id}/{rel}")
        };
        let rs = self.fetch(&path).await?;
        match rs.first() {
            Some(r) => Ok(item(r, kind, catalog)),
            None => Err(err(none)),
        }
    }

    /// The album a song belongs to.
    pub async fn album_of(&self, song_id: &str) -> Result<Item> {
        self.song_relation(song_id, "albums", KIND_ALBUM, "no album for this song")
            .await
    }

    /// The artist of a song.
    pub async fn artist_of(&self, song_id: &str) -> Result<Item> {
        self.song_relation(song_id, "artists", KIND_ARTIST, "no artist for this song")
            .await
    }

    /// Adds catalog songs, albums or playlists (kind is "songs", "albums"
    /// or "playlists") to the library. Apple adds them in the background:
    /// they show up in library listings a few seconds later. There is no
    /// way back through the API — removing is done in the Music app.
    pub async fn add_to_library(&self, kind: &str, ids: &[String]) -> Result<()> {
        if !matches!(kind, "songs" | "albums" | "playlists") {
            return Err(err(format!("cannot add {kind} to the library")));
        }
        if ids.is_empty() {
            return Ok(());
        }
        let q = text::encode_query(&[(format!("ids[{kind}]"), ids.join(","))]);
        self.send(Method::POST, &format!("/v1/me/library?{q}"), None)
            .await?;
        Ok(())
    }

    /// A song's public music.apple.com address.
    pub async fn link(&self, song_id: &str) -> Result<String> {
        let id = text::path_escape(song_id);
        let path = if song_id.starts_with("i.") {
            format!("/v1/me/library/songs/{id}/catalog")
        } else {
            format!("/v1/catalog/{}/songs/{id}", self.storefront().await)
        };
        let rs = self.fetch(&path).await?;
        match rs.first().map(|r| attr_str(&r.attributes, "url")) {
            Some(u) if !u.is_empty() => Ok(u.to_string()),
            _ => Err(err("this song has no public link")),
        }
    }
}

#[cfg(test)]
pub(crate) mod testserver {
    //! A stand-in for the Apple Music API: canned JSON by path.
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// One request as the server saw it.
    #[derive(Clone, Debug)]
    pub struct Seen {
        pub method: String,
        pub target: String,
        pub headers: HashMap<String, String>,
        pub body: Vec<u8>,
    }

    pub type Handler = Arc<dyn Fn(&Seen) -> (u16, String) + Send + Sync>;

    /// Serves handler on a loopback port; returns its base address and
    /// what it was asked.
    pub async fn serve(handler: Handler) -> (String, Arc<Mutex<Vec<Seen>>>) {
        let ln = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", ln.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = ln.accept().await else {
                    return;
                };
                let handler = handler.clone();
                let log = log.clone();
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    let head_end = loop {
                        let n = s.read(&mut chunk).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            break i + 4;
                        }
                    };
                    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                    let mut lines = head.split("\r\n");
                    let first: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
                    let mut headers = HashMap::new();
                    for l in lines {
                        if let Some((k, v)) = l.split_once(':') {
                            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
                        }
                    }
                    let len: usize = headers
                        .get("content-length")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    let mut body = buf[head_end..].to_vec();
                    while body.len() < len {
                        let n = s.read(&mut chunk).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        body.extend_from_slice(&chunk[..n]);
                    }
                    let seen = Seen {
                        method: first[0].to_string(),
                        target: first.get(1).unwrap_or(&"").to_string(),
                        headers,
                        body,
                    };
                    log.lock().unwrap().push(seen.clone());
                    let (status, text) = handler(&seen);
                    let reply = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                        text.len()
                    );
                    let _ = s.write_all(reply.as_bytes()).await;
                });
            }
        });
        (base, seen)
    }

    /// The path and the query of a request target.
    pub fn split(target: &str) -> (String, HashMap<String, String>) {
        let (path, q) = target.split_once('?').unwrap_or((target, ""));
        let u = url::Url::parse(&format!("http://x/?{q}")).unwrap();
        (
            path.to_string(),
            u.query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect(),
        )
    }

    /// A fake API answering paths (with filter[identity]/[featured] as
    /// #suffix) from docs; the storefront is "de", offering German and
    /// English.
    pub async fn fake_api(docs: Vec<(&str, &str)>) -> (super::Client, Arc<Mutex<Vec<Seen>>>) {
        let docs: HashMap<String, String> = docs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let (base, seen) = serve(Arc::new(move |s: &Seen| {
            let (path, q) = split(&s.target);
            if path == "/v1/me/storefront" {
                return (
                    200,
                    r#"{"data":[{"id":"de","attributes":{"defaultLanguageTag":"de-DE","supportedLanguageTags":["de-DE","en-GB"]}}]}"#.into(),
                );
            }
            let mut key = path;
            for f in ["filter[identity]", "filter[featured]"] {
                if let Some(v) = q.get(f) {
                    key = format!("{key}#{v}");
                }
            }
            match docs.get(&key) {
                Some(d) => (200, d.clone()),
                None => (404, "{}".into()),
            }
        }))
        .await;
        let mut c = super::Client::with_base(&base, "dev", "user");
        c.locale = "de_DE.UTF-8".into(); // the storefront's own: no language asked for
        (c, seen)
    }
}

#[cfg(test)]
mod tests {
    use super::testserver::*;
    use super::*;
    use std::sync::Arc;

    #[test]
    fn pick_lang_cases() {
        let sup: Vec<String> = ["en-US", "es-MX", "fr-FR"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        for (locale, want) in [
            ("en_US.UTF-8", ""),
            ("es_ES.UTF-8", "es-MX"),
            ("fr_FR@euro", "fr-FR"),
            ("de_DE.UTF-8", ""),
            ("C", ""),
            ("", ""),
        ] {
            assert_eq!(pick_lang(locale, "en-US", &sup), want, "{locale}");
        }
    }

    #[test]
    fn parse_link_cases() {
        let (it, song) = parse_link("https://music.apple.com/de/album/x/123?i=456").unwrap();
        assert_eq!(
            (it.kind.as_str(), it.id.as_str(), song.as_str()),
            (KIND_ALBUM, "123", "456")
        );
        let (it, song) = parse_link("https://music.apple.com/us/song/name/789").unwrap();
        assert_eq!((it.id.as_str(), song.as_str()), ("", "789"));
        assert!(parse_link("https://example.com/x").is_none());
    }

    #[tokio::test]
    async fn library_pagination_keeps_favorites_and_limit() {
        let (base, seen) = serve(Arc::new(|s: &Seen| {
            let (_, q) = split(&s.target);
            assert_eq!(q.get("limit").map(String::as_str), Some("100"), "pagination lost the page size: {}", s.target);
            assert_eq!(q.get("extend").map(String::as_str), Some("inFavorites"), "pagination lost the favorites: {}", s.target);
            if q.get("offset").map(String::as_str) == Some("1") {
                return (200, r#"{"data":[{"id":"i.two","type":"library-songs","attributes":{"name":"Two","inFavorites":true}}]}"#.into());
            }
            (200, r#"{"next":"/v1/me/library/songs?offset=1","data":[{"id":"i.one","type":"library-songs","attributes":{"name":"One","inFavorites":false}}]}"#.into())
        }))
        .await;
        let c = Client::with_base(&base, "dev", "user");
        let (ts, e) = c.songs().await;
        assert!(e.is_none(), "{e:?}");
        assert_eq!(seen.lock().unwrap().len(), 2);
        assert_eq!(ts.len(), 2);
        assert!(!ts[0].favorite && ts[1].favorite);
        let s = &seen.lock().unwrap()[0];
        assert_eq!(
            s.headers.get("authorization").map(String::as_str),
            Some("Bearer dev")
        );
        assert_eq!(
            s.headers.get("music-user-token").map(String::as_str),
            Some("user")
        );
    }

    #[tokio::test]
    async fn listings_report_partial_and_unauthorized() {
        let (base, _) = serve(Arc::new(|s: &Seen| {
            if s.target.contains("offset=1") {
                return (500, "{}".into());
            }
            (200, r#"{"next":"/v1/me/library/albums?offset=1","data":[{"id":"l.1","type":"library-albums","attributes":{"name":"A"}}]}"#.into())
        }))
        .await;
        let c = Client::with_base(&base, "dev", "user");
        let (its, e) = c.albums().await;
        assert_eq!(its.len(), 1);
        assert_eq!(e, Some(Error::Partial));

        let (base, _) = serve(Arc::new(|_: &Seen| (401, "{}".into()))).await;
        let c = Client::with_base(&base, "dev", "user");
        let (its, e) = c.playlists().await;
        assert!(its.is_empty());
        assert_eq!(e, Some(Error::Unauthorized));
        assert_eq!(c.link("123").await, Err(Error::Unauthorized));

        let (base, _) = serve(Arc::new(|_: &Seen| (404, "{}".into()))).await;
        let c = Client::with_base(&base, "dev", "user");
        let (its, e) = c.artists().await;
        assert!(
            its.is_empty() && e.is_none(),
            "an empty library is no error"
        );
    }

    #[tokio::test]
    async fn routed_container_tracks_follow_all_pages_in_their_storefront() {
        for kind in [KIND_ALBUM, KIND_PLAYLIST] {
            // A US chart card keeps its storefront even when the account
            // browses Germany.
            let path = format!("/v1/catalog/us/{kind}s/123/tracks");
            let p = path.clone();
            let (base, _) = serve(Arc::new(move |s: &Seen| {
                let (got, q) = split(&s.target);
                if got == "/v1/me/storefront" {
                    return (200, r#"{"data":[{"id":"de"}]}"#.into());
                }
                if got != p {
                    return (404, "{}".into());
                }
                if q.get("offset").map(String::as_str) == Some("1") {
                    return (200, r#"{"data":[{"id":"second","type":"songs","attributes":{"name":"Second"}}]}"#.into());
                }
                (200, format!(r#"{{"data":[{{"id":"first","type":"songs","attributes":{{"name":"First"}}}}],"next":"{p}?offset=1"}}"#))
            }))
            .await;
            let c = Client::with_base(&base, "dev", "user");
            let it = Item {
                kind: kind.into(),
                id: "123".into(),
                catalog: true,
                route: format!("/v1/catalog/us/{kind}s/123"),
                ..Default::default()
            };
            let (ts, e) = c.tracks(&it).await;
            assert!(e.is_none(), "{e:?}");
            assert_eq!(ts.len(), 2);
            assert_eq!(ts[1].id, "second");
        }
    }

    #[test]
    fn items_carry_facts_and_notes() {
        let r: Resource = serde_json::from_str(
            r#"{"id":"pl.1","type":"playlists","href":"/x","attributes":{"name":"Mix","curatorName":"Apple Music",
            "releaseDate":"2013-05-17","trackCount":13,"audioTraits":["lossless","atmos","spatial"],"isLive":true,
            "description":{"short":"<b>Your</b> favourites &amp; more"},"artwork":{"url":"https://a/{w}x{h}.jpg"},
            "playParams":{"id":"pl.1"},"canEdit":true}}"#,
        )
        .unwrap();
        let it = item(&r, KIND_PLAYLIST, true);
        assert_eq!(it.artist, "Apple Music");
        assert_eq!(it.info, "2013 · 13 songs · Lossless · Dolby Atmos · live");
        assert_eq!(it.note, "Your favourites & more");
        assert_eq!(it.artwork, "https://a/600x600.jpg");
        assert!(it.editable && it.route.is_empty());
    }
}
