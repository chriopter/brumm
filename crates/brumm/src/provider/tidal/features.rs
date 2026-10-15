//! Tidal pages retain the service's vocabulary and ordered module collections.
use super::{Client, mapping::Document};
use crate::{
    error::{Error, Result, err},
    music::{ExplorePage, Shelf},
};
use serde_json::Value;

const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

const MIXES: [(&str, &str, &str); 3] = [
    ("daily", "userDailyMixes", "My Mix"),
    ("discovery", "userDiscoveryMixes", "Daily Discovery"),
    ("new", "userNewReleaseMixes", "My New Arrivals"),
];

fn shelf(value: &Value, account: &str, title: &str, route: String) -> Shelf {
    let doc = Document::new(value);
    let mut shelf = Shelf::titled(title);
    shelf.route = route;
    for root in Document::data(value) {
        let items = if matches!(
            root["type"].as_str(),
            Some("tracks" | "albums" | "artists" | "playlists")
        ) {
            vec![root]
        } else {
            doc.related(root, "items")
        };
        for item in items {
            if item["type"] == "tracks" {
                if let Some(track) = doc.track(item) {
                    shelf.tracks.push(track);
                }
            } else if let Some(item) = doc.item(item, account) {
                shelf.items.push(item);
            }
        }
    }
    shelf
}

impl Client {
    async fn full_collection(&self, route: &str, title: &str) -> Result<ExplorePage> {
        let (documents, failure) = self.pages(route).await;
        if matches!(failure, Some(Error::Unauthorized)) {
            return Err(Error::Unauthorized);
        }
        let mut row = Shelf::titled(title);
        for (doc, root) in documents {
            let entries = if matches!(
                root["type"].as_str(),
                Some("tracks" | "albums" | "artists" | "playlists")
            ) {
                vec![&root]
            } else {
                doc.related(&root, "items")
            };
            for item in entries {
                if item["type"] == "tracks" {
                    if let Some(track) = doc.track(item) {
                        row.tracks.push(track);
                    }
                } else if let Some(item) = doc.item(item, &self.account) {
                    row.items.push(item);
                }
            }
        }
        if row.is_empty()
            && let Some(error) = failure
        {
            return Err(error);
        }
        Ok(ExplorePage {
            title: title.into(),
            shelves: vec![row],
            info: if failure.is_some() {
                "Tidal returned part of this selection. Refresh to try again.".into()
            } else {
                String::new()
            },
            ..Default::default()
        })
    }

    async fn mix(&self, name: &str) -> Result<Shelf> {
        let (_, resource, title) = MIXES
            .iter()
            .find(|(id, _, _)| *id == name)
            .ok_or(Error::Unsupported)?;
        let value = self.get(&format!("{resource}/me/relationships/items?include=items,items.coverArt,items.owners&locale=en-US")).await?;
        Ok(shelf(
            &value,
            &self.account,
            title,
            format!("tidal:mix:{name}"),
        ))
    }

    pub(super) async fn mixes_page(&self) -> Result<ExplorePage> {
        let mut page = ExplorePage {
            title: "Home".into(),
            ..Default::default()
        };
        let mut unavailable = Vec::new();
        for (id, _, title) in MIXES {
            match self.mix(id).await {
                Ok(shelf) => {
                    if !shelf.is_empty() {
                        page.shelves.push(shelf);
                    }
                }
                Err(Error::Unauthorized) => return Err(Error::Unauthorized),
                Err(_) => unavailable.push(title),
            }
        }
        if !unavailable.is_empty() {
            page.info = format!(
                "Unavailable from Tidal: {}. Other sections remain available.",
                unavailable.join(", ")
            );
        } else if page.shelves.is_empty() {
            page.info = "Tidal has not populated your personal mixes yet.".into();
        }
        Ok(page)
    }

    #[cfg(test)]
    pub(super) async fn mixes(&self) -> Result<Vec<Shelf>> {
        Ok(self.mixes_page().await?.shelves)
    }

    async fn module(&self, id: &str, title: &str) -> Result<Shelf> {
        let include = "items,items.albums:artists,items.albums:coverArt,items.artists:profileArt,items.playlists:coverArt,items.tracks:artists,items.tracks:albums,items.tracks:albums.coverArt";
        let value = self.get(&format!("dynamicModules/{}/relationships/items?include={include}&countryCode={}&locale=en-US&deviceType=DESKTOP&systemType=DESKTOP&clientVersion={CLIENT_VERSION}&refreshSeed={}", Self::id(id), self.country, self.refresh_seed)).await?;
        Ok(shelf(
            &value,
            &self.account,
            title,
            format!("tidal:module:{}", Self::id(id)),
        ))
    }

    async fn native_page(&self, page_type: &str, title: &str) -> Result<ExplorePage> {
        let value = self.get(&format!("dynamicPages?filter[pageType]={page_type}&filter[subject]=users:{}&include=modules&countryCode={}&locale=en-US&deviceType=DESKTOP&systemType=DESKTOP&clientVersion={CLIENT_VERSION}&refreshSeed={}", Self::id(&self.account), self.country, self.refresh_seed)).await?;
        let doc = Document::new(&value);
        let mut page = ExplorePage {
            title: title.into(),
            ..Default::default()
        };
        let mut missing = false;
        for root in Document::data(&value) {
            for module in doc.related(root, "modules") {
                let module = doc.expand(module);
                let Some(id) = module["id"].as_str() else {
                    continue;
                };
                let name = module["attributes"]["title"]
                    .as_str()
                    .unwrap_or("Tidal selection");
                match self.module(id, name).await {
                    Ok(shelf) => {
                        if !shelf.is_empty() {
                            page.shelves.push(shelf);
                        }
                    }
                    Err(Error::Unauthorized) => return Err(Error::Unauthorized),
                    Err(_) => missing = true,
                }
            }
        }
        if missing {
            page.info = "Some Tidal sections could not be loaded. Try refreshing.".into();
        }
        if page.shelves.is_empty() {
            return Err(err("Tidal has no accessible page sections"));
        }
        Ok(page)
    }

    pub(super) async fn home_page(&self) -> Result<ExplorePage> {
        match self.native_page("HOME_STATIC", "Home").await {
            Ok(page) => Ok(page),
            Err(Error::Unauthorized) => Err(Error::Unauthorized),
            Err(_) => self.mixes_page().await,
        }
    }

    pub(super) async fn feature_page(&self, route: &str) -> Result<ExplorePage> {
        match route {
            "tidal:home" => self.home_page().await,
            "tidal:explore" => match self.native_page("HOME_EDITORIAL", "Explore").await {
                Ok(page) => Ok(page),
                Err(Error::Unauthorized) => Err(Error::Unauthorized),
                Err(_) => self.compatibility_explore().await,
            },
            _ => {
                if let Some(page) = route.strip_prefix("tidal:page:") {
                    return self.compatibility_route(page).await;
                }
                if let Some(id) = route.strip_prefix("tidal:mix:") {
                    let (_, resource, title) = MIXES
                        .iter()
                        .find(|(name, _, _)| *name == id)
                        .ok_or(Error::Unsupported)?;
                    return self.full_collection(&format!("{resource}/me/relationships/items?include=items,items.coverArt,items.owners&locale=en-US"), title).await;
                }
                if let Some(id) = route.strip_prefix("tidal:module:") {
                    let id = url::form_urlencoded::parse(format!("id={id}").as_bytes())
                        .next()
                        .map(|(_, v)| v.into_owned())
                        .ok_or(Error::Unsupported)?;
                    return self.full_collection(&format!("dynamicModules/{}/relationships/items?include=items,items.albums:artists,items.albums:coverArt,items.artists:profileArt,items.playlists:coverArt,items.tracks:artists,items.tracks:albums,items.tracks:albums.coverArt&deviceType=DESKTOP&systemType=DESKTOP&clientVersion={CLIENT_VERSION}&refreshSeed={}", Self::id(&id), self.refresh_seed), "").await;
                }
                Err(Error::Unsupported)
            }
        }
    }
}
