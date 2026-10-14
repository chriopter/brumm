use super::*;
use std::sync::Arc;

/// An independent provider: no Apple client, resource mapping or HTTP server.
#[derive(Default)]
pub(crate) struct Fixture {
    pub failure: Option<Error>,
}
impl Catalog for Fixture {
    fn search<'a>(&'a self, term: &'a str) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(async move {
            if let Some(e) = &self.failure {
                return Err(e.clone());
            }
            Ok(vec![Shelf {
                title: term.into(),
                tracks: vec![Track {
                    id: "fixture-song".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }])
        })
    }
    fn tracks<'a>(&'a self, _item: &'a Item) -> Reply<'a, Listing<Track>> {
        Box::pin(async move {
            (
                vec![Track {
                    id: "fixture-song".into(),
                    ..Default::default()
                }],
                self.failure.clone(),
            )
        })
    }
    fn album_of<'a>(&'a self, _song: &'a str) -> Reply<'a, Result<Item>> {
        Box::pin(async {
            Ok(Item {
                kind: crate::music::KIND_ALBUM.into(),
                id: "fixture-album".into(),
                ..Default::default()
            })
        })
    }
    fn artist_of<'a>(&'a self, _song: &'a str) -> Reply<'a, Result<Item>> {
        Box::pin(async {
            Ok(Item {
                kind: crate::music::KIND_ARTIST.into(),
                id: "fixture-artist".into(),
                ..Default::default()
            })
        })
    }
    fn link<'a>(&'a self, _song: &'a str) -> Reply<'a, Result<String>> {
        Box::pin(async { Ok("https://example.org/music/track".into()) })
    }
    fn resolve<'a>(&'a self, link: &'a str) -> Reply<'a, Result<(Item, String)>> {
        Box::pin(async move {
            Ok((
                Item {
                    kind: crate::music::KIND_ALBUM.into(),
                    id: link.into(),
                    ..Default::default()
                },
                "fixture-song".into(),
            ))
        })
    }
}
impl Library for Fixture {
    fn playlists<'a>(&'a self) -> Reply<'a, Listing<Item>> {
        Box::pin(async move {
            (
                vec![Item {
                    kind: crate::music::KIND_PLAYLIST.into(),
                    id: "fixture-playlist".into(),
                    ..Default::default()
                }],
                self.failure.clone(),
            )
        })
    }
    fn albums<'a>(&'a self) -> Reply<'a, Listing<Item>> {
        Box::pin(async { (vec![], None) })
    }
    fn artists<'a>(&'a self) -> Reply<'a, Listing<Item>> {
        Box::pin(async { (vec![], None) })
    }
    fn songs<'a>(&'a self) -> Reply<'a, Listing<Track>> {
        Box::pin(async { (vec![], None) })
    }
}
impl Api for Fixture {}

#[tokio::test]
async fn independent_provider_is_object_safe_and_preserves_partial_results() {
    let api: Arc<dyn Api> = Arc::new(Fixture {
        failure: Some(Error::Partial),
    });
    let (items, failure) = api.playlists().await;
    assert_eq!(items[0].id, "fixture-playlist");
    assert_eq!(failure, Some(Error::Partial));
    let (item, song) = api.resolve("provider:opaque").await.unwrap();
    assert_eq!(item.id, "provider:opaque");
    assert_eq!(song, "fixture-song");
}

#[tokio::test]
async fn optional_operations_do_not_fall_back_to_apple() {
    let api: Arc<dyn Api> = Arc::new(Fixture::default());
    assert_eq!(api.search("query").await.unwrap()[0].title, "query");
    assert_eq!(api.home().await, Err(Error::Unsupported));
    assert_eq!(
        api.station(&Item::default(), "song").await,
        Err(Error::Unsupported)
    );
    assert_eq!(api.ratings(&[]).await, Err(Error::Unsupported));
    assert_eq!(
        api.add_to_library("songs", &[]).await,
        Err(Error::Unsupported)
    );
    assert_eq!(api.favorite(&[]).await, Err(Error::Unsupported));
    assert_eq!(
        api.create_container("folder", "name", "").await,
        Err(Error::Unsupported)
    );
    assert_eq!(api.feature("app:discover").await, Err(Error::Unsupported));
}
