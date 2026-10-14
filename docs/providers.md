# Music API providers

The Rust core separates the music service from everything else. brumm plays
Apple Music and Spotify; Tidal is listed as coming soon. New installations
ask for a service at startup, and existing Apple sessions keep theirs.

## Module boundaries

```text
crates/brumm/src/
├── music.rs              Shared browse documents
├── provider/
│   ├── mod.rs            Catalog, Library and Api contracts
│   ├── registry.rs       Services, first-run selection, what each lacks
│   ├── apple/            Apple REST, mapping and navigation
│   └── spotify/          Spotify Web API, tokens, mapping and links
├── daemon/
│   ├── service.rs        Composition point: API, playback page, session
│   └── known.rs          Songs handed to players, for the Spotify page
├── ipc.rs                Provider-independent socket protocol
├── tui/                  Terminal frontend
├── engine/               Playback in Chrome: player.html (MusicKit JS),
│                         spotify.html (Spotify Web Playback SDK)
├── login/                Apple sign-in; spotify.rs: OAuth with PKCE
└── config.rs             Options, sessions and per-service cache names

gui/                      Qt frontend
crates/brumm-shared/       Shared audio/accent code and Qt FFI
```

`music` owns `Item`, `Track`, `Ref`, `Shelf`, `Detail` and `ExplorePage`.
Both frontends, IPC, the cache and the player use these documents without
importing an API implementation. Playback state remains in `ipc::Player`.
The daemon holds `Arc<dyn provider::Api>`. Only its composition module,
`daemon/service.rs`, constructs a client, and it also picks the playback
page (`engine::Page`).

## Implementing another API

Add `provider/<name>` and declare the module in `provider/mod.rs`. Implement
`Catalog` and `Library`, then implement `Api` for your client, and return it
from `daemon/service.rs`.

`Catalog` supports search, container tracks, a song's album and artist,
share links, and resolving a share link to a container plus its selected song.
`Library` lists playlists, albums, artists and songs. Their successful empty
lists mean the collection is empty, not that the operation is unsupported.

Calls return `provider::Reply`, a boxed `Send` future. This keeps the contracts
object-safe without adding an async-trait dependency. For example, a search
implementation has this signature:

```rust
fn search<'a>(&'a self, term: &'a str)
    -> provider::Reply<'a, Result<Vec<music::Shelf>>>
```

Return `Box::pin(async move { ... })` from each method. Every borrowed input
shares the call's lifetime; the future must be safe to run on Tokio's workers.

`Api` supplies defaults for optional browsing, radio, ratings, library edits,
favorites, folder creation and Explorer pages. Override the operations your
provider supports. The defaults return `Error::Unsupported` and never call
Apple.

Use `Error::Unauthorized` when the session needs a fresh login. List and track
calls return `Listing<T>`, `(Vec<T>, Option<Error>)`, preserving the existing
partial-result contract. Return the usable prefix with `Some(Error::Partial)`
when pagination fails. The daemon shows those results without caching them as
complete. Return an ordinary error when the operation produced no usable data.

Keep IDs opaque. `Item.catalog` distinguishes library and catalog resources
without inferring that distinction from Apple's ID prefixes. `Item.route`,
`Shelf.route` and `ExplorePage.next` are provider-owned navigation handles.
Clients pass them back unchanged, including folder parent handles in create
requests. Validate handles before authenticated requests. Do not place tokens
in documents, handles or links.

`Api::feature` handles brumm's named entry points, including `app:discover`
and `app:favorites`, plus navigation handles returned by the provider.
`ExplorePage.visuals` supplies up to two named pages the frontends load as
artwork rows. `Shelf.route` opens the complete row. Frontends do not construct
Apple chart URLs or parse Apple folder IDs.

A service also needs a sign-in (`login/`), a way to keep its session in the
private config (`config.rs`, `Config::signed_in`), and a playback page with
the `brumm.*` calls of `engine/player.html`. `engine/mod.rs` tests that every
page defines every call the engine makes.

## What a service lacks

`registry::ProviderInfo::lacks` names what the players hide for a service:
`explore`, `radio`, `preview`, `dislike`, `favorites`, `folders`, `videos`,
`visualizer` (no audio to analyze; it breathes as if paused) and
`love-items` (only songs can be loved: Spotify saves albums and playlists,
it does not love them). The daemon
sends the list, and the service's name, in `ipc::State` (`lacks`,
`service`). The frontends then show "Spotify has no Explore for apps" in
place of a section, leave out the keys and hints, and say so when a key is
pressed anyway. Anything else unsupported still shows the provider's error.

## Spotify

Spotify's Web API serves apps in development mode with limits that shape
the adapter (as of 2026):

- An app serves five accounts, and its owner needs Premium. brumm ships no
  app: at sign-in, users paste the Client ID of their own, made at
  developer.spotify.com with the redirect URI
  `http://127.0.0.1:7777/spotify/callback`.
- Recommendations, related artists, previews, browse pages, an artist's top
  songs and batch lookups are gone. Search returns at most ten per kind.
- Playlist songs are listed only for playlists the user owns or
  collaborates on. Other playlists become `KIND_STATION` items with id
  `spotify:playlist:<id>`: they play as a whole and do not open.
- Saving and checking go through `PUT|DELETE /me/library` and
  `GET /me/library/contains`, with URIs. A saved song is a loved one.

Sign-in (`login/spotify.rs`) is OAuth with PKCE. The loopback page asks for
the Client ID, brumm answers with the authorize address (S256 challenge,
random state), and the callback trades the code for tokens. The config keeps
`spotify_client_id`, `spotify_refresh_token` and the account id
`spotify_user`; another account starts with empty caches.

`provider/spotify/auth.rs` refreshes the hourly access token and saves a
rotated refresh token. The API client and the daemon share one session: the
daemon pushes a new token into the playback page shortly before the old one
ends, and renews it at once when the page reports it refused.

`engine/spotify.html` runs the Web Playback SDK in the same headless Chrome
as MusicKit; playback needs Premium. The queue stays brumm's: the page hands
Spotify a window of up to 100 song URIs and follows along as songs change,
sending the next window near its end and brumm's order after a shuffle or
an insert. Because Spotify has no batch lookups, the daemon tells the page
what the songs look like (`Engine::describe`, from `daemon/known.rs`).
The SDK plays in a frame of its own, so the visualizer gets no audio.

## Caches and switching

Each service keeps its own library cache and resume state:
`library.json` and `resume.json` for Apple Music (the names they always
had), `library-spotify.json` and `resume-spotify.json` for Spotify
(`config::service_cache`).

`CMD_PROVIDER` with a `query` selects a service. When it changes, the daemon
saves the old service's place, closes its player, loads the new service's
library and place, and boots it; a boot it overtakes is dropped. Both
options menus open the picker ("Music Service"), and `brumm provider
[apple|spotify]` lists or switches from the command line. Mixed-service
queues are not supported.

## Validation

Run `cargo test --workspace --locked` and `cargo fmt --all -- --check`.
The Apple and Spotify HTTP fixtures live beside their adapters
(`provider/testserver.rs` serves both). Independent provider, daemon and
terminal tests exercise the contracts without an account. Build Qt with
`bin/build-gui`. The GUI provider tests run without a real daemon:

```sh
QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME= QT_QUICK_BACKEND=software \
  QT_STYLE_OVERRIDE=Basic /usr/lib/qt6/bin/qmltestrunner -input gui/tests
```

`FAKE_SPOTIFY=1` makes the fake daemon (`tools/fakedaemon`) report Spotify,
for screenshots of the Spotify wording. Playback itself needs a real
Spotify Premium account and a Client ID.
