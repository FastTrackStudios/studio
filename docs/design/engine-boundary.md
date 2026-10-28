# The engine boundary

Where Studio's engine ends and its GUI begins, and the path to the goal:
**the `studio` CLI is the whole engine** (`studio engine`), the desktop app
spawns it or attaches to it, everything is drivable from the CLI and MCP,
and the GUI is a remote control of the exact same engine.

Read with `docs/research/engine-process-split.md` (the survey and the
recommendation this follows), `docs/design/set-engine.md` and
`docs/design/one-engine.md`. Written 2026-10-27.

## Where it stands

Steps 1–3 of the research's migration path (§7) are under way:

| step | state |
|---|---|
| 1. The boundary inside one process | done, and cut: `session-engine` (`apps/session/engine`, 100 modules) and `session-ui` (`apps/session/ui`, the window); the compiler holds the line |
| 2. The Action List and set state to the engine | done: the list is the engine's; a window registers its own actions when it attaches; the edit cursor and time selection are engine state; Live's roll-on is the engine's |
| 3. `studio engine` | runs: the engine, the set, SetAudio, MCP on 47811, the daw facade on a Unix socket (`--listen` adds WS) |
| 4. The app attaches or spawns | done, and the only way: see Phase C and Phase D |
| 5. The data plane | done for what a window draws: see Phase D |
| 6. The desktop is always a client | done: no in-process engine in a desktop window |

### Phase D: one path (2026-10-27)

A window is a client of its engine everywhere; nothing in a window calls
the engine directly.

- **Desktop**: always another process. `link::choose` attaches to a
  running engine of the same build or spawns one — by default the app's
  own binary run as `studio-desktop --session-engine` (`session_desktop::
  engine_host`, the same code `studio engine` runs), so a window never
  drives an engine of another build; `FTS_STUDIO_BIN` spawns `studio
  engine` from a CLI instead, `FTS_ENGINE=attach` waits for one started by
  hand. An engine of another build (an update, a dev rebuild) is stopped
  (SIGTERM) and one of this build started with `--reopen`, which opens what
  the last one had open (`link::remember_open`, `last-open.json`).
  `FTS_ENGINE=inprocess` is gone.
- **Lifecycle**: `link::keep_linked` watches the link; the same engine is
  attached again while it answers, a dead one is replaced (spawned with
  `--reopen`) and the window says so (`link::take_notice`, shown by
  `start::EngineNotice`). The engine outlives the window.
- **iOS** (no helper process): the engine runs in the window's process and
  the window reaches it through the same client over an in-memory vox link
  (`open::link_window_here`, `architect::LocalServer` over the engine's
  router). A test does the same (`tests/it/view_link.rs`).
- **The engine's view service** (`view::EngineView`, `#[architect::rpc]`,
  served beside the daw facade on one router — `view_host::router`):
  streams `set`, `overlays` (hit overlay), `marks` (tracks' own marks and
  the points of interest), `streams` (arrival status, how songs are heard),
  `jobs` (the bottom bar), `edits` (where actions put the edit cursor and
  time selection), `changes` (what to read back, an undo among them),
  `queued` (what
  the engine asks of a window; subscribing is what attaches a window); calls
  `run` (the Action List over vox), `apply` (the window's edits, one batch a
  call — a kit's slip edit is one message), `take_window`, `waves` (coarse
  waveforms as the engine draws them: through take volume, cut from a
  recording already read, partial reads flagged), `detail` (fine detail at
  the zoom asked), `automation` (lanes, take volume curves, leveled items),
  `item_changes` + `journal_now` (the window's item read-back), `song_facts`
  (track kinds, chart), `playheads`, `show_edit` (where the window shows the
  edit cursor, time selection and selection: the latest change only, so
  `EDIT_*` and every action that reads the selection act on it).
- **The window's replicas** (`view_client`): the set it shows (read back
  over the facade as songs arrive, waveforms behind), the overlay, the
  marks, the streams, every song's playhead (from `TransportSync::positions`,
  carried on at its rate between stamps), what the engine asked. A frame
  reads replicas only.
- **Window actions the engine runs** (`SESSION_STEP`, `SESSION_VIEW`) ask
  the engine for what needs the song (`SESSION_PAGE`, `SESSION_SPAN`); the
  window's menus and toolbars run engine actions (`AUTOMATION_MENU`,
  `AUTOMATION_SET_MODE`, `ORGANIZE_PRESS`, `CHART_APPLY`, `VOCAL_RELEVEL`,
  `RIG_BROWSE`, `SESSION_SONG_SET`, `SESSION_CLOSE`, `SESSION_JOIN`,
  `SESSION_STOP_BEHAVIOUR`, `SESSION_RENDER_AHEAD`, `SESSION_GROUPING`,
  `SESSION_MULTITRACKS`); ⌘K lists and runs the engine's (`SESSION_ACTIONS`),
  Versions reads `SESSION_VERSIONS`. A recording pass's takes are settled by
  the engine.
- **The compiler holds it**: other crates name the engine as
  `session_engine`, `session-ui` re-exports only the engine modules its
  window uses, and what reads or writes engine-process state for a window
  is `pub(crate)` (`take_rank::settle_open`, `render_ahead::set`,
  `reference_play::{multitracks, any, load_multitracks}`,
  `open::restart_audio`, `open_song_versions`, …; `local_engine` was
  already) — a window that reaches for one does not build.
- **Still to do**: collaboration runs in the window (`collab`,
  `live_host`: song docs go over the facade, but the shared-transport
  follower needs the engine — it belongs in the engine beside
  `SESSION_JOIN`); three engine actions write window view state
  (`SESSION_ITEM_STYLE`, `SHOW_LANES`, an instrument's add folding its
  track) and should be window asks; the Linux app menu reads enabled flags
  from the window's own list; meters are the song on screen's (per-voice
  banks are `SetAudio` work); the web build keeps its own engine reach
  (`web_engine`, `web_host`) and its own set.

### Phase C: the window on an engine in another process

`FTS_ENGINE=spawn|attach|inprocess` (`session_engine::studio_engine::link`):

- **Discovery**: an engine writes `$XDG_CONFIG_HOME/fts/engines/studio-<pid>.json`
  (`link::Found`: pid, socket, MCP URL, WS); a window reads them newest
  first and attaches to the first that answers on its socket.
- **Spawn**: none answering, `studio engine` (`FTS_STUDIO_BIN`, else a
  `studio` beside the app, else on `PATH`) in a process group of its own,
  logging to `$XDG_CACHE_HOME/fts/studio-engine.log`; the window waits up
  to 30 s for it to say where it is. It outlives the window: a crash, a
  kill or a relaunch of the GUI leaves it playing. Stop it by its pid.
- **Attach**: the window is Remote on `RemoteTarget::Engine { socket }` —
  the daw facade over the engine's socket (`open::attach_to_local_engine`,
  which also takes an engine with nothing open). It serves no MCP (the
  engine does, on 47811). Its keys and menus run window actions here and
  every other action on the engine (`actions::run` → MCP); its picks
  (`set::pick` → `SESSION_SONG`), opens (`set::open_library`/`open_path`)
  and Live (`live::set_live` → `SESSION_LIVE`) go to the engine.
- **The set, followed** (`link::follow`): every 400 ms `SESSION_SONGS`; a
  new set is read back song by song through the facade without selecting
  any (`Setlist::from_engine`, songs already shown kept), another current
  song is marked current. Several windows can follow one engine.
- Verified by hand (2026-10-27): the dx app spawned the engine;
  `studio open "JT's Senior Recital"`, `studio song next`, `SESSION_PLAY_PAUSE`
  drove it and the window followed (waveforms drawn); the GUI killed, the
  engine played on; relaunched, it attached to the same engine without
  stopping it; a second window followed the same set; Live handed song 1
  to song 2 on the frame (`SESSION_TRANSPORT` read the new song at 3.2 s).
- What Phase C lacked (the replica, the window queue across processes,
  restarting a dead engine) is Phase D.

### Phase B: two crates

`session-daw` is gone: **`session-engine`** (`apps/session/engine`) is
the engine — the set, the Action List, MCP, opening, streaming, collab,
the project file, every edit of a song, `studio_engine::headless`, the
engine-only tools (`hits`, `prepare`, `reference`, `session-media`,
`show-split`, `chain-probe`); native and wasm (`native`, `web`).
**`session-ui`** (`apps/session/ui`) is the window, a view of it: it
re-exports every engine module at its root, so its code (and its
dependents, `session_ui::open`, `session_ui::studio_engine`) reads as
before. The engine still depends on a few window-kind crates (dioxus
signals, daw-ui's project types, vello's colour) for the read-back model
(`song_model`) a window draws from — step 5 moves that to the GUI.

### Phase A: the engine side stands alone

- **The Action List is the engine's** (`actions.rs`): transport, the set,
  the song's data — complete with no window. The window's actions
  (`window.rs`: zoom, panels, `SESSION_VIEW`, the edits its arrangement
  makes under its own selection, `SESSION_FRAMES`, `SESSION_STEP`) are
  registered on it by `window::attach()` when an arrangement comes up,
  enabled while one is attached and listed `enabled: false` ("no window
  attached") after. architect's `ActionList` has no unregister; that is
  the one gap between "listed disabled" and "gone".
- **The queue to the window** (`window_queue.rs`): the key `Action` type,
  the queue a window takes each frame, and whether a window is attached
  (`attach()` → a guard). With none attached nothing is queued.
- **Edit cursor and time selection** (`edit_state.rs`): engine state per
  song. A window `show`s its own as it draws; an action `ask`s and every
  window takes it up (`asked_since`). `SESSION_EDIT_STATE` reads it.
  Item and track selection were the project's already.
- **Live's roll-on** (`studio_engine/live.rs`): with the set in Live
  (`SESSION_LIVE`, set by a window in Live mode) the engine schedules the
  hand-over 0.5 s before the song's end (`SetAudio::play_after`): the next
  song starts on the frame the current one ends; then the set follows. A
  stop before the moment calls it off (`SetAudio::call_off`, new); a song
  that cannot be handed over is picked at its end, as before.
- **Cycles broken**: `song_model.rs` (StudioSession, the read-back flags),
  `stretch.rs`, `presence.rs` (out of `ghosts`), `organize_edit.rs`,
  `open_progress.rs`, `changes.rs` (the engine announces, a window
  listens — the marks strip), `poi_paint.rs`, `instrument_rail.rs`, the
  strip gestures (`mcp::click`/`drag`) out of `engine.rs`. Each view
  module that lost code re-exports it, so view code reads as before.

### What moved (this change)

- **`apps/session/engine/src/studio_engine/` (was `apps/session/daw/src/…`)** — the engine side's module,
  the seam the `studio-engine` crate will be cut along:
  - `set.rs`: the set the engine has open (`SetView`: revision, epoch,
    `Setlist`, `Opening`), published on a `tokio::sync::watch`. The one
    writer: `install`, `close`, `pick`, `mark_current`, `pick_by`,
    `recolor`, `reorder`, `remove`; opening runs here (`open_with`,
    `open_library`, `open_path`) on a thread, its progress published, and
    the songs arriving behind the first are drained into the set by the
    engine (they used to be drained by the window's shell). Honest pure
    parts — `resolve` (what `SESSION_SONG to` names), `rows`
    (`SESSION_SONGS`), `SongPick`, `position_named` — with unit tests
    (`set/tests.rs`).
  - `set_actions.rs`: `SESSION_OPEN`, `SESSION_OPEN_SETLIST`,
    `SESSION_SONGS`, `SESSION_SONG`, `SESSION_SONG_NEXT/PREV`, moved out
    of `actions.rs`, now run against the engine's set — and
    `SESSION_OPENING` (new: idle / opening with its step / failed with
    why), since an open no longer waits for a window to say how it went.
  - `mcp.rs`: the Action List's MCP server (was `mcp_server.rs`).
  - `headless.rs`: the engine stood up with no window (Engine mode,
    `open::start_engine`, MCP).
- **Removed** from `projects.rs`: the queues the Action List used to ask
  the window through — `OpenSet`/`show_open_set`/`open_set`,
  `SongPick`/`PICKED`/`pick_song_from_outside`/`take_song_pick`,
  `SetlistOpen`/`NEXT_SET`/`open_setlist_from_outside`/`pending_setlist`/
  `take_setlist`, `open_from_outside`. With no window those never ran.
- **The window is a view of the set** (`apps/session/desktop/src/native`):
  `start::App` shows whatever set the engine has (keyed by its epoch), the
  start screen while there is none or one is opening; the start screen
  opens through `set::open_with` and mirrors `Opening`; the shell mirrors
  the set's revisions into the `Signal<Setlist>` its panels read, and its
  picks, recolours, Live mode's roll-on and `FTS_SESSION_SONG` go through
  `set`. `setup.rs` (reorder, remove) and `collab_bar.rs` (follow a song)
  likewise on native; the web build keeps its own set. Studio's welcome
  screen enters the DAW view while the engine opens a set
  (`projects::pending_open` → `set::is_opening`), whoever asked.
- **`studio engine`** (`apps/session/desktop/src/cli/engine.rs`): see
  [Running it](#running-it).

### Running it

```console
$ studio engine                       # until Ctrl-C / SIGTERM
$ studio engine --listen 0.0.0.0:4040 --open "JT's Senior Recital" --org fasttrackaudio
$ studio open "JT's Senior Recital" --org fasttrackaudio
$ studio run SESSION_OPENING
$ studio songs
$ studio song next
$ studio run SESSION_PLAY_PAUSE
$ studio run SESSION_AUDIO
```

- Action List on MCP: `http://127.0.0.1:47811/mcp` (`FTS_MCP_PORT`),
  published in `$XDG_CONFIG_HOME/fts/session-mcp.json` as a window
  publishes it — the CLI cannot tell the two apart.
- daw facade over vox on `$XDG_RUNTIME_DIR/fts/engine-<pid>.sock` (0600;
  `--socket`), the same lane protocol REAPER's bridge serves, so
  `studio::cli::connect(Some(socket))` — and a window's Remote attach —
  dial it unchanged.
- `--listen ADDR`: the facade over WebSocket too (`ws://ADDR/vox`,
  architect's `EngineHost`). Off by default.
- `$XDG_CONFIG_HOME/fts/engines/studio-<pid>.json`: pid, socket, MCP and
  WS addresses, removed on exit — what step 4's discovery reads.
- The library is opened as the person signed in on the machine
  (`fts_account`, `FTS_ACCOUNT_FILE`), the org from `--org`, else
  `FTS_TASK_ORG`, else the one last picked.

`session-desktop --engine` (the old WRY app's `SessionEngine` +
`SetlistService`) and `studio-desktop --engine` (Signal's rig engine,
ws://:4040) are left as they are: the first is a different engine (no set,
no Action List) and goes when the WRY app does; the second folds in once
rigs are tracks of the one engine (`one-engine.md` steps 4–5) — then
`studio engine --rig` mounts the rig services beside the facade.

## Step 1: the inventory

Every place the GUI crates reach the engine directly — not through the vox
facade: `open::engine()`/`local_engine()`/`with_local_engine`/
`cue_engine`, `actions::song()`/`Song::current()` (both hand out
`&'static Standalone`), a `&Standalone` parameter, or a `daw_standalone`
item. 89 files: 79 in `apps/session/daw/src`, 10 in
`apps/session/desktop/src`; 63 engine, 12 view, 14 mixed. 49 of the daw
files are reached from the Action List.

**engine** — project data for its own sake (tracks, items, takes, hits,
comp, fx, automation, saving, opening, the set, transport, the device).
**view** — exists only to paint or lay out; a view that reads the engine
per frame is a read-back that becomes a replica/subscription (step 5).

### apps/session/daw/src

| file | reaches | what it does with the engine | side | Action List |
|---|---|---|---|---|
| actions.rs | ~90 | every data handler on `Song::current()`: save, open, undo, hits, quantize, comp; also the `WINDOW` queue, `MODE_REQUEST` | mixed: data handlers engine; mode/view/zoom/panels, `WINDOW`, `FOLLOW_UP` view | is the registry |
| audio_health.rs | 1 | reads `EngineStats` (callback timing, xruns) | engine | yes |
| automation_actions.rs | 9 | reads/writes envelope lanes, automates params by section | engine | yes |
| automation_edit.rs | 3 | lane gestures call `vocal_level::show_lane`/`relevel` | mixed: gestures view; those calls engine | no |
| automation_menu.rs | 9 | the automate-what menu from shown envelopes and FX params | mixed: menu view; reads/the pick's write engine | via automation_actions |
| automation.rs | 6 | lane store for drawing, refreshed from engine reads each resync | mixed: reads engine; store view read-back | via automation_actions |
| bar_step.rs | 3 | pages the view by bars; reads last bar and tracks | mixed | yes |
| bass_rig.rs | 1 | loads bass rig profiles onto bass tracks | engine | yes |
| bin/hits.rs, bin/bench.rs, bin/blitz_shot.rs, bin/chain-probe.rs | 1–3 each | own engine / `open_silent` / `open_and_serve` for tools | engine (bootstrap) | no |
| browser.rs | 1 | reads the selected track to fill the browser | view (read-back) | yes |
| chart_editor.rs | 2 | rebuilds the song from the edited `.kf`, saves behind | mixed: editor view; `apply` engine | no |
| collab.rs | 2 | hands the engine to doc sync and transport sync | engine | no |
| collab_bar.rs | 1 | follow-song picks (now through `studio_engine::set`) | view | no |
| comp_keep.rs | 6 | kept takes/stretches on comp lanes | engine | yes |
| comp_rank.rs | 11 | ranks takes per stretch, cycles, chooses | engine | yes |
| comp.rs | 11 | swipe comping onto the comp lane | engine | yes |
| comp_song.rs | 11 | comps the kit from its best takes | engine | yes |
| drum_group.rs | 2 | the kit as one track group | engine | via actions |
| drum_lanes.rs | 5 | COMP/EDIT lanes made and played | engine | via actions |
| drum_level.rs | 6 | volume curves levelling strokes | engine | via actions |
| drum_reference.rs | 5 | maps drum MIDI to the kit | engine | yes |
| drum_samples.rs | 6 | sampler plugins under the kit | engine | via actions |
| edit_actions.rs | 8 | EDIT_QUANTIZE/REVERT/LEVEL/SCOPE | engine | yes |
| editor_view.rs | 2 | loads take audio for analysis; `align_all` writes warp markers | mixed | no |
| engine.rs | 5 (+12 facade) | the window's edit queue, `apply_locally`, stretch, take volume; per-frame `playhead_of`, the `Transport` poller | mixed: edits engine; poller/playhead view read-back | yes |
| folder.rs | 3 | `.session` RPP text from parts | engine (file I/O) | via open_core |
| fundamental.rs | 6 | a drum's fundamental from its hits | engine | yes |
| fx_actions.rs | 13 | FX and params: list, set, add, remove | engine | yes |
| gain_stage.rs | 6 | offline render, then track gain | engine | via actions |
| grow.rs | 17 | double / second part: tracks and items | engine | yes |
| guide_instrument.rs | 4 | the click/count/guide synth into the engine | engine | no |
| hit_index.rs | 4 | per-song hit index, from the journal | engine | via hits |
| hit_overlay.rs | 1 | worker reads `hits::overlay`, publishes for painting | view (read-back) | yes |
| hit_place.rs | 3 | replays a stretch around moved hits | engine | yes |
| hits_midi.rs | 7 | hits to MIDI and back | engine | yes |
| hits.rs | 55 | detects, writes, corrects hit markers; `overlay()` for the view | engine | yes |
| instrument_tracks.rs | 9 | TRACK_ADD_INSTRUMENT | engine | yes |
| item_actions.rs | 8 | move/trim/slip/fade/split/delete items | engine | yes |
| lib.rs | 1 | wasm stub of `with_local_engine` | — | no |
| marks_strip.rs | 1 | worker reads track regions for the ruler strip | view (read-back) | yes |
| midi.rs | 8 | item bodies: take volume, cuts, peaks, partial streams read per item | view (read-back, per frame) | no |
| mix_chains.rs | 9 | builds the mix chains | engine | via actions |
| mix_export.rs | 2 | prepared song back to `.RPP` | engine | no |
| open_core.rs | 18 | THE song open: RPP load, media bay, materialize | engine | via open |
| open.rs | 35 | owns `ENGINE`, `AUDIO` (SetAudio), the runtime, mode, current song, remote attach | engine | yes |
| options.rs | 1 | grouping toggle flips an engine global | mixed | no |
| organize.rs | 4 | toolbar inserts sections/markers | mixed: toolbar view; inserts engine | via timeline_actions |
| plan.rs | 1 | row plan reads track kinds | view (read-back) | no |
| poi.rs | 11 | points of interest read/written | engine | yes |
| prepare.rs | 9 | prepares a song: organize, chart, click, render | engine | via project_new |
| project_file.rs | 21 | `Song.session` load/save, undo/redo through the doc, its own `InProcessDaw` | engine | yes |
| project_new.rs | 6 | `open_untitled`, stamps a new project | engine | yes |
| projects.rs | 2 | saved fingerprints for "unsaved"; recent | engine | yes |
| record.rs | 3 | arm, record, stop | engine | yes |
| reference_play.rs | 3 | streams a reference into the engine (aux renderer) | engine | no |
| reference.rs | 8 | renders the reference bounce in its own engine | engine | via stream_set |
| render_ahead.rs | 4 | audio I/O prefs, `restart_audio` | engine | yes |
| rig_actions.rs | 8 | signal rigs on tracks, tone | engine | yes |
| seams.rs | 5 | places comp seams | engine | yes |
| session_file.rs | 2 | `.session` beside the RPP | engine | via actions |
| setlist.rs | ~8 | opens songs into the engine, `sound`, `switch_song`; tabs, pictures | mixed: opening engine; tab model/pictures view | via studio_engine::set |
| show.rs | 2 | splits a Live set in its own engine | engine | no |
| song.rs | 10 | the `Song` handle (`&'static Standalone` + id) | engine | yes |
| song_stream.rs | 25 | media fetch, pending media, remote ogg, peaks | engine | via stream_set |
| stream_in.rs | 10 | Cue/streamed engine, transport-sync leader | engine | no |
| stream_set.rs | 29 | streamed set: sources, full resolution; `SHOWN` view feed | engine | yes |
| studio.rs | 9 | `read_project`, `fetch`, `patch_items`, kinds, chart; open/switch | mixed: open/switch engine; the rest view read-back | yes |
| take_rank.rs | 13 | rank markers on takes | engine | yes |
| timeline_actions.rs | 12 | tempo, time selection, sections, guide | engine | yes |
| tone_read.rs, tone.rs | 1 each | rack display reads a chain / tempo | view (read-back) | no |
| track_regions.rs | 23 | a track's own markers/regions | engine | yes |
| vocal_level.rs | 21 | vocal levelling lanes | engine | yes |
| wave_detail.rs | 1 | fine waveform stretches as the zoom asks | view (read-back) | no |
| web_audio.rs, web_engine.rs | 17, 9 | the page's in-process engine (AudioWorklet) | engine | no |

Borrow only the engine's runtime (no `Standalone`): `task_account.rs`,
`expression.rs`, `quantize_panel.rs`, `routes.rs`, `take_rating.rs`,
`widget.rs`, `studio_engine/mcp.rs`.

### apps/session/desktop/src

| file | reaches | what it does | side |
|---|---|---|---|
| session_engine.rs | 16 | the WRY app's own engine (`SessionEngine`, SetlistService) | engine |
| cli/player.rs | 10 | `studio player`'s own engine per song | engine |
| lib.rs | 4 | WRY UI buttons drive `session_engine` | mixed |
| engine_open.rs, engine_server.rs | 3, 3 | `session-desktop --engine` | engine |
| guide.rs | 2 | click/cues into the WRY engine | engine |
| active_engine.rs | 2 | picks the engine, for its facade clients | view |
| session_view.rs | 1 | writes `MixerRouting::shared()` from a panel | view writing engine global |
| native/shell.rs | 3 | picks (now through `studio_engine::set`) | view |
| native/mod.rs | 3 | process wiring: `serve_actions`, dialer, mode | mixed |

### Engine state held in statics

`open.rs`: `ENGINE`, `BUNDLE`, `RUNTIME`, `AUDIO`, `AUDIO_ASKED`,
`AUDIBLE`, `REMOTE_CURRENT`, `REMOTE_PICKS`, `CONNECTION`, `ATTACHED_TO`,
`ENGINE_DIALER`, `ENGINE_CONNECTION` (+ `audio_mode::STATE`).
`stream_set.rs`: `ARRIVALS`, `STREAMS`, `ARRIVING`, `ORIGINALS`,
`ORIGINALS_FAILED`, `SHOWN`. `actions.rs`: `LIST`, `WINDOW`,
`FOLLOW_UP`, `MODE_REQUEST`, `UNDONE`. `setlist.rs`: `OPENING`,
`PICTURES`. `engine.rs`: `SHARED` (transport poller), `JUMPS`, `MADE`,
`ISSUED`/`DONE`. `projects.rs`: `SAVED`, `NEXT`, `BACK` (the set queues
`OPEN_SET`, `PICKED`, `NEXT_SET` are gone — `studio_engine::set` holds
the set).

### Hardest to move

1. `actions.rs` — the registry mixes engine handlers with window actions
   and owns the window queue.
2. `open.rs` — the root of every reach, ~15 statics.
3. `hits.rs` — 55 reaches; also builds the overlay the window paints.
4. `engine.rs` — the window's edit pipeline with an in-process fast path,
   the per-frame playhead and the transport poller.
5. `studio.rs` — the read-back tangled with open/switch.
6. `stream_set.rs` — window-facing statics (`SHOWN`).
7. `setlist.rs` — the tab model that also opens songs.
8. `midi.rs` — per-item engine reads while drawing.
9. `automation*.rs` — lanes painted from engine reads; gestures write.
10. `project_file.rs` — a second `InProcessDaw`, undo through the doc.

The rest (comp*, drum_*, grow, track_regions, vocal_level, fx_actions,
timeline_actions, take_rank, …) already take `&Standalone` +
`ProjectContext` and are reached only through `actions::song()`: they
move as they are once `song()` and the registry live on the engine side.

## Step 2: what is left of it

In order, each a commit that keeps the build green:

1. **Split the registry.** `actions.rs` becomes `studio_engine::actions`
   (data handlers: `register_data` and everything under it, `song()`,
   `resolve`) and the window's `window_actions` (mode, view, zoom,
   panels, the `WINDOW` queue). The drum quantize engine inside
   `actions.rs` (~2.6k lines, `docs/design/session-daw-split/grab_bags.txt`)
   goes to `editing` or `studio_engine::quantize`.
2. **Window actions register with the engine.** A window, when it
   attaches, registers its window actions (`SESSION_VIEW`, `SESSION_MODE`,
   zooms) on the engine's list as forwarding entries; detached, they are
   listed `enabled: false` "no window attached". In one process this is a
   flag; across processes it is a vox service the window serves back.
3. **Selection, edit cursor and time selection become engine state**
   (project state, as the research says), so "quantize the selection"
   works headless and two windows agree.
4. **Live mode's roll-on moves to the engine.** `use_live_advance` picks
   on a redraw; the engine should schedule the next song on the frame the
   current one ends (`ScheduledLocate`, `set-engine.md` "Transitions").
5. **Cut the crate.** When `studio_engine` and the modules it calls
   (open, open_core, stream_set, song_stream, setlist's open half,
   project_file, the data handlers) no longer reach a window module, move
   them to `crates/studio-engine` (features: `audio`, `library`, `mcp`),
   and `session-daw` stops depending on `daw-standalone` except through an
   `inprocess` feature on the host crate. Today `session-daw` is one
   strongly connected component of 115 modules
   (`docs/design/session-daw-split/README.md`); that report's symbol moves
   are the work list. This is the step Ardour never finished.

## Plan: steps 4–6

### 4. The app attaches or spawns

- `FTS_ENGINE=inprocess|spawn|<addr>` (default `inprocess` until parity).
- **Discovery**: read `$XDG_CONFIG_HOME/fts/engines/*.json`; drop entries
  whose pid is gone; dial the socket. A live engine with a set open is
  attached as is (the window shows its set).
- **Spawn**: `studio engine --socket …` in its own session (`setsid`), so
  closing the window or a GUI crash does not stop the audio. Generalise
  `apps/studio/desktop/src/engines.rs` (child or systemd unit, kills only
  what it started).
- **The GUI's link**: the window already speaks the facade for Remote mode
  (`open::attach_to_reaper` → `studio::cli::connect`); `studio engine`'s
  socket serves the same lanes, so Remote-on-a-local-engine works first.
  What it lacks is the set: add a `Set` vox service on the engine
  (`subscribe` = `set::subscribe` as a `#[subscribe]` stream of a
  lightweight `SetView` — lineup + opening, no `StudioSession`) and the
  set's operations as RPCs; the window's `use_engine_set` reads that
  stream instead of the in-process watch. The Action List stays MCP (and
  can be mounted on vox too, for the GUI's palette).
- **Exit policy**: the engine lingers when its last client goes; exits
  after N s only if stopped and nothing is dirty; `--persist` for a rig.
  Quit in the app asks the engine to save and stop; closing a window does
  not.

### 5. The data plane (the GUI-side model cache)

- **Replica**: the `StudioSession` (rows, items, previews, chart) leaves
  `Setlist`: the GUI builds it from a snapshot over the facade
  (`StudioSession::read_current` already does this for Remote) and keeps
  it with the journal (`#[subscribe]` item/track events, revisions). The
  frame loop reads the replica only; it never awaits an RPC.
- Per-frame engine reads in the inventory's view rows (`midi.rs` item
  bodies, `automation.rs` lanes, `engine::Transport` poller,
  `wave_detail.rs`, `hit_overlay`/`marks_strip` workers) move onto:
  pushed `MeterFrame`s, `TransportSync::positions` with `ClockEstimator`
  (the playhead extrapolated), peak *paths* (`.reapeaks` on disk, `mmap`
  locally, `take_peaks` remotely), and journal events.
- **Edits**: the window's `engine.rs` queue sends `Edit`s over the facade
  (`apply_locally` goes); gestures render from the replica + gesture and
  commit one command (faders: a throttled stream).
- Shared memory only for a view that measures short (scopes, analyzers):
  a memfd ring passed with `vox-fdpass`.

### 6. Flip the desktop default to spawn

- iOS and web stay in-process permanently (no helper process on iOS; the
  page's AudioWorklet engine): the same `studio_engine` over the memory
  link (`build_in_process_daw`), the same `SetView` stream.
- Tests before flipping: GUI killed while playing → audio continues → GUI
  reattaches; engine killed → GUI offers restart, the engine reloads from
  its autosave; two GUIs on one engine stay consistent; the
  `set-engine.md` frame-time check runs over the socket.
- Measure first (research step 0): command round trips and frame times on
  the memory link and on the socket.

## Risks

- **Two writers while migrating.** Until step 2.1–2.3 are done, window
  code still edits `Standalone` directly; a second GUI or the CLI can race
  it. The set is now single-writer; the rest is not.
- **`StudioSession` built on the engine side.** Songs are still read back
  as they open, on the engine's threads; with no window that work is
  wasted. Step 5 moves it.
- **Ambient statics.** The engine is reached through process globals
  (`open::*`, `set::cell`), so one process is one engine; a test that
  wants two engines needs the step-2.5 crate with an `Engine` context.
