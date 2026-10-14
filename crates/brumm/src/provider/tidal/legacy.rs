//! Device-client discovery pages. These compatibility endpoints are separate
//! from the public v2 catalog and are validated before attaching credentials.
use super::Client;
use crate::{
    error::{Error, Result, err},
    music::{ExplorePage, Item, Shelf, Track},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, Deserialize, Default)]
struct Page {
    path: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    kind: String,
}
fn route(path: &str, title: &str, kind: &str) -> String {
    format!(
        "tidal:page:{}",
        URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&Page {
                path: path.into(),
                title: title.into(),
                kind: kind.into()
            })
            .unwrap()
        )
    )
}
fn page_url(path: &str, country: &str) -> Result<url::Url> {
    if !path.starts_with("pages/") || path.len() > 4096 {
        return Err(Error::Unsupported);
    }
    let base = url::Url::parse("https://api.tidal.com/v1/").unwrap();
    let mut url = base.join(path).map_err(|_| Error::Unsupported)?;
    if url.origin() != base.origin()
        || !url.path().starts_with("/v1/pages/")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Unsupported);
    }
    let pairs: Vec<_> = url
        .query_pairs()
        .filter(|(k, _)| !matches!(k.as_ref(), "countryCode" | "locale" | "deviceType"))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    url.set_query(None);
    url.query_pairs_mut()
        .extend_pairs(pairs)
        .append_pair("countryCode", country)
        .append_pair("locale", "en_US")
        .append_pair("deviceType", "BROWSER");
    Ok(url)
}
fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or("").into()
}
fn id(v: &Value) -> String {
    v.as_str()
        .map(str::to_owned)
        .or_else(|| v.as_u64().map(|n| n.to_string()))
        .unwrap_or_default()
}
fn image(value: &str) -> String {
    if value.len() != 36 || !value.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return String::new();
    }
    format!(
        "https://resources.tidal.com/images/{}/640x640.jpg",
        value.replace('-', "/")
    )
}
fn artists(v: &Value) -> String {
    v["artists"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v["name"].as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| text(&v["artist"], "name"))
}
fn item(v: &Value, kind: &str) -> Option<Item> {
    let resource_id = if kind == "playlist" {
        id(&v["uuid"])
    } else {
        id(&v["id"])
    };
    if resource_id.is_empty() {
        return None;
    }
    let name = if kind == "artist" {
        text(v, "name")
    } else {
        text(v, "title")
    };
    if name.is_empty() {
        return None;
    }
    let art = if kind == "artist" {
        text(v, "picture")
    } else if kind == "playlist" {
        text(v, "squareImage")
    } else {
        text(v, "cover")
    };
    Some(Item {
        id: resource_id,
        kind: kind.into(),
        name,
        artist: artists(v),
        artwork: image(&art),
        catalog: true,
        ..Default::default()
    })
}
fn track(v: &Value) -> Option<Track> {
    let resource_id = id(&v["id"]);
    let title = text(v, "title");
    if resource_id.is_empty() || title.is_empty() {
        return None;
    }
    Some(Track {
        id: resource_id,
        title,
        artist: artists(v),
        album: text(&v["album"], "title"),
        artwork: image(&text(&v["album"], "cover")),
        duration: v["duration"].as_f64().unwrap_or(0.),
        number: v["trackNumber"].as_i64().unwrap_or(0),
        ..Default::default()
    })
}
fn page_link(v: &Value) -> Option<Item> {
    let path = v["apiPath"].as_str()?;
    // Video discovery is outside this provider's initial capabilities.
    if path.starts_with("pages/videos") || page_url(path, "US").is_err() {
        return None;
    }
    let name = text(v, "title");
    Some(Item {
        kind: "shelf".into(),
        id: path.into(),
        name: name.clone(),
        route: route(path, &name, ""),
        catalog: true,
        ..Default::default()
    })
}
fn promotion(v: &Value) -> Option<Item> {
    let resource_id = text(v, "artifactId");
    let name = ["shortHeader", "header", "shortSubHeader"]
        .iter()
        .find_map(|k| v[k].as_str().filter(|s| !s.trim().is_empty()))
        .unwrap_or("")
        .to_owned();
    let kind = match v["type"].as_str()? {
        "ALBUM" => "album",
        "PLAYLIST" => "playlist",
        "ARTIST" => "artist",
        "CATEGORY_PAGES" => "shelf",
        _ => return None,
    };
    let page_route = if kind == "shelf" {
        page_url(&resource_id, "US").ok()?;
        route(&resource_id, &name, "")
    } else {
        String::new()
    };
    if resource_id.is_empty() || name.is_empty() {
        return None;
    }
    Some(Item {
        kind: kind.into(),
        id: resource_id,
        name,
        artist: text(v, "shortSubHeader"),
        artwork: image(&text(v, "imageId")),
        catalog: true,
        route: page_route,
        ..Default::default()
    })
}
fn render(value: &Value, page: &Page) -> ExplorePage {
    let mut out = ExplorePage {
        title: value["title"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(&page.title)
            .into(),
        ..Default::default()
    };
    let synthetic;
    let modules: Vec<&Value> = if value["rows"].is_array() {
        value["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|row| row["modules"].as_array().into_iter().flatten())
            .collect()
    } else {
        synthetic = json!({"type":page.kind,"title":page.title,"pagedList":value});
        vec![&synthetic]
    };
    for module in modules {
        let kind = module["type"].as_str().unwrap_or("");
        let mut shelf = Shelf::titled(module["title"].as_str().unwrap_or(""));
        let list = if module["pagedList"].is_object() {
            &module["pagedList"]
        } else {
            module
        };
        for entry in list["items"].as_array().into_iter().flatten() {
            let raw = if entry["item"].is_object() {
                &entry["item"]
            } else {
                entry
            };
            match kind {
                "TRACK_LIST" => {
                    if let Some(t) = track(raw) {
                        shelf.tracks.push(t);
                    }
                }
                "ALBUM_LIST" | "PLAYLIST_LIST" | "ARTIST_LIST" => {
                    let resource_kind = match kind {
                        "ALBUM_LIST" => "album",
                        "PLAYLIST_LIST" => "playlist",
                        _ => "artist",
                    };
                    if let Some(it) = item(raw, resource_kind) {
                        shelf.items.push(it);
                    }
                }
                "PAGE_LINKS" | "PAGE_LINKS_CLOUD" => {
                    if let Some(it) = page_link(raw) {
                        shelf.items.push(it);
                    }
                }
                "FEATURED_PROMOTIONS" => {
                    if let Some(it) = promotion(raw) {
                        shelf.items.push(it);
                    }
                }
                _ => {}
            }
        }
        if let Some(path) = module["showMore"]["apiPath"]
            .as_str()
            .filter(|s| page_url(s, "US").is_ok())
        {
            shelf.route = route(path, &shelf.title, kind);
        } else if let Some(path) = list["dataApiPath"]
            .as_str()
            .filter(|s| page_url(s, "US").is_ok())
        {
            shelf.route = route(path, &shelf.title, kind);
        }
        if !shelf.is_empty() {
            out.shelves.push(shelf);
        }
    }
    if !value["rows"].is_array() {
        let offset = value["offset"].as_u64().unwrap_or(0);
        let count = value["items"].as_array().map_or(0, |a| a.len()) as u64;
        let total = value["totalNumberOfItems"].as_u64().unwrap_or(0);
        if count > 0
            && offset + count < total
            && let Ok(mut url) = page_url(&page.path, "US")
        {
            let pairs: Vec<_> = url
                .query_pairs()
                .filter(|(k, _)| !matches!(k.as_ref(), "offset" | "limit"))
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            url.set_query(None);
            url.query_pairs_mut()
                .extend_pairs(pairs)
                .append_pair("offset", &(offset + count).to_string())
                .append_pair("limit", "100");
            out.next = route(
                &format!(
                    "{}?{}",
                    url.path().trim_start_matches("/v1/"),
                    url.query().unwrap_or("")
                ),
                &page.title,
                &page.kind,
            );
        }
    }
    if out.shelves.is_empty() {
        out.info = "Tidal has no supported music sections on this page.".into();
    }
    out
}
impl Client {
    pub(super) async fn compatibility_explore(&self) -> Result<ExplorePage> {
        self.compatibility_page(Page {
            path: "pages/explore".into(),
            title: "Explore".into(),
            kind: String::new(),
        })
        .await
    }
    async fn compatibility_page(&self, page: Page) -> Result<ExplorePage> {
        let url = page_url(&page.path, &self.country)?;
        let value = self.request_url(reqwest::Method::GET, url, None).await?;
        Ok(render(&value, &page))
    }
    pub(super) async fn compatibility_route(&self, raw: &str) -> Result<ExplorePage> {
        if raw.len() > 16384 {
            return Err(Error::Unsupported);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(raw)
            .map_err(|_| err("invalid Tidal page"))?;
        let page: Page = serde_json::from_slice(&bytes).map_err(|_| err("invalid Tidal page"))?;
        self.compatibility_page(page).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_retains_native_groups_and_validates_nested_routes() {
        let v = json!({"title":"Explore","rows":[{"modules":[{"type":"FEATURED_PROMOTIONS","title":"Featured","items":[{"type":"ALBUM","artifactId":"123","shortHeader":"New album"}]},{"type":"PAGE_LINKS_CLOUD","title":"Genres","pagedList":{"items":[{"title":"Jazz","apiPath":"pages/genre_jazz"},{"title":"Bad","apiPath":"https://evil.example/"}]}}]}]});
        let page = render(&v, &Page::default());
        assert_eq!(page.title, "Explore");
        assert_eq!(page.shelves.len(), 2);
        assert_eq!(page.shelves[0].items[0].kind, "album");
        assert_eq!(page.shelves[1].items.len(), 1);
        assert!(page.shelves[1].items[0].route.starts_with("tidal:page:"));
        for raw in [
            "https://evil.example/",
            "pages/../../users/me",
            "pages/test#fragment",
            "//evil.example/pages/test",
        ] {
            assert!(page_url(raw, "GB").is_err());
        }
    }
    #[test]
    fn music_lists_keep_duplicates_and_pagination() {
        let v = json!({"offset":0,"totalNumberOfItems":3,"items":[{"id":123,"title":"One","duration":200},{"id":123,"title":"One","duration":200}]});
        let page = render(
            &v,
            &Page {
                path: "pages/data/fictional".into(),
                title: "Tracks".into(),
                kind: "TRACK_LIST".into(),
            },
        );
        assert_eq!(page.shelves[0].tracks.len(), 2);
        assert!(!page.next.is_empty());
        let bytes = URL_SAFE_NO_PAD
            .decode(page.next.strip_prefix("tidal:page:").unwrap())
            .unwrap();
        let next: Page = serde_json::from_slice(&bytes).unwrap();
        assert!(next.path.contains("offset=2"));
    }
}
