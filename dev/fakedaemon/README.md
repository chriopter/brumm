# Local music-service demo

From the repository root:

```sh
dev/demo            # First start: choose and connect services
dev/demo connected  # All three connected, Apple Music selected
dev/demo tidal      # All three connected, Tidal selected
dev/demo spotify    # All three connected, Spotify selected
```

Open **Music Services** under **Options** in the GUI menu, or press
**Ctrl+O**, to add or switch a service. Connecting succeeds locally without
opening a browser. Press **g** to switch between GUI and TUI.

The demo builds the current source and starts `dev/fakedaemon`. It uses
sample music, generated covers and a simulated playback clock. It does not
play audio or use credentials, accounts or the running player's configuration.
Each run starts with fresh demo data. Close its windows to end the demo.

Requires the normal GUI build dependencies, Go, and a terminal such as foot
for switching to the TUI. Set `BRUMM_PROTOTYPE_BUILD` to reuse a demo build
directory. The standalone fixture daemon also understands
`BRUMM_FIXTURE_FIRST_RUN`, `BRUMM_FIXTURE_ALL_SERVICES`, and
`BRUMM_FIXTURE_PROVIDER=apple|tidal|spotify`.
