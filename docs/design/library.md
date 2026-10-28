# The library: songs, setlists, albums, projects, and the rigs that play them

Status: draft for review (2026-10-26). Builds on `docs/design/sync.md` and
ADR 0002, and revises two of its rows (see "What changes in ADR 0002").

> **Superseded for the library's model (2026-10-28):** the library lives in
> Task projects, not in Loro typed documents of Studio's own. A *session*
> is what a person sees; underneath it is a Task project with the
> `music-production` capability, nested by its parent, tagged with the
> project's tags. See `docs/design/studio-app-shell.md` ("Decided"). What
> this document says about media, rigs and sync still stands where it does
> not describe how library items are stored.

Studio's iPad home becomes one library: everything a person or a band has
(songs, setlists, albums, projects, rigs), organized and opened from one
place, live-editable, synced through Task, and readable inside Task without
Task knowing what a song is.

## The boundary with Task

**Task knows nothing about Studio.** It has no `Song`, `Setlist` or
`Album` type. It offers generic primitives, and Studio operates inside them:

1. **Typed docs.** Every library item is a Loro doc in the doc catalog
   (`crdt::docs`), with an opaque `kind` (`studio.song`, `studio.setlist`,
   …). Task stores, relays and merges it like any other doc.
2. **A readable face, by convention.** Every doc keeps a top-level `face`
   map: a title, typed properties (Task's `PropertyType` vocabulary: text,
   number, date, enum, link, link list), and links to other docs by id.
   Task reads only `face`. That is enough to list, search, filter, render a
   property sheet, draw the link graph, and show a diff of the history in
   words ("key A → B", "added *Reckless Love*") instead of raw CRDT ops.
   Everything else in the doc (a chart's source, an arrangement's form, a
   part's patch map) is the app's own and Task does not read it.
3. **The face describes itself.** Each property carries its label and
   its type, so Task renders any kind with no schema registered and no
   Task change, and another app (lighting, video) works the same way.
   The shape, in the doc's root:

   ```text
   face: Map
     title:    String
     subtitle: String            (optional: "Harbor & Pine · 2026")
     colour:   String            (optional: "#22c55e")
     props:    List of Map { key, label, type, value }
               type ∈ text | number | duration | date | enum | tags | bool
               value: String, f64, i64, bool, or a List of String (tags)
     links:    List of Map { rel, label, doc }
               rel: the app's word ("plays", "arrangement-of", "part-of")
               doc: "doc:<uuid>", a catalogued doc in this or another space
   ```

   The app keeps the face current as it edits (one transaction with the
   change it describes). Task reads only this map.
4. **History.** Loro keeps every change. Task shows it per doc from the
   `face` diff, and lets a person name a version ("Sunday 10/26 as
   played") and restore it, both generic.
5. **Comments and activity**, anchored on any doc by `(kind, id)` plus an
   optional anchor path inside it (`entries/<entry id>`,
   `arrangements/<id>/form/3`). Threads, mentions, resolve, and the
   activity feed are Task's, and know nothing of what they anchor to.
6. **Sharing** follows the doc's space: a band's org, or a person's own.
   A doc may link to a doc in another space; the link resolves if the
   reader may read the target (ADR 0002's dangling-reference rules).

What Task has already (`apps/server/src/app_docs.rs`): the catalog per
org (`DocsHost` in `<org>/app_docs.sqlite`, the kind opaque), sync over
the org's one lane, tombstones, members-only access, and a project
naming its docs with a link (`project:<path>` → `doc:<uuid>`). What it
lacks, all generic: reading a doc's `face` (with the kind's schema) to
list, search and render it; comment threads anchored on `doc:<uuid>`
plus a path inside it (threads anchor on tasks and projects today); and
a view of an org's app docs in its UI.

## Projects are Task's

A **project** is a Task project: the container for one undertaking, with
its tasks, people, dates, status and comments, all of which Task already
has. *JT's Senior Recital* is a project; so is the *Paper Boats* album.
Studio attaches its docs to a project with Task's existing links
(`project:<path>` → `doc:<uuid>`):

- its **setlists** (the recital's program, each night of a run),
- its **sessions** (`.session` stores: rehearsal recordings, tracking,
  mixes; what earlier drafts called a project),
- for a record, its **album** (the tracklist).

Songs are not inside a project: they are the library's, and a recital's
setlist and an album's tracklist point at the same songs. Rigs and parts
stay with their player. In Task, a project's page lists what is attached
to it, and an org-wide view lists every setlist (or every session)
across projects; in Studio, the sidebar's Projects are Task projects.

## The model

```
Song ───────────────┬─ Arrangement (Full, Short live, Orchestral, …)
 │ assets (by hash)  │   key · tempo · form · chart · audio edits
 │ sections          │   └─ Project (.session) optional, per arrangement
 │                   │
Setlist (live) ── Entry ── a Piece: an Arrangement of a Song, or another
 │ date · venue      │      kind of piece (transition, medley, talk, pad)
 │                   └─ per-show key / tempo / notes
 └─ Parts: one per rig per player ── per-entry sound (patch, overrides)

Album (studio) ── Track ── an Arrangement + the Project that releases it

Rig ── profiles · patches · song presets keyed by Song (and Arrangement)
```

### Song: the hub

One doc per song (`studio.song`): title, writers, CCLI and the other
catalog facts in `face`, plus:

- **Assets**, each by sha256: stems, mixes, click, MIDI, lyrics, the
  Keyflow chart source, PDFs. Stored once, used by every arrangement.
- **Sections**, the song's material (Verse 1, Chorus, Bridge, …), each
  with its chart bars and lyrics.
- **Arrangements**, inside the same doc because they reference the same
  sections and assets densely and change together (ADR 0002 rule 1):
  name, key, tempo, meter, a **form** (the order of sections, with
  repeats, cuts and vamps), and an **audio edit list** (regions of the
  shared stems in form order). A short live version is a different form
  over the same sections and the same audio; an orchestral one adds its
  own assets. Nothing is duplicated.
- An arrangement may point at a **Project** (a `.session`) where it is
  produced. A song may have several projects (demo, tracking, mix).

The chart a band reads is the song's Keyflow source rendered through an
arrangement's form, key and tempo, so a transposed or shortened
arrangement needs no second chart.

### Setlist: what a show plays

One doc per setlist (`studio.setlist`), in the band's space: date, venue,
and ordered **entries**, each with a stable id. An entry is a **piece**:

- an arrangement of a song, with per-show key, tempo and notes; or
- another kind of piece: a transition, a medley (several arrangements
  played through), spoken word, a pad, a click-only count-in. Not every
  thing on a setlist is a song.

Reordering moves entries, and everything keyed by entry id moves with them.

### Parts: a rig attached to a setlist

Attaching a rig to a setlist makes a **part** (`studio.part`), in the
player's own space: `(setlist, rig, player)` → per-entry sound (a patch or
a profile of that rig, plus overrides), keyed by entry id. A setlist can
have any number of parts (my guitar, my keys, the bassist's bass). Each
syncs on its own, so the band sees the setlist and charts while each
player's sounds stay theirs. Opening a part opens that rig on that
setlist, with the song up.

When a part has nothing for an entry, the sound falls back through the
rig's **song preset** for that song and arrangement, then the profile's
default patch, then the library default, and the view says which it used
(ADR 0002's resolution order).

### Song presets live in the rig

A rig doc keeps, keyed by song id (and optionally arrangement id), the
sound that song wants on that rig. It follows the song into every setlist
that plays it, and a part only records what differs for one show.

### Album and Project

An album (`studio.album`) is the studio's ordered list: tracks, each an
arrangement plus the project that releases it, with release facts
(title, artist, date, ISRC per track) in `face`. A project is a
`.session` store, registered in the catalog as `studio.project`, linked
from its arrangement.

## What changes in ADR 0002

- The `song` doc gains arrangements (a song's versions are one doc).
- A setlist entry names a **piece** (an arrangement or a non-song piece),
  not a song.
- New kinds: `album`, `project` (a `.session` in the catalog).
- Every kind carries the `face` map, so Task can read it.
- Kind names are namespaced (`studio.song`) so they cannot collide with
  another app's.

## The screens (iPad first)

The library is the shell (`docs/design/unified-shell/spec.md`) with
library panels, so the same regions, rails and size classes apply.

- **Library home.** Left: Songs, Setlists, Albums, Projects, Rigs, then the
  spaces (personal, each band). Main: the list for the selection, with
  sync and offline state per row. Right: the selected item's inspector
  (its face, links, comments). Top: what is on next (tonight's set) and
  where you were.
- **Song.** Arrangements as tabs; the chart for the selected arrangement
  in main; right: sections and form, assets, the song presets of each of
  your rigs, where it is used (setlists, albums, projects), comments.
- **Setlist.** Entries in order with key, tempo, length and readiness
  (chart, audio, your sounds). Attached parts as chips across the top
  ("Guitar · Cody", "Keys · Sam", "+ Attach rig"). Play opens Signal on a
  part; Rehearse opens the setlist in Session; Share sends charts.
- **Album.** Tracks with each one's project, status and versions.
- **Comments** everywhere as a right-hand thread on the selected thing,
  anchored to an entry, a section or a bar.

## Build order

1. The screens on a `LibrarySource` trait, fed by a fixture, so the UI
   is reviewed on the iPad before storage is decided in code.
2. The doc kinds with `face`, synced through the existing `DocsDriver`
   (only `rig` today); songs and setlists import from the gig folders and
   the rig books.
3. Parts and song presets, wired to the rigs.
4. Task: catalog-registered docs, `face` schemas, generic comment anchors
   (a Task change, its own issue).
5. Albums and projects in the catalog.
