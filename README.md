# ʕ•ᴥ•ʔ brumm

**Apple Music for [Omarchy](https://omarchy.org).** Your whole library and the full catalog in a terminal that looks like it belongs on your desktop: pixel-art covers, a live spectrum, full tracks, and your Omarchy theme's colors. Music keeps playing in the background with media keys and a widget in the bar.

![brumm](docs/screenshot.png)

- Playlists, albums, artists, songs, a live catalog search and the queue, with keyboard and mouse
- Hold <kbd>space</kbd> on a song to preview it, <kbd>f</kbd> for a fullscreen visualizer
- Opens where you left off, and updates itself with signed releases

## Install

```sh
curl -fsSL https://github.com/chriopter/brumm/releases/latest/download/install.sh | bash
brumm login
```

Then run `brumm`, or click the music widget in the bar. You need an Apple Music subscription.

## Update

brumm checks for a new version every time it starts; press <kbd>U</kbd> when it offers one. Or run `brumm update`.

Inspired by [vibez](https://github.com/simonepelosi/vibez).

## Development

```sh
bin/setup   # Build and install from this checkout
bin/dev     # Run the daemon in the foreground (`bin/dev ui` for the TUI)
bin/update  # Pull and reinstall
```
