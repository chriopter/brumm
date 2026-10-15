# ʕ•ᴥ•ʔ brumm

<img src="docs/bear.webp" alt="brumm's dancing bear" width="180">

**Music player for [Omarchy](https://omarchy.org) & Linux, with GUI & TUI**

**Supported services**

| Service | Integration by |
|---|---|
| Apple Music | [@chriopter](https://github.com/chriopter) |
| Tidal | [@keylimesoda](https://github.com/keylimesoda) |
| Spotify | [@sean-imus](https://github.com/sean-imus) |

**Features**

- ⇄ GUI and TUI: press <kbd>g</kbd> to switch while music keeps playing
- 🎵 Library, playlists, favorites and search as you type
- 🧭 Home and Explore: recommendations, charts, artists and radio
- 🎬 Music video previews in the GUI
- 🌈 Cover art and thirteen visualizers
- ⌨️ Keyboard and mouse controls
- 🐧 Desktop theme, media keys and bar widget

## Install

```sh
curl -fsSL https://github.com/chriopter/brumm/releases/latest/download/install.sh | bash
```

Music-video previews need Qt 6 Multimedia. The installer adds
`qt6-multimedia` on Omarchy and Arch Linux; install your distribution's
Qt 6 Multimedia QML package on other Linux systems.

| TUI | GUI |
|:---:|:---:|
| ![brumm in the terminal](docs/tui.webp) | ![brumm as a window](docs/gui.webp) |
| **Explore** | **Visualizer** |
| ![Explore with covers and the glass player](docs/release-1.0.0.webp) | ![the visualizer, terminal and window](docs/visualizer.webp) |

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

Enable <kbd>Super</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd> to open brumm with
`brumm keys on`; `brumm keys off` restores the previous music-key behavior.

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

The daemon and TUI use Rust; the GUI uses Qt Quick/QML and C++.
Both interfaces share the same daemon and music session.

```text
crates/brumm/src/
├── provider/  # Service APIs: sign-in, library, catalog and track metadata
└── engine/    # Playback: run each service's web player in Chrome
```

Providers handle Apple Music, Tidal and Spotify. The engine runs their
playback pages and controls the audio through Chrome DevTools. See the
[provider guide](docs/providers.md) for details.

```text
  Apple Music           Tidal              Spotify
       ^                  ^                   ^
       |                  |                   |
  Apple provider    Tidal provider    Spotify provider
       \                  |                  /
        +-----------------+-----------------+
                          |
                   brumm daemon <----> Playback engine
              Shared provider interface   Chrome / web player
         Session / Library / Queue / Options
                          |
              +-----------+------------+
              |                        |
       Unix socket / JSON         MPRIS / D-Bus
           /       \                   |
         TUI       GUI           Omarchy widget
```

### Development scripts

`bin/` contains the scripts for building, running and previewing brumm.
Building needs Rust 1.89+, Qt 6 and a C++ compiler; sample-data previews
also need Go. Rebuilding the Tidal adapter needs Node.js/npm.

```sh
bin/setup        # build and install this checkout
bin/dev          # foreground daemon; add ui or gui to open a player
bin/preview      # isolated preview with the real account (needs Python 3)
bin/prototype    # isolated preview with sample music
bin/gui-shot out.png  # screenshot with sample data
bin/build-tidal  # rebuild after changes to the Tidal web adapter
```

For fake service setup and switching, run `tools/demo-services`, or
`tools/demo-services connected` to start with all three connected.
The [demo](tools/fakedaemon/README.md) needs no account and plays no audio.
`bin/preview` uses the real account, including actions taken in its UI.
`bin/setup` installs brumm; `bin/dev` stops an installed brumm user service.

Inspired by [vibez](https://github.com/simonepelosi/vibez).
