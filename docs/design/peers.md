# Studio engines as peers

Every Studio engine signed in to one FastTrackStudio account is a peer of
the others: a desk playing a set's tracks, an iPad running the guitar rig,
one doing both, one that changes role between songs. None leads by design.
`features/peers` (`studio-peers`) is the whole of it; Architect supplies the
pieces (studio#285 phase 2–3 and studio#370, done on Architect's devices).

## Who is let in

Architect's devices (`architect-devices`):

- **The account pass** (`architect_devices::account::pass`, run by
  `Mesh::keep_on_account` against `auth.fasttrackstudio.app`, or
  `FTS_AUTH_ISSUER`) puts the engine in the issuer's address book at its
  iroh endpoint and admits the account's other devices (`Admission`).
- **The engine's own row.** Its device id is the install's and Studio's
  together (`mesh::this_engine`), so it never fights an Architect node on
  the same machine for the one endpoint a row holds.
- **Finding them**: the endpoint is bound with `PeerLookup::N0AndLan`
  (n0 relays and DNS, and mDNS `_fts._udp` on the LAN), so an admitted
  endpoint id is dialled bare. Nobody types an address.
- **The gate**: every connection gets a router behind a `PermissionsGate`
  whose engine admits the account's devices only, and is closed when a
  device leaves the account (`architect_devices::serve`).

## What each runs and plays

`StudioPeer` (the wire every peer serves): `about()` (name, platform,
`Roles { tracks, rigs }`, whom it follows) and `now()` / `playing()`
(`NowPlaying`: the set, the song up, the section, the rig cue, the
transport). The rig engine publishes the guitar rig's set and the engine's
transport; a Session window in another process publishes its set through
the engine's `PeersControl::publish`, and the engine then runs `tracks`.

## Following

Any peer may follow any other (`Mesh::follow`; `studio peers follow`), and
one nobody drives (an iPad) follows the one peer that plays tracks by itself
(`Node::auto_follow`) until told otherwise. The rules are pure
(`follow::Follow::step`): the song by title (case and punctuation aside,
a short title inside a long one), switching to another of the rig's sets
when the one up lacks it (an archived song is there, out of the order); the
section by name; the **rig cue** at the playhead (a patch, part or preset
the set names); the tempo; play, stop and the position when it drifts past
a quarter second.

**Rig cues** come from the show: `studio peers cues <set.als> --write`
reads a Live set's cue track (`GP GTR +CLICK`: clips named for guitar
patches and scenes) per song (its locators), in seconds into each song, and
keeps them in the guitar rig's book (`cues.styx`), so they sync like the rest.

## The rig books

Each rig library is a `crdt::docs` document (`Lineage::Merge`) with one id
per account (the account's own space, no server org) on every device, kept
by a driver on the device. Peers sync them directly: each serves its
driver's host (`DocsDriver::host`) on the gated router and runs
`DocsDriver::run_peer` against every admitted peer. There is no hub; an
Architect node can be one more peer later. A fresh book waits for a peer's
library before it takes the shipped defaults, ten seconds at most.

## Tests

`features/peers/tests/mesh.rs`, two engines of one account over a real
issuer (in-memory SQLite) and loopback iroh: admission by the account alone;
a stranger refused; following the desk's tracks through the recital, then
the other way after the roles swap; a book edited on either side arriving on
the other with the issuer gone; an iPad following the peer that plays tracks
by itself. `follow::tests`: the rules, the recital's titles and cues.

## Not yet

- The desk's Session window publishes song and transport but no section
  or set name; the follower takes cues from the rig's own song cues.
- No in-app picker of whom to follow (the CLI, and auto-follow).
- The iOS multicast entitlement is not requested, so an iPad finds peers
  through n0 (internet) rather than mDNS.
- Architect: one endpoint per device row (worked around by Studio's own
  row); no shared "engine as playhead clock" yet (`architect.playhead`'s
  `Clock` "engine" is designed, not built), so the follow lane is Studio's.
