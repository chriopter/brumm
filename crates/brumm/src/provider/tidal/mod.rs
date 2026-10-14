//! Tidal v2 JSON:API adapter. IDs and navigation are opaque to the frontends.
pub mod auth;
pub mod device;
mod features;
mod mapping;
pub mod session;
use crate::{
    error::{Error, Result, err},
    music::{ExplorePage, Item, Ref, Shelf, Track},
    provider::{Api, Catalog, Library, Listing, Reply},
};
use mapping::Document;
use reqwest::Method;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    time::Duration,
};

type Membership =
    std::sync::Arc<tokio::sync::Mutex<BTreeMap<String, (std::time::Instant, HashSet<String>)>>>;

const BASE: &str = "https://openapi.tidal.com/v2/";
const TRACK_INCLUDE: &str = "albums,artists,albums.coverArt";
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: url::Url,
    account: String,
    country: String,
    refresh_seed: String,
    membership: Membership,
}
impl Client {
    pub fn new(session: &auth::Session) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            base: url::Url::parse(BASE).unwrap(),
            account: session.user_id.clone(),
            country: session.country.clone(),
            refresh_seed: crate::engine::random_hex(8),
            membership: Default::default(),
        })
    }
    fn url(&self, route: &str) -> Result<url::Url> {
        let base = &self.base;
        let mut url = base.join(route).map_err(|_| err("invalid Tidal route"))?;
        if url.origin() != base.origin()
            || !url.path().starts_with("/v2/")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(err("invalid Tidal route"));
        }
        if !url.query_pairs().any(|(key, _)| key == "countryCode") {
            url.query_pairs_mut()
                .append_pair("countryCode", &self.country);
        }
        Ok(url)
    }
    async fn request(&self, method: Method, route: &str, body: Option<Value>) -> Result<Value> {
        let url = self.url(route)?;
        let mut session = auth::credentials(&self.account, None).await?;
        for attempt in 0..3 {
            let mut request = self
                .http
                .request(method.clone(), url.clone())
                .bearer_auth(&session.access_token)
                .header("Accept", "application/vnd.api+json");
            if let Some(body) = &body {
                request = request
                    .header("Content-Type", "application/vnd.api+json")
                    .json(body);
            }
            let response = request.send().await?;
            let cfg = crate::config::Config::load()?;
            if cfg.effective_provider() != "tidal" || cfg.tidal.user_id != self.account {
                return Err(Error::Unauthorized);
            }
            let status = response.status();
            if status.as_u16() == 401 && attempt == 0 {
                session = auth::credentials(&self.account, Some(&session.access_token)).await?;
                continue;
            }
            if status.as_u16() == 429 && method == Method::GET && attempt < 2 {
                let seconds = response
                    .headers()
                    .get("retry-after")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(2);
                if seconds > 10 {
                    return Err(err("Tidal rate limit reached; try again later"));
                }
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                continue;
            }
            if !status.is_success() {
                let resource = url
                    .path()
                    .trim_start_matches("/v2/")
                    .split('/')
                    .next()
                    .unwrap_or("request")
                    .to_owned();
                let response: Value = response.json().await.unwrap_or(Value::Null);
                let code = response["errors"][0]["code"].as_str().unwrap_or("");
                let diagnostic = if code.chars().all(|c| c.is_ascii_uppercase() || c == '_')
                    && !code.is_empty()
                {
                    format!("; {resource}: {code}")
                } else {
                    format!("; {resource}")
                };
                return match status.as_u16() {
                    401 => Err(Error::Unauthorized),
                    403 => Err(err(
                        "Tidal: this account or application cannot access this feature",
                    )),
                    404 => Err(err("Tidal: content is unavailable")),
                    _ => Err(err(format!("Tidal request failed ({status}{diagnostic})"))),
                };
            }
            if status.as_u16() == 204 {
                return Ok(Value::Null);
            }
            let bytes = response.bytes().await?;
            if bytes.is_empty() {
                return Ok(Value::Null);
            }
            return Ok(serde_json::from_slice(&bytes)?);
        }
        Err(err("Tidal request could not complete"))
    }
    async fn get(&self, route: &str) -> Result<Value> {
        self.request(Method::GET, route, None).await
    }
    fn id(id: &str) -> String {
        url::form_urlencoded::byte_serialize(id.as_bytes()).collect()
    }
    async fn resource(&self, kind: &str, id: &str) -> Result<(Document, Value)> {
        let include = match kind {
            "tracks" => TRACK_INCLUDE,
            "albums" => "artists,coverArt",
            "artists" => "profileArt",
            "playlists" => "coverArt,owners",
            _ => return Err(Error::Unsupported),
        };
        let value = self
            .get(&format!("{kind}/{}?include={include}", Self::id(id)))
            .await?;
        let doc = Document::new(&value);
        let resource = value["data"]
            .as_object()
            .map(|_| value["data"].clone())
            .ok_or_else(|| err("Tidal returned no resource"))?;
        Ok((doc, resource))
    }
    /// Relationship lists retain order and repeated tracks. Only navigation URLs are deduplicated.
    async fn pages(&self, route: &str) -> (Vec<(Document, Value)>, Option<Error>) {
        let mut result = Vec::new();
        let mut next = route.to_string();
        let mut visited = HashSet::new();
        for _ in 0..200 {
            if !visited.insert(next.clone()) {
                return (result, Some(Error::Partial));
            }
            let value = match self.get(&next).await {
                Ok(v) => v,
                Err(e) => return (result, Some(e)),
            };
            let doc = Document::new(&value);
            for data in Document::data(&value) {
                result.push((doc.clone(), doc.expand(data).clone()));
            }
            let link = value["links"]["next"]
                .as_str()
                .or_else(|| value["links"]["next"]["href"].as_str())
                .unwrap_or("");
            if link.is_empty() {
                return (result, None);
            }
            // Tidal emits API-root-relative links without the deployed /v2 prefix.
            // Keep the fixed API origin/version while following its opaque cursor.
            let link =
                if link.starts_with('/') && !link.starts_with("//") && !link.starts_with("/v2/") {
                    format!("/v2{link}")
                } else {
                    link.to_owned()
                };
            next = match self
                .url(&next)
                .and_then(|url| url.join(&link).map_err(|_| err("invalid Tidal pagination")))
            {
                Ok(url) => url.to_string(),
                Err(_) => return (result, Some(Error::Partial)),
            };
            if next.is_empty() {
                return (result, None);
            }
            if self.url(&next).is_err() {
                return (result, Some(Error::Partial));
            }
        }
        (result, Some(Error::Partial))
    }
    async fn track_list(&self, route: &str) -> Listing<Track> {
        let (resources, mut error) = self.pages(route).await;
        let mut tracks = Vec::new();
        for (doc, value) in resources {
            if value["type"] == "tracks" {
                if let Some(track) = doc.track(&value) {
                    tracks.push(track);
                } else {
                    error = Some(Error::Partial);
                }
            }
        }
        if !tracks.is_empty() && error.is_some() {
            error = Some(Error::Partial);
        }
        (tracks, error)
    }
    async fn item_list(&self, route: &str, favorite: bool) -> Listing<Item> {
        let (resources, mut error) = self.pages(route).await;
        let mut items = Vec::new();
        for (doc, value) in resources {
            if let Some(mut item) = doc.item(&value, &self.account) {
                item.favorite = favorite;
                items.push(item);
            } else {
                error = Some(Error::Partial);
            }
        }
        if !items.is_empty() && error.is_some() {
            error = Some(Error::Partial);
        }
        (items, error)
    }
    fn collection(kind: &str) -> Result<&'static str> {
        match kind {
            "song" | "songs" | "tracks" => Ok("userCollectionTracks"),
            "album" | "albums" => Ok("userCollectionAlbums"),
            "artist" | "artists" => Ok("userCollectionArtists"),
            "playlist" | "playlists" => Ok("userCollectionPlaylists"),
            _ => Err(Error::Unsupported),
        }
    }
    fn resource_kind(kind: &str) -> Result<&'static str> {
        match kind {
            "song" | "songs" | "tracks" => Ok("tracks"),
            "album" | "albums" => Ok("albums"),
            "artist" | "artists" => Ok("artists"),
            "playlist" | "playlists" => Ok("playlists"),
            _ => Err(Error::Unsupported),
        }
    }
    async fn saved(&self, kind: &str) -> Result<HashSet<String>> {
        if let Some((at, ids)) = self.membership.lock().await.get(kind)
            && at.elapsed() < Duration::from_secs(30)
        {
            return Ok(ids.clone());
        }
        let (resources, error) = self
            .pages(&format!(
                "{}/me/relationships/items",
                Self::collection(kind)?
            ))
            .await;
        if let Some(e) = error {
            return Err(e);
        }
        let ids: HashSet<_> = resources
            .iter()
            .filter_map(|(_, v)| v["id"].as_str().map(str::to_owned))
            .collect();
        self.membership
            .lock()
            .await
            .insert(kind.into(), (std::time::Instant::now(), ids.clone()));
        Ok(ids)
    }
    async fn save(&self, kind: &str, ids: &[String], add: bool) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let data: Vec<_> = ids
            .iter()
            .map(|id| json!({"type": Self::resource_kind(kind).unwrap_or("tracks"), "id": id}))
            .collect();
        self.request(
            if add { Method::POST } else { Method::DELETE },
            &format!("{}/me/relationships/items", Self::collection(kind)?),
            Some(json!({"data": data})),
        )
        .await?;
        self.membership.lock().await.clear();
        Ok(())
    }
    pub async fn player_tracks(&self, ids: &[String]) -> Result<Vec<Track>> {
        let mut mapped = std::collections::HashMap::new();
        for batch in ids.chunks(20) {
            let joined = batch.join(",");
            let value = self
                .get(&format!(
                    "tracks?filter[id]={}&include={TRACK_INCLUDE}",
                    Self::id(&joined)
                ))
                .await?;
            let doc = Document::new(&value);
            for resource in Document::data(&value) {
                if let Some(track) = doc.track(resource) {
                    mapped.insert(track.id.clone(), track);
                }
            }
        }
        ids.iter()
            .map(|id| {
                mapped
                    .get(id)
                    .cloned()
                    .ok_or_else(|| err("Tidal returned incomplete track metadata"))
            })
            .collect()
    }
}

impl Catalog for Client {
    fn search<'a>(&'a self, term: &'a str) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(async move {
            // Tidal permits at most ten include paths, counting implicit parent paths.
            // Fetch artist and playlist artwork separately to retain complete search cards.
            let query = Self::id(term);
            let catalog = format!(
                "searchResults?filter[query]={query}&include=tracks,albums,artists,playlists,tracks.artists,tracks.albums,tracks.albums.coverArt,albums.artists,albums.coverArt"
            );
            let artwork = format!(
                "searchResults?filter[query]={query}&include=artists,artists.profileArt,playlists,playlists.coverArt"
            );
            let (value, artwork) = tokio::join!(self.get(&catalog), self.get(&artwork));
            let value = value?;
            let mut doc = Document::new(&value);
            if let Ok(artwork) = artwork {
                doc.extend(&artwork);
            }
            let mut shelves = Vec::new();
            for (kind, title) in [
                ("tracks", "Tracks"),
                ("albums", "Albums"),
                ("artists", "Artists"),
                ("playlists", "Playlists"),
            ] {
                let mut shelf = Shelf::titled(title);
                for root in Document::data(&value) {
                    if let Some(id) = root["id"].as_str() {
                        shelf.route = format!("tidal:search:{kind}:{id}");
                    }
                    for resource in doc.related(root, kind) {
                        if kind == "tracks" {
                            if let Some(t) = doc.track(resource) {
                                shelf.tracks.push(t);
                            }
                        } else if let Some(i) = doc.item(resource, &self.account) {
                            shelf.items.push(i);
                        }
                    }
                }
                if !shelf.is_empty() {
                    shelves.push(shelf);
                }
            }
            Ok(shelves)
        })
    }
    fn tracks<'a>(&'a self, item: &'a Item) -> Reply<'a, Listing<Track>> {
        Box::pin(async move {
            let kind = match Self::resource_kind(&item.kind) {
                Ok(k) => k,
                Err(e) => return (vec![], Some(e)),
            };
            self.track_list(&format!("{kind}/{}/relationships/items?include=items,items.artists,items.albums,items.albums.coverArt", Self::id(&item.id))).await
        })
    }
    fn album_of<'a>(&'a self, song: &'a str) -> Reply<'a, Result<Item>> {
        Box::pin(async move {
            let (doc, track) = self.resource("tracks", song).await?;
            let album = doc
                .related(&track, "albums")
                .first()
                .copied()
                .ok_or_else(|| err("Tidal track has no album"))?;
            doc.item(album, &self.account)
                .ok_or_else(|| err("Tidal returned incomplete album metadata"))
        })
    }
    fn artist_of<'a>(&'a self, song: &'a str) -> Reply<'a, Result<Item>> {
        Box::pin(async move {
            let (doc, track) = self.resource("tracks", song).await?;
            let artist = doc
                .related(&track, "artists")
                .first()
                .copied()
                .ok_or_else(|| err("Tidal track has no artist"))?;
            doc.item(artist, &self.account)
                .ok_or_else(|| err("Tidal returned incomplete artist metadata"))
        })
    }
    fn link<'a>(&'a self, song: &'a str) -> Reply<'a, Result<String>> {
        Box::pin(async move { Ok(format!("https://tidal.com/browse/track/{}", Self::id(song))) })
    }
    fn resolve<'a>(&'a self, link: &'a str) -> Reply<'a, Result<(Item, String)>> {
        Box::pin(async move {
            let url = url::Url::parse(link).map_err(|_| err("invalid Tidal link"))?;
            if url.scheme() != "https"
                || !matches!(
                    url.host_str(),
                    Some("tidal.com" | "www.tidal.com" | "listen.tidal.com")
                )
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(err("unsupported Tidal link"));
            }
            let mut parts: Vec<_> = url
                .path_segments()
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
                .collect();
            if parts.first() == Some(&"browse") {
                parts.remove(0);
            }
            if parts.len() != 2 {
                return Err(err("unsupported Tidal link"));
            }
            if parts[0] == "track" {
                let item = self.album_of(parts[1]).await?;
                return Ok((item, parts[1].into()));
            }
            let kind = Self::resource_kind(parts[0])?;
            let (doc, value) = self.resource(kind, parts[1]).await?;
            Ok((
                doc.item(&value, &self.account)
                    .ok_or_else(|| err("Tidal returned incomplete metadata"))?,
                String::new(),
            ))
        })
    }
}
impl Library for Client {
    fn playlists<'a>(&'a self) -> Reply<'a, Listing<Item>> {
        Box::pin(async move {
            let (saved, saved_error) = self.item_list("userCollectionPlaylists/me/relationships/items?include=items,items.coverArt,items.owners", true).await;
            let (mut owned, owned_error) = self
                .item_list(
                    "playlists?filter[owners.id]=me&include=coverArt,owners",
                    false,
                )
                .await;
            for item in &mut owned {
                item.editable = true;
            }
            let mut all = owned;
            for item in saved {
                if let Some(existing) = all.iter_mut().find(|it| it.id == item.id) {
                    existing.favorite = true;
                } else {
                    all.push(item);
                }
            }
            let failure = saved_error.or(owned_error);
            let failure = if failure.is_some() && !all.is_empty() {
                Some(Error::Partial)
            } else {
                failure
            };
            (all, failure)
        })
    }
    fn albums<'a>(&'a self) -> Reply<'a, Listing<Item>> {
        Box::pin(async move {
            self.item_list("userCollectionAlbums/me/relationships/items?include=items,items.artists,items.coverArt",true).await
        })
    }
    fn artists<'a>(&'a self) -> Reply<'a, Listing<Item>> {
        Box::pin(async move {
            self.item_list(
                "userCollectionArtists/me/relationships/items?include=items,items.profileArt",
                true,
            )
            .await
        })
    }
    fn songs<'a>(&'a self) -> Reply<'a, Listing<Track>> {
        Box::pin(async move {
            let (mut tracks,error) = self.track_list("userCollectionTracks/me/relationships/items?include=items,items.artists,items.albums,items.albums.coverArt").await;
            for track in &mut tracks {
                track.favorite = true;
            }
            (tracks, error)
        })
    }
}
impl Api for Client {
    fn home<'a>(&'a self) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(async move { Ok(self.home_page().await?.shelves) })
    }
    fn artist<'a>(&'a self, item: &'a Item) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(async move {
            let id = Self::id(&item.id);
            let (tracks,error) = self.track_list(&format!("artists/{id}/relationships/tracks?collapseBy=FINGERPRINT&include=tracks,tracks.artists,tracks.albums,tracks.albums.coverArt")).await;
            if let Some(e) = error {
                return Err(e);
            }
            let (albums,error) = self.item_list(&format!("artists/{id}/relationships/albums?include=albums,albums.artists,albums.coverArt"),false).await;
            if let Some(e) = error {
                return Err(e);
            }
            Ok(vec![
                Shelf {
                    title: "Top Tracks".into(),
                    tracks,
                    ..Default::default()
                },
                Shelf {
                    title: "Releases".into(),
                    items: albums,
                    ..Default::default()
                },
            ])
        })
    }
    fn open<'a>(&'a self, item: &'a Item) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(async move {
            if item.kind == "artist" {
                self.artist(item).await
            } else {
                Err(Error::Unsupported)
            }
        })
    }
    fn favorites<'a>(&'a self, refs: &'a [Ref]) -> Reply<'a, Result<BTreeMap<String, bool>>> {
        Box::pin(async move {
            let mut result = BTreeMap::new();
            for resource in refs {
                let kind = Self::resource_kind(&resource.kind)?;
                result.insert(
                    format!("{}:{}", resource.kind, resource.id),
                    self.saved(kind).await?.contains(&resource.id),
                );
            }
            Ok(result)
        })
    }
    fn set_favorite<'a>(&'a self, refs: &'a [Ref], saved: bool) -> Reply<'a, Result<()>> {
        Box::pin(async move {
            for resource in refs {
                self.save(&resource.kind, std::slice::from_ref(&resource.id), saved)
                    .await?;
            }
            Ok(())
        })
    }
    fn add_to_library<'a>(&'a self, kind: &'a str, ids: &'a [String]) -> Reply<'a, Result<()>> {
        Box::pin(async move { self.save(kind, ids, true).await })
    }
    fn favorite<'a>(&'a self, refs: &'a [Ref]) -> Reply<'a, Result<()>> {
        self.set_favorite(refs, true)
    }
    fn create_playlist<'a>(
        &'a self,
        name: &'a str,
        songs: &'a [String],
    ) -> Reply<'a, Result<Item>> {
        Box::pin(async move {
            if name.trim().is_empty() {
                return Err(err("playlist name is required"));
            }
            let value = self.request(Method::POST,"playlists",Some(json!({"data":{"type":"playlists","attributes":{"name":name.trim(),"accessType":"UNLISTED"}}}))).await?;
            let doc = Document::new(&value);
            let mut item = doc
                .item(&value["data"], &self.account)
                .ok_or_else(|| err("Tidal returned no playlist"))?;
            item.editable = true;
            if !songs.is_empty()
                && let Err(e) = self.add_to_playlist(&item.id, songs).await
            {
                return Err(err(format!(
                    "Playlist created ({}), but tracks were not added: {e}. Open Playlists to retry.",
                    item.id
                )));
            }
            Ok(item)
        })
    }
    fn add_to_playlist<'a>(&'a self, id: &'a str, songs: &'a [String]) -> Reply<'a, Result<()>> {
        Box::pin(async move {
            if songs.is_empty() {
                return Ok(());
            }
            let (doc, resource) = self.resource("playlists", id).await?;
            if !doc
                .item(&resource, &self.account)
                .is_some_and(|item| item.editable)
            {
                return Err(err("Only your owned Tidal playlists can be edited"));
            }
            let data: Vec<_> = songs
                .iter()
                .map(|id| json!({"type":"tracks","id":id}))
                .collect();
            self.request(
                Method::POST,
                &format!("playlists/{}/relationships/items", Self::id(id)),
                Some(json!({"data":data})),
            )
            .await?;
            Ok(())
        })
    }
    fn create_container<'a>(
        &'a self,
        kind: &'a str,
        name: &'a str,
        _parent: &'a str,
    ) -> Reply<'a, Result<Item>> {
        Box::pin(async move {
            if kind == "playlist" {
                self.create_playlist(name, &[]).await
            } else {
                Err(Error::Unsupported)
            }
        })
    }
    fn feature<'a>(&'a self, route: &'a str) -> Reply<'a, Result<ExplorePage>> {
        Box::pin(async move {
            if let Some(search) = route.strip_prefix("tidal:search:") {
                let (kind, id) = search.split_once(':').ok_or(Error::Unsupported)?;
                if !matches!(kind, "tracks" | "albums" | "artists" | "playlists") {
                    return Err(Error::Unsupported);
                }
                let base = format!("searchResults/{}/relationships/{kind}", Self::id(id));
                let mut shelf = Shelf::titled(match kind {
                    "tracks" => "Tracks",
                    "albums" => "Albums",
                    "artists" => "Artists",
                    _ => "Playlists",
                });
                let error = if kind == "tracks" {
                    let (tracks,error) = self.track_list(&format!("{base}?include=tracks,tracks.artists,tracks.albums,tracks.albums.coverArt")).await;
                    shelf.tracks = tracks;
                    error
                } else {
                    let include = match kind {
                        "artists" => "artists,artists.profileArt",
                        "albums" => "albums,albums.artists,albums.coverArt",
                        _ => "playlists,playlists.coverArt,playlists.owners",
                    };
                    let (items, error) = self
                        .item_list(&format!("{base}?include={include}"), false)
                        .await;
                    shelf.items = items;
                    error
                };
                if let Some(error) = error {
                    return Err(error);
                }
                return Ok(ExplorePage {
                    title: shelf.title.clone(),
                    shelves: vec![shelf],
                    ..Default::default()
                });
            }
            self.feature_page(route).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture(
        pages: Vec<(u16, Value)>,
    ) -> (Client, tokio::task::JoinHandle<Vec<crate::httpd::Request>>) {
        let session = auth::Session {
            user_id: "fixture".into(),
            access_token: "fictional".into(),
            country: "GB".into(),
            expires_at: chrono::Utc::now().timestamp() + 3600,
            ..Default::default()
        };
        crate::config::Config {
            provider: "tidal".into(),
            tidal: session.clone(),
            ..Default::default()
        }
        .save()
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut client = Client::new(&session).unwrap();
        client.base =
            url::Url::parse(&format!("http://{}/v2/", listener.local_addr().unwrap())).unwrap();
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for (status, body) in pages {
                let (mut socket, _) = listener.accept().await.unwrap();
                let request = crate::httpd::read_request(&mut socket, 4096).await.unwrap();
                assert_eq!(
                    request.headers.get("authorization").map(String::as_str),
                    Some("Bearer fictional")
                );
                assert!(request.target.contains("countryCode=GB"));
                let url = url::Url::parse(&format!("http://fixture{}", request.target)).unwrap();
                if let Some((_, include)) = url.query_pairs().find(|(key, _)| key == "include") {
                    let mut paths = HashSet::new();
                    for path in include.split(',') {
                        let mut parent = String::new();
                        for part in path.split('.') {
                            if !parent.is_empty() {
                                parent.push('.');
                            }
                            parent.push_str(part);
                            paths.insert(parent.clone());
                        }
                    }
                    assert!(paths.len() <= 10, "Tidal include limit exceeded");
                    if url.path().starts_with("/v2/userDailyMixes/")
                        || url.path().starts_with("/v2/userDiscoveryMixes/")
                        || url.path().starts_with("/v2/userNewReleaseMixes/")
                    {
                        assert!(!include.contains("items.albums"));
                        assert!(!include.contains("items.artists"));
                    }
                }
                crate::httpd::respond(
                    &mut socket,
                    status,
                    &[("Content-Type", "application/vnd.api+json")],
                    &serde_json::to_vec(&body).unwrap(),
                )
                .await;
                requests.push(request);
            }
            requests
        });
        (client, task)
    }
    #[tokio::test]
    async fn mixes_expand_playlist_cards() {
        let Some(_env) = crate::config::testenv::isolated() else {
            return;
        };
        let response = json!({"data":{"type":"userDailyMixes","id":"me","relationships":{"items":{"data":[{"type":"playlists","id":"mix"}]}}},"included":[{"type":"playlists","id":"mix","attributes":{"name":"My Mix"}}]});
        let (client, task) = fixture(vec![
            (200, response.clone()),
            (200, response.clone()),
            (200, response),
        ])
        .await;
        let shelves = client.mixes().await.unwrap();
        assert_eq!(shelves.len(), 3);
        assert_eq!(shelves[0].items[0].name, "My Mix");
        task.await.unwrap();
    }
    #[tokio::test]
    async fn search_splits_includes_and_retains_artwork() {
        let Some(_env) = crate::config::testenv::isolated() else {
            return;
        };
        // Either concurrent request may arrive first; both documents carry the same fixture.
        let response = json!({"data":{"type":"searchResults","id":"query","relationships":{"artists":{"data":[{"type":"artists","id":"artist"}]}}},"included":[{"type":"artists","id":"artist","attributes":{"name":"Artist"},"relationships":{"profileArt":{"data":[{"type":"artworks","id":"art"}]}}},{"type":"artworks","id":"art","attributes":{"files":[{"href":"https://example.org/artist.jpg","meta":{"width":640}}]}}]});
        let (client, task) = fixture(vec![(200, response.clone()), (200, response)]).await;
        let shelves = client.search("artist").await.unwrap();
        assert_eq!(
            shelves[0].items[0].artwork,
            "https://example.org/artist.jpg"
        );
        task.await.unwrap();
    }
    #[tokio::test]
    async fn paginated_playlists_keep_duplicates_and_partial_results() {
        let Some(_env) = crate::config::testenv::isolated() else {
            return;
        };
        let song =
            json!({"type":"tracks","id":"1","attributes":{"title":"Song","duration":"PT3M"}});
        let (client, task) = fixture(vec![
            (
                200,
                json!({"data":[song.clone(),song.clone()],"links":{"next":"/playlists/p/relationships/items?include=items&page[cursor]=second"}}),
            ),
            (500, json!({"errors":[]})),
        ])
        .await;
        let (tracks, error) = client
            .track_list("playlists/p/relationships/items?include=items")
            .await;
        assert_eq!(
            tracks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["1", "1"]
        );
        assert_eq!(error, Some(Error::Partial));
        task.await.unwrap();
    }
    #[tokio::test]
    async fn foreign_next_links_are_rejected_without_following_them() {
        let Some(_env) = crate::config::testenv::isolated() else {
            return;
        };
        let (client,task)=fixture(vec![(200,json!({"data":[{"type":"tracks","id":"1","attributes":{"title":"Song","duration":"PT3M"}}],"links":{"next":"https://evil.example/v2/tracks"}}))]).await;
        let (tracks, error) = client.track_list("tracks").await;
        assert_eq!(tracks.len(), 1);
        assert_eq!(error, Some(Error::Partial));
        task.await.unwrap();
    }
    #[test]
    fn pagination_cannot_send_credentials_to_other_origins() {
        let client = Client::new(&auth::Session {
            country: "GB".into(),
            ..Default::default()
        })
        .unwrap();
        assert!(client.url("https://evil.example/v2/tracks").is_err());
        assert!(client.url("//evil.example/v2/tracks").is_err());
        assert!(client.url("../v1/tracks").is_err());
        assert!(
            client
                .url("https://user@openapi.tidal.com/v2/tracks")
                .is_err()
        );
        let url = client.url("tracks?page[cursor]=next").unwrap();
        assert_eq!(url.host_str(), Some("openapi.tidal.com"));
        assert!(
            url.query_pairs()
                .any(|(k, v)| k == "countryCode" && v == "GB")
        );
    }
    #[tokio::test]
    async fn track_links_reject_untrusted_hosts_before_network_access() {
        let client = Client::new(&auth::Session::default()).unwrap();
        assert!(
            client
                .resolve("https://tidal.com.evil.example/track/1")
                .await
                .is_err()
        );
        assert!(client.resolve("http://tidal.com/track/1").await.is_err());
    }
    #[tokio::test]
    async fn home_keeps_working_mixes_when_one_endpoint_fails() {
        let Some(_env) = crate::config::testenv::isolated() else {
            return;
        };
        let response = json!({"data":[{"type":"playlists","id":"discovery","attributes":{"name":"Daily Discovery"}}]});
        let (client, task) = fixture(vec![
            (400, json!({"errors":[]})),
            (200, response),
            (200, json!({"data":[]})),
        ])
        .await;
        let page = client.mixes_page().await.unwrap();
        assert_eq!(page.shelves.len(), 1);
        assert_eq!(page.shelves[0].title, "Daily Discovery");
        assert!(page.info.contains("My Mix"));
        task.await.unwrap();
    }

    #[tokio::test]
    async fn playlist_union_preserves_ownership_and_favorite_state() {
        let Some(_env) = crate::config::testenv::isolated() else {
            return;
        };
        let item = |id: &str| json!({"type":"playlists","id":id,"attributes":{"name":id}});
        let (client, task) = fixture(vec![
            (200, json!({"data":[item("shared"),item("saved-only")]})),
            (200, json!({"data":[item("shared"),item("owned-only")]})),
        ])
        .await;
        let (items, error) = client.playlists().await;
        assert!(error.is_none());
        assert_eq!(items.len(), 3);
        assert!(
            items
                .iter()
                .any(|it| it.id == "shared" && it.editable && it.favorite)
        );
        assert!(
            items
                .iter()
                .any(|it| it.id == "owned-only" && it.editable && !it.favorite)
        );
        assert!(
            items
                .iter()
                .any(|it| it.id == "saved-only" && !it.editable && it.favorite)
        );
        let requests = task.await.unwrap();
        assert!(requests[1].target.contains("owners.id"));
    }

    #[tokio::test]
    async fn collection_preferences_use_explicit_desired_state() {
        let Some(_env) = crate::config::testenv::isolated() else {
            return;
        };
        let (client, task) = fixture(vec![(204, Value::Null), (204, Value::Null)]).await;
        let refs = [Ref {
            kind: "song".into(),
            id: "track-one".into(),
        }];
        client.set_favorite(&refs, true).await.unwrap();
        client.set_favorite(&refs, false).await.unwrap();
        let requests = task.await.unwrap();
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[1].method, "DELETE");
        for r in requests {
            let body: Value = serde_json::from_slice(&r.body).unwrap();
            assert_eq!(body["data"][0]["id"], "track-one");
            assert_eq!(body["data"][0]["type"], "tracks");
        }
    }
}
