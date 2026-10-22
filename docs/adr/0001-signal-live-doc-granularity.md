# ADR 0001: Signal's live docs are one Loro doc per entity

- Status: proposed (live S4, STA-87). The Captain's spec left this open as Q3.
- Date: 2026-10-22
- Code: `features/signal/proto/src/doc/` (codec, layouts, seed library),
  `features/signal/proto/tests/doc_granularity.rs` (the measurement),
  `features/signal/proto/tests/doc_roundtrip.rs` (the round-trip properties).

## Question

Signal's library (rigs, patches, profiles, songs, setlists and the default
profile pointer) goes into Loro docs that sync between the Captain's devices
and agents (live S5/S7, over fts-auth account device sync). How should it be
cut into docs?

1. **Per entity**: one doc per rig, patch, profile, song and setlist, plus
   `library:settings`, each named by its key (`patch:<slug>`, `song:<id>`, …).
2. **Whole library**: one doc holding everything.
3. **Per setlist**: one doc per setlist holding the setlist and everything it
   plays, plus `library:rest` for whatever no setlist plays.

## Decision

**One doc per entity**, named by the entity's key. A setlist references its
patches as `patch:<slug>` (the identity in spec §11, which Task's setlist link
keeps in live S8). A profile lists its patches by key, and the default profile
is the doc `library:settings`. No entity ever lives in two docs.

## Evidence

Run with
`cargo test -p signal-proto --features doc --test doc_granularity -- --nocapture`.
The library is the demo one (`doc::seed`): Task's dev-seed songs and setlists
on Signal's guitar and keys megarigs. That is 2 rigs, 28 patches, 5 profiles,
13 songs (4 scenes inherit All-Around), 2 setlists and `library:settings`:
51 entities, 38.1 KiB as JSON. Timings are medians from the airlock Mac mini
(M-series), dev profile.

### Size

| layout | docs | entity copies (unique) | snapshot total | largest doc | median doc |
|---|---:|---:|---:|---:|---:|
| per-entity | 51 | 51 (51) | 90.5 KiB | 3281 B | 1404 B |
| whole-library | 1 | 51 (51) | 59.8 KiB | 59.8 KiB | 59.8 KiB |
| per-setlist | 3 | 56 (51) | 65.0 KiB | 32.2 KiB | 24.4 KiB |

### Typical edits: update bytes (docs touched)

| edit | per-entity | whole-library | per-setlist |
|---|---:|---:|---:|
| Captain tweaks a patch (amp gain) | 239 B (1) | 242 B (1) | 242 B (1) |
| rename a patch | 98 B (1) | 101 B (1) | 101 B (1) |
| agent reorders a setlist | 81 B (1) | 85 B (1) | 85 B (1) |
| agent adds a song to a setlist | 331 B (1) | 335 B (1) | 2127 B (2) |
| change a song scene's patch | 125 B (1) | 128 B (1) | 128 B (1) |
| new patch (added to All-Around) | 744 B (2) | 635 B (1) | 911 B (3) |
| Captain edits All-Around's default patch | 239 B (1) | 242 B (1) | 484 B (2) |
| switch the library's default profile to Worship | 135 B (1) | 138 B (1) | 3712 B (3) |

### Sync and merge cost

| layout | cold join, whole library: bytes · docs · import+read | cold join, one gig (Sunday Setlist): bytes · docs · import+read | merge one patch tweak |
|---|---|---|---|
| per-entity | 90.6 KiB · 51 · 1965 µs | 36.6 KiB · 20 · 791 µs | 249 B in 1 doc (10 µs) |
| whole-library | 59.8 KiB · 1 · 960 µs | 59.8 KiB · 1 · 915 µs | 252 B in 1 doc (15 µs) |
| per-setlist | 64.4 KiB · 3 · 1338 µs | 56.0 KiB · 2 · 1156 µs | 252 B in 1 doc (13 µs) |

### Concurrent edits

The agent adds "Open Doors" to the Album Release Show and opens it with
"Brighter Still". At the same time the Captain turns up
`patch:worship-tremolo` (which Open Doors plays) and renames
`patch:worship-lead`. Then the two exchange updates.

| layout | agent's edit lands | Captain's tweak lands | replicas converge | entities whose copies disagree |
|---|---|---|---|---|
| per-entity | yes | yes | yes | none |
| whole-library | yes | yes | yes | none |
| per-setlist | yes | **no**: 1 of 2 copies of worship-tremolo kept the old value | yes | patch:worship-tremolo |

## Why per entity

- **Per setlist is ruled out by correctness, not cost.** A patch two setlists
  play has two copies, and nothing reconciles copies across docs. In the run
  above, the agent's write into the release show's doc copied the tremolo
  patch from its own older view, and the Captain's tweak was lost there for
  good, although both replicas converge. Its edits also fan out: adding a
  song costs 6× the bytes, and switching the default profile 27× the bytes
  across 3 docs.
- **Per entity and whole library are equally correct.** Every edit and merge
  costs the same within a few bytes, and the concurrent case lands in both.
  The difference is what a device has to hold:
  - A gig device needs only what its set plays. Per entity, that is 20 docs
    and 36.6 KiB, which is 61% of the library. A whole-library doc is all or
    nothing, and a real library (hundreds of patches and songs, years of
    history) is much bigger than this demo; its history only ever grows.
  - Identity matches Task. Task's setlist (live S8), agents (`doc_read` /
    `doc_apply`, live S3) and Signal all name `patch:<slug>`. A doc per key
    means a link is a doc name, and each entity has its own history, undo
    and (later) its own share/permission scope.
- **The cost of per entity is about 600 B of fixed overhead per doc** (90.5
  vs 59.8 KiB snapshot total over 51 docs) plus 51 docs to version and sync
  instead of 1. At these sizes that is ~1 ms more for a cold join of
  everything. The transport has to batch many small docs per session, and
  device sync should do so rather than run one round trip per doc.

## Consequences / follow-ups

- **A device needs the list of docs.** A cold join has to know which keys
  exist. S5 should add a small index (the keys and their kinds) beside
  `library:settings`, or have device sync list the account's docs. The codec
  does not need it.
- **A patch's overrides are one value.** When two people change different
  overrides of the same patch at once, one is lost (last writer wins; 4b in
  the test output). Different fields of a patch, such as a rename and an
  override, both land. If that matters live, a later schema can key
  overrides by path, as setlist entries are keyed by id. The codec already
  does that for any list whose items have ids.
- A patch without a slug (saved before slugs existed) is keyed
  `patch:<id>`. The SQLite repo gives a legacy row the slug its name makes
  when it loads.
- A patch's slug is unique across the library, not only within its profile.
  The seed qualifies slugs with the profile (`worship-clean`). Signal's
  create path should use `slug::unique_slug` (S5).
