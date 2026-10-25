# Sync: one model, every device, offline first

Studio is one app over one model: Signal (the live rig) and Session (the
full DAW) are two views of the same documents. Those documents sync
between every device a person signs in on, merge without conflicts, keep
working with no network, and are saved on the device. This is how.

## Everything is a Loro doc

| doc kind  | what it is                                               | on a device          |
|-----------|----------------------------------------------------------|----------------------|
| `session` | a project, one doc per branch                            | a `.session` file    |
| `rig`     | a rig: blocks, modules, presets, patches and profiles    | the library store    |
| `song`    | key, tempo, sections, chart and audio by hash            | the library store    |
| `setlist` | entries in order (with ids), per-set key, tempo, start   | the library store    |
| `part`    | one person's sound per entry of one setlist              | the library store    |

A rig on a track of a session and a rig loaded as the live guitar rig are
the same `rig` doc, opened by a different view: a preset chosen in Session
is the one the live rig plays. Nothing is copied between docs; everything
references by id, so a profile edit reaches every setlist that plays it.
Where the boundaries are, and why, is ADR 0002
(`docs/adr/0002-studio-doc-boundaries.md`).

A kind is an opaque string. Task never reads a doc; it only stores,
relays and merges Loro updates by version vector. A new kind needs no
server change.

## One file format: `.session`

There is one project file, `.session`, and no `.signal`. A second
extension would mean a second format and a second code path for what is
one model; the doc's kind says what it is, and Studio opens it in Signal
or Session by that. A live set is a `setlist` doc that references the
`session` docs it plays — it is not a file that embeds sessions.

We keep the name: `.session` is already the format's name (renamed once,
from `.daw`), and a session is what the thing is. `.studio` would name the
app, not the document.

## On a device

- **A project** is its `.session` store (`session-format/src/store`): the
  doc's updates, snapshots, branches and blobs in one Turso file. It is
  the device's replica: the store implements `crdt::Persistence`, so a
  save is an `append_update` and a fold is a `compact`. Each branch is
  its own doc; its id is kept in the store's `meta` (`doc:<branch>`).
- **The library** (setlists, rigs, patches, profiles) is one Turso
  database per device, through `crdt`'s own `turso` persistence.
- **The browser** keeps its replicas in IndexedDB (`crdt`'s `indexeddb`).

Edits go into the local replica first, always. Sync is a background loop
that never blocks an edit; offline, `SyncedDoc` buffers the updates and
catches up by version vector when a connection comes back.

## Through Task

A schema-agnostic doc space, `crdt::docs` in architect (beside `crdt`'s
persistence backends, its `DocRegistry` and its peer lane), so a device and
the server run the same catalog and store primitives. Task mounts one per
org; a device serves the same host to its peers over iroh. It gives:

- **Owner:** any org, or the person's own account. The same API for both.
- **Catalog:** create (with the doc id the client chose, so a doc made
  offline registers later, idempotently), list by kind, rename, delete
  (a tombstone, so an offline device learns of it), and a `#[subscribe]`
  stream of changes so a new device discovers what exists.
- **Lineage:** a doc may name its `parent` (a branch is a fork of its
  parent's history). Task carries it and does not interpret it.
- **Sync:** the doc ids route through Task's existing `DocSync`, persisted
  in the org's database (`crdt-seaorm`).
- **Permissions:** an org's members read and write its docs; the personal
  space is its owner's.
- **Blobs:** large immutable content (plugin states, MIDI, chart PDFs,
  practice audio) is referenced from a doc by sha256. On a device it is in
  the `.session` store's `blobs`; in Task, a Files root `studio/blobs`
  (`sha256/<ab>/<hex>`, written create-only), uploaded once.
- **The device driver** (architect too): pending creates pushed parent
  first, the catalog pulled from the last revision seen, a `SyncedDoc` per
  doc, each doc on its own `Persistence` (a project's file, or the
  library's database), and what arrives persisted before it counts.

## Peer to peer

Devices also exchange docs directly over iroh (`crdt::peer`, the lane
Task's own nodes use for vault docs). Task is the always-on peer, so a
sleeping iPad still gets its edits when it wakes, and the rendezvous where
a person's devices find each other (the device registry, #85). A CRDT does
not care which peer an update came from.

## Signing in

`fts-account` (device-code sign-in, one kept session for every FTS app on
the device) is what starts sync. Signed out, everything still works on the
device alone.

## What ships in the app

Only the demo: the demo setlist and what it plays. Everything real is in
the person's account and arrives by sync. Gigs stop riding inside the iOS
build (`GIG_BUNDLE`); a gig is imported into the account once.

## Replaces

- ADR 0001 (one doc per Signal entity): replaced by ADR 0002 (a rig is
  one doc; setlists, songs and parts point into it).
- The rigs' own `.styx` perform libraries (`perform-store`, the guitar
  rig's `home().save_setlists`) and `signal-storage`'s SQLite: one model,
  moved onto library docs.

## Order of work

1. architect: `crdt::docs` (catalog, stores, host, device driver), tagged;
   Task mounts it per org (the personal org included) with its permits.
2. `session-format`: the store as `crdt::Persistence`.
3. The sync driver: signed in, every local doc runs a `SyncedDoc` against
   its owner's Task, and the catalog stream opens docs other devices made.
4. The library: rigs, songs, setlists and parts as docs (ADR 0002); the
   rigs read and write them instead of their own files.
5. Imports: CAA 2026 and the rest of the real sets into the account; the
   app keeps the demo only.
6. Peer to peer and the device registry (#85).
