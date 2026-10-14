# Tidal provider specification

Status: approved product decisions and implementation plan, 2026-10-14.
Implementation starts from the Apple provider snapshot `eb6a79b` on branch
`tidal-provider`. This document describes work to implement; it is not evidence
that Tidal is enabled in this branch yet.

## Decisions

- Use the working device-login approach from Untidy and the preserved Tidal
  prototype. Device login and reuse of that approach are settled decisions.
- Keep one active provider/account. Retain each provider's saved session when
  switching. Complete the shared switching lifecycle before enabling Tidal.
- Give collection categories direct access: Home, Explore, Playlists, Albums,
  Artists, Tracks. Brumm appends Search and Queue.
- Follow Tidal's native content organization and returned page titles/order.
  The earlier proposed assignment of specific mixes to Home/Explore is superseded.
- Provide one Favorite/Add to My Collection action for a resource, plus playlist
  creation and adding tracks to editable playlists.
- Defer folders, playlist rename/delete/reordering/removal, recommendation
  blocking, radio, autoplay, videos, offline audio, and device control.

## Provider boundaries

Add `provider/tidal/` implementing the existing `Definition`, `Catalog`,
`Library`, and `Api` contracts. The definition owns navigation, capabilities,
authentication, account identity, API construction, and playback setup. Routes
and resource IDs remain opaque to the GUI and TUI.

Proposed file ownership:

```text
provider/tidal/
├── session.rs          Definition, account identity, playback construction
├── auth.rs             Saved credentials, refresh serialization, session checks
├── device.rs           Device authorization, polling, public prompt
├── mod.rs              Catalog/Library/Api implementation and bounded requests
├── mapping.rs          JSON:API documents → Item, Track, Shelf, ExplorePage
├── features.rs         Home, Explore, mix and paginated shelf routes
└── player.html         Private playback page loading the bundled SDK adapter

web/tidal/              Locked SDK dependencies, adapter source, license notices
daemon/                Shared session switching and stale-work protection
engine/                Shared Chrome/CDP lifecycle and private playback transport
gui/ and tui/          Shared navigation, preference actions and player rendering
```

Port reviewed pieces from prototype commit `5e83cf9`, rather than replacing the
current daemon/frontends with prototype versions. The prototype's generic code
predates the Apple provider extraction. Its device flow, JSON:API mapping,
pagination, search include limits, and SDK adapter are implementation references.

## Navigation and content

Use these stable entry IDs and typed destinations:

| ID | Label | Destination | API mapping |
| --- | --- | --- | --- |
| `home` | Home | page `tidal:home` | Native personal page modules when accessible; verified personal mix endpoints provide a reduced page when needed |
| `explore` | Explore | page `tidal:explore` | Verified Tidal editorial/discovery modules; expose an informative unavailable state if the client cannot access them |
| `playlists` | Playlists | library `playlists` | Owned playlists plus favorited playlists |
| `albums` | Albums | library `albums` | `userCollectionAlbums/me/relationships/items` |
| `artists` | Artists | library `artists` | `userCollectionArtists/me/relationships/items` |
| `tracks` | Tracks | library `songs` | `userCollectionTracks/me/relationships/items` |

`Tracks` is the Tidal label. The shared library list token and resource kind
remain `songs` and `song`; no frontend should translate Tidal API names.
Persistence uses provider/account and entry ID rather than label or index.

Tidal's help places My New Arrivals and Your History on Home, while My Mix is
documented under My Collection. Direct collection categories change the Brumm
navigation shape, so My Mix can also be linked from the personal Home page when
no Collection landing page exists. This is an explicit Brumm adaptation.
Daily Discovery retains its Tidal name and updates through Tidal's endpoint.
Use the server's native placement whenever a verified dynamic page supplies it.

Your History contains most-listened mixes by time period. It is not a chronological
Recently Played list. Add a Recently Played shelf only after verifying that the
device client's API supplies that exact concept. Do not substitute local history
or relabel History Mixes as Recently Played.

Home sections load independently. A failed or inaccessible mix must not discard
other successful shelves. An empty mix is a valid new-account state; 400, 403,
and expired authorization are different states. Keep useful content and expose
a concise section error through shared page presentation.

Explore must represent Tidal discovery. Do not fill it with a second copy of
Home's personal mixes or an Apple-specific page. Access to editorial Home
modules is a candidate to investigate, not proof of a public Explore endpoint.

## API surface and access evidence

The live OpenAPI schema fetched on 2026-10-14 reports version `1.10.172`, SHA-256
`9ef8f72e5c165c7bd16bf5d422eaca03a80aef5844104856cb35e060b15837f1`.
The schema includes endpoints at several access tiers. Appearance in the API
tree does not establish access for Brumm's device client.

| Surface | Relevant resource/relationship | Schema tier | First version |
| --- | --- | --- | --- |
| Catalog/search | albums, artists, tracks, playlists, searchResults, searchSuggestions | THIRD_PARTY for the read operations we need | Required |
| Personal mixes | userDailyMixes, userDiscoveryMixes, userNewReleaseMixes and their items | THIRD_PARTY | Verify each independently |
| Favorites | userCollectionTracks/Albums/Artists/Playlists and their items | THIRD_PARTY | Required |
| Owned playlists | `playlists?filter[owners.id]=me` | THIRD_PARTY | Required |
| Playlist create/add | POST playlists; POST playlist items | THIRD_PARTY | Required |
| Native page modules | dynamicPages → modules → dynamicModules.items | INTERNAL | Probe with device client; conditional |
| Feed/activity | userFeedItems, userFeeds | INTERNAL | Deferred |
| Collection folders / Save for Later | userCollectionFolders, folders, folderItems, userCollectionSaveForLaters | INTERNAL | Deferred |
| Radio relationships | artists/tracks → radio | THIRD_PARTY | Deferred despite schema support |
| Recommendation blocks | userRecommendationBlocks relationships | INTERNAL | Deferred |
| Remote playback queues/state | playQueues, userPlaybackStates | INTERNAL | Deferred; Brumm maintains its own playback queue |
| Playback manifests | trackManifests | THIRD_PARTY | SDK-owned consumption; tier alone does not establish full-track entitlement |

Read this table as operation-specific evidence. The same resource may have
internal authoring operations alongside third-party reads. The legacy device
client uses legacy scopes; the schema's published OAuth security annotations
do not guarantee those credentials can call an endpoint. Capture actual
status/error codes, pagination behavior, and returned resource types in fixtures.

Dynamic page queries must use documented filters and relationship includes.
Select the appropriate page type, user subject, country, locale, device/system
type and refresh seed. Inspect returned modules before choosing native placement.
Never invent an `EXPLORE` page type: the inspected schema does not define one.

All REST calls stay under the fixed Tidal API origin/version. Validate opaque
page routes and pagination links before attaching credentials. Follow cursor
links unchanged. Respect include depth/resource limits, preserve track order
and playlist duplicates, and avoid caching a partial response as complete.
Retry reads conservatively for 429/transient failures. Do not blindly retry a
playlist create or append after an ambiguous response, since it may duplicate data.

## Favorites and playlists

Tidal collection membership is the positive preference for tracks, albums,
artists, and playlists. Display one heart/action labeled Favorite or Add to My
Collection, with Remove from My Collection as the inverse. Offer it on rows,
details, and the current track. Hide separate library-add and love controls when
they would perform the same remote operation. Negative ratings are unavailable
in this scope.

Add a small shared preference descriptor to express resource kinds, label,
collection equivalence, and whether removal is supported. Add explicit desired
state to the provider and socket mutation (`saved = true/false`). Preserve the
existing Apple favorite and rating behavior through the same generic presentation.
Do not model Tidal collection membership as an Apple recommendation rating.

Read membership before presenting a toggle as known. Unknown and false differ.
Use `(kind, id)` as the lookup key. Apply confirmed results to cached rows and
invalidate the relevant collection list; preserve state on a failed mutation.

Playlists combines owned and favorited playlists, deduplicated by resource ID.
Ownership and favorite state remain independent. Determine editability from
Tidal ownership/permissions, not from presence in the user's collection.
The playlist picker includes confirmed editable playlists only.

Create playlists using Tidal's normal personal, non-public default; verify its
API accessType mapping before choosing a value. Require a name and keep the
description optional. Creating a playlist and appending tracks are separate
operations: if creation succeeds but appending fails, retain/report the new
playlist and let the user retry the append. Preserve duplicate tracks and order.

## Device login and saved sessions

Reuse the working device flow and compatibility client from Untidy/prototype.
Do not reopen the app-registration/PKCE decision as a user prerequisite.
Keep its credential source identifiable for maintenance; encoding credentials
does not make them secret. Document the compatibility dependency for distribution.

The daemon owns device authorization and polling. Send only verification URL,
public user code, expiry, and progress over IPC. Honor server polling interval
and slow-down responses. Cancel on user cancellation, provider change, shutdown,
denial, or expiry. A cancelled/obsolete flow cannot save credentials afterward.

Store token/refresh token, granted scopes, expiry, Tidal user ID, and country in
the private Tidal credential record. Retain Apple's existing fields and saved
session. Redact private credentials from Debug, diagnostics, fixtures and IPC.
Use Tidal's verified user ID to derive a stable cache key across token refreshes.

Serialize refresh per saved account. REST and SDK access share the same refresh
coordinator. On 401, retry once only if the rejected token is still current.
Persist rotated tokens atomically to their owning record. Refresh must not
overwrite selection or another provider's credentials.

## Playback

Port the official Tidal Web SDK adapter from the prototype into the provider's
playback page, with locked versions and bundled license notices. Keep the shared
`brumm` controls/state contract: queue, index, position, play/pause, seek, next,
previous, volume, shuffle, repeat and errors. The provider translates SDK events.

Brumm owns one local queue. Do not introduce a competing remote playQueues
authority. Preserve duplicate entries, play-next/append semantics and exact
current position during GUI/TUI handoff. SDK callbacks name their session and
queue generation so late events cannot mutate a new queue or provider.

The current `PlaybackPage` contains HTML and a refresh script; its server serves
only the page. Extend the shared engine with a private provider-owned request
handler for SDK credentials and metadata if the adapter requires that bridge.
The handler captures a specific session and supports finite operations; it must
not forward arbitrary URLs or select whichever provider is active on each call.
Keep loopback Host/path validation, bound request sizes, no-store responses and
cross-origin rejection. Private tokens never travel over the frontend socket.

Verify actual full-track presentation and duration, not just a successful load or
PLAYING event. A preview-only response must be identified as a preview and must
not silently advance the queue as if the full track played. Report the actual
quality/format separately from requested quality. Begin with a conservative
supported quality; advertise higher quality only after runtime verification.

Playback/catalog access can depend on account, country and client entitlement.
Report unavailable content, region restrictions, preview limits, revoked streaming
privileges and expired sessions distinctly. Reuse the SDK playback path; do not
extract streams as an alternative backend. Public playback/distribution terms
remain relevant to release readiness; successful compatibility login is technical
evidence, not a claim of Tidal approval.

## Shared work required before enabling Tidal

1. Implement serialized provider transitions and a session generation carried
   by provider-bound requests, replies, events and background tasks. Cancel old
   work and reject stale mutations before invoking an API. A reply carries the
   generation it started under, not the generation current when it finishes.
2. Capture provider/account storage paths in the active session. Current cache
   helpers reload the selected Config, which can redirect an old task's write.
3. Restrict legacy Apple cache migration to Apple. The current migration helper
   could copy legacy library/resume data into the first Tidal namespace.
4. Scope navigation and resume to provider/account; clear outgoing pages,
   artwork, selection, pending mutations and playback state during transitions.
5. Add the generic preference descriptor/desired-state operation and private
   playback transport. Keep Apple implemented against each shared change.
6. Register Tidal as available only after authentication, core library/catalog
   reads and playback initialization pass the required evidence checks.

Temporary Tidal metadata/art caches need an expiry and refresh policy; do not
inherit indefinite Apple cache lifetime. Resume state stores only what is needed
for restoration. Provider-specific retention must be documented before release.

## Implementation and validation sequence

1. Shared session transitions, credential records and storage ownership, tested
   with fictional providers and delayed reads/writes/auth callbacks.
2. Device login, refresh and Tidal Definition, with a fictional device flow for
   deterministic GUI/TUI checks and one live login for service validation.
3. Catalog and collection mapping. Reuse prototype fixtures after checking their
   assumptions against current responses. Verify owned/favorited playlist union.
4. Read-only Home/Explore endpoint probes. Record tier, request parameters,
   response/error, available sections and native ordering, without personal data
   in the repository. Implement accessible sections and useful unavailable states.
5. Unified preference actions and playlist create/append. Fixture checks cover
   inverse state, unauthorized ownership, duplicates and creation/append failure.
6. SDK playback and account-bound private bridge. Check full presentation, queue
   controls, restore, quality reporting and session cancellation. Run live audio
   checks only in a user-authorized real audio session; omabox has no audio output.
7. Apple → Tidal → Apple regression checks, including switching during login,
   API requests, mutations, playback loads and delayed callbacks. Run Qt/TUI
   fixtures and desktop checks in omabox. Apple paid playback stays unverified
   until an eligible account is available.

Live playlist/favorite writes use explicitly authorized disposable resources.
Local fixture tests can run freely. The prior prototype established that device
login, albums and playlists returned usable results; Home returned 400 and
audible/full-track playback was not confirmed. Those observations are starting
evidence rather than completed validation for this new provider.

## Sources and related documents

- [Shared abstraction roadmap](provider-abstraction-plan.md)
- [Implemented provider guide](providers.md)
- [Tidal API reference](https://tidal-music.github.io/tidal-api-reference/)
- [Inspected OpenAPI schema](https://tidal-music.github.io/tidal-api-reference/tidal-api-oas.json)
- [Tidal SDK auth design](https://github.com/tidal-music/tidal-sdk/blob/main/Auth.md)
- [Tidal Web SDK](https://github.com/tidal-music/tidal-sdk-web)
- [My New Arrivals](https://support.tidal.com/hc/en-us/articles/29710779228817-My-New-Arrivals)
- [Daily Discovery](https://support.tidal.com/hc/en-us/articles/23351150845329-Daily-Discovery)
- [My Mix](https://support.tidal.com/hc/en-us/articles/360000702697-My-Mix)
- [Your History](https://support.tidal.com/hc/en-us/articles/360009257397-Your-History)
- [Developer Terms](https://developer.tidal.com/documentation/guidelines/guidelines-developer-terms)
