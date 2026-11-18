# The arrangement, the mixer and the track panel as runtime extensions

Status: plan, 2026-11-17 (studio#510 step 5, architect#840). Written
before the code; the PR this lands in keeps it current. Paths are
`repo:path`.

## 0. In one page

- Three extensions, each a `kind = "process"` package installed into a
  stock Architect, none compiled into it:
  - **`daw`**: the engine. It holds the open session, plays it with the
    `daw` engine (`daw:crates/daw/standalone`), and provides two
    services: `fts.session` (the session primitive's, unchanged) and
    `fts.daw` (the DAW's own: tracks, items, peaks, transport, mixer
    edits). It draws nothing.
  - **`studio.arrangement`**: the arrangement view.
  - **`studio.mixer`**: the mixer, and the track control panel as a
    second view of it.
- The arrangement and the mixer each `[uses]` `fts.session` and
  `fts.daw`. Neither names the other, so either works with the other
  not installed, and either can be replaced by anything that reads the
  same services. `[uses]`, not `[requires]`: with no engine the
  arrangement still draws a session's lanes and items, read through
  `fts.session` alone.
- What moves every frame never crosses from a view: the playhead is a
  host animation on the app's playhead clock, meters are engine values
  read at compose, faders are host drags. Through Architect's
  compositor (`architect:features/scene`), a frame of playback costs a
  view 0 bytes.
- **One Architect change**: a value table in shared memory, so an engine
  in its own process writes `engine.*` values the host reads lock-free,
  and the host writes the `host.*` values a fader drag makes into the
  same table, where the engine reads them while the drag is held.

## 1. What exists

### 1.1 In Studio

The arrangement, track control panel (TCP) and mixer are compiled into
the Session window, `apps/session/ui` (`session-ui`), and mounted by
Studio (`apps/studio/desktop`):

| piece | where | drawn with |
|---|---|---|
| arrangement | `widget.rs` (`ArrangementWidget`, a Blitz `Widget`), `arrangement.rs` + `arrangement/record.rs` (the recorded scene), `arrange_camera.rs`, `ruler/*` (bars, tempo, chords, lanes), `lane_stack.rs` | one `anyrender::Scene` recorded in content space (seconds across, rows down), replayed under an `Affine` for scroll and zoom; Vello through Blitz |
| TCP | `tcp.rs`, recorded into the same scene so it cannot tear from the lanes; geometry from `daw_ui::art::geometry::tcp` (REAPER-measured) | the same scene; text through `text.rs` (skrifa, DejaVu Sans, no shaper) |
| mixer | `mixer_panel.rs` (`DawPanels`, its own `Widget`), `mcp.rs` (strip chrome, hit test, drags), `overlay.rs` (values drawn every frame over the chrome), `strip.rs` | recorded chrome + an overlay per frame |

What they read and how they talk to the engine:
- **State**: `daw_proto::Track`, `daw_ui::studio::project::Project`
  (`ProjectRef`, `RowsRef`), `session_engine` (`engine::Edit`,
  `song_model::StudioSession`), `timeline::rows::RowLayout`. All
  in-process Rust types.
- **Engine**: `session_engine` in the same process, or `studio engine`
  over vox (`FTS_ENGINE=spawn|attach`, `session_engine::studio_engine::link`).
  Edits are applied locally first and handed to the engine (`Edit`
  queues), the mixer and arrangement echo each other's edits
  (`mixer_panel::Links`, `widget::MixerLinks`).
- **Measured there**: the painted arrangement scrolls in 1.06 ms and zooms
  in 0.71 ms at 5120×1440 on the golden session (`widget.rs` docs).

The design that made it fast (content-space scene, a transform for scroll
and zoom, re-record on data change only) is the compositor's model, so it
carries over. The code does not: it is `daw_ui` + `daw_proto` +
`session_engine` + Blitz widgets in one process, and the coupling between
the mixer and the arrangement (`Links`) is exactly what replaceable
extensions must not have. The extensions are written new, against the
services, borrowing the measured constants and the text approach.

### 1.2 In `session` (`session:docs/design/session.md`)

Song structure (tempo map, markers, regions), generic lanes and timed
items with a kind and a payload it does not read, per-extension data
(registers, maps, text), transport as data, playback sync. Served as
`fts.session` by a provider extension: `read` (`ReadData`,
`ReadTimeline`, `ReadStructure`), `write` (own namespace, lanes and
items), `structure` (tempo, markers, regions). Another extension's
payloads read as null; the caller is the host's word.

### 1.3 In Architect's compositor (`runtime-compositor`, 51e50333)

Layers per pane (content by hash, sent once; a move is a transform),
host-run animation on `Clock::Wall`/`Clock::Playhead` (the playhead deck's
active source), named values (`view.`, `host.`, `engine.`), hit regions
and host drags with coast, engine values from an in-process
`EngineTable`. The sandbox `Timeline` (`architect:apps/sandbox/sketch`)
and Keyflow's chart (`keyflow:feat/runtime-chart`) are the worked
examples.

## 2. What each piece needs, and whose state it is

Three kinds of state, never mixed:

- **Document**: in the session, synced and undone by Architect.
- **Engine**: realtime, owned by the engine process, reaching views as
  engine values or the playhead clock; never in the document.
- **View**: one pane's, in the view's process; what is one person's
  (selection, the edit cursor) later goes to presence.

| what | arrangement | mixer / TCP | kind | carried by |
|---|---|---|---|---|
| tempo map, meters, bars/beats | ruler, grid, snapping | — | document (core) | `fts.session` `ReadStructure` |
| markers, regions | marker and region lanes | — | document (core) | `ReadStructure` |
| lanes, items: kind, time, owner | every lane and item, generically | — | document (core) | `ReadTimeline` |
| track: name, colour, order, folder | row headers | strip names | document (`daw` lanes) | `ReadTimeline` (lane name, order, parent) |
| track mixer state: volume, pan, mute, solo | header state | faders, pans, buttons | document (`daw` map `track:<lane>`) | `fts.daw` `tracks` |
| item: source, offset, gain, fades, name | clips | — | document (`daw.item` payload) | `fts.daw` `items` |
| waveform peaks | waveforms | — | derived (cache, by hash) | `fts.daw` `peaks` by hash |
| playhead, playing, rate | playhead line | — | engine | the playhead deck → `Clock::Playhead` |
| meters | (header meter) | strip meters | engine | shared table, `engine.daw.*` |
| a fader while it is dragged | — | fader, its knob | host → engine | shared table, `host.daw.*` |
| zoom, scroll, row heights on screen | camera | strip scroll | view | — |
| selection, edit cursor | selection layer | selected strip | view (presence later) | — |

A fader is document state at rest and engine state while it moves: the
drag writes `host.daw.track.<n>.volume` into the shared table at the
display's rate and the engine follows it; on release the view writes the
value to the document once (one undo step), which every other device
then sees.

## 3. The extension boundary

### 3.1 The `daw` extension's data in a session

As `session:docs/design/session.md` §4.1 proposes, owner `daw`:

```text
lane   { owner: "daw", kind: "daw.track", name, order, parent }
item   { owner: "daw", lane, kind: "daw.item", at: seconds, end: seconds,
         payload: Map {
           name, source (path, relative to the session's media root),
           offset (seconds into the source), gain, fade_in, fade_out,
           mute, color } }
ext:daw  map  track:<lane id>  { volume (fader 0..1), pan (-1..1), mute, solo, color }
ext:daw  reg  sample_rate, media_root, provenance { format, source }
```

A source becomes a blob by hash (§1.3 there) when Architect's blob store
reaches runtime extensions; until then a path relative to `media_root`.

### 3.2 `fts.session`

Unchanged. Provided by `daw` for the sessions it has open
(`session::extension::answer`, three actions `DAW_SESSION_READ/WRITE/
STRUCTURE`). When Architect hosts sessions itself (session §9), the
provider moves there and nothing in the views changes.

### 3.3 `fts.daw`

Provided by `daw`. Methods (each an action `DAW_<M>`):

| method | effect | answers |
|---|---|---|
| `open` | mutating | opens a session (a `.session` document) and plays it: its id |
| `state` | read | the open session, revision, transport, the value table's path and its names |
| `tracks` | read | per `daw.track` lane: index in the value table, volume, pan, mute, solo, colour |
| `items` | read | per `daw.item`: lane, start, end, name, gain, fades, mute, and the hash of its peaks |
| `peaks` | read | one peak set by hash (base64 i8 min/max pairs, at the levels it has) |
| `transport` | mutating | play, stop, pause, toggle, seek |
| `set` | mutating | one track field (volume, pan, mute, solo): written to the document, then applied |

The views ask through the host (`HostRequest::Service`), so the host
names the caller and holds them to the declared effect.

### 3.4 Peaks without re-sending

- The engine computes each source's peaks once (min/max per 256 frames,
  and a coarser level per 4096), names them by the hash of their bytes,
  and keeps them in memory and in a cache directory.
- `items` gives each item's peak hash; a view asks `peaks` only for hashes
  it has not seen, and keeps them by hash.
- Each clip's waveform is a fragment in the view's layers; the
  compositor sends a fragment once by its hash, and the host keeps it.
  So a source crosses the engine→view boundary once, and each clip's
  drawing crosses view→host once, whatever is panned or replayed.

### 3.5 Values (the Architect change)

`architect_scene::engine` keeps a table of atomic slots in the host's
memory. An engine in another process cannot write it. The change, on
`architect:runtime-compositor-arrangement`:

- **A table in shared memory**: a file the engine creates (written to a
  temporary name, then renamed, so an open mapping is never truncated),
  a header (magic, version, slot count), the names, the slots. Writes are
  the same two atomic stores. The host maps it and reads it at compose,
  and watches it for wakes as it does an in-process table.
- **`host.` slots in it**: names the engine lists that the host writes
  when a drag of a granted pane writes the same name. The engine reads
  them lock-free, at whatever rate it likes, on any thread.
- **Which table a pane reads**: the tables of the providers of the
  services its extension `[uses]` or `[requires]`, when it is granted
  `ui = ["engine"]`. No new manifest key, and the engine is replaceable:
  any provider of `fts.daw`. A table published after the pane mounted, or
  replaced by an engine that restarted, is picked up.

A process extension is already native code running as the person, so a
mapping it can corrupt adds no new trust; the values are still checked
on read (finite, in range) as today.

### 3.6 Transport

The engine is the leader. It puts its song on the playhead deck as its
source (`PLAYHEAD_SET`: title, duration, sections from the session's
regions, tempo) and moves it with `PLAYHEAD_PLAY/PAUSE/SEEK` whenever its
transport moves, so `Clock::Playhead` (what every host animation reads)
and the app's Now Playing bar follow it. The bar's own buttons reach the
engine through a hook on `app:playhead.changed`. Views ask for transport
through `fts.daw transport`. The deck extrapolates between the engine's
reports; the engine re-anchors it every few seconds while playing, so
the host's clock and the audio clock cannot drift apart visibly. Later:
the deck's clock read from the engine's own snapshot (`session::sync`),
which the playhead's `clock.rs` already names ("later an engine").

## 4. The views

### 4.1 Arrangement (`studio.arrangement`, view `arrangement`)

Layers (content space: seconds × 100 px across, rows down):

| layer | content | moves by |
|---|---|---|
| grid | bar and beat lines from the tempo map | x: the camera's pan and zoom |
| ruler | bar numbers, tempo, markers, regions | x only |
| one per track | its clips: body, waveform, name | the camera |
| headers | track names, colour, mute/solo state | y only |
| selection | selected clips' outline | the camera |
| playhead | one line | x from `Clock::Playhead`, under the camera |

- A pan or a scroll is a new transform per layer (bytes per frame:
  the layers that moved × ~60 B). A zoom is an x-scale while it moves;
  when it rests the view re-records at the new scale so text and
  waveforms are sharp.
- Generic first: any lane and item from `ReadTimeline` is drawn (a lane
  as a row, an item as a block), so an Ignition cue lane or a lyrics lane
  shows. `daw.track`/`daw.item` get names, colours and waveforms from
  `fts.daw`.
- Input: the wheel pans, Ctrl-wheel zooms about the pointer, a click
  selects, Shift extends, a click in the ruler seeks, Space plays and
  stops; a scrollbar is a host drag with coast.
- The TCP beside the lanes is the arrangement's own header column (name,
  colour, M/S state), in its scene, so it never tears from the lanes.

### 4.2 Mixer and TCP (`studio.mixer`, views `mixer` and `tcp`)

- A strip per `daw.track`: name, fader, pan, mute, solo, meter. Layers
  per strip; the strip row pans as a transform.
- Fader: a hit region with a `Drag` on Y writing
  `host.daw.track.<n>.volume`; the knob is animated by that value; the
  host writes it into the engine's table. On release the view calls
  `fts.daw set` once. Pan the same on X. Mute and solo are regions whose
  press calls `set`.
- Meter: a layer whose `ScaleY` is `engine.daw.track.<n>.peak`.
- `tcp`: the same controls as vertical panels, one per track, for a pane
  docked beside the arrangement. It does not follow the arrangement's
  vertical scroll yet (two panes share no values; §6).

## 5. Where the extensions live

**In this repo, as a workspace of their own (`extensions/`), for now.**
Their Cargo pins differ from Studio's: they need Architect at the
compositor branch (one rev for the whole graph, with the `www.` patch
Keyflow uses), and Studio's root is on the v0.12.6 tag; moving all of
Studio to an unreleased branch is a much larger change than this one.
`extensions/` is not a member of the root workspace: its own
`Cargo.toml`, lockfile and target. Each extension is one folder with its
`extension.toml`, installable as is, and the whole directory moves to a
repo of its own with `git mv` (recommended once Architect tags the
compositor: `FastTrackStudios/studio-extensions`, or one repo per
extension if third parties are to replace them).

## 6. Not in this PR, and gaps known now

- **Linked scroll between panes** (the TCP view following the
  arrangement): two panes share no values. Proposed for Architect: a
  window-scoped value space panes opt into, so a scroll drag in one moves
  layers in another at 0 bytes.
- `fts.session` has no change notification: views re-read on the
  engine's revision (`fts.daw state`).
- More than ~340 tracks need more slots than one table's 1,024 (three per
  track).
- Recording, editing items (move, trim, split), automation, sends, FX,
  folders collapsing.
- The web: the engine as wasm in an audio worklet writes the same table
  layout into a `SharedArrayBuffer`; not built.

## 7. Assumptions until `session`'s note on how `daw` attaches is out

- The `daw` engine is the leader of the transport and publishes it; the
  document never holds the playhead.
- The DAW's data is under owner `daw` as §3.1; the `daw` repo designs
  the final schema, and these extensions read it only through `fts.daw`,
  so a schema change moves one provider.
- The engine builds what it plays from the session's `daw` data on each
  document change, off the audio thread.

## 8. As built (2026-11-17)

`extensions/` holds five crates: `daw-view` (the `fts.daw` contract), `paint-kit` (glyph text, theme palette), `daw` (the engine extension), `arrangement` (`studio.arrangement`), `mixer` (`studio.mixer`, views `mixer` and `tcp`). `extensions/dev.sh` builds them, installs each program into its package and starts an isolated stock Architect (`architect-desktop dev`) with only them and the playhead. Architect is pinned at `runtime-compositor-arrangement` `f39ce235`.

Differences from the plan, and why:

- **A service's methods share its provider's prefix.** `fts.session`'s `read` is the engine's action `DAW_READ`, `fts.daw`'s `state` is `DAW_STATE`: one prefix (`DAW_`) for both services. Action ids must be in the extension's namespace (`STUDIO_ARRANGEMENT_*`, `STUDIO_MIXER_*`).
- **The engine builds what it plays through REAPER text.** It writes the session's `daw` tracks, clips and mixer state as `.rpp` text and hands it to `daw-standalone`'s loader, which decodes the sources. The session is the source of truth (an import from `.rpp`, the `daw` extension's `import`); the RPP text is the loader's input format only. A direct builder is the `daw` repo's step 4 of `session` §5.3.
- **The arrangement is tiled.** Its clips are recorded in tiles (sixteen rows by about 2,048 px of time), only near the view, kept by the view and held by the host by hash. Recorded whole, a 300-track, 20,100-clip arrangement was a 54.6 MB first paint, and a re-record after a zoom exceeded the host's 64 MB batch.
- **Transport echoes.** The engine leads the deck; the deck's `playhead.changed` comes back to it through a hook. The engine ignores the deck's events for 600 ms after its own reports, or a seek's `pause` echoed after a `play` stops it.

### Measured

Bytes are what the real view sends through the `ops::Sender` a process uses (`tests/measure.rs` in each view); "host" is the host's `Receiver` composing the pane into a scene (CPU, the GPU's raster not included). Dev build: the extensions at opt-level 2, their dependencies at 1. THEBATTLESHIP.

Arrangement, 8 tracks / 320 clips, 1280×800 (synthetic waveforms):

| | bytes | view work p50 / p99 | host compose p50 / p99 |
|---|---|---|---|
| first paint (font 740 KB included) | 1,190,386 | 0.66 ms | 2.5 ms with decode |
| a frame of playback (the host runs the playhead) | 0 | 0.001 ms | 0.13 / 0.33 ms |
| a frame of panning | ~400 (transforms) | 0.001 ms | 0.13 / 0.22 ms |
| a frame of a zoom gesture | ~435 | 0.001 ms | 0.12 / 0.15 ms |
| the zoom at rest (re-recorded once) | 423,013 | 0.9 ms | 0.32 ms |

Arrangement, 300 tracks / 20,100 clips, 3440×1440:

| | bytes | view work p50 / p99 | host compose p50 / p99 |
|---|---|---|---|
| first paint | 3,309,941 | 7.4 ms | 23 ms with decode |
| playback | 0 | 0.007 ms | 1.4 / 4.3 ms |
| panning, into tiles not yet sent | 6,920 a frame | 0.006 ms | 1.1 / 4.1 ms |
| panning back over tiles already sent | 646 a frame | 0.007 ms | 1.4 / 4.0 ms |
| scrolling into new tiles / back | 10,333 / 704 a frame | 0.011 ms | 2.7 / 4.9 ms |
| zoom gesture | 39,227 a frame (tiles the wider view reveals) | 0.007 ms | 1.1 / 3.5 ms |
| zoom at rest | 2,284,454 once | 4.7 ms | 1.9 ms |

Mixer and track panel:

| tracks | view | first paint | layers | 240 frames of meters | 120 frames of a fader drag | scroll a frame | host compose (drag) |
|---|---|---|---|---|---|---|---|
| 8 | mixer | 774,688 B | 35 | 0 B | 0 B | 0 B | 0.02 ms |
| 300 | mixer | 1,304,098 B | 1,207 | 0 B | 0 B | 38 B | 1.0 ms |
| 8 | tcp | 773,854 B | 35 | 0 B | 0 B | 0 B | 0.02 ms |
| 300 | tcp | 1,267,868 B | 1,207 | 0 B | 0 B | 38 B | 1.0 ms |

Live, in a stock Architect in a GPU Loft session (RTX 4080, a 60 Hz output), the engine playing "Drive To LA" from the owner's recital set into a private PipeWire null sink:

- **The playhead against the engine's clock**, sampled with `grim` every ~25 ms: median 0.10 px, max 0.9 px at 20 px/s (5 ms, 45 ms); at 200 px/s median 0.86 px, p95 4.9 px, max 6.4 px (4 ms, 24 ms, 32 ms), within a frame or two of a 60 Hz output. The same with the 20,100-clip fixture on screen: median 0.10 px, max 1.05 px.
- **A fader held at the bottom**: the engine's post-fader meter reads 0.0 while the document still holds the old value; one write on release.
- **What capped it, and what is unproven**: the session's output is 60 Hz, so nothing above 60 frames a second was presented. The work a frame (view ≤ 0.01 ms, host compose 0.1–4 ms for the scene, GPU raster not measured) leaves room for 240 Hz on the 8-track song and is borderline at 20,000 clips (host compose p99 4.3–4.9 ms against a 4.2 ms frame). **240 fps on an ultrawide is not shown**: it needs a high-refresh output and a measurement of the window's whole frame (scene, raster, present), which Architect does not report yet.
- **Found and fixed on the way** (Architect `f39ce235`): the development watcher read and hashed every file of a development copy's folder on the window's thread every poll. A debug program is hundreds of megabytes, so the window drew nothing for ~1.1 s every ~1.5 s, and the playhead held and jumped. Large files are now known by size and time.

### Not done yet

- Recording, item editing (move, trim, split), automation, sends, FX, folders collapsing; selection is the view's (not presence); markers and regions are read-only.
- The track panel does not follow the arrangement's vertical scroll (§6).
- The engine re-reads the whole session on open only: a track or clip added by another device reaches the views (by the revision) but not the engine until the session is opened again. Mixer fields do reach it.
- Beats in the ruler hold each tempo change as a step (ramps drawn as steps).
- A source that is not a WAV gets no peaks.
- The tab strip of the window is under Loft's bar in the "with chrome" capture (a window-mode artefact of the test session, not the views).
