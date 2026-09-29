# ʕ•ᴥ•ʔ brumm

**Apple Music for [Omarchy](https://omarchy.org).** Your whole library and the full catalog in a terminal that looks like it belongs on your desktop: pixel-art covers, full tracks, and your Omarchy theme's colors. Music keeps playing in the background with media keys and a widget in the bar.

![brumm](docs/screenshot.png)

## Features

- **Everything in Apple Music:** your playlists, albums, artists and songs, and the queue. Paste a music.apple.com link to open it.
- **Home:** recently played and recently added, heavy rotation, Apple's picks for you, and the top charts.
- **Search as you type:** suggestions, top results, the catalog by kind (stations too), and matches in your own library.
- **Artists:** their station, top songs, latest release, albums, singles, and artists like them. Click the artist or album under the title to open it.
- **Radio:** a tab with your station, Apple's live radio and the stations you played; <kbd>R</kbd> starts a station from the selected song or artist; with autoplay on, similar music plays on when the queue ends.
- **Your library:** <kbd>*</kbd> loves and <kbd>d</kbd> dislikes a song, album, playlist or station; <kbd>i</kbd> adds from the catalog to your library; <kbd>P</kbd> adds to a playlist or makes a new one. Albums show their year, label, quality and Apple's notes.
- **Queue and preview:** <kbd>z</kbd> queues, <kbd>Z</kbd> plays next; hold <kbd>space</kbd> (or press <kbd>O</kbd>) on a song to hear its clip, and your music continues after. Scrolling shows the selected cover on top of the playing one for a moment.
- **Keyboard and mouse:** <kbd>←</kbd> <kbd>→</kbd> or <kbd>1</kbd>–<kbd>8</kbd> switch sections, <kbd>shift</kbd>+<kbd>←</kbd> <kbd>→</kbd> seek; click to open or play, drag the divider, click the bar to seek. <kbd>?</kbd> lists every key, and the options can put each key right on its button; the footer's keys are buttons too.
- **Visualizer:** <kbd>f</kbd> for fullscreen: twelve styles that hear the kick drum — MilkDrop, synthwave, a starfield, 3D wireframes, fireworks, a lava lamp, an oscilloscope, plasma, fire, digital rain, a ridgeline, an LED EQ. <kbd>tab</kbd> switches, <kbd>v</kbd> lists them all, <kbd>a</kbd> stops the change every minute.
- **Options:** <kbd>o</kbd> opens a few switches: covers as pixel art, smooth, or the original image (kitty, Ghostty), the level meter, scrolling names, cover cards, autoplay, keys right on the buttons, and the visualizer at 30, 60 or 120 fps. Changes show at once.
- **Light in the background:** nothing polls while music is paused, the visualizer runs only while you can see it, and after ten idle minutes the player quits until you press play.
- **At home in Omarchy:** your theme's colors, media keys, a widget in the bar. <kbd>Q</kbd> or closing the window leaves the music playing, and it picks up where you left off; <kbd>q</kbd> stops it and quits.

What Apple's API does not allow, brumm cannot do either: deleting or renaming playlists and removing songs from them or from the library (use the Music app), lyrics, lossless or Dolby Atmos playback (MusicKit on the web streams 256 kbps AAC), crossfade, and Replay.

## Install

```sh
curl -fsSL https://github.com/chriopter/brumm/releases/latest/download/install.sh | bash
brumm login
```

`brumm login` opens the player once you are signed in. Later, run `brumm` or click the music widget in the bar. You need an Apple Music subscription.

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
