# Music API providers

The Rust core separates the REST API from Apple Music. New installations ask
for a provider at startup; existing Apple sessions keep their selection.
A Tidal API adapter does not by itself add Tidal login or audio playback.

## Module boundaries

```text
crates/brumm/src/
├── music.rs              Shared browse documents
├── provider/
│   ├── mod.rs            Catalog, Library and Api contracts
│   ├── registry.rs       Available services and first-run selection
│   └── apple/            Apple REST, mapping and navigation
├── daemon/
│   └── apple.rs          API composition point
├── ipc.rs                Provider-independent socket protocol
├── tui/                  Terminal frontend
├── engine/               MusicKit playback and Chrome/CDP
├── login/                Apple sign-in
└── config.rs             Options and Apple credentials

gui/                      Qt frontend
crates/brumm-shared/       Existing shared audio/accent code and Qt FFI
```

`music` owns `Item`, `Track`, `Ref`, `Shelf`, `Detail` and `ExplorePage`.
Both frontends, IPC, the cache and the player use these documents without
importing an API implementation. Playback state remains in `ipc::Player`.
The daemon holds `Arc<dyn provider::Api>`. Only its composition module,
`daemon/apple.rs`, constructs the Apple client.

## Implementing another API

Add `provider/tidal` and declare the module in `provider/mod.rs`. Implement
`Catalog` and `Library`, then implement `Api` for your client. Once credentials
and provider selection exist, the composition point can return:

```rust
Arc::new(tidal::Client::new(/* provider-specific credentials */))
```

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
Apple. The current frontends display that error; capability-based hiding of
unsupported controls is a later integration step.

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
Apple chart URLs or parse Apple folder IDs. Another provider need not offer
Apple Replay or Apple's lookup actions.

## Completing Tidal support

On a new installation, both frontends ask for the music provider before
starting sign-in. The choice is saved as `provider` in the private config.
Existing configurations with an Apple user token remain Apple sessions without
an extra prompt. `CMD_PROVIDER` reads the registry or selects an available
provider using `query`. Apple Music is available; Tidal is shown as coming soon
and cannot be selected until its adapter exists. Invalid or unavailable choices
leave the configuration untouched.

Before enabling a second provider in a running application:

1. Add its login, private credential storage and token refresh, then wire its
   factory and enable its registry entry. Replace the Apple-only boot/reload
   credential checks in the composition path.
2. Add the required playback adapter. The current engine still embeds
   MusicKit; API methods return metadata, not audio.
3. Partition cache files, persisted queues and resume state by provider and
   account. Existing single-provider item keys and JSON fields retain their
   old format. Do not switch providers against an unpartitioned cache.
4. Advertise capabilities and hide unsupported frontend actions.

The optional `visuals` and shelf `route` fields are additive. Existing Rust/Qt
FFI, audio code and release packaging are preserved. Mixed-provider queues
are not part of this first extraction.

## Validation

Run `cargo test --workspace --locked` and `cargo fmt --all -- --check`.
The Apple HTTP fixtures live beside the adapter. Independent provider,
daemon and terminal tests exercise the contracts without an Apple account.
Build Qt with `bin/build-gui`. The GUI provider tests run without a real daemon:

```sh
QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME= QT_QUICK_BACKEND=software \
  QT_STYLE_OVERRIDE=Basic /usr/lib/qt6/bin/qmltestrunner -input gui/tests
```
