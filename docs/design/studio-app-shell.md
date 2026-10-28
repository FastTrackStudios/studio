# Studio's app shell

The one layout Studio has on every device. Phone, tablet and desktop are
sizes, not modes: a desktop window sized like a phone is laid out as a
phone is. Touch is an input style beside it (larger targets, no hover),
never what picks the layout. Modelled on Task's shell (its tab pill,
drawers and sheets), rebuilt for Blitz. Asked for 2026-10-28.

## The three modes

| Mode | What it is for | What it opens |
|---|---|---|
| **Sessions** | Control a session completely: the DAW | Every session in full — multitracks, mixer, editing; a new empty local session |
| **Signal** | The standalone live-rig experience | A session as its stereo preview, the click and the chart, nothing more; and the rigs — load a Guitar, Keys or Drums rig and play with it |
| **Charts** | A chart player and the chart editor | A session's chart, quickly; Keyflow charts made and edited here |

The three are views of the same things: a setlist built in one is the
same setlist in the others. Sessions is where a session is controlled
whole; Signal and Charts each stand alone.

## The layout

- **Bottom left: the three modes** — Sessions, Signal, Charts.
- **Beside them: global search and open** (where Task has its fleeting
  capture button): find any session, setlist, song, rig or chart and
  open it in the mode you are in.
- **A sidebar** (a drawer on a phone).
- **On a phone held sideways, the modes run down the side** (a rail, as
  Task's wide layout has), not along the bottom: sideways the height is
  what is short, and a bar along it takes the room the content needs.
  Upright they are along the bottom, as above.
- **Top right: the mode's important actions**, always there. One of them
  in every mode: join a remote session / Signal / chart.

## Sessions, not projects and setlists

The library has no separate Projects and Setlists. It has **Sessions**:
every music session, and sessions nest. A session has a kind:

- **LIVE-SONG** — one song as it is played live.
- **STUDIO** — a studio session.
- **LIVE-SETLIST** — either one session holding several songs (a show
  split into songs, kept tied to it), or a setlist that references other
  sessions (the songs).

Sessions are **tagged**, through Task's own tagging, so they are easy to
search: all of the HSM setlists, say.

**Recurring setlists are folders, not recents.** A setlist for a
recurring event or a tour date does not pile into the recents list: it
lives under a folder entry, nested as deep as it needs —
`Church → HSM → <the actual setlist session>`.

## Building a setlist

One flow, in all three modes: the setlist builder Signal already has,
combined with Session's and made to build setlists of sessions wherever
it is opened.

### A setlist's entries

A setlist is an ordered list of entries, each with an id of its own (its
place in the list, stable while it is reordered). An entry is one of:

- **a song** (`song:<slug>`): plays the song's default session;
- **a session** (a project): plays that session — one arrangement of a
  song, a medley, a session made for this show.

A session of a song is tied to the song (its project links `song:<slug>`).
So "is this song in the set already?" and "the song's key and tempo for
this set" look through a session entry to its song: one song in a set is
one song, whichever way it was added, and an edit made to the entry
applies to what it references. Changing an entry from a song to one of its
sessions keeps the entry's id, place and per-entry settings (key, tempo,
rig settings). Existing setlists of songs need no migration.

## A new empty session

Sessions can open a brand new session with nothing in it: an empty
project, entirely local, to work in as in any DAW.

## Decided (2026-10-28)

- **The library lives in Task projects.** A session is a Task project with
  the `music-production` capability; sessions nest by the project's
  parent; folders (Church → HSM) are plain parent projects; tags are the
  project's own tags, apart from folders. `docs/design/library.md`'s Loro
  typed-docs library gives way to this.
- **Live or studio is a Task capability**: `live-performance`, beside
  `music-production`. LIVE-SONG is a live song project; a LIVE-SETLIST is
  a `live-set` project holding a setlist collection, or a `concert`
  project whose children are the songs of one split show.
- **A setlist references both** songs (the `song:` items that stream
  today) and whole sessions (project items).
- **Rig data syncs completely.** The one setlist builder writes to Task,
  and everything a rig keeps about a setlist and its songs (start patch,
  profile, per-song key and tempo, the rigs themselves) syncs too —
  nothing a rig needs lives only on one device.
- **A new empty session** is saved to disk at once (Documents), and can
  be put into Task later.
- **Every view works upright and sideways on a phone.** Signal was built
  for a phone held sideways only; it is redesigned to work both ways, and
  the iPhone then takes every orientation
  (`apps/studio/desktop/ios/app-plist.sh`, which holds it sideways today).
- **A section card named after its song is that song's Intro** (a Live
  set split into songs).

## Related

- `docs/design/engine-boundary.md`: every window is a client of its
  engine. A phone is usually a remote control of an engine elsewhere, and
  can run one itself.
- The Charts mode's editor is keyflow's, exported as a component (the
  keyflow `editor-keyflow` `view` feature).
