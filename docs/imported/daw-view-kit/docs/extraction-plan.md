# Taking the DAW views out of the studio repo

Plan for moving `daw-view-kit`, `arrangement-view`, `mixer-view` and the MIDI
editor into their own repos, so that they depend on daw's contracts and on
nothing in the studio repo.

Surveyed on 2026-11-24 at studio `aec4909a2` (worktree
`.worktrees/studio-arrangement-split`), daw `e7fedc99`, session `aa3dc6ef`.
Nothing was built.

**How the counts were made.** Comments are stripped, `use` trees are expanded,
and each `crate::path::item` occurrence counts once. An engine module the kit
re-exports (`pub use session_engine_client::X`) is counted where the views
reach it through `crate::X` (section 1.2). These are exact paths, so the
totals are lower than the substring counts in the brief: `session::` comes to
130, not 581. Column keys: **k** = daw-view-kit, **a** = arrangement-view,
**m** = mixer-view, **t** = daw-view-tools.

## Destinations

| Tag | Where it goes |
|---|---|
| **DV** | `daw-view` in the daw repo (daw#51): the EngineView protocol and link, the song model, transport, selection, overlays and the plain types both sides share |
| **DT** | `daw-theme` in the daw repo (daw#52) |
| **DE** | `daw-edit` in the daw repo, already a git dep at rev `6dd7a8b`: edit-algorithm contracts |
| **KIT** | the daw-view-kit repo (the kit, plus daw-ui, timeline, audio-controls) |
| **ARR / MIX / MIDI** | arrangement-view / mixer-view / midi-editor |
| **SP** | the session primitive (FastTrackStudios/session) |
| **ACT** | an engine action. The view calls `actions::run(id, arg)` from DV with a string id and stops linking the code |
| **APP** | stays in studio (`apps/studio` or an extension) and is injected into the views through a slot, a context or a registry |
| **DEL** | delete |

## 0. Findings that shape the plan

1. **Half of the kit is not kit.** About 13k of the kit's 31.6k lines are
   Studio's app panels: `shell`, `web_host`, `chart_panel`, `chart_editor`,
   `collab_bar`, `collab_pointers`, `compact`, `setup`, `record_view`,
   `transport_bar`, `progress`, `performer`, `lyrics_panel`, `navigator`,
   `mix_guide`, `versions`, `browser`, `loading`, `app_menu`, `window`, `dock`.
   Nearly every use of `session::`, `app_shell::`, `setlist`, `collab`,
   `web_engine`, `audio_mode` and `editor_state` sits in those panels. Moving
   them into studio first removes most of the problem before any engine code
   changes.
2. **`expression` and `editor_view` (2.9k lines) are the MIDI editor's.**
   Together they hold 47 of the kit's 81 `editor::` uses. The arrangement
   reaches the editor in only two ways: `editor::kit` (drum lane roles, 7 uses)
   and `crate::editor_view::picked` (1 use, `widget.rs:4458`).
3. **The kit's `engine` feature mirrors the engine.** The kit re-exports 26
   `session_engine::X` modules in place of the `session_engine_client::X`
   versions (`lib.rs:14-160`). This is how session-desktop, the tools and the
   web page run the engine in-process. Once the views move, the app builds an
   in-process link (`architect::LocalServer`) and gives it to DV's client. The
   feature and every `session_engine` re-export are then deleted.
4. **The view crates pull studio-adapters as well as the studio repo:**
   - `studio` is listed in all three view crates but never used: **DEL**.
   - `editor`'s `daw` and `audio-daw` features pull `dep:studio`, but its only
     `studio::` uses are `studio::reaper::*` and `reaper_ui::dock::*`, which
     belong to the `reaper` feature: **DEL** from those two features.
   - `keybinds`, used by daw-ui, the kit and the editor, is open question Q4.
   - `dynamic-template` uses `daw_reaper` and `dawfile_reaper` (39 uses). It
     stays in studio either way.
5. **The song model the views draw is `daw_ui::studio::project`.** It has 74
   uses, plus `ProjectRef`, `RowsRef` and `folded`, and `song_model.rs` patches
   it. It goes to **DV**, not to the kit.
6. **The session primitive already covers some of this.** It has `TempoMap`,
   `Marker`, `Region`, `Lane`, `Item`, `Transport` and `sync`. It has no
   presence, modes or mix phases. Nothing the views use today comes from it
   directly. The views draw the daw song model, not the session's.

## 1. Inventory and destinations

### 1.1 `session` (studio `features/session`, to be dissolved): 130 uses

| Symbol | Uses | Crates | Destination |
|---|---|---|---|
| `mix_phases::MixPhase` (+`::ALL`, `from_slug`, variants) | 45 | m 37, k 4, t 4 | **MIX**. The mixer's own working method. `rails.rs` moves with it |
| `modes::Mode` (+variants, `ALL`, `display_name`) | 37 | k 21, a 6, m 7, t 3 | **Q1**. Default: a small `Mode` in **DV**, with the app supplying the current value through context |
| `mode_phases::{version, current, of, choose}` | 11 | k 6, m 4 | **APP** state, given to the mixer as `Signal<Phase>` plus a callback. The phase list itself is MIX |
| `key::{KEY_TRACK, parse_key, format_key}` | 9 | k 4, a 5 | Parsing and formatting: `keyflow::key` directly. `KEY_TRACK` (a track-name convention) is engine data: key changes arrive in DV's `SongFacts` |
| `ui::components::{lyrics, chart_text, sidebar_items, progress, SectionProgressBar, Me, transport_controls}` | 15 | k 15 | **APP**, together with the panels that use them (`lyrics_panel`, `chart_editor`, `progress`, `transport_bar`) |
| `sync::presence::{PeerState, STATE, Pointer}` | 7 | k 4, a 1 | **SP**, as a new `sync::presence` module. It is plain data for collaboration on a session. The ghosts' painter in the kit reads `Pointer` |
| `lyrics::{is_lyrics_track, Lyrics, SectionSpan}` | 4 | k 4 | **APP**, with `lyrics_panel` |
| `section_kinds::{MarkerKind, SectionKind}` | 2 | a 2 | **SP**, as a kind on `Marker`/`Region`. Otherwise the kind comes from keyflow's section types (Q6) |
| `keyflow::time_signature::TIME_SIGNATURES` | 1 | a 1 | Use `keyflow` directly |

The engine client also uses `session::load_selection`, `modes`, `ruler_lanes`,
`sync::net` and `presence`. Those uses move with the engine work (step 9), not
with the views.

### 1.2 `session-engine-client` (`features/engine/client`, 20k lines): what the views use

The kit re-exports these modules and the views reach them as `crate::X`.

| Module (main items) | Uses | Crates | Destination |
|---|---|---|---|
| `view` (`EngineView`, `ItemNow`, `LiveFrame`, `PerfFrame`, `TrackPerf`, every `*Frame`/`*Ask`) | 10 | k, a | **DV**: the protocol |
| `view_client` (`ask`, `is_linked`, `playhead_of`, `marks`, `live`, `perf`, `jobs`, `LinkedAudio`, `show_edit`) | 16 | k 12, a 4 | **DV**: the link and replica (native) |
| `window_queue` (`Action`, `Attached`, `queue`, `take_window`, `attach`) | 19 | k 13, a 6 | **DV** |
| `wake` (`Waker`, `register`) | 3 | k | **DV** |
| `changes` (`Change`, `listen`) | 3 | k | **DV** |
| `engine` (`Edit` 54, `Transport` 15, `Move`, `Applier`, `apply_event`, `Reading`, `Meters`, `meter_fraction`, `Watch`, `FxWatch`, `METER_TRACKS`, `db_to_gain`) | 149 | k 54, a 43, m 52 | **DV**. `Edit` and `Transport` are the edit and transport wire. `db_to_gain` and `meter_fraction` are plain math and go to **KIT** |
| `song_model` (`StudioSession`, `Planner`, `patch_with`, `Resync`, `fetch`) | 40 | k 23, a 15, m 2 | **DV**. `StudioSession` is renamed `SongView`. `Planner` and `plan_rows*` take DV's `plan::Rows`, never `dynamic_template` |
| `num` (`coord` 145, `index`, `narrow`, `quantise`) | 220 | m 154, a 51, k 15 | **DV** (89 lines, used by the engine and the views) |
| `edit_state` (cursor, time selection) | 6 | k, a | **DV**: selection |
| `edit_actions::publish` (selection publish) | 1 | a | **DV**: selection |
| `options` (`LOCKING`, `SNAP`, `GRID`, `RAZOR`, `AUTO_CROSSFADE`, `GROUPING`) | 21 | k, a, m | **DV**. They are REAPER-style editing options and are DAW-generic |
| `hit_overlay` (`current`, `CompSpan`, `Overlay`, `TakeMark`, `Stretch`) | 48 | a | **DV**: overlays |
| `poi` | 6 | k, a | **DV**: overlays |
| `track_regions` | 5 | k, a | **DV**: overlays |
| `kit_gate` | 5 | a | **DV**: overlays |
| `drum_edit::is_edited` | 1 | a | **DV**: overlays |
| `drum_settings::current` | 1 | a | **DV**: overlays |
| `midi` (`Wave`, `Previews`, `Note`, `Child`, `fold_*`) | 64 | a 60, k 4 | **DV** (song-model drawing data). `fold_*` goes to **ARR** |
| `wave_detail` (`Source`, `Detail`, `generation`) | 11 | a | **DV**: `Detail` appears in an `EngineView` signature |
| `wave_pool::spawn` | 3 | a | **KIT** (native rayon pool) |
| `lanes` (`speaks_for`, `playing`, `expanded`, `folder_view`, `COMP`, `EDIT`) | 26 | a 24, k 2 | **DV** (song model) |
| `automation` (`take_curve`, `lanes_height`, `Lane`, `Kind`, `LEVELER`) | 29 | a 28 | The wire (`Lane`, `Kind`, curve data) goes to **DV**. The layout and editing helpers go to **ARR** |
| `automation_actions::MenuFacts` | 1 | a | **DV** |
| `leveler` (`Stage`, `Dial`, `with_item`, `set_dial`) | 31 | a | `Stage` and `Dial` data go to **DV** (next to `WireLeveled`). The editing helpers go to **ARR**. The writes are **ACT** |
| `gain_stage::{clear_live, stage_live}` | 2 | a | **ACT** |
| `stretch::stretch` | 1 | a | **DE** ("the engine's arithmetic") |
| `folds` (`show_cuts`, `toggle`, `generation`) | 37 | a 25, k 8, m 4 | **KIT**: the window's own choice. Its `dynamic_template` use is replaced by DV `plan` |
| `plan` (`Rows`, `Kinds`) | 17 | a 10, m 6, k 1 | **DV** `plan`: rows, kinds, `Audience`, scene id. Resolving a scene against `dynamic-template` stays in the engine |
| `settings::Settings` | 13 | m 10, k 3 | **KIT**. `Audience` comes from DV |
| `zoom` (`Request`, `Command`, `Target`, `History`) | 41 | a 37, k 4 | **KIT**, because `keys` uses it too |
| `take_colors` (`TakeColors`, `swatch`) | 25 | a | **KIT** (naming colours, no engine) |
| `jobs` (`Running`, `start`) | 9 | k 7, a 2 | **DV** (`JobsFrame`) |
| `stream_set` (`version`, `play_waiting`, `shown`, `listening`) | 9 | k 6, a 3 | **DV**: media still arriving is DAW-generic. Q7 |
| `comp` (`SHOW_LANES`, `SWIPE_COMP`, `lane_step`) | 4 | a 3, k 1 | **ACT** ids. `lane_step` goes to **ARR** |
| `instrument_tracks` (`Instrument`, `ADD`) | 2 | a | **ACT** |
| `organize_edit::Button` | 1 | a | **ACT** (the toolbar's button list comes from the action's meta) |
| `page` (`PageOut`, `ViewArg`, `SpanOut`) | 3 | k | **DV** |
| `actions` (`run` 24, `off_the_window` 14, `meta`, `list`, `undone`, `PLAY_*`, `UNDO`, `REDO`, `SAVE`, `GO_TO_START`) | 75 | k 56, a 19 | `run`, `list`, `meta`, `undone` and the DAW-generic ids go to **DV**. `DockRequest`, `docked`, `request_dock`, `DOCKS`, `MODE` and `shown_mode` go to **APP** (the app's docking) |
| `open` (`runtime` 19, `traced_blocking` 15, `current_song`, `facade_blocking`, `switch_song`, `block_on_engine`) | 50 | k 26, a 22, m 2 | The runtime and blocking helpers and `current_song` go to **DV** (link). `switch_song` is **ACT** |
| `arrange_actions::gesture` | 1 | a | **ARR** |
| `notes_actions::{NOTES_WRITE, NotesArg}` | 2 | k (→ MIDI) | The `NotesArg` type goes to **DV**. The write is **ACT** |
| `setlist` (`Setlist` 22, `ReadOnlySet`) | 27 | k | **APP**: only the app panels use it. `SetFrame` stays in DV |
| `collab` | 19 | k | **APP** |
| `presence` | 2 | k | **APP** |
| `audio_mode` | 13 | k | **APP** |
| `song_modes` | 2 | k | **APP** |
| `stream_quality` | 3 | k | **APP** |
| `render_ahead` | 6 | k | **APP** |
| `projects` | 1 | k | **APP** |
| `open_progress` | 1 | k | **APP** |
| `prepare` | 1 | k | **APP** |
| `play_return` | 3 | k | **APP** |
| `studio_engine` | 4 | k | **APP** |
| `web_audio` | 2 | k | **APP** |

Not used by the views: `routes`, `slices`, `song_bars`, `song_chart`, `body`,
`open_web`, `open_native`. They stay with the engine.

### 1.3 `session-engine`: 64 direct uses plus 26 re-exports (`engine` feature)

| Use | Crates | Destination |
|---|---|---|
| The 26 `pub use session_engine::X` re-exports (`automation_actions`, `collab`, `comp`, `open`, `actions`, `web_engine`, `studio_engine`, `live_host`, `rig_actions`, `drum_samples`, `mix_chains`, `take_rank`, `vocal_level`, `reference_play`, `render_ahead`, `stream_set`, `setlist`, `poi`, `prepare`, `page`, `track_regions`, `projects`, `organize_edit`, `instrument_tracks`, `edit_actions`) | k | **DEL**. The in-process engine is attached by the app through a DV link. Five of them (`drum_samples`, `mix_chains`, `take_rank`, `vocal_level`, `reference_play`) have no users at all |
| `web_engine::{EngineRef, Listening, WebSource, …}` | k 41, a 1, m 1 | **APP** (`web_host`). The `WebArrangement(engine: EngineRef)` and mixer-panel props take a DV link handle instead |
| `live_host::{hosted, last_error}` | k 2 | **APP** (`collab_bar`) |
| `rig_actions::RigView` | k 3 | **APP** (`browser`) |
| `web_actions::{press, extend}` | k (`press.rs`) | **DV** `actions::run` |
| `view::ItemNow` | a 4 (tests only) | **DV** |
| `settings`, `plan`, `open`, `folds`, `gain_stage`, `mix_export`, `prepare`, `engine::apply_now` | t 32 | **APP**. daw-view-tools stays in studio: it is the bench for Studio's engine |

### 1.4 `editor` (features/editor, 57k lines): 90 uses

| Symbol | Uses | Crates | Destination |
|---|---|---|---|
| `paint::{chrome, paint, stack, keys, theme, interaction, text, canvas, demo}`, `Editor`, `ExpressionDoc`, `audio::*` (`Alignment`, `AudioSession`, `align*`, `retime`, `TakeConfig`, …), `doc::*`, `rows::*`, `actions::takes`, `mouse::*`, `Viewport`, `Tool`, `Mode`, `Edit`, `UndoAsk`, `RowSpace`, `StripLane`, `memagic`, `daw::{to_doc, to_content}` | ~70 | k (`expression.rs`, `editor_view.rs`), t 2 | **MIDI**, together with those two kit modules |
| `cursor::Cursor`, `paint::cursor::{SIZE, draw}` | 8 | k (`tool`, `window_cursor`) | **KIT**. The pointer glyphs are shared, so they move into the kit and the editor imports them from there |
| `paint::theme::{TEXT_BRIGHT, PITCH_CLASS}`, `rows` | 3 | k (`window_cursor`, `skin/export`) | **DT** binding fields (pitch-class colours as theme tokens) |
| `paint::keys::is_pending` | 1 | k (`keys.rs`) | **KIT** gets a "text entry pending" registry; the editor registers with it |
| `kit::{LaneRole, kit_role}` | 7 | a | **DE**: drum material roles beside editing's per-material profiles. Fallback: **DV** `lanes` (Q5) |
| `crate::editor_view::picked` | 1 | a | **KIT** slot: the arrangement emits "item picked for editing" and the app wires that to midi-editor |

### 1.5 `daw-ui` (features/ui/daw-ui, 37.9k lines): 229 uses

| Symbol | Uses | Crates | Destination |
|---|---|---|---|
| `studio::project` (`Project`, tracks, items), `studio::ProjectRef`, `studio::RowsRef` | 95 | all | **DV** song model. It moves out of daw-ui before daw-ui moves (step 6) |
| `studio::folded` | 18 | a 17, k 1 | **ARR**. If the engine uses it too, **DV** |
| `studio::lanes` (CSS colours), `studio::panel` (TCP geometry) | 9 | k, a, m | **KIT** |
| `theming::Theme`, `theme::{Color, Chrome, Theme}`, `theming::Color` | 37 | all | **DT**. The implementation goes to **KIT** |
| `art::{paint, geometry, mixer_controls, collapse, vector_controls}` | 60 | all | **KIT** (daw-ui moves there) |
| `controls::{VolumeWidget, Collapse, PanAnchor, record_input_name}` | 6 | k, m | **KIT** |
| `components::folders` | 4 | m 1, t 3 | **KIT** |

### 1.6 `timeline` (features/timeline, 6k lines, no in-repo deps): 103 uses

All 65 items it exports (`snap`, `grid`, `razor`, `gestures`, `input`,
`mouse`, `rows`, `hit`, `drag`, `scroll`, `camera`, `cursor`, `select`,
`clipboard`, `time`, `actions::{cursor, grid, view}`) are used by a 78 and k
25. The whole crate goes to **KIT** as decided. daw#51 proposes `daw-timeline`
in the daw repo instead (Q2).

### 1.7 The smaller crates

| Crate and symbols | Uses | Crates | Destination |
|---|---|---|---|
| `editing::scope::{Scope, Node, Span, Span::between, profile_of}` | 15 | a | **DE**: the edit's selection scope is the contract the edit actions take. `profile_of` is read through **ACT**/DV |
| `editing::{groove, written}` (re-exported) | 0 | k | **DEL** the re-exports |
| `patch_list::*` (`PatchList`, `StudioProfile`, `Layered`, `validate`, `find_album`, fixtures, …) | 19 | a (`patch_list.rs`, optional feature) | **APP**. The Patch List view is a Studio feature: `arrangement/src/patch_list.rs` moves to a studio extension |
| `app_shell::ui::*` (`PerformerScreen`, `PerformDock`, `NowBand`, `playback::*`, `Screen`, `Insets`, `FormFactor`, `use_panel_registry`, …) | 25 | k (`performer`, `compact`, `shell`, `collab_bar`) | **APP**, with those panels |
| `app_shell::ui::form::touch_device` | 1 | k (`touch.rs`) | **KIT**. Inline it (around 10 lines, a media query) or take it as context |
| `dynamic_template::scenes::{Audience, scene, scenes, follow::in_mode, is_pair_half}` | 16 | k, m, t | **DV** `plan`: `Audience` and scene ids. The resolution stays in the engine |
| `dynamic_template::visibility::{normalize_key, groups}` | 3 | k, a | **DV** `plan`: the engine sends the group names |
| `editor_state::*`, `editor_view::DecorationSource` | 9 | k (`chart_editor`) | **APP** |
| `utils::{keyboard, prefs, paths}` | 10 | k, m | Already in the daw repo. Keep |
| `keybinds::{proto::MouseModifierContext, Profile, load_profile_dir, embedded}` | 8 | k, a | studio-adapters (Q4) |

## 2. The transitive studio crates

| Crate | Size | Used by | Recommendation |
|---|---|---|---|
| `audio-controls` | 9.6k | daw-ui only | **Moves to the daw-view-kit repo with daw-ui.** Its only deps are daw-proto and dioxus |
| `dock` | 10k | daw-ui (3 calls in `panel_registration`/`test_panels`), app-shell, session, the engine | **Cut it from daw-ui.** Move daw-ui's REAPER panel registration (`panel_registration.rs`, `test_panels.rs`, feature `reaper-test-panels`) to studio-adapters' `daw-reaper-dioxus-ext`, or to studio. dock stays in studio |
| `lyrics-ui` | 1.6k | session | **Stays in studio (APP)**, with `lyrics_panel` |
| `transport-ui` | 1.1k | session | **Stays in studio (APP)** with `transport_bar`. If the owner wants the transport bar in the kit, it moves to the kit repo instead (it depends only on dioxus and lucide). Q3 |
| `dynamic-template` | 25.6k | the views, patch-list, session, the engine | **Stays in studio** (it uses daw-reaper and dawfile-reaper). The views stop using it once DV's `plan` replaces it |
| `patch-list` | 1.5k | arrangement-view, kit (optional) | **Stays in studio.** Its view moves out of arrangement-view (step 4) |
| `app-shell` | 4k | kit (the app panels), session-desktop, the engine | **Stays in studio.** It leaves the kit when the panels do (step 3) |
| `editing` | 2.6k | arrangement-view, kit, the engine | **Splits.** `scope` (and the `kit` roles) go to `daw-edit`. The algorithms (quantize, groove, level, comp) stay with the engine (fleet repos rhythm/pitch, daw#50) |
| `session` (studio) | large | everything | **Dissolved** as section 1.1 says, and by the engine work for the rest |
| `session-runtime` | — | the engine, the client | Not used by the views. It goes with the engine |

## 3. Steps

Each step is one PR and leaves Studio building (`cargo check --workspace`, the
wasm check and `just test`). Steps marked **⏸ engine** touch the source of
`session-engine` or `session-engine-client` and must wait for the three
in-flight engine branches to merge. **▶ now** steps can start immediately.

1. **▶ now: Prune dead deps.** Remove `studio` from the three view crates'
   manifests. Remove `dep:studio` from editor's `daw` and `audio-daw` features
   (check that `reaper` still compiles). Remove the kit's unused re-exports
   (`editing::groove`, `editing::written`) and the five unused
   `session_engine` re-exports.
2. **▶ now: Cut dock from daw-ui.** Move `panel_registration` and `test_panels`
   behind the REAPER extension (studio-adapters PR plus a rev bump), or into a
   studio crate.
3. **▶ now: Take the app panels out of the kit.** Create a studio crate,
   `studio-panels`, under `apps/studio` or `extensions/`, and move the
   section-0 list into it: `shell`, `web_host`, `chart_*`, `collab_*`,
   `compact`, `setup`, `record_view`, `transport_bar`, `progress`,
   `performer`, `lyrics_panel`, `navigator`, `mix_guide`, `versions`,
   `browser`, `loading`, `app_menu`, `window`, the kit's `dock`, `rails`
   (which goes to the mixer later, in step 8). This removes app-shell,
   editor-state, editor-view, patch-list, setlist, collab, audio_mode and most
   of `session::ui` from the kit. Studio's extensions import those modules
   from the new crate.
4. **▶ now: Take the app's features out of arrangement-view.** Move
   `arrangement/src/patch_list.rs` to a studio extension. Replace `crate::editor_view::picked` with
   a kit `EditTarget` slot that the app fills.
5. **▶ now: Create the midi-editor crate (still inside studio).** Move
   `expression.rs` and `editor_view.rs` into `features/editor` behind a `view`
   feature, or into a sibling crate that will become the repo. Move the
   pointer glyphs (`editor::cursor`, `paint::cursor`) into the kit; the editor
   then depends on the kit.
6. **⏸ engine: Create `daw-view` in the daw repo.** It gets the modules marked
   DV in 1.2, plus `daw_ui::studio::project`/`ProjectRef`/`RowsRef`, plus a new
   `plan` (`Rows`, `Kinds`, `Audience`, scene id, group names) that replaces
   the `dynamic_template` uses. Then the studio side: session-engine-client
   re-exports daw-view's modules under their old paths, so no call site
   changes. In this one PR the daw repo gains the crate, studio bumps the
   daw rev, and the client shrinks to a façade.
7. **⏸ engine: Route edits through the Action List.** `gain_stage`, `comp`,
   `instrument_tracks`, `organize_edit`, `switch_song`, `notes write`,
   `leveler` writes and the docking `actions` become ACT calls by id. Add an
   app-side test asserting that every id a view names is in the engine's
   Action List.
8. **⏸ engine: Move the kit-, MIX- and SP-destined modules.** `folds`,
   `settings`, `zoom`, `take_colors`, `wave_pool` and the drawing helpers of
   `automation`, `leveler`, `midi`, `lanes` and `comp` move into the kit or ARR.
   `MixPhase` and `rails` move to mixer-view. `presence` and section kinds go to
   the session primitive (session PR first, then studio takes it by git).
   `scope` and `kit` roles go to `daw-edit` (daw PR first, then a rev bump).
9. **⏸ engine: Delete the kit's `engine` feature.** session-desktop,
   daw-view-tools and the web host build an in-process `LocalServer` link
   and give it to the DV client. Remove every `session_engine` dep from the
   view crates. This step also takes the engine's own remaining `session::`
   uses (`load_selection`, `modes`, `ruler_lanes`, `sync::net`).
10. **⏸ engine: Rename `session-engine` to `studio-engine`.** Do it after step
    9 so the rename touches no view crate.
11. **▶ once 2 is merged (independent of the engine): Create `daw-theme` in the
    daw repo (daw#52).** It gets the spec from `skin::schema()`/`skin::control`
    and the loader. The kit implements it. The theme packs
    (studio-fts-theme, studio-reaper-theme) move out of
    `features/engine/ui/themes`.
12. **Open the daw-view-kit repo.** It takes daw-ui, audio-controls, timeline
    and the kit (plus transport-ui if Q3 says so), at a tag. Studio drops
    them from its workspace and takes them by git. Gate: `cargo tree` for the
    kit shows no studio-repo source. Steps 1–5 can bring daw-ui, timeline and
    audio-controls over earlier, since they have no engine deps once dock is
    gone. The kit itself must wait for step 9.
13. **Open the arrangement-view, mixer-view and midi-editor repos**, each
    depending on daw-view, the kit repo and daw-edit. Studio takes them by
    git. A `[patch]` with paths for co-development, never committed, as
    AGENTS.md describes.
14. **Remove `features/session` from studio** once nothing depends on it.

## 4. Risks

### Dependency cycles

- **editor and the kit.** Today the kit depends on editor (for `expression`)
  and editor depends on daw-ui. After step 5 the editor (MIDI) depends on the
  kit, and the kit must not name the editor again. The guard: the kit's
  manifest has no `editor`.
- **arrangement-view and midi-editor.** The arrangement must not depend on
  the editor. `editor::kit` roles go to daw-edit, and the editor panel
  reaches the arrangement only through the app's slot (step 4).
- **daw-view and the daw facade.** `song_model` builds on `daw-proto` and
  `daw-control`, which are in the same repo, so this is fine. daw-view must
  not depend on `daw-standalone`, but the client does today (`bootstrap`),
  so the in-process bootstrap stays in studio (step 9).
- **`[patch]` and the www host.** The view repos get daw by `rev` and studio
  gets them by git. Every repo must pin the same daw, architect and frame
  revisions, or the types won't unify (the same rule as `[patch]` in
  AGENTS.md). Do the bumps in the order daw → kit → views → studio, like
  `just bump-architect`.
- **session-engine-client's dual API.** Each re-exported module exists in both
  the engine and the client and must keep the same signatures until step 9.
  A change on either side in the in-flight branches would break the
  re-export façade. This is the main reason the ⏸ steps wait.

### wasm and Blitz

- **wasm.** daw-view must build for wasm32. `view_client`, `wave_pool`,
  `open`'s tokio runtime and `rayon` are native-only today and need the same
  `cfg` split they have now. The web path keeps `architect::platform` for
  timers. Every new repo needs the wasm check in its CI, not only studio.
- **Blitz.** The kit, the views and the editor render under Blitz, so the
  rules in AGENTS.md apply: inline styles only, no external stylesheets, no
  `position: fixed`. `studio::lanes` (CSS colours) and the committed Tailwind
  sheets must be exported as bytes from the owning crate. `include_str!`
  across repos breaks.
- **Root `App`s.** The root components take no props, so the injected slots
  (`EditTarget`, current mode, mix phase, docking) arrive through
  `use_context_provider`, not props.
- **Builds.** kache keys must stay stable: no `env!("CARGO_MANIFEST_DIR")` in
  the moved code, and test fixtures go through `utils::paths`.
- **Cost of a change.** Splitting into repos makes every view change a
  cross-repo PR and a rev bump. Plan for the `[patch]`-with-paths dev loop
  (`stack-overrides.py`) to cover the new repos.

### Open questions for the owner

1. **Q1 `Mode`.** Should Studio's work modes (Organize, Record, Edit, Mix,
   Master, Live…) be a daw-view contract type, or stay app-only, with the views
   given narrower facts (for example "show comp lanes", "show the mix rail")?
2. **Q2 timeline.** The target layout puts timeline in daw-view-kit; daw#51
   puts it in daw as `daw-timeline`. Which one? The engine client uses
   `timeline` (`edit_state`, `zoom`, `play_return`). If it lives in the kit
   repo, daw-view cannot use it: the selection types would have to be DV's
   own, or timeline would have to be in daw.
3. **Q3 transport bar and transport-ui.** Is the transport bar part of the
   kit, so another DAW's adapter gets it, or part of Studio's chrome?
4. **Q4 keybinds.** `keybinds` lives in studio-adapters, the REAPER and
   format adapters' repo. Can the views depend on it, or should its host-neutral
   parts (`MouseModifierContext`, profiles) move to daw or to
   architect-input?
5. **Q5 drum roles and edit scope.** Should `editor::kit` roles and
   `editing::scope` go to `daw-edit`? It is the natural home, but that crate
   is the edit-algorithm contract, so the owner should confirm.
6. **Q6 section and marker kinds.** Do the session primitive's
   `Marker`/`Region` get a kind, or does the arrangement read kinds from
   keyflow?
7. **Q7 streaming state.** Streamed-song state (`stream_set`, `Listening`) is
   Studio's own. Is it in daw-view as DAW-generic "media still arriving",
   or app-injected?
8. **Q8 app panels.** Where do the app panels from step 3 live: one
   `studio-panels` crate, or one extension each (shell, chart, lyrics,
   performer…)? The plan assumes one crate first and a split later.

## Owner decisions (2026-11-24)

These answer the open questions above and supersede the defaults in the tables.

- **Q1 `Mode`:** Studio's, not a `daw-view` type. Live, Edit, Mix and the other modes are Studio's: a mode only calls into the views and shows or hides parts of them (the arrangement's lanes, panels). The views expose what can be shown or hidden; Studio maps each mode onto that. The 37 `modes::Mode` uses in the views become those view settings.
- **Q2 timeline:** split by what it holds. Real state (what is selected, the clipboard, the edit model shared between views) goes to daw (`daw-timeline` / `daw-view`, daw#51). Pure UI (gestures, zoom, camera, scroll, mouse mapping, drawing) goes to daw-view-kit. Decide module by module in step 8.
- **Q3 transport bar:** Studio's chrome; it stays in studio with `transport-ui`.
- **Q4 keybinds:** the views don't depend on `keybinds`; the app passes the bindings in.
- **Q5:** edit scope and drum roles go in `daw-edit`.
- **Q6:** marker and region kinds go in the session primitive.
- **Q7:** streaming state is passed in by the app, not part of `daw-view`.
- **Q8:** one `studio-panels` crate for now.
