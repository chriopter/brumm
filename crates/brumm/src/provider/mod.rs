//! The provider-independent boundary for catalog and library APIs.
//!
//! Async calls are object-safe, so the daemon can hold Arc<dyn Api>.
//! Navigation handles and IDs belong to the provider, never to a frontend.
use crate::error::{Error, Result};
use crate::music::{ExplorePage, Item, Ref, Shelf, Track};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

pub type Reply<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
/// Usable results and an optional failure. Partial results must not be cached as complete.
pub type Listing<T> = (Vec<T>, Option<Error>);

pub trait Catalog: Send + Sync {
    fn search<'a>(&'a self, term: &'a str) -> Reply<'a, Result<Vec<Shelf>>>;
    fn tracks<'a>(&'a self, item: &'a Item) -> Reply<'a, Listing<Track>>;
    fn album_of<'a>(&'a self, song: &'a str) -> Reply<'a, Result<Item>>;
    fn artist_of<'a>(&'a self, song: &'a str) -> Reply<'a, Result<Item>>;
    fn link<'a>(&'a self, song: &'a str) -> Reply<'a, Result<String>>;
    fn resolve<'a>(&'a self, link: &'a str) -> Reply<'a, Result<(Item, String)>>;
}

pub trait Library: Send + Sync {
    fn playlists<'a>(&'a self) -> Reply<'a, Listing<Item>>;
    fn albums<'a>(&'a self) -> Reply<'a, Listing<Item>>;
    fn artists<'a>(&'a self) -> Reply<'a, Listing<Item>>;
    fn songs<'a>(&'a self) -> Reply<'a, Listing<Track>>;
}

/// Catalog and Library are required. Optional operations explicitly report
/// Unsupported until a provider implements them. No Apple fallback is used.
pub trait Api: Catalog + Library {
    fn home<'a>(&'a self) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(async { Err(Error::Unsupported) })
    }
    fn artist<'a>(&'a self, _item: &'a Item) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(async { Err(Error::Unsupported) })
    }
    fn open<'a>(&'a self, _item: &'a Item) -> Reply<'a, Result<Vec<Shelf>>> {
        Box::pin(async { Err(Error::Unsupported) })
    }
    fn station<'a>(&'a self, _item: &'a Item, _song: &'a str) -> Reply<'a, Result<Item>> {
        Box::pin(async { Err(Error::Unsupported) })
    }
    fn ratings<'a>(&'a self, _refs: &'a [Ref]) -> Reply<'a, Result<BTreeMap<String, i64>>> {
        Box::pin(async { Err(Error::Unsupported) })
    }
    fn rate<'a>(&'a self, _resource: &'a Ref, _value: i64) -> Reply<'a, Result<()>> {
        Box::pin(async { Err(Error::Unsupported) })
    }
    fn add_to_library<'a>(&'a self, _kind: &'a str, _ids: &'a [String]) -> Reply<'a, Result<()>> {
        Box::pin(async { Err(Error::Unsupported) })
    }
    fn create_playlist<'a>(
        &'a self,
        _name: &'a str,
        _songs: &'a [String],
    ) -> Reply<'a, Result<Item>> {
        Box::pin(async { Err(Error::Unsupported) })
    }
    fn add_to_playlist<'a>(&'a self, _id: &'a str, _songs: &'a [String]) -> Reply<'a, Result<()>> {
        Box::pin(async { Err(Error::Unsupported) })
    }
    fn favorite<'a>(&'a self, _refs: &'a [Ref]) -> Reply<'a, Result<()>> {
        Box::pin(async { Err(Error::Unsupported) })
    }
    fn create_container<'a>(
        &'a self,
        _kind: &'a str,
        _name: &'a str,
        _parent: &'a str,
    ) -> Reply<'a, Result<Item>> {
        Box::pin(async { Err(Error::Unsupported) })
    }
    fn feature<'a>(&'a self, _route: &'a str) -> Reply<'a, Result<ExplorePage>> {
        Box::pin(async { Err(Error::Unsupported) })
    }
}

pub mod apple;

#[cfg(test)]
pub(crate) mod tests;

pub mod registry;
