# ʕ•ᴥ•ʔ brumm

<img src="docs/bear.webp" alt="brumm's dancing bear" width="180">

**Music player for [Omarchy](https://omarchy.org) & Linux, with GUI & TUI**

- 🎧 Supported Services: Apple Music by [@chriopter](https://github.com/chriopter), Tidal by [@keylimesoda](https://github.com/keylimesoda), Spotify by [@sean-imus](https://github.com/sean-imus)
- 🎵 Library, catalog, search as you type
- 🏠 Home: recent, heavy rotation, picks
- 🧭 Explore: charts, genres, videos, countries
- ❤️ Favorites, Replay and playlist folders
- 🎬 Music video previews in the GUI
- 📻 Radio: your station, live, any song
- 🎤 Artists: top songs, releases, similar ones
- ♥ Love, dislike, library, playlists
- 🖼 Covers: pixel art, smooth, or real
- 🌈 Thirteen visualizers that hear the kick
- ⌨️ <kbd>?</kbd> for keys, mouse works everywhere
- 🔋 Nearly idle while paused
- 🐧 Theme colors, media keys, bar widget
- ⇄ Terminal and window, <kbd>g</kbd> to switch, music plays on
- 🎹 <kbd>Super</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd> opens brumm instead of Spotify, if you like — `brumm keys on|off`

## Install

```sh
curl -fsSL https://github.com/chriopter/brumm/releases/latest/download/install.sh | bash
```

Music-video previews need Qt 6 Multimedia. The installer adds
`qt6-multimedia` on Omarchy and Arch Linux; install your distribution's
Qt 6 Multimedia QML package on other Linux systems.

![brumm in the terminal](docs/tui.webp)
TUI

![brumm as a window](docs/gui.webp)
GUI

![Explore with covers and the glass player](docs/release-1.0.0.webp)
Explore

![the visualizer, terminal and window](docs/visualizer.webp)
Visuals

## Keys

| | |
|---|---|
| <kbd>enter</kbd> | play / open |
| <kbd>space</kbd> | pause · hold to preview |
| <kbd>←</kbd> <kbd>→</kbd> <kbd>1</kbd>–<kbd>9</kbd> | sections |
| <kbd>/</kbd> | filter here · <kbd>tab</kbd> search everywhere |
| <kbd>R</kbd> | radio from song or artist |
| <kbd>*</kbd> <kbd>d</kbd> <kbd>i</kbd> <kbd>P</kbd> | love · dislike · library · playlist |
| <kbd>f</kbd> | visualizer: <kbd>tab</kbd> <kbd>v</kbd> <kbd>a</kbd> <kbd>x</kbd> <kbd>F</kbd> |
| <kbd>g</kbd> | switch between terminal and window |
| <kbd>o</kbd> | options |
| <kbd>Ctrl</kbd>+<kbd>o</kbd> | connect or switch music services |
| <kbd>U</kbd> | look for an update |
| <kbd>Q</kbd> / <kbd>q</kbd> | close, keep playing / quit |

Not possible with Apple's API: deleting or renaming playlists, lyrics, lossless.
Features vary by service; radio, Replay and playlist folders are Apple Music features.
Spotify needs your own [developer app](https://developer.spotify.com/dashboard) and Premium.
Its visualizer has no access to the audio. See [service setup and limits](docs/providers.md).

## More

- 🔑 Needs an Apple Music, Tidal or Spotify Premium subscription. Connect your services on first start.
- ⇄ Click the service name or press <kbd>Ctrl</kbd>+<kbd>o</kbd> to switch or add services later.
- ▶️ Later: `brumm`, the launcher, or the bar widget — whichever face you used last
- 🔄 Updates: <kbd>U</kbd> when offered, or `brumm update`
- 🔏 Signed releases (Ed25519) and GitHub-attested builds — check the installer first:

```sh
curl -fsSLO https://github.com/chriopter/brumm/releases/latest/download/install.sh
gh attestation verify install.sh -R chriopter/brumm && bash install.sh
```

## Development

The Rust daemon owns service connections, playback, the queue and shared
options. The TUI and GUI talk to it over a local socket, so switching between
them keeps the same music session. The [provider guide](docs/providers.md)
explains the API boundary and service integrations.

### Repository layout

```text
brumm/
├── crates/
│   ├── brumm/                 # Rust application
│   │   ├── src/
│   │   │   ├── daemon/        # Background service, sessions, library and queue
│   │   │   ├── provider/      # Service APIs, authentication and data mapping
│   │   │   │   ├── apple/     # Apple Music
│   │   │   │   ├── tidal/     # Tidal
│   │   │   │   └── spotify/   # Spotify
│   │   │   ├── engine/        # Browser playback and Chrome DevTools bridge
│   │   │   ├── chrome/        # Private Chrome download and verification
│   │   │   ├── tui/           # Terminal UI, input, navigation and visualizers
│   │   │   ├── art/           # Cover rendering for the terminal
│   │   │   ├── update/        # Installation, updates and signature checks
│   │   │   ├── ipc.rs         # Local socket protocol
│   │   │   ├── config.rs      # Configuration and shared options
│   │   │   ├── music.rs       # Shared music data types
│   │   │   ├── mpris.rs       # Desktop media controls
│   │   │   └── main.rs        # CLI entry point
│   │   └── tests/             # Daemon integration tests
│   └── brumm-shared/          # Shared Rust calculations and C interface
│       └── src/               # Cover colors and audio analysis
├── gui/
│   ├── qml/                  # GUI screens, controls and navigation
│   │   └── viz/              # Visualizer styles and feedback textures
│   ├── shaders/              # GLSL effects for the GUI and visualizers
│   ├── src/                  # C++ Qt host, daemon bridge and frame clock
│   └── tests/                # QML navigation and layout tests
├── web/
│   ├── tidal/                # Source and tests for the Tidal Web SDK adapter
│   └── spotify/              # Tests for the Spotify playback adapter
├── tools/
│   ├── demo-services         # Start the fake music-service demo
│   ├── fakedaemon/           # Sample music, covers and simulated service APIs
│   ├── devtoken/             # Apple Music developer-token generator
│   └── sign/                 # Release signing tools
├── bin/                      # Build, install, preview and benchmark scripts
├── omarchy/
│   ├── plugin/               # Bar widget and plugin manifest
│   ├── brumm.service         # systemd user service
│   └── brumm.desktop         # Desktop launcher
├── docs/                     # Provider guides, screenshots and release notes
├── benchmarks/               # Saved benchmark data
├── .github/                  # CI, release workflows and repository rules
├── Cargo.toml                # Rust workspace and build settings
└── go.mod                    # Go development-tool dependencies
```

### Build and run

Building needs Rust 1.89 or newer, a C++ compiler and Qt 6, including Qt Quick,
Quick Controls and Shader Tools. Video previews also need Qt Multimedia.
Go is needed for the demo and signing tools, Python 3 for the live preview,
and Node.js/npm when rebuilding the Tidal Web SDK adapter.

```sh
bin/setup        # build and install this checkout
bin/dev          # run the daemon in the foreground
bin/dev ui       # open the TUI against the daemon
bin/dev gui      # build and open the GUI against the daemon
bin/build-tidal  # rebuild the bundled adapter after changing web/tidal
```

`bin/setup` installs the application and desktop integration. `bin/dev` stops
an installed brumm user service before starting the development daemon.

### Preview and demo

```sh
tools/demo-services            # first start with fake Apple Music, Tidal and Spotify
tools/demo-services connected  # all three services already connected
bin/preview                    # isolated player using the existing real account
bin/gui-shot out.png           # screenshot with sample data
bin/viz-thumbs                  # regenerate visualizer thumbnails
```

The service demo needs no account and plays no audio. Its connections and
changes stay in memory; each run starts fresh. See the
[demo instructions](tools/fakedaemon/README.md) for individual services.
`bin/preview` uses private configuration and caches, but playback and actions
in that preview use the real account.

### Checks

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
go test ./tools/fakedaemon
node --test web/tidal/player.test.mjs web/spotify/player.test.mjs
QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME= "$(qmake6 -query QT_HOST_BINS)/qmltestrunner" -input gui/tests
```

Inspired by [vibez](https://github.com/simonepelosi/vibez).
