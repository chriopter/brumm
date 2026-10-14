# Music providers

Brumm keeps the existing Rust core, Unix socket, Qt frontend, terminal
frontend, and shared player. Provider implementations are built in and run
inside the daemon. The current branch makes Apple Music use the provider
contract end to end; Tidal remains unavailable pending its own authentication,
API, and playback implementation.

## Boundaries in this branch

```text
crates/brumm/src/
├── provider/
│   ├── session.rs          Provider definition, navigation and capabilities
│   ├── registry.rs         Built-in choices and selection payload
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
version, navigation, capabilities, and the existing provider choices. Unknown
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
create a fresh cache namespace. Stable account identity and a full provider
switch lifecycle belong to the next phase.

## Next steps

1. Complete and review provider switching, including stale request and callback
   handling, session restoration, and account identity.
2. Add Tidal authentication, API mapping, navigation, capabilities, and a
   playback backend behind the same contracts.
3. Validate live sign-in and audible playback with eligible accounts, then
   prepare the upstream pull request.

Apple's catalog token and fixture-backed browsing can be checked without a
subscription. This branch has not verified Apple Music subscription playback;
the available environment has no Apple Music subscription. The clean
architecture plan and phase roadmap are in
[`provider-abstraction-plan.md`](provider-abstraction-plan.md).

## Development checks

Run Rust and QML tests in the Omarchy test box when they may touch the desktop
session. Build the Qt frontend with `bin/build-gui`. `tools/fakedaemon` supplies
fictional provider data and does not require personal credentials.
