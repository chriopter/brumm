# API preview

`bin/preview` builds this checkout and opens the GUI with the existing Apple
Music account on workspace 4, without focusing it. It uses a private temporary
socket, configuration and cache. Credentials stay on this machine. This is a
real player: playback and actions chosen in the UI use the account's API.

`bin/prototype` opens the separate sample-data version. Its player and spectrum
are simulated; its changes are stored only in memory. Lookup results there
are canned. It never writes to the real Apple Music account.

## Navigation

GUI and TUI use Home, Playlists, Albums, Artists, Songs, Radio, Search in that
order. The GUI has icons and text across the top, a small search field and a
Help icon. Queue sits before Volume in the playback controls. The song's
three-dot menu contains playlist/library actions, radio and dislike.

Press `g` to switch between GUI and TUI, retaining the daemon and navigation.
Preview windows stay on workspace 4. On Hyprland, normal ungrouped tiled windows
hand off through a hidden workspace and one compositor batch, preserving their
layout position. Floating or grouped windows use a normal launch.

## Connected features

- Home keeps its quick-start list, with Favorites, Replay and playlist folders.
- Explore has its own section in the same position in GUI and TUI.
- Favorites uses `inFavorites`, fetched with the library and filtered locally.
  The detail view can submit typed songs, albums, artists and playlists to
  Apple's favorites endpoint. A successful submission is reported as accepted;
  the refreshed library supplies the actual favorite state.
- Replay requests the latest eligible year and its top songs, albums and
  artists. Sparse song relationships are hydrated in batches. Further pages
  remain accessible.
- Explore links to genres, radio genres, local/global/city/video charts,
  recommendations, catalog lookup, and countries. Country pages offer local
  charts, genres, and supported language information.
- Search requests songs, albums, artists, playlists, stations, videos,
  activities, curators, Apple curators and record labels. Library search
  includes music videos. Suggestions and hints share the suggestions shelf;
  result and artist views expose further pages.
- Artist pages include featured albums, top videos and featured videos.
- Playlist folders resolve the root resource from Apple's response metadata,
  then browse children. Folder and empty-playlist forms create resources, with
  a typed parent relationship inside a folder. If Apple returns no folder root,
  the page explains this and offers root creation actions. Flat playlists remain
  available in Playlists.
- Metadata opens with `Ctrl+i` in either face, or a GUI right-click. Explorer
  metadata lives behind a Details row so it does not crowd out the music.
  Available composer, classical work/movement, ISRC/UPC, genres, release dates,
  audio variants, editorial notes and favorite fields are preserved.
- Catalog lookup accepts ISRC, UPC, equivalent song IDs, multiple IDs of one
  type, and mixed typed IDs (`songs:123; albums:456`).
- Public API resource views, relationships and next-page routes are shared
  between GUI and TUI. Relative routes are validated before authenticated
  requests. Library pagination retains requested limits and attribute extensions.
- GUI music videos play the public preview through Qt Multimedia when installed.
  Full video opens in Apple Music; terminal video selection opens its Apple
  Music URL. Complete DRM video is not yet rendered in the native window.

## Verified locally

The Go test suite passes, including typed favorite requests, folder parent
relationships, root-folder metadata, sparse Replay resources, encoded catalog
queries, shared navigation and Hyprland handoff selection.

Read-only checks against the signed-in account returned Replay, video/city/global
charts, and library favorites. The running preview rendered 100 Replay tracks,
13 Replay albums, 15 artists plus pagination, 57 favorite songs, and 20 video
previews. Favorite submission was checked idempotently against an already
favorited song; Apple accepted the typed request. Folder creation has not been
executed against this account. Its root response currently contains no folder ID.
Native video rendering was verified with a local test clip.

## Boundaries

Lyrics availability is metadata; the public API supplies no lyric text.
Atmos/Lossless availability does not enable those formats in the MusicKit web
engine. Unsupported playlist deletion, rename, reordering and track removal
are not presented as writes. Favorite removal is not implemented through an
undocumented endpoint. Complete DRM video in the GUI remains unfinished.

Claude consultation was attempted with a product description only. The installed
CLI reported its weekly limit and supplied no design feedback.
