# Music providers

Brumm keeps the existing Rust core, Unix socket, Qt frontend, terminal
frontend, and shared player. Provider implementations are built in and run
inside the daemon. Apple Music and Tidal use the provider
contract end to end. Tidal is selectable on this development branch. Live
sign-in, browsing and full-track playback state pass. Audible output
is still unverified.

## Boundaries in this branch

```text
crates/brumm/src/
├── provider/
│   ├── session.rs          Provider definition, navigation and capabilities
│   ├── registry.rs         Built-in choices and selection payload
│   ├── tidal/
│   │   ├── session.rs      Definition, private SDK bridge and account identity
│   │   ├── auth.rs         Device login and serialized token refresh
│   │   ├── mod.rs          Catalog, collections and explicit preference writes
│   │   ├── features.rs     Native pages and independently loaded mix fallback
│   │   └── player.html     Bundled official Web SDK adapter
│   └── apple/
│       ├── session.rs      Apple provider definition and playback page
│       ├── auth.rs         Apple sign-in flow
│       ├── provider.rs     Apple API adapter
│       └── features.rs     Apple-specific browse pages
├── daemon/
│   ├── providers.rs        Construct the selected provider API
│   └── mod.rs              Shared commands, lifecycle and socket
├── engine/                 Shared Chrome/CDP lifecycle and controls
├── login/mod.rs            Generic login command and progress bridge
├── ipc.rs                  Versioned newline-delimited JSON socket messages
├── tui/                    Dynamic provider navigation and rendering
└── config.rs               Provider selection and account-scoped cache paths

gui/                        Qt frontend; consumes provider metadata over IPC
tools/fakedaemon/            Deterministic provider fixture for frontend checks
```

`provider::session::Definition` describes a provider's stable ID, navigation,
capabilities, sign-in state, API client, playback page, and authentication
entry point. The registry is the only place that constructs a definition.
Navigation destinations are typed as home, page route, library list, container,
search, or queue. Frontends render the supplied order, labels, and semantic
icons; they do not choose Apple shelf titles or infer resource behavior from
Apple ID prefixes.

The socket remains the GUI/TUI-to-daemon boundary. Provider implementations do
not expose another socket. The provider selection response includes a protocol
version, session generation, navigation, capabilities, and provider choices. Unknown
or incompatible protocol versions produce a visible error. Apple keeps its
legacy Home command while other typed destinations use shared browse commands.

Apple owns the MusicKit login page, callback flow, API composition, feature
routes, and HTML playback page under `provider/apple/`. The shared engine owns
Chrome/CDP startup and common playback controls. It receives an opaque playback
page from the selected provider instead of loading Apple credentials itself.
The generic login command reports provider progress and cancellation without
knowing Apple's token format.

## Navigation and capabilities

The Apple definition preserves its current order: Home, Explore, Playlists,
Albums, Artists, Songs, and Radio. Brumm appends Search and Queue as shared
utilities. Capabilities declare which resource kinds support ratings,
favorites, and collection edits, plus features such as stations and previews.
Both frontends use these declarations to hide unavailable actions.

Cache files use a provider and account-derived namespace. Legacy Apple library
and resume files migrate once into the first namespace and remain in place for
rollback. The current Apple account key is a digest of the Music User Token,
not a stable Apple account identifier; signing in again with a rotated token can
create a fresh cache namespace. Tidal uses a digest of its verified user ID. Signed-out namespaces are separate
from signed-in accounts. Only Apple imports legacy cache files.

## Switching and Tidal

Both frontends expose Switch Music Provider (Ctrl+O). The daemon serializes
transitions, waits for dispatched mutations, cancels outgoing background work,
closes playback and the private SDK bridge, then installs the next account.
Queued commands keep their original session generation. Replies and asynchronous
UI results from an obsolete session cannot update the new provider's view.
Switching retains saved credentials and restores each account's queue paused.

Tidal supplies Home, Explore, Playlists, Albums, Artists and Tracks. Search and
Queue remain shared. Home first requests native modules, with independent
personal mix fallbacks; inaccessible editorial discovery returns an informative
Explore page. Internal endpoint access remains a live validation question.

Tidal exposes one collection preference, with explicit desired-state writes and
membership reads. Owned and favorited playlists are combined without treating
favorites as editable. Playlist create and append preserve order and duplicates;
a failed append reports the created playlist for recovery. Advanced edits,
folders, radio, autoplay and previews are not advertised.

Device login reuses the approved compatibility client from Untidy. This is a
service compatibility dependency, not a claim of Tidal application approval.
The official Web SDK and its locked dependencies are bundled from `web/tidal/`;
rebuild with `bin/build-tidal`. The private loopback transport exposes finite,
account-bound credential and metadata operations with origin/Host/path checks,
request limits and no-store responses. Private credentials never enter the
frontend socket. Preview-only playback is rejected and actual presentation and
quality are reported separately from the requested quality.

Tidal library and resume caches expire after 24 hours. Shared artwork cache
entries expire after 24 hours without use and retain the existing size bound.
Personal tokens and real account data are excluded from fixtures and commits.

Apple's catalog token and fixture-backed browsing can be checked without a
subscription. This branch has not verified Apple Music subscription playback;
the available environment has no Apple Music subscription. The clean
architecture plan and phase roadmap are in
[`provider-abstraction-plan.md`](provider-abstraction-plan.md).
The accepted Tidal product decisions and implementation details are in
[`tidal-provider-spec.md`](tidal-provider-spec.md).

## Development checks

Run Rust and QML tests in the Omarchy test box when they may touch the desktop
session. Build the Qt frontend with `bin/build-gui`. `tools/fakedaemon` supplies
fictional provider data and does not require personal credentials.

### Current implementation evidence (2026-10-14)

- 170 Rust core unit tests, 5 shared-library tests and 3 daemon integration tests pass.
- 12 QML checks and 10 SDK adapter checks pass.
- Rust formatting, Clippy with warnings denied, Qt build and diff whitespace checks pass.
- A fictional Tidal desktop fixture renders the approved navigation and unified heart.
- Live device login completes and survives restarts and Apple → Tidal switching.
- Live albums, owned/favorited playlists, artists, tracks, search and all three
  personal mix endpoints return content. Favorite membership reads pass for all
  four resource kinds. Tidal's API-root-relative cursor links omit `/v2`; the
  adapter normalizes them while retaining the fixed origin and opaque cursor.
- Native HOME_STATIC/HOME_EDITORIAL requests require `clientVersion` in addition
  to device/system/subject. Corrected requests return HTTP 500 for this client;
  Home uses its personal mix fallback and Explore explains unavailability.
- The live SDK reports FULL presentation, HIGH quality, AAC at 44.1 kHz, and
  320.403-second duration. Position advances past 32 seconds without an error.
  Seeking near the end automatically transitions to the next track. Duplicate
  queue entries, manual next, pause and resume pass live checks in silent omabox.
- The media CDN returns HTTP 403 for the loopback browser Origin, while the same
  signed byte request without Origin succeeds with HTTP 206. The SDK now uses a
  provider-owned native byte transport for the verified
  `sp-ad-fa.audio.tidal.com` HTTPS origin. Other hosts retain normal browser
  networking. The SDK still owns manifests, decoding, DRM and queue events.
- The transport is account-bound, accepts only same-origin SDK requests, forwards
  only a validated single byte Range, follows no redirects, sends no cookies or
  OAuth tokens, and buffers at most 32 MiB per response with four concurrent
  deliveries. Responses are no-store; closing the engine cancels active requests.
  No signed media URLs are persisted or sent to frontends. New CDN hosts and
  larger quality tiers need separate verification before expanding this allowlist.
- Audible playback remains unverified because omabox has no audio output.
- Live favorite/playlist mutations were not performed on the user's account;
  deterministic fixtures cover them. Apple paid playback remains unverified.
- Live testing exposed and fixed a nested heartbeat payload that had hidden SDK
  readiness. The adapter now emits the shared flat player/revision document;
  an SDK test guards both immediate and heartbeat responses.

For a fictional Tidal preview, run `tmp/fakedaemon` with
`BRUMM_FIXTURE_PROVIDER=tidal` inside omabox, then launch the Qt frontend in the
same box. The default fixture remains Apple.
