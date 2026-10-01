# Working on brumm

## Release notes

The notes of a release are read twice: on GitHub, and inside brumm, where
the update box (<kbd>U</kbd>) shows their list items under "vX is
available". So they are short and plain:

```markdown
# ʕ•ᴥ•ʔ brumm 0.8: now with a GUI

## New
- 🪟 brumm comes with a GUI. Press <kbd>g</kbd> to switch.
- 🧱 Cover wall. Click a cover to play it.
- 🌈 12 visualizers in GUI mode.

## Faster
- 🪶 The terminal player uses a third of the CPU.

## Fixed
- ⏭️ Songs from the cover wall keep the queue.

**Update:** press <kbd>U</kbd> when brumm offers it, or run `brumm update`.
```

- The points stand in sections, in this order and only those that have
  something: `## New`, `## Changed`, `## Faster`, `## Fixed`. The update
  box shows the sections and their points, nothing else.
- Every point starts with one emoji, then says it the way you would tell a
  friend: what it is, and the key or command to get it. "brumm comes with a
  GUI. Press g to switch." Not "A window, too: brumm --gui, in your theme's
  and the cover's colors".
- Plain words, the usual names: GUI, terminal, cover wall, visualizer. No
  poetry, no bold catchwords, no adjectives that sell.
- One line per point, about 60 characters, two short sentences at most.
- Seven points at most, three for a patch release. Put things together
  (the GUI and the key that switches to it are one point); leave out what
  is small.
- No em dashes (—), anywhere.
- No introduction. One image or video
  above the points is fine for a big release.
- The title is the version and a few plain words: `v0.8: now with a GUI`
  on GitHub, `# ʕ•ᴥ•ʔ brumm 0.8: now with a GUI` in the notes.
- The **Update:** line ends it; anything brumm needs anew (a package) goes
  there.
- One release per minor version stays on the releases page: a new minor's
  notes take in what its patch releases brought, and those are deleted (the
  tags stay).

## Releases

- A signed tag `vX.Y.Z` on main releases: the workflow builds, signs and
  publishes. Afterwards its notes are written by hand, as above
  (`gh release edit vX.Y.Z --title … --notes-file …`).
- brumm's updater and the installer take GitHub's latest release, which is
  always a stable one. To test a release first, tag a pre-release
  (`v0.9.0-rc1`): the workflow marks it so, and nobody is offered it.
