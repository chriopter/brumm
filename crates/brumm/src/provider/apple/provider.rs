//! Maps the Apple client onto the common provider contracts.
use super::Client;
use crate::error::{Result, err};
use crate::music::{ExplorePage, Item, Ref, Shelf, Track};
use crate::provider::{Api, Catalog, Library, Listing, Reply};
use std::collections::BTreeMap;

impl Catalog for Client {
    fn search<'a>(&'a self, term: &'a str) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(Client::search(self, term))
    }
    fn tracks<'a>(&'a self, item: &'a Item) -> Reply<'a, Listing<Track>> {
        Box::pin(Client::tracks(self, item))
    }
    fn album_of<'a>(&'a self, song: &'a str) -> Reply<'a, Result<Item>> {
        Box::pin(Client::album_of(self, song))
    }
    fn artist_of<'a>(&'a self, song: &'a str) -> Reply<'a, Result<Item>> {
        Box::pin(Client::artist_of(self, song))
    }
    fn link<'a>(&'a self, song: &'a str) -> Reply<'a, Result<String>> {
        Box::pin(Client::link(self, song))
    }
    fn resolve<'a>(&'a self, link: &'a str) -> Reply<'a, Result<(Item, String)>> {
        Box::pin(async move {
            let (mut item, song) =
                super::parse_link(link).ok_or_else(|| err("not an Apple Music link"))?;
            if item.id.is_empty() {
                item = self.album_of(&song).await?;
            }
            Ok((item, song))
        })
    }
}

impl Library for Client {
    fn playlists<'a>(&'a self) -> Reply<'a, Listing<Item>> {
        Box::pin(Client::playlists(self))
    }
    fn albums<'a>(&'a self) -> Reply<'a, Listing<Item>> {
        Box::pin(Client::albums(self))
    }
    fn artists<'a>(&'a self) -> Reply<'a, Listing<Item>> {
        Box::pin(Client::artists(self))
    }
    fn songs<'a>(&'a self) -> Reply<'a, Listing<Track>> {
        Box::pin(Client::songs(self))
    }
}

impl Api for Client {
    fn home<'a>(&'a self) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(Client::home(self))
    }
    fn artist<'a>(&'a self, item: &'a Item) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(Client::artist(self, item))
    }
    fn open<'a>(&'a self, item: &'a Item) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(Client::open(self, item))
    }
    fn station<'a>(&'a self, item: &'a Item, song: &'a str) -> Reply<'a, Result<Item>> {
        Box::pin(Client::station(self, item, song))
    }
    fn ratings<'a>(&'a self, refs: &'a [Ref]) -> Reply<'a, Result<BTreeMap<String, i64>>> {
        Box::pin(Client::ratings(self, refs))
    }
    fn rate<'a>(&'a self, resource: &'a Ref, value: i64) -> Reply<'a, Result<()>> {
        Box::pin(Client::rate(self, resource, value))
    }
    fn add_to_library<'a>(&'a self, kind: &'a str, ids: &'a [String]) -> Reply<'a, Result<()>> {
        Box::pin(Client::add_to_library(self, kind, ids))
    }
    fn create_playlist<'a>(
        &'a self,
        name: &'a str,
        songs: &'a [String],
    ) -> Reply<'a, Result<Item>> {
        Box::pin(Client::create_playlist(self, name, songs))
    }
    fn add_to_playlist<'a>(&'a self, id: &'a str, songs: &'a [String]) -> Reply<'a, Result<()>> {
        Box::pin(Client::add_to_playlist(self, id, songs))
    }
    fn favorite<'a>(&'a self, refs: &'a [Ref]) -> Reply<'a, Result<()>> {
        Box::pin(Client::favorite(self, refs))
    }
    fn create_container<'a>(
        &'a self,
        kind: &'a str,
        name: &'a str,
        parent: &'a str,
    ) -> Reply<'a, Result<Item>> {
        Box::pin(Client::create_container(self, kind, name, parent))
    }
    fn feature<'a>(&'a self, route: &'a str) -> Reply<'a, Result<ExplorePage>> {
        Box::pin(Client::feature(self, route))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testserver::fake_api;
    use super::*;

    #[tokio::test]
    async fn resolve_keeps_link_parsing_in_the_adapter() {
        let (client, _) = fake_api(vec![(
            "/v1/catalog/de/songs/7/albums",
            r#"{"data":[{"id":"42","type":"albums","attributes":{"name":"Album"}}]}"#,
        )])
        .await;
        for (link, kind, id, selected) in [
            (
                "https://music.apple.com/de/album/name/42?i=7",
                crate::music::KIND_ALBUM,
                "42",
                "7",
            ),
            (
                "https://music.apple.com/de/song/name/7",
                crate::music::KIND_ALBUM,
                "42",
                "7",
            ),
            (
                "https://music.apple.com/de/playlist/name/pl.42",
                crate::music::KIND_PLAYLIST,
                "pl.42",
                "",
            ),
        ] {
            let (item, song) = Catalog::resolve(&client, link).await.unwrap();
            assert_eq!(
                (item.kind.as_str(), item.id.as_str(), song.as_str()),
                (kind, id, selected)
            );
        }
        for link in [
            "https://example.org/de/album/name/42",
            "https://evilmusic.apple.com/de/album/name/42",
            "file://music.apple.com/de/album/name/42",
        ] {
            assert!(Catalog::resolve(&client, link).await.is_err());
        }
    }

    #[tokio::test]
    async fn provider_supplies_resolvable_explore_visuals() {
        let (client, seen) = fake_api(vec![("/v1/catalog/de/charts", r#"{"results":{}}"#)]).await;
        let page = client.feature("app:discover").await.unwrap();
        assert_eq!(page.visuals.len(), 3);
        assert_eq!(page.visuals[0].route, "app:discover:recent");
        for (i, visual) in page.visuals.iter().skip(1).enumerate() {
            client.feature(&visual.route).await.unwrap();
            let target = seen.lock().unwrap().last().unwrap().target.clone();
            assert!(target.contains(if i == 0 {
                "types=albums,playlists"
            } else {
                "types=music-videos"
            }));
            assert!(target.contains("limit=6"));
        }
    }
}
