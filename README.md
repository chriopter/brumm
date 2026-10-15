# ʕ•ᴥ•ʔ brumm

<img src="docs/bear.webp" alt="brumm's dancing bear" width="180">

**Music player for [Omarchy](https://omarchy.org) & Linux, with GUI & TUI**

- <details>
  <summary>🎧 Apple Music by <a href="https://github.com/chriopter">@chriopter</a></summary>

  Requires an Apple Music subscription. Connect through Music Services
  (<kbd>Ctrl</kbd>+<kbd>o</kbd>).

  Includes radio, Replay and playlist folders. Playlist deletion and renaming,
  lyrics and lossless playback are not available in brumm.

  </details>

- <details>
  <summary>🎧 Tidal by <a href="https://github.com/keylimesoda">@keylimesoda</a></summary>

  Requires a Tidal subscription. Connect through Music Services
  (<kbd>Ctrl</kbd>+<kbd>o</kbd>) and complete the device sign-in.

  Includes library, search, favorites and playlist creation. Advanced playlist
  edits, Replay, folders, radio, autoplay and video previews are not available.

  </details>

- <details>
  <summary>🎧 Spotify by <a href="https://github.com/sean-imus">@sean-imus</a></summary>

  Requires Spotify Premium and your own
  <a href="https://developer.spotify.com/dashboard">developer app</a> with
  Web API and Web Playback SDK enabled. Register
  `http://127.0.0.1:7777/spotify/callback` as the redirect URI, then connect
  through Music Services (<kbd>Ctrl</kbd>+<kbd>o</kbd>) using the Client ID.
  No client secret is needed.

  Radio, Explore, Replay, folders, autoplay and previews are not available.
  The visualizer has no access to Spotify's audio spectrum.
  See the [service setup guide](docs/providers.md#spotify) for details.

  </details>

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

Press <kbd>?</kbd> in brumm for the full shortcut list.

| Action | Shortcut |
|---|---|
| Play or open the selection | <kbd>Enter</kbd> |
| Pause or resume | <kbd>Space</kbd> |
| Switch between GUI and TUI | <kbd>g</kbd> |
| Filter or search | <kbd>/</kbd> |
| Connect or switch music services | <kbd>Ctrl</kbd> + <kbd>O</kbd> |
| Open or close the visualizer | <kbd>f</kbd> |
| Close the interface, keep music playing | <kbd>Shift</kbd> + <kbd>Q</kbd> |
| Quit brumm and stop the music | <kbd>q</kbd> |

While filtering, press <kbd>Tab</kbd> to search the entire service.

<details>
<summary>More player shortcuts</summary>

| Action | Shortcut |
|---|---|
| Switch section | <kbd>1</kbd>–<kbd>9</kbd> |
| Show Now Playing and the queue | <kbd>c</kbd> |
| Open options | <kbd>o</kbd> |
| Next track | <kbd>n</kbd> |
| Previous track | <kbd>p</kbd> |
| Preview the selection | Hold <kbd>Space</kbd> |
| Like the selection | <kbd>*</kbd> |
| Dislike the selection | <kbd>d</kbd> |
| Add to the library | <kbd>i</kbd> |
| Add to a playlist | <kbd>Shift</kbd> + <kbd>P</kbd> |
| Start radio from the song or artist | <kbd>Shift</kbd> + <kbd>R</kbd> |

Available actions depend on the music service.

</details>

<details>
<summary>Visualizer shortcuts</summary>

| Action | GUI | TUI |
|---|---|---|
| Choose a style | <kbd>v</kbd> | <kbd>v</kbd> |
| Next style | <kbd>Tab</kbd> | <kbd>Tab</kbd> |
| Previous style | <kbd>Shift</kbd> + <kbd>Tab</kbd> | <kbd>Shift</kbd> + <kbd>Tab</kbd> |
| Toggle automatic style changes | <kbd>a</kbd> | <kbd>a</kbd> |
| Toggle auto-hide controls | <kbd>x</kbd> | <kbd>x</kbd> |
| Change frame rate | <kbd>Ctrl</kbd> + <kbd>F</kbd> | <kbd>Shift</kbd> + <kbd>F</kbd> |
| Toggle fullscreen | <kbd>Shift</kbd> + <kbd>F</kbd> | Uses the terminal window |
| Leave the visualizer | <kbd>Esc</kbd> | <kbd>Esc</kbd> |

</details>

<details>
<summary>Desktop shortcut</summary>

Enable <kbd>Super</kbd> + <kbd>Shift</kbd> + <kbd>M</kbd> to open brumm:

```sh
brumm keys on
```

Run `brumm keys off` to restore the previous music-key behavior.

</details>

## More

- 🔑 Needs an Apple Music, Tidal or Spotify Premium subscription. Connect your services on first start.
- ⇄ Open Music Services in the menu or press <kbd>Ctrl</kbd>+<kbd>o</kbd> to switch or add services.
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
+--------------------------------------------------------+
| brumm daemon - Rust / Tokio                            |
| systemd background service                             |
|                                                        |
| Providers: Apple Music / Tidal / Spotify               |
| Library / queue / sign-in / options                    |
+--+-----------------------------------------------------+
   |
   +-- playback --> Engine: Chrome / Widevine
   |                MusicKit JS / Tidal SDK / Spotify SDK
   |
   +-- socket ----> TUI: Rust / Ratatui
   |
   +-- socket ----> GUI: Qt Quick / QML
   |
   +-- MPRIS -----> Omarchy widget
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
