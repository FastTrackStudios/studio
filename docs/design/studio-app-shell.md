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

## A new empty session

Sessions can open a brand new session with nothing in it: an empty
project, entirely local, to work in as in any DAW.

## Related

- `docs/design/engine-boundary.md`: every window is a client of its
  engine. A phone is usually a remote control of an engine elsewhere, and
  can run one itself.
- The Charts mode's editor is keyflow's, exported as a component (the
  keyflow `editor-keyflow` `view` feature).
