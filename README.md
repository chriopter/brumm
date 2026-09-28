# ʕ•ᴥ•ʔ brumm

**Apple Music for [Omarchy](https://omarchy.org).** Your whole library and the full catalog in a terminal that looks like it belongs on your desktop: pixel-art covers, full tracks, and your Omarchy theme's colors. Music keeps playing in the background with media keys and a widget in the bar.

![brumm](docs/screenshot.png)

## Features

- **Everything in Apple Music:** playlists, albums, artists, your songs, a catalog search that answers as you type, and the queue. Paste a music.apple.com link to open it.
- **Keyboard and mouse:** <kbd>←</kbd> <kbd>→</kbd> switch sections, <kbd>shift</kbd>+<kbd>←</kbd> <kbd>→</kbd> seek; click to open or play, drag the divider, click the bar to seek; <kbd>?</kbd> lists every key.
- **Preview:** hold <kbd>space</kbd> (or press <kbd>o</kbd>) on a song to hear it; your music continues after.
- **Visualizer:** <kbd>f</kbd> for fullscreen, ten styles from Winamp bars to a MilkDrop ring.
- **Favorites and queue:** <kbd>*</kbd> to love a song, <kbd>z</kbd> to queue it, <kbd>Z</kbd> to play it next.
- **At home in Omarchy:** your theme's colors, media keys, a widget in the bar; music keeps playing when the window closes and picks up where you left off.

## Install

```sh
curl -fsSL https://github.com/chriopter/brumm/releases/latest/download/install.sh | bash
brumm login
```

Then run `brumm`, or click the music widget in the bar. You need an Apple Music subscription.

To check the installer before running it, download it, verify that GitHub built it from this repository, then run it:

```sh
curl -fsSLO https://github.com/chriopter/brumm/releases/latest/download/install.sh
gh attestation verify install.sh -R chriopter/brumm
bash install.sh
```

## Update

brumm checks for a new version every time it starts; press <kbd>U</kbd> when it offers one. Or run `brumm update`.

Updates are signed: brumm and the installer only install a release whose checksums carry this project's Ed25519 signature for that exact version. Every build also has a GitHub attestation (`gh attestation verify brumm-linux-amd64.tar.gz -R chriopter/brumm`).

Inspired by [vibez](https://github.com/simonepelosi/vibez).

## Development

```sh
bin/setup   # Build and install from this checkout
bin/dev     # Run the daemon in the foreground (`bin/dev ui` for the TUI)
bin/update  # Pull and reinstall
```
