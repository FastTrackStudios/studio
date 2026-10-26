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
| `setup`   | audio device and MIDI control presets, each device's pick | the library store    |
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
- **A rig's library** is a *book* (`signal_rig_host::book`): one Loro doc
  in `<library dir>.session` beside the styx directory it replaces —
  presets, patches, profiles, songs, setlists, setup, each old file an
  entry under its name, laid out by the codec so devices merge field by
  field. The rig's code did not change: a book registers for its
  directory, and the typed reads and writes every rig already made
  (`StyxDir`, the guitar's `config_watch`) go to it by path; what another
  device changes reaches the rig's hot reload by name. The styx files
  move in lazily as they are read and stay as a backup. What is this
  device's alone stays a file: `last-state.styx`, a perform rig's
  `cursor.styx`, the library's bookkeeping. The engine turns books on
  (`books::enable`, not for design or ephemeral runs); signed in, a fresh
  book waits for the account's library before seeding the shipped
  defaults into it.
- **Assets are files, never doc content.** NAM captures, IRs, practice
  audio, chart PDFs move by file sync (the blobs layer); a doc holds how
  they are used (`models/Fender Clean.nam` in a preset) and, in the
  book's asset index, what each one is by content: BLAKE3 hash and size.
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
- **Media is never in a doc.** A doc holds a sha256; the bytes (chart
  PDFs, practice audio, NAM captures, IRs, plugin states) move through
  architect's blobs layer (see below).
- **The device driver** (architect too): pending creates pushed parent
  first, the catalog pulled from the last revision seen, a `SyncedDoc` per
  doc, each doc on its own `Persistence` (a project's file, or the
  library's database), and what arrives persisted before it counts.

## Media: architect's blobs layer

Every offline-first, multiplayer app on architect needs content-addressed
media beside its docs, so it lives in architect, not in Task:

- **A content-addressed store** (sha256 in, bytes out): the filesystem on a
  device, IndexedDB in the browser, disk or object storage on a server. A
  project's `.session` store keeps its own blobs (`Store::put_blob`).
- **Transfer by hash** over vox and peer to peer over iroh, chunked,
  resumable, and verified against the hash before anything is kept.
- **References from docs:** each kind says which hashes a doc references
  (the app knows its schema; architect does not). The driver uploads what
  the owner's space lacks and fetches what is missing when it is opened.
- **Pinning for offline:** pin a setlist, and everything it references,
  through its songs and parts to the rigs' captures, is on the device
  before it leaves the house. The rest is fetched on demand and evictable.
- **Permissions follow the docs** (who can read a doc can fetch what it
  references), and content no live doc references is collected.

Task keeps the product on top: folders and roots, versions, renditions
(proxies, waveforms), review, share links, the OS integrations.

## The mount: Task hosts it

The code is architect's (the engine, each platform's mount); **Task is the
deployment** that runs the device's one sync engine and registers the one
mount, with a folder per app space (Studio, Task, …):

- **macOS:** Task's files daemon (a LaunchAgent) runs the engine; Task's
  File Provider registers one domain.
- **iOS:** the Task app embeds the File Provider and the engine.
- **Linux:** Task's daemon as a systemd user unit, mounted through FUSE.
- **Windows** (the Cloud Files API) and **Android** (a DocumentsProvider)
  later; WebDAV everywhere as the fallback.

Studio hands its spaces to Task when Task is installed (and offers "Get
Task" when it is not), through the shared App Group
`group.app.fasttrackstudio.task`. Or it embeds the same code and registers
a mount of its own. Either way it talks the server's own wire (Docs,
DocSync, DocPresence, Blobs) to the engine, on a local socket or in
process, and keeps the docs it is editing in its own memory. On iOS the
foreground app holds the engine and never waits for it; nothing of it
runs on the audio thread.

## Projects

A project syncs the way a rig's book does, with three differences: it
belongs to an org that is often not the person's own, it is open in the
engine while it syncs, and its media is far bigger than its doc.

- **One driver per org.** The device runs a `DocsDriver` for every org
  that owns something on it, each with its own index
  (`<config>/sync/<org>.index`), all on the one kept session. Crescendum
  is `fasttrackaudio`'s, so it syncs there, not in the personal org.
- **A branch is a doc of kind `session`.** Its id is the store's
  `doc:<branch>` (`Store::doc_id`); `main` is made `Strict`, and an
  alternative is a doc whose `parent` is the doc it was forked from.
  Registering a project is `driver.create` for each branch, with
  [`Replica`] over the project's own file as its persistence. Nothing is
  copied: the file stays the one place the project lives.
- **One doc in memory.** While a synced project is open, the bridge edits
  the driver's `CrdtDoc` (as a book edits its doc), so a remote update and
  a local step land in the same `LoroDoc`. The `CrdtDoc` persists every
  local commit itself, so the project file stops appending its own steps
  for that branch; the step's name still reaches the history (the replica
  is told the label before the commit). A remote update goes through the
  bridge's diff to the engine, as a live collaborator's does.
- **Opening one made elsewhere.** The org's catalog, listed by kind
  `session`, is the device's list of projects. Opening one creates its file
  under the sync directory (or where the person puts it), adopts the doc
  ids, and waits until it has caught up before the engine reads it.
- **Media, two ways.** The doc names its media relative to the song
  folder and records the File Root that holds it (`media.root`, an opaque
  id Task can resolve). *Local:* what the doc references and the device
  lacks is fetched from that root into the song folder before it plays
  (`architect::blob::ResumableFetch`, sha256-verified, resumable). *Streamed:*
  the project opens with nothing local and every take streams from the root
  (`song_stream`'s Task source, as the web demo does), proxies first.

### Task stays app-agnostic

Task never learns what a session is. A project names the app docs that
belong to it the same way it names anything: a link from the project to
an app doc (`NodeKind::AppDoc`, the doc's id), with the catalog's kind and
name to show. Attaching, listing and detaching are generic — a Keyflow
chart set or a Signal rig attaches the same way — and a session link is
built from what the link already holds: the org, the doc id, the kind.

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

Nothing that is content. The demo is a project like any other: its songs
are `session` docs in FastTrackStudio's org, attached to a Demo project,
their media in that project's File Root, and the app opens it the way it
opens anyone's — by link, streamed, with no account needed to play it (a
public share). It needs no code of its own and no copy in the binary;
"open the demo" is a link the home screen carries. Everything real is in
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
7. Projects (see Projects): a driver per org; a project's branches as
   `session` docs, the open song editing the driver's doc; the org's
   projects listed and opened on another device; media fetched or
   streamed from the project's File Root; app docs attached to Task
   projects by link. Unbreakable first.
