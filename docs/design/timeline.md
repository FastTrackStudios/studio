# `timeline` — one editing engine for the arrangement and the Editor

> "We want the same mouse handling, zoom logic, camera, grid, everything
> logic in places like the arrangement and editor."

Status: design, 2026-10-13. Nothing here is implemented yet.

The line numbers are from the working tree on that date. The Editor is
partway through being reorganised into `interact/`, `model/`, `editor/`,
`drums/` and `pitch/`, and the paths below use that new layout. Paths are
relative to these roots unless they are written out in full:

- arrangement: `apps/session/daw/src/`
- Editor: `features/editor/src/`
- shared crate: `features/editing/src/`

## 0. The shape of the answer

Each of the two views has its own camera, zoom, grid, cursor, mouse map,
drag state machine, selection and undo. Each one is better than the other
at roughly half of them. `features/editing` already holds the parts both
views use: the adaptive grid arithmetic, razor areas, marquee and wheel
rules, and the zoom drag. The views currently agree on those parts and on
nothing else.

Plan: rename `editing` to **`timeline`**, then move the better of each pair
into it, one concern at a time. The end state has these layers:

- **`timeline` core.** Time-by-rows interaction with no renderer, wasm-safe
  and `#![forbid(unsafe_code)]`. It depends on nothing heavier than `std`,
  and optionally `facet` and `input-proto` behind features. It contains:
  - the camera and zoom (history, magnets, contextual zoom)
  - the grid and snapping (tempo-map aware)
  - the cursor and time selection
  - selection, marquee and razor
  - the mouse map, tools, gesture classification and the drag engine
  - wheel, pinch and fling
  - the clipboard container
  - the undo *transaction protocol*
- **Surfaces.** The arrangement and the Editor implement a handful of
  traits: what a row is, what an item is, how time maps to beats, and what
  their own verbs do. They keep their painting (vello/anyrender), their
  documents and their stores.
- **Views stay views.** `timeline` returns numbers, such as the camera,
  gridlines, hit results and intents. Each view draws them its own way.

The core does not get a single shared "document". The two sides edit
different things: engine-side REAPER-shaped `Edit`s over items, and a
local `ExpressionDoc` over notes. `timeline` produces **intents** such as
"move these ids by dt and drows" or "trim this edge to t", and each surface
applies them with its own verb. The razor already works this way: one
rectangle, with a different verb on each side.

---

## 1. Inventory

Each concern below gives the arrangement (A), the Editor (E) and what
`editing` (X) already provides, followed by the differences that matter.

### 1.1 Rows and layout

**A.** A row is one planned visible track, as `(daw_proto::Track, depth)`.
- Planned in `studio.rs:479 plan_rows` / `:489 plan_rows_with`, which runs
  folder visibility, `plan::apply_scene` and `folds::apply`.
- Row slot = the track's height plus the automation lanes under it
  (`layout.rs:138 Layout::slot_of`).
- Height rules: the stored height, else `Layout.default` (32), floored at
  `min` (26) (`layout.rs:148 height_of`).
- `Arrangement` (`arrangement.rs:178`) records geometry in content space:
  - `offsets` holds cumulative row tops, `rows + 1` entries (:248)
  - `track_h` (:251)
  - `row_at(content_y)` is a `partition_point` binary search (:834)
  - also `row_box` :750, `lanes_box` :782, `row_band` :817 (× `zoom_y`),
    `visible_rows` :926
- Fixed lanes (takes, COMP, EDIT) are **fractional bands inside a row**
  (`ItemBox.band`, :1780; `slot_band` ~:4830), not rows of their own.
- Folders (`folds.rs`) and expanded lanes (`lanes.rs:183`) are global
  `HashSet` state, and views replan when `folds::generation()` changes.
- Rows are drawn top-down. Heights vary per row. Vertical zoom is a
  multiplier `zoom_y` applied to all of them.
- Below `tcp::BAND_BELOW` (11 px) a row is drawn as a coloured band.

**E.** A row is an integer in a `RowSpace` (`model/rows.rs:562`):
- The spaces are `Pitch` (0..127), `Drums(DrumMap)` (a lane index),
  `Strings(StringTuning)` and `Bands(SliceBands)`.
- Every row is the same height: `VerticalCamera.px_per_row` (continuous).
- Rows run upwards, so a higher pitch is drawn higher.
- `RowFold` (`interact/camera.rs:132`, `MAX_FOLD = 32`) hides rows. A
  hidden row collapses onto the visible row above it, and fractions are
  kept (`slot` / `row`). It is built by `Editor::refresh_fold`
  (`editor/workspace.rs:153`) for one-handed drum pieces.
- **Stack** (multi-track):
  - `Workspace::stack(height, active_boost, min_row) -> Vec<StackRow>`
    (`model/tracks.rs:852`) gives each lane a floor, shares the spare
    height by weight, and overflows into `Editor.stack_scroll`.
  - `Workspace::row_at` is at :942.
  - Each lane has its own `VerticalCamera` (`Editor.lane_cameras`). The
    horizontal camera is shared (pinned by
    `multitrack::the_horizontal_camera_is_shared_and_stays_that_way`).
- Kit lanes come from `drums/kit.rs` (`LaneRole`, `fold_roles`).

**X.** Nothing. Razor rows are a bare `i32`, and `gestures::Span` takes
"continuous row positions" that each surface defines.

**Differences that matter.**
- Variable height against uniform height.
- Top-down against bottom-up.
- One vertical scroll against a stack of per-lane vertical cameras.

Both sides are covered by **a row layout built from per-row heights (in
row units) plus a per-view vertical scale**, with orientation and folding
as properties of the layout.

### 1.2 Camera and zoom

**A.**
- The horizontal camera is `scroll_x` (zoomed px) and
  `pps = studio::PPS (40) × zoom_x`. The vertical camera is `scroll_y`
  and `zoom_y`. These live in `widget.rs:209 View`.
- The panel owns them as Signals (`panel.rs:171 ArrangementPanel`) and
  writes `View` every frame (`frame_tick` :748). `Viewport` is at
  `arrangement.rs:951`.
- Anchoring:
  - `panel.rs:130 zoom_about(at, scroll, was, to)` keeps the pointer's
    point fixed for the wheel and trackpad pinch.
  - The zoom drag anchors to its press point.
  - Key steps (`+`/`-` = ×1.25/×0.8) anchor to the middle of the lanes.
  - Touch pinch anchors to the midpoint of the fingers.
  - Nothing anchors to the edit cursor.
- Limits (`panel.rs:566 limits`, `:555 extent`):
  - zoom-out stops when the whole session fills the frame
  - zoom-in stops at `ZOOM_IN` (20 px per sample at 48 kHz; `y` × 40)
  - scroll is clamped to the extent
- `zoom.rs` (complete):
  - `Command`: ToggleTracks, FitTracks, Project, Selection,
    ToggleSelection, Items, Back, Forward, Step. `of(id)` maps REAPER/SWS
    ids.
  - `Request`: Frame, Open, Back, Forward, Shape, ScrollBy, Scale, Pinch.
  - `Target {zoom_x, zoom_y, scroll_x, scroll_y}`
  - `frame(now, Frame, time, rows) -> Target` (:173)
  - `History::go` (:204): a 64-deep back/forward stack, where a toggle
    pressed again at the framed view goes back
- Framing commands are the `z` tree (`widget.rs:2062 zoom_request`):
  - `z t` selected tracks (toggles)
  - `z v` all rows
  - `z x` the whole project
  - `z s`/`z z` the time selection, else the selected items
  - `z f` the items
- `fit_on_open` (:3466) uses `opening_floor`.
- No animation.
- **Two zoom drags exist:**
  - `panel.rs:661 Drag::Zoom` is the live one. It is incremental per move
    (`factor(moved)`), and **down zooms the rows in** ("as REAPER's zoom
    tool").
  - `arrange_edit.rs:1097 zoom_press` / `zoom_move` / `zoom_release` wraps
    `editing::gestures::ZoomDrag`, where **up zooms in** and Alt does a
    sweep box. Outside tests nothing calls it: it is tested but dead.

**E.**
- `interact/camera.rs`: `Camera {t0, units_per_px, vertical:
  VerticalCamera {center, px_per_row}, fold: RowFold}`.
  - `x(t)`, `t_at(x)`, `y(row, vp)`, `pitch_at`
  - `zoom_time_about(anchor_t, f)` (:311), `zoom_pitch_about` (:318),
    `pan_px` (:324)
- **One clamp**, `constrain(Bounds, vp)` (:339):
  - Time is open, with a screen of slack past each edge and a
    `max_zoom_out` cap.
  - Rows are closed: no empty space beyond the row range, and a minimum
    px-per-row (`Bounds`: 3.5..400 px per semitone).
- `reset_view(content, vp, cushion, pad, fold)` is the `V` key. Pinned by
  `core::v_returns_to_the_same_camera_from_anywhere` and
  `repeated_zoom_out_converges_on_reset_view`.
- **Magnets**, via `Influence {camera, weight}` and
  `blend(base, &[Influence])` (linear for positions, log-space for
  scales):
  - `edge_magnet`
  - `reset_tail` / `reset_progress`
  - `pitch_focus`
  - `deep_zoom_center`
- Used by `editor/navigation.rs`:
  - `zoom_in_at` (:196) zooms both axes about the pointer and then blends
    the edge, pitch-focus and deep-centre magnets.
  - `zoom_out_at` (:239) blends pitch-focus and the reset tail.
  - Every camera write ends in `settle_camera()` (:305), which runs
    `constrain` and then `grid.refit`.
- **Contextual zoom** (`interact/memagic.rs`, a port of FeedTheCat's
  MeMagic):
  - What it does depends on the region under the pointer:
    - NoteArea: Smart span plus scroll to the anchor
    - Piano: fit the item plus fit the notes in view
    - Ruler: keep plus highest
    - CcLane: keep plus lowest
    - elsewhere: keep plus fit the notes in the item
  - Span algorithm: Shepard inverse-distance weighting.
    - Each note's (length + capped gap) is weighted by
      `1/distance^smoothing`.
    - span = weighted average × `target_notes` (20).
    - Floored so that the nearest note is visible (×2.5).
    - Optionally snapped to 1/2/4/8 bars.
    - Placed at `alignment` (0.5).
    - "restrict" slides the window into the item.
  - Vertical: fit the row range, with at least `min_rows` (8) rows and at
    most 32 px per row.
  - Anchor time: playhead, then pointer, then view centre
    (`ui/roll.rs:69`).
- `interact/zoom.rs` `SmartZoom` / `HorizontalMode` / `VerticalMode` is an
  **older duplicate** of MeMagic. It is still reachable from `f`/`Shift+f`
  (`paint/interaction.rs:2822`) and from the toolbar.
- Units: horizontal is **document units**, either `TimeBase::Ppq` (ticks)
  or `Frames` (`model/doc.rs:31`). Vertical is rows.

**X.**
- `gestures::ZoomDrag`: exponential, `ZOOM_GAIN` 200 / 800 fine, right/up
  zooms in, Alt sweeps.
- `zoom_factor`.
- `wheel()`.

**Differences that matter.**
- The time units differ: seconds on A, ticks or frames on E.
- A's vertical is a scale over pixel heights, E's is px-per-row about a
  centre row.
- E's single `constrain` beats A's scattered limits/extent clamps.
- A has zoom history and framing commands, and E has neither.
- E has magnets, contextual zoom and reset-view, and A has none of them.
- The vertical direction of the zoom drag disagrees inside A itself.

### 1.3 Grid

**A.**
- `widget.rs:438` holds `grid: editing::grid::Adaptive` (default), and
  `FINEST = 1/16` (`widget.rs:81`).
- Drawing is `ruler/bars.rs:67 grid(...)`, in this order:
  1. the quantize grid of `grid_in_force` (Edit mode, per region)
  2. else the `adaptive.fit` subdivisions
  3. beats if ≥ 6 px
  4. region lines
  5. bars
- **Bug:** the lane grid is built from `Bars::at(bpm)`, one tempo in 4/4
  (`widget.rs:724`). The ruler (`ruler::ruler` with `scene.tempo()`) walks
  the full tempo map through `ruler/timeline.rs:45 Timeline::beats`.
- `grid_in_force.rs`:
  - `InForce {default, spans: Vec<(u64 µs, u64 µs, u32 div)>}`, where the
    most specific region wins
  - set every frame in Edit mode from `drum_settings` and
    `marks_strip::quantize_regions` (`widget.rs:4443`)
- `bar_step.rs` pages N bars of an instrument (`SESSION_STEP`).
- The grid **does not drive snapping** (§1.12).

**E.**
- `interact/tools.rs:437 Grid`:
  - `division` is the ceiling, a fraction of a whole note (default 1/16)
  - private `fitted`
  - `triplet` / `dotted` (mutually exclusive)
  - `enabled`
  - `adaptive: editing::grid::Adaptive`
- Methods:
  - `step(upb)` (:503)
  - `snap(t, origin, upb)` (:533) rounds to a step from the document
    origin
  - `refit(bar_px)`, `coarser`, `finer`, `set_division` (1/128..1),
    `label`
- Constant `Editor.bpm` and `beats_per_bar`. The host's real bar lines
  are in `doc.bars` and are used only by `page_bars` / `frame_bars`.
- **Bug:** `refit` hands `Adaptive::fit` the *bar* width, but `fit`'s
  `finest` is a fraction of a whole note. The two agree only in 4/4.

**X.**
- `grid::Adaptive::fit(finest, measure_px)` (`grid/mod.rs:205`) and
  `Density`, with ceiling semantics (§ "The setting is a ceiling").
- Tests pin coarsening, spacing, ordering, power-of-two scaling, fixed,
  degenerate cases, ceiling and round-trip.

**Differences.**
- E's `Grid` holds the user's settings (division, triplet, dotted,
  enabled).
- A has quantize-grid regions and tempo-map walking.
- Both assume one tempo for anything except the ruler numbers.

### 1.4 Cursor, playhead and time selection

**A.**
- `cursor.rs`:
  - `Playhead` (:34): extrapolates between engine reports at the
    transport rate, capped at 250 ms
  - `Span {start, end}` (:118): `between`, `is_meaningful` > 10 ms
  - `Edit {at, selection}` (:157): `click` clears the selection, `drag`
    selects and puts the cursor at the start
  - globals `publish` / `current` / `ask` / `take_asked`, so actions such
    as `TIME_SELECTION_SET` can reach the cursor
  - the cursor trail look
- `play_return.rs`: `Stop {Stay, Return}` and `Locate`, REAPER's "stop
  returns to the start". A locate during play becomes the new start.
- The time selection is made on the **ruler** (drag). A drag on an empty
  lane is a marquee.
- No loop region in the view; there is only an engine flag.

**E.**
- `Editor.playhead: Option<f64>` is both the edit cursor and the play
  cursor:
  - `cursor()` falls back to the left edge of the view
  - `move_cursor`, `move_cursor_extending` (hjkl)
- `Editor.time_selection: Option<(f64, f64)>` comes from `Drag::TimeSelect`
  (`paint/interaction.rs:1709`), which snaps unless Shift is held.
- Ruler: click = MovePlayhead, drag = TimeSelect.
- In the stack, the transport playhead is the host's `playhead_secs`
  signal.
- `interact/cursor.rs` is the **pointer glyph** (`Cursor::for_action`),
  not the edit cursor. It is the counterpart of A's `tool.rs icon_for`
  plus `window_cursor.rs`.

**X.** Nothing.

### 1.5 Razor

**A.**
- `Editor.razor: editing::razor::RazorSet` and `RazorDrag {gesture, snap,
  on_click}` (`arrange_edit.rs:190–217`).
- `razor_pressed` (:532) runs first in `press`.
- A zero-width draw falls back to `ClickInstead` (select or seek).
- Verbs:
  - Delete → `razor_delete` (:1591) → `razor_carve` (:1502): split at
    t0/t1 and delete the inside, as one undo step
  - `Ctrl+N` comps take N (`comp_razors` :2342)
- No moving or copying of contents: moving an area moves the rectangle.
- Folded rows expand through `targets()`.

**E.**
- `interact/razor.rs`:
  - `carve`, `peek`, `delete_contents`
  - `move_contents(dt, drows, copy, insert)`, `stretch_contents`
  - `reverse_*`, `invert_pitches`, `set_velocity`, `clear_lane`
- `editor/razor_ops.rs` `razor_op` (:24) makes one undo step over every
  area. **Bug:** it calls `begin_gesture` even when nothing changes.
- Razor key mode (`paint/interaction.rs:2698–2721`):
  - r, Ctrl+r, v, x, d, s, u, f
  - sticky i / h / l: insert, and axis lock via `RazorAxis`
  - arrows nudge by a grid step or a bar; Shift resizes
  - Escape removes the areas first, then the mode
- The menu (`interact/menu.rs:141`) and the keys agree (tested).

**X.**
- `razor`: `RazorArea`, `RazorSet::add` (merges on the same rows), `at`,
  `RazorGesture` (draw/grab/edge, relative to the press), `Released`,
  `RazorAxis`.
- Both sides already use it, so this concern is **already converged on
  geometry**.

### 1.6 Marquee and drag state machines

**A.** `arrange_edit::Editor` (:158) is the state machine.
- Entry points: `press(hit, keys, scene, effects)` (:332),
  `moved(at, pps, bpm, keys)` (:727), `release(...)` (:838), plus
  `razor_moved`, `marquee_moved` and `marquee_released`.
- Output is `Effect {Send(Edit), ReRecord, Transport, Playhead, Refused}`
  (:62).
- Threshold: `|Δt × pps| > gesture::SLOP` (3 px), horizontal only (:737).
- **No vertical or cross-row item moves.**
- **Modifiers are latched at press** (:398–404).
- A refused edit snaps back with a 0.16 s animation (:238–273).
- Edits fan out to folded or grouped mics through `targets` / `grouped`.
- Marquee: `MarqueeDrag {from, to, mode, base}` in seconds × session
  units, `Span::touches` + `combine`. A click clears the selection and
  seeks.
- Routing: `widget.rs` `event()` (:3336) sends hit-marker, ruler-band and
  swipe gestures first. `took()` (:3817–4133) then tries automation,
  take-volume, TCP and the volume dial before `editor.press`.
- `gesture.rs Gestures` (:67) is used by the mixer only:
  - a click is decided on release
  - a double click is 400 ms
  - `FINE = 0.25`

**E.** `paint/interaction.rs`.
- `enum Drag` (:19) has about 35 variants.
- `pointer_down` (:485) tries, in order: `separator_press` →
  `handle_press` → `MouseMap::resolve_for` → `run_action` (:567, every
  `Action` listed explicitly) → `legacy_pointer_down`.
- Then `pointer_move` (:1700) and `pointer_up` (:2303).
- `Axis::Commit` locks to the dominant axis after `DEAD_PX = 4` (:337).
- Preview runs against a base: `apply_live` plus `revert_gesture`, and
  some drags recompute from a base captured at press.
- **The roll never produces Click or DoubleClick.** `pointer_down` maps
  left to Drag, so:
  - a "click" on a note is a MoveNote drag that selects
  - a click on empty canvas is a Replace marquee within SLOP
  - Click/DoubleClick bindings are reached only by the stack
    (`DOUBLE_MS = 400`, `paint/stack/interact.rs:53`)
- The stack (`paint/stack/interact.rs`, `press` :434 … `key` :724) emits
  `HitGesture`s for the host, which is already intent-shaped.

**X.**
- `SLOP`.
- `Marquee`, `MarqueeMode {Replace, Add, Toggle}`, `combine`,
  `Span::touches` (any overlap, edges included).
- `ZoomDrag`.

### 1.7 Mouse-modifier map

**A.** `mousemap.rs`:
- A hard-coded `match` (`verb` :129–211) over
  `input::proto::MouseModifierContext` (`features/input/src/proto/context.rs:45`),
  plus `Custom("razor-area")` and `Custom("razor-edge")`.
- `Mods {shift, ctrl, alt}`, `Gesture {Click, DoubleClick, Drag}`.
- `resolve` returns `Bound {action, snap}`, where
  `snap = !shift && options::SNAP` (:124).
- Not serialised. The doc comment promises `MouseProfileConfig`.

**E.** `interact/mouse.rs`:
- **A table of `Binding` rows**: `MouseMap {name, bindings}` (:393).
- `ModKey(u8)` uses REAPER's bit layout (:328), with `notation()` giving
  `<S-C-A->`.
- Gestures: Click, DoubleClick, Drag, RightClick, MiddleClick.
- `resolve` (:440) tries an exact match, then the unmodified binding, then
  None.
- `resolve_for(.., tool)` (:464): the armed tool claims only the
  unmodified gestures it lists.
- `set`, `for_context`.
- Presets: `fts()` (default), `reaper_like()`, `drums()`, `riffer()`,
  `lyrics()`, chosen per mode.
- `set_host_overlay` (:417) lets the REAPER host overlay
  `reaper-mouse.ini`.
- `Action::is_edit()` (:279) drives undo snapshots, and `ignores_snap()`
  is at :311.

The default bindings, side by side:

| where | gesture | arrangement | Editor (`fts`) |
|---|---|---|---|
| empty | drag | marquee | marquee |
| empty | Shift+drag | marquee add | marquee add |
| empty | Ctrl+drag | razor | razor |
| empty | Alt+drag | — | insert note, drag carries it |
| empty | click | set edit cursor | (Replace marquee within SLOP = deselect) |
| body | drag | move item | move note(s) |
| body | Alt+drag | **copy** | **copy** |
| body | Ctrl+drag | **razor** | **velocity** |
| body | Ctrl+Alt+drag | **slip contents** | **move, no snap** |
| body | Shift+drag | move, **snap off** | move **one axis** (also snap off) |
| body | click / Ctrl+click | select / toggle | (drag that selects) / toggle (stack only) |
| edge | drag | trim | resize edge |
| edge | Shift+drag | trim, snap off | **stretch selection** |
| edge | Ctrl+drag | razor | resize, no snap |
| fade | drag / Ctrl+drag | fade length / shape | — |
| ruler | click / drag | edit cursor / time selection | playhead / time selection |
| razor area | drag / click | move area / remove | **move contents** / remove |
| razor area | Shift+drag | — | move area only |
| razor edge | drag | resize | resize (Shift: stretch contents) |

Shift as "ignore snap" is universal on A. On E it is universal for the
snap itself (`!mods.shift`), but Shift also picks a *different verb* on
the body and on edges.

### 1.8 Tools and hit testing

**A.**
- `tool.rs:66 Tool {Map, Zoom (held z), Pan (held middle)}`. A non-Map
  tool owns the mouse.
- `hit.rs:81 arrangement(...) -> Hit {target, context}`, where
  `Target ∈ {Mode, Ruler, Track, Lane, Item{row, index, zone, seconds},
  Empty}`.
- Item geometry is `Arrangement::item_at` (`arrangement.rs:1511`):
  - the volume dial comes first
  - then fade handles (top 10 px band, 6 px grab)
  - then edges (5 px)
  - then the body, with the last-drawn item winning
- Razor layering is `Editor::context_at` (`arrange_edit.rs:310`): inside
  an area means RAZOR_AREA, within 6 px of an end means RAZOR_EDGE.
- Automation points go through `automation_draw.rs:110 Spot` and
  `automation_edit.rs`, **outside the mouse map**.
- Pointer glyphs: `tool.rs:175 icon_for`, `hovering`, `dragging`; and
  `window_cursor.rs` rasterises the glyphs.

**E.**
- `interact/tools.rs:15 Tool`: Select, Pen, Curve, Eraser, NoteDraw,
  NoteErase, Zoom, Razor, Velocity. `claims()` is at :66.
- `Hit` (:169) is one of `Note{id, zone}`, `NoteEdge{id, start_edge}`,
  `ZoneSplit`, `CurvePoint`, `Empty{t, pitch}`.
- `hit_test` (:209) ranks split, then edge (6 px), then body, then curve
  point (5 px). The whole row band belongs to the note.
- `context_at` (`paint/interaction.rs:421`) works in this order:
  1. ruler / keys
  2. CC
  3. razor (6 px edge)
  4. `hit_test`
- `interact/handles.rs` holds the Vovious-style handles on the note (strip
  handles beat the body).
- `interact/multitool.rs` holds the transform zones over the selection's
  bounding box.
- Pointer glyphs: `interact/cursor.rs Cursor::for_action` plus
  `paint/cursor.rs` (painted, because Blitz has no `cursor:url`).

### 1.9 Selection

**A.**
- `Editor.selected: HashSet<String>` holds item GUIDs, which can be the
  synthetic `folded:` ones. It is mirrored to the engine with
  `Edit::SelectItem` and friends.
- Track selection is a `selected` flag on `Track`.
- Automation points have no persistent selection.

**E.**
- `Selection {notes: Vec<NoteId>, points: Vec<(NoteId, Dimension, t)>}`
  (`interact/tools.rs:307`), keyed by stable id.
- `gesture_targets` (:404) means "the selection if any, else the note
  under the pointer". This is pinned by
  `core::a_drag_belongs_to_the_selection_not_the_note_under_the_pointer`.

### 1.10 Clipboard

**A.** There is none. Copying is only Alt+drag (`spread_item_copy`
:3071), which predicts the GUIDs `{guid}-copy@{t}`.

**E.** `model/clipboard.rs Clipboard {notes, origin_row}`:
- `copy_from` rebases to t = 0 and the lowest row to 0, and keeps the old
  contents when nothing resolves.
- `placed(t, row)`.
- A paste lands at the playhead and mints fresh ids.

### 1.11 Undo

**A.**
- The engine brackets edits REAPER-style: `bracket()` / `as_one_step`
  (`arrange_edit.rs:3008/3019`) wrap a gesture's sends in
  `Edit::BeginUndo(label)` … `EndUndo(label)`.
- Labels are "Edit item", "Copy item", "Delete razor contents" and
  similar.
- `SESSION_UNDO` / `SESSION_REDO` (`actions.rs:1754/1764`) first try
  per-peer document undo (`project_file::undo`, the Loro doc), and fall
  back to `daw::Projects::undo`.
- The window predicts edits on its `Project` copy and then resyncs.

**E.** `model/edit.rs`:
- `Edit` (about 45 variants), `apply -> bool`.
- `History {past, future: Vec<ExpressionDoc>, limit}` (:1085) is
  **snapshot** based:
  - `apply` pushes only if something changed
  - `begin_gesture` pushes unconditionally
- `apply_live` bypasses history, and `revert_gesture` restores the base.
- One history per track (`HISTORY_LIMIT = 10`).
- `Action::is_edit()` decides whether a press begins a gesture.

### 1.12 Snapping and magnets

**A.** One rule, copied three times (`arrange_edit.rs:615`, `:741`,
`:788`): `round(t / (60/bpm)) × (60/bpm)`.
- It snaps to the **quarter note at one constant tempo**. It ignores the
  visible grid, the tempo map and the quantize regions.
- Item move snaps the **item start, absolutely**: `snap(x0 + dx)`.
- Trims snap the moved edge.
- Slip snaps the source offset.
- Razor draw snaps both corners. Razor move snaps the delta,
  `snap(t) − snap(from)`.
- Fades, the marquee and automation never snap.
- No magnets. Butted edges within 1 ms ride along on trims
  (`arrangement.rs:1605 edges_at`).

**E.** `Grid::snap` against `doc.start`.
- **Relative** for moves: MoveNotes rounds the *delta* to steps, so an
  off-grid note keeps its offset (`paint/interaction.rs:1810–1822`).
  RazorDrag works the same way.
- **Absolute** for resize, stretch, razor create and edges, time select,
  playhead and insert.
- Pitch snaps to the tuning (`ed.snap_pitch != shift`).
- No item magnets; magnets are a camera feature only.

**X.** Nothing. `RazorGesture::moved` takes a `snap` closure.

### 1.13 Keyboard and action binding

**A.**
- `keys.rs`: `Keys` wraps `input::InputProcessor` (a keymap trie with
  modes and sequences). The profile comes from `FTS_INPUT_PROFILE`, else
  the embedded `fasttrackstudio` profile, else `BUILTIN`.
- `action_of(id)` (:167) maps REAPER ids to `keys::Action`.
- `widget.rs:2600 acted` → `actions::through_list` (the global
  `architect::action::ActionList`) → `Editor::key` (`arrange_edit.rs:1292`).
- `which_key.rs` is the popup.

**E.**
- `interact/actions.rs` declares `#[architect::actions]` groups
  `FTS_EDITOR_*` (grid, cursor, chord, razor, velocity, view, takes).
- `run(ed, id)` (:646).
- Raw keys go through `paint/interaction.rs:2760 key_down`:
  - tools s/w/d/e/p/x
  - grid 1/2/t
  - f / Shift+f zoom
  - Ctrl+Z/Y
  - Delete
- Which-key comes from the `input` crate sequences.

Both run on architect actions and the `input` keymap. Neither has a
*shared* action id for a shared verb: "zoom in" is a different action on
each side.

### 1.14 Scrolling

**A.**
- `panel.rs:712` calls `editing::gestures::wheel(-dx, -dy, zooming,
  shift)`:
  - plain wheel scrolls both axes
  - Shift scrolls sideways
  - `z` zooms time, `z` + Shift zooms rows
- Trackpad pinch zooms time about the pointer. The middle button pans.
- Follow playhead (`panel.rs:967 follow_cursor`) **pages**: the cursor is
  placed 5 % from the left once it leaves the view, and the view turns a
  `bar_step` page when one is on screen.
- No autoscroll during a drag: `Editor::dragging()` (:1183) exists "for
  the edge autoscroll" but nothing reads it.

**E.**
- Roll: `paint/interaction.rs:2592 wheel`, after `scroll::notches`.
  - The `input` crate's `ScrollBindingTable` maps plain to vscroll, Shift
    to hscroll and Ctrl+Shift to nudge time.
  - Holding `z` zooms **both** axes.
  - `PAN_GAIN = 140`, `ZOOM_DIVISOR = 3`.
- In Session, `expression.rs:825` already routes the embedded editor's
  wheel through `editing::gestures::wheel`, so the **same editor answers
  the wheel differently standalone and in Session.**
- Stack: hscroll, vscroll on `stack_scroll`, zoom on time only.
- No autoscroll and no follow-playhead.

### 1.15 Touch

**A.**
- `touch.rs`:
  - `Fling` (:71): 325 ms decay
  - `Speed`
  - `pinch_step` (:230): per-axis spread clamped 0.8–1.25, an axis
    ignored under 40 px, carries the midpoint
  - `Fingers`
- `widget.rs:3605 fingered`:
  - one finger scrolls, and a release flings
  - two fingers make a per-axis pinch with pan
  - a finger on the playhead handle scrubs
  - a sideways swipe on the TCP sends `Shape`
- `swipe.rs` is take-lane swipe-comping, a surface verb.

**E.** None. There is only touchpad normalisation in the wheel.

---

## 2. The `timeline` crate's API

### 2.1 Crate rules

- Package `timeline` at `features/timeline` (`git mv features/editing`).
- No dioxus, blitz, vello, anyrender, kurbo, tokio or daw. Everything
  runs on wasm, and **no global state** (A's `options::SNAP`, `folds`,
  `cursor::publish` and the editor's `HOST_OVERLAY` stay in the
  surfaces).
- Deps: `std` only by default. The features are:
  - `facet`: derive `Facet` on settings that are persisted (grid, mouse
    map, zoom history)
  - `input`: an adapter from `input::proto::{MouseBindDef,
    MouseProfileConfig, MouseModifierContext}` to `MouseMap`
  - `actions`: `#[architect::actions]` for the shared verbs
- f64 everywhere. Time is in the **surface's own units**. Rows are `i32`
  indices in the surface's own row space, which is what `RazorArea`
  already uses.
- Coordinates come in three kinds, and every function says which it
  takes:
  - screen px `(x, y)`, relative to the lanes' origin
  - content `(t, r)`, where `r` is a continuous row position
  - row index `i32`

### 2.2 Module map

```
timeline/
  time.rs        TimeMap, ConstantTempo, TempoMap, Beat
  rows.rs        RowModel, RowLayout, RowFold, Orientation, Stack
  camera.rs      TimeView, RowView, Camera, Viewport, Bounds
  zoom/          history.rs (ZoomHistory, Framing), magnet.rs (Influence, blend, …),
                 contextual.rs (MeMagic), drag.rs (ZoomDrag — from gestures)
  grid/          adaptive.rs (Adaptive, Density — as today), grid.rs (Grid),
                 regions.rs (GridRegions — grid_in_force), lines.rs (GridLine iterator)
  snap.rs        Snapper, SnapRule
  cursor.rs      EditCursor, TimeSpan, Playhead, Locate/StopMode
  select.rs      Selection<Id>, MarqueeMode, Marquee, combine, Span
  razor/         as today (RazorArea, RazorSet, RazorGesture, RazorAxis) + RazorTarget trait
  input.rs       Mods, ModKey, Button, Pointer, Clicks (click/double classifier), SLOP, AxisLock
  mouse.rs       Context<S>, Verb<V>, Gesture, Binding, MouseMap<S, V>, presets
  tool.rs        ToolState<T>, Quasi, ToolClaims
  hit.rs         ItemHit, EdgeHit, Hit<Id, Z>, hit()
  model.rs       ItemModel, ItemSpan, PointModel, Surface
  drag.rs        Interaction<S>, Intent<S>, Phase, DragTrack
  scroll.rs      wheel, Wheel, EdgeScroll, Follow, Fling, pinch_step
  clipboard.rs   Clipboard<T: Clip>
  undo.rs        Transaction, UndoSink, SnapshotHistory<D>
  actions.rs     (feature) TIMELINE_* action group
```

### 2.3 Time

```rust
/// How the surface's time unit maps to musical time. Arrangement: seconds
/// over the project tempo map. Editor: ticks (Ppq) or frames over its
/// doc's tempo (constant today; `doc.bars` when the host supplies them).
pub trait TimeMap {
    /// Quarter-note beats from the timeline origin at time `t`.
    fn beats_at(&self, t: f64) -> f64;
    /// Inverse of `beats_at`.
    fn time_at_beats(&self, beats: f64) -> f64;
    /// The measure containing `beats`: its start (in beats), its length
    /// (in beats), and its 1-based number as the user counts it
    /// (`ruler::bars::from_song` — no bar 0).
    fn measure_at(&self, beats: f64) -> Measure;
    /// Seconds, for snapping in ms and for the arrangement's sample limit.
    fn seconds_at(&self, t: f64) -> f64;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measure { pub start: f64, pub beats: f64, pub number: i64, pub denom: u32 }

/// One tempo, one signature — what both sides assume today.
pub struct ConstantTempo { pub units_per_beat: f64, pub beats_per_bar: f64, pub origin: f64, pub units_per_second: f64 }

/// A piecewise tempo map — `ruler/timeline.rs::Timeline` and `ruler/bars.rs`
/// generalised: a tempo change carries the beat count, a signature change
/// starts a measure.
pub struct TempoMap { /* sorted points: (t, beats, bpm, num, denom) */ }
impl TempoMap {
    pub fn new(points: impl IntoIterator<Item = TempoPoint>, units_per_second: f64) -> Self;
    pub fn beats_between(&self, t0: f64, t1: f64) -> impl Iterator<Item = Beat> + '_;
}
```

### 2.4 Rows

```rust
/// What a row is. Arrangement: planned tracks (variable height, top-down,
/// folders). Editor: a RowSpace (uniform, bottom-up, folded drum hands).
pub trait RowModel {
    /// Rows in the model's own index space, low..=high.
    fn bounds(&self) -> (i32, i32);
    /// Height of a row in **row units** before vertical zoom
    /// (arrangement: session px at zoom_y = 1; editor: 1.0 per row).
    fn height(&self, row: i32) -> f64;
    /// Rows hidden by folding (they take no height; content on them is
    /// drawn on the row above — `RowFold`'s rule).
    fn hidden(&self, _row: i32) -> bool { false }
    /// Whether an item may be dragged from one row to another
    /// (arrangement: no, today; editor: yes; drums: only to the other hand).
    fn accepts_move(&self, _from: i32, _to: i32) -> bool { true }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation { TopDown, BottomUp }

/// Cumulative offsets for a RowModel — arrangement.rs's `offsets` +
/// `partition_point`, with RowFold folded in. Rebuilt when the model's
/// generation changes; O(log n) lookups.
pub struct RowLayout { /* offsets: Vec<f64>, lo: i32, orientation, fold */ }
impl RowLayout {
    pub fn build(model: &dyn RowModel, orientation: Orientation) -> Self;
    pub fn uniform(lo: i32, hi: i32, orientation: Orientation, fold: RowFold) -> Self; // pitch rows, no Vec
    pub fn top(&self, row: i32) -> f64;          // row units from the layout's top
    pub fn extent(&self, row: i32) -> (f64, f64);// (top, height)
    pub fn row_at(&self, r: f64) -> Option<i32>;
    /// Continuous row position (row + fraction within it) — what marquee
    /// spans and razor rows use.
    pub fn position_at(&self, r: f64) -> f64;
    pub fn total(&self) -> f64;
}

pub struct RowFold { /* as interact/camera.rs:132, moved verbatim */ }

/// Several lanes stacked, each with its own RowLayout and RowView, sharing
/// one TimeView — the Editor's stack (`Workspace::stack`), and the shape
/// the arrangement's fixed-lane bands would take if they became lanes.
pub struct Stack { pub lanes: Vec<StackLane>, pub scroll: f64 }
pub struct StackLane { pub y: f64, pub height: f64, pub weight: f32, pub floor: f64 }
impl Stack {
    pub fn fit(weights: &[(f32, f64 /*floor*/)], height: f64, boost: Option<(usize, f32)>) -> Self;
    pub fn lane_at(&self, y: f64) -> Option<usize>;
}
```

### 2.5 Camera

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport { pub w: f64, pub h: f64 }

/// Horizontal: the Editor's (t0, units_per_px) — unit-agnostic. The
/// arrangement's (scroll_x px, pps) is the same thing: t0 = scroll_x/pps,
/// units_per_px = 1/pps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeView { pub t0: f64, pub units_per_px: f64 }
impl TimeView {
    pub fn x(&self, t: f64) -> f64;
    pub fn t_at(&self, x: f64) -> f64;
    pub fn span(&self, vp: Viewport) -> (f64, f64);
    pub fn zoom_about(&mut self, anchor_t: f64, factor: f64);   // factor > 1 = in
    pub fn pan_px(&mut self, dx: f64);
    pub fn frame(&mut self, t0: f64, t1: f64, vp: Viewport, cushion: f64);
}

/// Vertical: a scroll in row units and a scale in px per row unit.
/// Arrangement: y0 = scroll_y / zoom_y, px_per_unit = zoom_y.
/// Editor: y0 = center − h/2/px_per_row, px_per_unit = px_per_row
/// (BottomUp layouts flip inside RowLayout, not here).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowView { pub y0: f64, pub px_per_unit: f64 }
impl RowView {
    pub fn y(&self, r: f64) -> f64;
    pub fn r_at(&self, y: f64) -> f64;
    pub fn zoom_about(&mut self, anchor_r: f64, factor: f64);
    pub fn pan_px(&mut self, dy: f64);
    pub fn frame(&mut self, r0: f64, r1: f64, vp: Viewport);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera { pub time: TimeView, pub rows: RowView }

/// The one clamp (Editor's `constrain`), parameterised so the arrangement's
/// limits fit: time may be open (slack past the ends) or closed; rows
/// closed (no empty space past the layout) with min/max px per unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub t_min: f64, pub t_max: f64,
    pub time_slack_screens: f64,        // editor 1.0, arrangement 0.0
    pub min_units_per_px: f64,          // arrangement: 1 sample / 20 px
    pub max_zoom_out: ZoomOut,          // Fit (arrangement) | Times(20.0) (editor)
    pub rows_total: f64,
    pub px_per_unit: (f64, f64),        // editor 3.5..400; arrangement (fit, 40)
}
impl Camera {
    pub fn constrain(&mut self, b: &Bounds, vp: Viewport);
}
```

### 2.6 Zoom

```rust
// zoom/history.rs — arrangement zoom.rs, generalised from (zoom_x, scroll_x…) to Camera.
pub struct ZoomHistory { /* back: Vec<Camera> (64), forward, toggled: Option<(Camera, Camera)> */ }
pub enum Framing { Time(f64, f64), Rows(f64, f64), Both((f64, f64), (f64, f64)) }
impl ZoomHistory {
    /// Frame something; `toggle` = pressing it again at the framed view goes back.
    pub fn frame(&mut self, now: Camera, to: Camera, toggle: bool) -> Camera;
    pub fn back(&mut self, now: Camera) -> Option<Camera>;
    pub fn forward(&mut self, now: Camera) -> Option<Camera>;
}

// zoom/magnet.rs — interact/camera.rs, moved verbatim, over Camera.
pub struct Influence { pub camera: Camera, pub weight: f64 }
pub fn blend(base: Camera, inf: &[Influence]) -> Camera;
pub fn edge_magnet(base: Camera, mouse_t: f64, content: Content, vp: Viewport, dead: f64, white: f64) -> Option<Influence>;
pub fn reset_tail(cur: Camera, reset: Camera, tail_start: f64) -> Option<Influence>;
pub fn row_focus(base: Camera, local_r: f64, mouse_r: f64, lw: f64, mw: f64) -> Vec<Influence>; // pitch_focus
pub fn deep_zoom_center(base: Camera, content: Content, onset: f64, max_ppu: f64) -> Option<Influence>;
pub fn reset_view(content: Content, vp: Viewport, cushion: f64, pad: f64) -> Camera;

/// The policy a surface picks for "zoom in/out at the pointer":
/// plain (arrangement today) or magnetic (Editor today).
pub enum ZoomFeel { Plain, Magnetic(MagnetConfig) }
pub fn zoom_at(cam: Camera, at: (f64, f64), factor: (f64, f64), feel: &ZoomFeel, content: Content, vp: Viewport) -> Camera;

// zoom/contextual.rs — memagic.rs over ItemModel spans instead of ExpressionDoc.
pub enum Region { Body, RowHeader, Ruler, Lane, Elsewhere }  // NoteArea/Piano/Ruler/CcLane/…
pub struct Config { /* target_items: 20, smoothing, alignment, min_rows, max_px_per_row */ }
pub fn smart_span(items: &mut dyn Iterator<Item = (f64, f64)>, anchor_t: f64, cfg: &Config) -> Option<f64>;
pub fn contextual(cam: Camera, region: Region, anchor: Anchor, items: &dyn ItemModel<..>, map: &dyn TimeMap, vp: Viewport, cfg: &Config) -> Camera;
```

### 2.7 Grid and snap

```rust
// grid/grid.rs — the Editor's tools::Grid, moved, with the bar/whole-note bug fixed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub division: f64,          // ceiling, fraction of a whole note
    pub feel: Feel,             // Straight | Triplet | Dotted (one enum: they are exclusive)
    pub enabled: bool,          // snapping on (A's options::SNAP, E's grid.enabled)
    pub visible: bool,          // drawn (A's options::GRID)
    pub adaptive: Adaptive,
    fitted: Option<f64>,
}
impl Grid {
    /// Refit to the zoom: `whole_px` = px per WHOLE NOTE (not per bar).
    pub fn refit(&mut self, whole_px: f64) -> bool;
    pub fn effective(&self) -> f64;              // fitted × feel, in whole notes
    pub fn step_beats(&self) -> f64;             // effective × 4
    pub fn coarser(&mut self); pub fn finer(&mut self); pub fn label(&self) -> String;
}

// grid/regions.rs — grid_in_force::InForce, in surface units not µs.
pub struct GridRegions { pub default: Option<f64>, pub spans: Vec<(f64, f64, f64)> }
impl GridRegions { pub fn at(&self, t: f64) -> Option<f64>; pub fn between(&self, t0: f64, t1: f64) -> Vec<(f64, f64, f64)>; }

// grid/lines.rs — what both rulers and lane backgrounds draw.
pub enum LineKind { Bar, Beat, Sub, Region }
pub struct GridLine { pub t: f64, pub x: f64, pub kind: LineKind }
pub fn lines<'a>(grid: &'a Grid, regions: Option<&'a GridRegions>, map: &'a dyn TimeMap, view: TimeView, vp: Viewport) -> impl Iterator<Item = GridLine> + 'a;

// snap.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapRule {
    /// The moved point lands on a line (edges, creation, cursor, time selection).
    Absolute,
    /// The delta is a whole number of steps; off-grid things stay off-grid by
    /// the same amount (Editor note move, razor move).
    Relative,
    /// The thing's start lands on a line whatever the grab offset
    /// (arrangement item move today).
    AnchorStart,
}
pub struct Snapper<'a> { pub grid: &'a Grid, pub regions: Option<&'a GridRegions>, pub map: &'a dyn TimeMap }
impl Snapper<'_> {
    pub fn snap(&self, t: f64) -> f64;                                  // to nearest line in force at t
    pub fn delta(&self, rule: SnapRule, start: f64, raw_dt: f64) -> f64;// what a drag moves by
}
```

### 2.8 Cursor, selection, clipboard

```rust
// cursor.rs — arrangement cursor.rs + play_return.rs, minus painting/globals.
pub struct TimeSpan { pub start: f64, pub end: f64 }               // ordered, ≥ 0
pub struct EditCursor { pub at: f64, pub selection: Option<TimeSpan> }
impl EditCursor { pub fn click(&mut self, t: f64); pub fn drag(&mut self, from: f64, to: f64); pub fn step(&mut self, by: f64, extend: bool); }
pub struct Playhead { /* extrapolating — cursor.rs:34 */ }
pub enum StopMode { Stay, Return }
pub struct Locate { /* play_return.rs:69 */ }

// select.rs — Selection<Id> replaces HashSet<String> (A) and Vec<NoteId> (E).
pub struct Selection<Id: Clone + Eq + Hash> { /* insertion-ordered set */ }
impl<Id: Clone + Eq + Hash> Selection<Id> {
    pub fn set_single(&mut self, id: Id); pub fn toggle(&mut self, id: Id); pub fn add(&mut self, id: Id);
    pub fn clear(&mut self); pub fn contains(&self, id: &Id) -> bool; pub fn iter(&self) -> impl Iterator<Item = &Id>;
    /// "the selection if the grabbed thing is in it, else the grabbed thing"
    pub fn targets(&self, under: &Id) -> Vec<Id>;
}
pub use gestures::{Marquee, MarqueeMode, Span, combine}; // as today

// clipboard.rs — the Editor's Clipboard, generic.
pub trait Clip: Clone { fn start(&self) -> f64; fn row(&self) -> i32; fn shifted(&self, dt: f64, drows: i32) -> Self; }
pub struct Clipboard<T: Clip> { items: Vec<T> }
impl<T: Clip> Clipboard<T> {
    pub fn copy(&mut self, items: impl IntoIterator<Item = T>) -> bool;    // rebased to (0, lowest row); empty keeps old
    pub fn placed(&self, t: f64, row: i32) -> Vec<T>;
}
```

### 2.9 Surface model

```rust
/// A thing on a row with a span — an item, a take, a note, a hit.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemSpan<Id> {
    pub id: Id, pub row: i32, pub start: f64, pub end: f64,
    /// Fraction of the row's height it occupies (fixed-lane bands); (0,1) for a whole row.
    pub band: (f32, f32),
}

pub trait ItemModel {
    type Id: Clone + Eq + Hash + Debug;
    /// Surface-specific zones beyond body and edges (fade handles, the volume
    /// dial, Vovious handles, zone splits). `()` if none.
    type Zone: Copy + Eq + Debug;
    fn visit(&self, t0: f64, t1: f64, rows: (i32, i32), f: &mut dyn FnMut(&ItemSpan<Self::Id>));
    fn span(&self, id: &Self::Id) -> Option<ItemSpan<Self::Id>>;
    /// A zone the core's body/edge test would not find, given the item's
    /// screen rect and the pointer. Called before the edge test (A: volume
    /// dial and fades beat edges; E: handles and splits beat edges).
    fn zone(&self, _id: &Self::Id, _rect: Rect, _at: (f64, f64)) -> Option<Self::Zone> { None }
}

/// Points on curves — automation points (A), curve/CC points (E).
pub trait PointModel {
    type Id: Clone + Eq + Hash + Debug;
    fn near(&self, at: (f64, f64), cam: &Camera, px: f64) -> Option<Self::Id>;
}

pub trait Surface {
    type Items: ItemModel;
    /// Surface verbs the core does not know (slip, fade, velocity, pen, comp…).
    type Verb: Copy + Eq + Hash + Debug;
    /// Surface contexts beyond the shared ones (fade, CC lane, keys, gutter…).
    type Place: Copy + Eq + Hash + Debug;
    fn time(&self) -> &dyn TimeMap;
    fn rows(&self) -> &RowLayout;
    fn items(&self) -> &Self::Items;
    /// Contexts the surface owns, tested before the core's (ruler, keys, TCP, CC).
    fn place_at(&self, at: (f64, f64)) -> Option<Self::Place> { None }
}
```

### 2.10 Input, mouse map and tools

```rust
// input.rs
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ModKey(u8);                       // E's bit layout: S=1 C=2 A=4; Cmd→C upstream
pub enum Button { Left, Middle, Right }
pub const SLOP: f64 = 3.0;                   // Manhattan px; as today
pub const DOUBLE_MS: u64 = 400;
/// Click/double/drag classification decided the way A's gesture.rs does it:
/// a click is known on release; a double is a second press within 400 ms and SLOP.
pub struct Clicks { /* last: Option<(at, ms)> */ }
impl Clicks { pub fn press(&mut self, at: (f64, f64), ms: u64) -> PressKind; }
pub enum PressKind { First, Double }
/// The Editor's Axis::Commit, for one-axis drags.
pub struct AxisLock { pub dead_px: f64 /* 4 */, committed: Option<bool> }

// mouse.rs
pub enum Context<P> {
    Empty, Body, Edge, Zone, Point, Ruler, RazorArea, RazorEdge,
    Surface(P),                              // Fade, CcLane, Keys, LaneGutter, TCP…
}
pub enum Gesture { Click, DoubleClick, Drag, RightClick, MiddleClick }
pub enum Verb<V> {
    // shared — the core runs these itself
    Marquee(MarqueeMode), SetCursor { snap: bool }, TimeSelect { snap: bool },
    Move { rule: SnapRule, snap: bool, axis: AxisMode }, Copy { snap: bool },
    Trim { snap: bool }, Select, ToggleSelect, AddSelect, DeselectAll,
    RazorDraw, RazorAdd, RazorMoveArea, RazorMoveContents { copy: bool, snap: bool },
    RazorResize, RazorStretch, RazorRemove, Pan, ZoomDrag, ZoomSweep, Menu, None,
    // the surface's — the core tracks the drag and hands it over
    Surface(V),
}
pub struct Binding<P, V> { pub context: Context<P>, pub gesture: Gesture, pub mods: ModKey, pub verb: Verb<V> }
pub struct MouseMap<P, V> { pub name: Cow<'static, str>, bindings: Vec<Binding<P, V>> }
impl<P: Eq + Copy, V: Copy> MouseMap<P, V> {
    pub fn resolve(&self, c: Context<P>, g: Gesture, m: ModKey) -> Verb<V>;    // exact → unmodified → None
    pub fn resolve_for<T: ToolClaims<P>>(&self, c: Context<P>, g: Gesture, m: ModKey, tool: T) -> Verb<V>;
    pub fn set(&mut self, b: Binding<P, V>);
    pub fn for_context(&self, c: Context<P>) -> impl Iterator<Item = &Binding<P, V>>;
}
/// Shared rows every preset starts from (§3, "Modifier conventions"): empty ground, razor, ruler.
pub fn common<P, V>() -> Vec<Binding<P, V>>;
#[cfg(feature = "input")]
pub fn overlay<P, V>(map: &mut MouseMap<P, V>, defs: &[input_proto::MouseBindDef], verb_of: impl Fn(&str) -> Option<Verb<V>>);

// tool.rs
pub trait ToolClaims<P> { fn claims(&self, c: Context<P>, g: Gesture) -> bool; }
/// Held quasi-modes override the armed tool: `z` = zoom, middle = pan.
pub enum Quasi { None, Zoom, Pan }
pub struct ToolState<T> { pub armed: T, pub quasi: Quasi }
```

### 2.11 Hit testing and the drag engine

```rust
pub enum Hit<Id, Z> {
    RazorEdge { index: usize, start: bool },
    RazorArea { index: usize },
    Zone { id: Id, zone: Z },
    Edge { id: Id, start: bool, seam: bool },
    Body { id: Id },
    Point,                                     // resolved by PointModel
    Empty { t: f64, r: f64 },
}
pub struct HitConfig { pub edge_px: f64 /* 6 */, pub razor_edge_px: f64 /* 6 */, pub seam_ms: f64 /* 1 */ }
/// Razor first, then items (last-drawn wins), zones before edges before body.
pub fn hit<S: Surface>(s: &S, razor: &RazorSet, cam: &Camera, vp: Viewport, at: (f64, f64), cfg: &HitConfig)
    -> Hit<<S::Items as ItemModel>::Id, <S::Items as ItemModel>::Zone>;

/// What the drag engine asks the surface to do. Every drag has the same
/// lifecycle — Begin on crossing SLOP (never at press: no empty undo step),
/// Live with the TOTAL change from the press, Commit or Cancel.
pub enum Phase { Begin, Live, Commit, Cancel }
pub struct DragTrack { pub from: (f64, f64), pub now: (f64, f64), pub dt: f64, pub drows: i32, pub mods: ModKey, pub fine: bool }
pub enum Intent<Id, V> {
    Move { ids: Vec<Id>, dt: f64, drows: i32, copy: bool, phase: Phase },
    Trim { ids: Vec<Id>, start: bool, dt: f64, phase: Phase },
    Select(Vec<Id>),                            // the whole new selection
    Cursor(EditCursor),
    Razor { set: RazorSet, released: Released },
    RazorContents { area: RazorArea, dt: f64, drows: i32, copy: bool, stretch: bool, phase: Phase },
    Camera(Camera),
    Menu { at: (f64, f64), under: Option<Id> },
    Surface { verb: V, under: Option<Id>, track: DragTrack, phase: Phase },
}

pub struct Interaction<S: Surface> {
    pub camera: Camera, pub grid: Grid, pub cursor: EditCursor,
    pub selection: Selection<<S::Items as ItemModel>::Id>,
    pub razor: RazorSet, pub razor_axis: Option<RazorAxis>,
    pub map: MouseMap<S::Place, S::Verb>, pub history: ZoomHistory,
    clicks: Clicks, drag: Option<DragState<S>>,
    /// Modifiers: the VERB is latched at press (A's rule); the snap bypass
    /// (Shift) is read live (E's rule) — see §3, "Modifier conventions".
}
impl<S: Surface> Interaction<S> {
    pub fn press(&mut self, s: &S, at: (f64, f64), b: Button, m: ModKey, ms: u64, tool: impl ToolClaims<S::Place>) -> Vec<Intent<..>>;
    pub fn moved(&mut self, s: &S, at: (f64, f64), m: ModKey) -> Vec<Intent<..>>;
    pub fn release(&mut self, s: &S, at: (f64, f64), m: ModKey) -> Vec<Intent<..>>;
    pub fn cancel(&mut self) -> Vec<Intent<..>>;
    pub fn wheel(&mut self, s: &S, at: (f64, f64), d: (f64, f64), zooming: bool, m: ModKey) -> Vec<Intent<..>>;
    pub fn pinch(&mut self, s: &S, step: PinchStep) -> Vec<Intent<..>>;
    /// Edge autoscroll while a drag is held near the lanes' edge (new: neither side has it).
    pub fn tick(&mut self, s: &S, dt_ms: f64) -> Vec<Intent<..>>;
    /// The pointer glyph the current state implies (A's tool::icon_for / E's Cursor::for_action).
    pub fn glyph(&self, s: &S, at: (f64, f64), m: ModKey) -> Glyph;
}
```

### 2.12 Scroll and touch

```rust
pub use gestures::{wheel, Wheel, zoom_factor, ZOOM_GAIN, ZOOM_GAIN_FINE}; // as today
pub struct EdgeScroll { pub band_px: f64, pub max_px_per_s: f64 }          // new
pub struct Follow { pub mode: FollowMode /* Page | Continuous */, pub lead: f64 /* 0.05 */ } // panel.rs:967
pub struct Fling { /* touch.rs:71 */ }
pub struct PinchStep { pub sx: f64, pub sy: f64, pub mid: (f64, f64), pub dx: f64, pub dy: f64 }
pub fn pinch_step(a0: (f64, f64), b0: (f64, f64), a1: (f64, f64), b1: (f64, f64)) -> PinchStep; // touch.rs:230
```

### 2.13 Undo

`timeline` defines **the transaction protocol, not a store**:

```rust
pub trait UndoSink {
    /// A gesture has started editing (first Phase::Begin). Label for the menu.
    fn begin(&mut self, label: &str);
    /// It ended and changed something — one step.
    fn commit(&mut self);
    /// It ended and changed nothing, or was cancelled — no step, base restored.
    fn cancel(&mut self);
}
/// The Editor's model/edit.rs History, generic over the document.
pub struct SnapshotHistory<D: Clone> { past: Vec<D>, future: Vec<D>, limit: usize, open: Option<D> }
impl<D: Clone + PartialEq> SnapshotHistory<D> {
    pub fn begin(&mut self, now: &D); pub fn base(&self) -> Option<&D>;
    pub fn commit(&mut self, now: &D);  // pushes only if now != base
    pub fn cancel(&mut self) -> Option<D>;
    pub fn undo(&mut self, now: D) -> Option<D>; pub fn redo(&mut self, now: D) -> Option<D>;
}
```

The arrangement implements `UndoSink` with `Edit::BeginUndo` / `EndUndo`,
which is what `bracket` does today. The Editor implements it with
`SnapshotHistory<ExpressionDoc>`.

---

## 3. Which side wins, and what the other side must change

| concern | winner | why | the other side changes |
|---|---|---|---|
| Rows | **A's offsets + binary search**, generalised; **E's `RowFold`** and **stack** kept | variable heights are the general case (uniform is `RowLayout::uniform`); E's fold and weighted stack are features A lacks | A: `Arrangement.offsets/row_at/visible_rows` → `RowLayout`. E: `VerticalCamera.center` → `RowView`, `Workspace::stack`/`row_at` → `timeline::rows::Stack` |
| Camera | **E's `Camera` / `constrain`** | unit-agnostic, explicit anchors, one clamp; A's clamps are spread over `panel.rs` `limits`/`extent`/`rezoom` | A: `View{scroll_x, zoom_x, …}` + `panel.rs:130 zoom_about` → `Camera`; `limits()` → `Bounds` |
| Zoom history and framing | **A's `zoom.rs`** | E has no back/forward or framing toggle | E gains back/forward and `z` framing |
| Magnets, contextual zoom, reset view | **E** (`memagic.rs`, magnets) | A has nothing | A gains MeMagic on `f` and reset on `V` (as a `ZoomFeel` choice). E **deletes `interact/zoom.rs` SmartZoom** (a duplicate of MeMagic) |
| Zoom drag | **`editing::ZoomDrag`** (relative to press, Alt sweep) | tested on both sides, no drift | A: delete `panel.rs` `Drag::Zoom` *or* `arrange_edit` `zoom_press` (one is dead). Vertical direction: open question Q4 |
| Grid | **E's `Grid` settings + A's regions + tempo walking** | E's has division, triplet/dotted and enabled; A's has quantize regions and the tempo map; neither alone is enough | both: `Grid::refit(whole_px)` (fixes the E 4/4 bug); A: lane grid from `TempoMap` (fixes the one-bpm bug) |
| Snap | **E's rule set** (relative move, absolute edges) + `Snapper` over the grid | snapping to what is drawn, through the tempo map, is correct on both; A's `60/bpm` copied three times is a bug | A: replace the three closures with `Snapper` (behaviour change: snaps to the visible grid). Item move keeps `AnchorStart` unless Q2 says otherwise |
| Cursor and time selection | **A's `cursor.rs` / `play_return.rs`** | separate edit cursor and playhead, extrapolation, stop-return; E conflates them in `playhead` | E: `playhead` → `EditCursor` + `Playhead`; `time_selection` → `EditCursor.selection` |
| Razor geometry | already shared | — | — |
| Razor verbs | **E's set** (move/copy/stretch contents, axis lock, key mode, menu) as the *protocol*; each side keeps its carve | A only deletes and comps | A gains `RazorContents` intents (move or copy contents = split + move items) when ready; key mode moves to `timeline` |
| Marquee | already shared | — | E: start using `Toggle` from the map consistently; A gains Toggle (Ctrl+Shift+drag) |
| Click classification | **A's `gesture.rs`** (decided on release, 400 ms double) | E's roll never produces Click, so its click bindings are dead in the roll | E's roll: `pointer_down` stops mapping left to Drag; Click/DoubleClick become real. See Q3 |
| Mouse map | **E's `MouseMap`** (table, presets, fallback, tool claims, overlay) | data rather than a `match`; can be rebound; already REAPER-indexed | A: `mousemap.rs verb()` → an `arrange()` preset table; `Bound.snap` → `Verb` carries `snap`; `options::SNAP` folds into `Grid.enabled` |
| Tools | **E's armed tool + `claims`**, plus **A's quasi-modes** (held z / middle) | E has real tools; A's held-key modes are the cleaner way to do zoom and pan | A: `Tool::Map` = no armed tool; E: the `z` held zoom in `paint/stack/zoom.rs` becomes `Quasi::Zoom` |
| Hit testing | **shared skeleton** (razor → zone → edge → body), surface `zone()` hook | both already have this order | A: `item_at` becomes `ItemModel::zone` + core edges (5 → 6 px); E: `hit_test` likewise |
| Selection | **E's id-keyed set** (`Selection<Id>`) + `targets` | ids rather than indices on both sides; `targets` is pinned | A: `HashSet<String>` → `Selection<ItemGuid>` (keeps order for "first selected") |
| Clipboard | **E** | A has none | A gains item copy/paste: `Clip` for `ItemSpan` + GUID minting. A new feature, so a late step |
| Undo | **protocol from E** (begin on the first change, preview against a base, one step per gesture); **stores stay separate** | A's engine undo and Loro per-peer undo are not replaceable by snapshots; E's snapshots are right for a local doc | E: begin on threshold, not press (fixes the empty steps in `razor_op`/`begin_gesture`); A: `bracket` implements `UndoSink` |
| Keys and actions | **architect actions + `input` keymap** (both use them) + a shared `TIMELINE_*` group | the same verb should be the same action id on both | both: zoom/scroll/grid/cursor/razor verbs move to `timeline::actions`; surfaces keep `FTS_EDITOR_*` / `SESSION_*` for their own verbs |
| Wheel | **`editing::gestures::wheel`** | already shared by A and by E in Session | E standalone roll: `paint/interaction.rs:2592` uses `timeline::wheel` (z zooms time, z+Shift rows); the `input` `ScrollBindingTable` stays only as an optional override. See Q6 |
| Follow playhead | **A's paging** | E has none | E's stack gains it |
| Autoscroll | **new** | neither has it | both |
| Touch | **A's `touch.rs`** | E has none | E gains pinch and fling via `Interaction::pinch` |

### Modifier conventions (the merged default)

The FTS presets already agree on empty ground: drag is a marquee,
Shift-drag adds to it, and Ctrl-drag draws a razor. They also agree that
Alt on a body copies. So the shared `common()` rows are:

- **Empty:** drag = marquee; Shift = marquee add; Ctrl+Shift = marquee
  toggle; Ctrl = razor draw; Shift+Ctrl = razor add.
- **Ruler:** click = set cursor; drag = time select.
- **Razor area:** drag = move; click = remove; Shift+click = add;
  double-click = delete contents.
- **Razor edge:** drag = resize; Shift = stretch contents.
- **Body:** Alt+drag = copy.

Shift means "no snap" everywhere it is not a verb. The snap bypass is read
live, and the verb is latched at press.

---

## 4. Migration plan

Every step should compile and leave both views working, with the tests
listed for it green. Each step is done as one batch with one `just ci
check` and one `nextest` run, as the batch-before-testing memory asks.

### 4.0 Tests that pin current behaviour

**`features/editing` (inline):**
- grid: `zooming_out_coarsens_the_grid`, `the_lines_are_never_closer_than_asked`,
  `density_orders_as_its_names_do`,
  `a_fitted_division_is_the_old_one_scaled_by_the_factor`,
  `a_fixed_grid_is_left_alone`, `a_view_with_no_width_moves_nothing`,
  `the_setting_is_the_finest_the_grid_ever_gets`,
  `the_grid_comes_back_to_where_it_started`, `custom_spacing_is_in_pixels`
- razor: 8 tests (normalise, half-open, merge, topmost, span, draw,
  move/remove, edge)
- gestures: 5 tests (zoom drag, sweep, wheel, marquee modes, span)

**Arrangement (inline `#[cfg(test)]` in `apps/session/daw/src`):**
- `arrange_edit.rs` (65), including:
  - zoom: `a_sideways_drag_zooms_time_about_the_press`,
    `an_upward_drag_zooms_the_rows`,
    `shift_is_the_fine_control_and_the_press_must_be_on_the_lanes`,
    `an_alt_sweep_frames_its_box_on_release`
  - marquee: `a_marquee_selects_the_items_its_box_touches`
  - razor: `a_razor_areas_edge_resizes_it`,
    `a_ctrl_drag_draws_a_razor_area_and_a_plain_one_does_not`,
    `areas_merge_on_the_same_rows_and_not_across_them`,
    `an_area_slices_the_items_at_both_its_edges`,
    `deleting_an_area_removes_exactly_what_it_covered`,
    `a_press_inside_an_area_moves_it`, `a_click_inside_an_area_removes_it`
  - selection: `a_click_selects_and_ctrl_adds`,
    `a_modified_click_takes_an_item_back_out`,
    `a_plain_click_on_a_selected_item_keeps_it`
  - copy, slip, move: `alt_drag_copies_the_item_instead_of_moving_it`,
    `a_plain_drag_still_moves_rather_than_copying`,
    `ctrl_alt_drag_slips_the_contents_and_leaves_the_item`,
    `a_body_drag_moves_the_item_to_the_beat`, `the_edges_trim`,
    `a_fade_handle_drags_the_fade`
  - undo steps: `a_plain_drag_is_one_undo_step`,
    `a_folded_drag_is_one_undo_step`,
    `deleting_across_several_items_is_one_step`
  - cursor and keys: `the_cursor_moves_by_the_grid_and_the_pointer`,
    `track_steps_walk_the_rows`, `an_unbound_action_falls_through`
  - ruler: `a_press_that_never_moved_changes_nothing`,
    `delete_takes_the_ruler_selection_first`
  - the folded and refusal tests
- `mousemap.rs` (8), all
- `gesture.rs` (9): click on release, double/triple, no jump on start,
  fine, cancel
- `zoom.rs` (6): framing rows and time, tiny-span limit, toggle back,
  back/forward, profile ids
- `cursor.rs` (21): playhead glide and rate, seek, selection
  ordering/non-negative, `clicking_clears_and_dragging_selects`,
  `a_twitch_is_a_click`, trail
- `play_return.rs` (7)
- `layout.rs` (6)
- `row.rs` (9)
- `folds.rs` (6)
- `lanes.rs` (4)
- `tool.rs` (5): glyphs per tool and modifier, edges and fades, drag keeps
  its shape
- `pointer.rs` (5)
- `hit.rs` (1)
- `touch.rs` (2): `a_pinch_spreads_each_axis_on_its_own_and_carries_the_middle`,
  `one_finger_drags_and_a_second_makes_it_a_pinch`
- `swipe.rs` (4)
- `grid_in_force.rs` (2)
- `bar_step.rs` (2)
- `ruler/tests.rs` (10): bars, signatures, tempo changes, zoom ticks,
  song-relative counting
- `widget.rs` (27), including `a_click_under_a_tool_is_the_tools`,
  `a_gesture_begun_before_a_tool_still_finishes`,
  `z_t_asks_the_panel_to_frame_the_selected_track`,
  `z_used_as_the_tool_closes_the_tree_on_release`, the automation-point
  and item tests, and `a_drag_along_a_take_lane_is_a_swipe_and_on_the_comp_is_not`
- `keys.rs` (13)
- `arrangement.rs` (21, drawing)

**Arrangement (`apps/session/daw/tests/`):**
- `editor_view.rs` (the embedded editor)
- `automation_item_edits.rs`, `automation_by_section.rs`
- `comp_lanes.rs`, `split_keeps_audio.rs`, `track_regions.rs`,
  `drum_quantize.rs`, `default_view.rs`, `folder_items.rs`,
  `remote_lanes.rs`

These are integration-level. They are run at the end of the steps that
touch editing verbs (§4 steps 11–14).

**Editor (`features/editor/tests/`):**
- `core.rs` (250), including:
  - zoom and camera: `zoom_pins_the_anchor_under_the_pointer`,
    `scales_blend_geometrically_so_zoom_stays_even`,
    `zooming_out_can_see_past_either_end_of_the_item`,
    `zooming_out_still_stops_somewhere`,
    `v_returns_to_the_same_camera_from_anywhere`,
    `zooming_in_never_drifts_toward_reset_view`,
    `repeated_zoom_out_converges_on_reset_view`
  - smart and contextual zoom: `smart_zoom_*`,
    `contextual_modes_zoom_differently_per_pointer_region`,
    `cursor_alignment_places_the_anchor_in_the_view`
  - selection and drag: `marquee_selects_the_notes_it_covers`,
    `a_drag_belongs_to_the_selection_not_the_note_under_the_pointer`
  - grid: `grid_snapping_uses_the_documents_own_origin`,
    `the_local_grid_reads_out_as_musical_fractions`,
    `an_adaptive_grid_coarsens_as_the_view_zooms_out`,
    `a_fixed_grid_ignores_the_zoom`, `an_adaptive_grid_keeps_its_triplets`
  - razor: `a_razor_slices_notes_at_its_edges_rather_than_selecting_whole_ones`,
    `razor_bindings_do_not_steal_the_plain_marquee_drag`,
    `hit_testing_a_razor_set_finds_the_topmost_area`
  - undo: `undo_restores_zones_notes_and_expression_as_one_step`,
    `a_failed_edit_records_no_undo_step`,
    `edits_are_distinguished_from_navigation_for_undo_grouping`,
    `each_track_keeps_its_own_undo_history`
  - clipboard and commands:
    `pasting_mints_fresh_ids_so_a_double_paste_is_two_phrases`,
    `a_failed_copy_leaves_the_clipboard_alone`,
    `a_command_on_an_unselected_note_acts_on_that_note`
  - `wheel_alternatives_do_what_their_labels_say`
  - the modifier resolution, preset and handle tests
- `mouse_actions.rs` (52): every mapped action, including
  `anchored_zoom_keeps_the_time_under_the_pointer`,
  `a_zoom_holds_the_point_it_was_started_on`,
  `one_axis_commits_to_the_first_direction`, all the razor tool and key
  tests, `a_razor_drag_is_still_one_undo`, and
  `dragging_a_razor_in_many_steps_lands_where_one_step_would`
- `tools.rs` (Blitz DOM, `#[tokio::test]`): plain drag marquees, Shift
  adds, Alt inserts, Ctrl razors, tool claims
- `ui_cursor.rs` (28): glyph follows the resolved action, modifier
  preview, armed tool, locked surface
- `multitrack.rs` (66): the stack, lane floors and scroll,
  `each_lane_fits_its_own_range`, `zooming_one_lane_leaves_the_others_alone`,
  `the_horizontal_camera_is_shared_and_stays_that_way`
- `stack.rs` (15)
- `stack_slip.rs` (11): `held_z_zooms_instead_of_slipping_a_hit_and_release_restores_editing`,
  `held_z_alt_drag_frames_a_time_range_without_adding_hits`
- `mouse_stack.rs` (6)
- `geometry.rs` (12): `the_status_bar_carries_the_grid_and_its_adaptive_setting`,
  `the_middle_button_pans_the_strip_too`
- `ruler_geometry.rs` (7)
- `mode_drums.rs` (8): `moving_a_hit_to_the_other_hand_moves_it_to_that_hand_s_row`,
  `sticking_survives_undo_as_one_step`
- `undo_routing.rs`, `arrange_scroll.rs`
  (`wheel_over_lanes_moves_the_parent_viewport_on_both_axes`),
  `hit_after_mutation.rs`, `cc.rs` (14), `mpe_gestures.rs`, `modes.rs`
- `editor_state.rs` (39):
  `the_camera_is_not_part_of_it`, the per-take view

**What is missing today.** Write these first, in `timeline`, as
characterisation tests for the arrangement's camera:
- A has no test that the wheel zooms about the pointer. `panel.rs`
  `zoom_about` has two unit tests of its own, but nothing tests it
  composed with the clamps in `rezoom`.
- A has no test of `limits`/`extent`.
- A has no test of the live `Drag::Zoom` direction.

*Done (step 2):* `timeline/tests/arrangement_camera.rs` has 20 tests over
a line-for-line copy of the panel's camera arithmetic. One finding for
step 10: A's live `Drag::Zoom` is incremental (each move zooms from the
last, clamped), so travel past a limit is lost and reversing zooms back
at once. `ZoomDrag` measures from the press, so it would first unwind
the overshoot. `the_z_drag_turns_back_at_once_after_hitting_a_limit`
pins A's behaviour. Keep it or change it on purpose.

### Steps

1. **Rename.** `git mv features/editing features/timeline`; set the
   package name to `timeline`; change the workspace entry in the root
   `Cargo.toml` (members line 14, dep line 196). Then
   `s/editing::/timeline::/` in:
   - arrangement: `arrange_edit.rs`, `arrangement.rs`, `widget.rs`,
     `panel.rs`, `expression.rs`, `ruler/mod.rs`, `mousemap.rs`
   - Editor: `lib.rs`, `interact/{tools,razor,actions}.rs`,
     `editor/{mod,navigation,razor_ops}.rs`,
     `paint/{chrome,interaction}.rs`, `ui/toolbar.rs`, `tests/core.rs`,
     `tests/mouse_actions.rs`
   - both `Cargo.toml`s

   `gestures` stays where it is for now. Pins: everything compiles, and
   all the `editing` tests pass under the new name.
2. **Characterisation tests for A's camera** (see "What is missing today"
   above) in `timeline/tests/arrangement_camera.rs`, written against
   `panel::zoom_about` behaviour reimplemented as a fixture. Pins: the new
   tests.
3. **`input.rs`.**
   - Move `ModKey` from `interact/mouse.rs:328` and re-export it from the
     editor.
   - Add `Clicks` (port `gesture.rs` `Gestures` press/release/double
     logic; A's `gesture.rs` keeps its slider-specific `FINE` and
     `dragging_up_increases`).
   - Move `AxisLock` from `paint/interaction.rs:337`.
   - A's `mousemap::Mods` becomes a thin `From<ModKey>`.

   Pins: `gesture.rs` (9), `core` modifier tests,
   `one_axis_commits_to_the_first_direction`.
4. **`time.rs`.**
   - Add `TimeMap`, `ConstantTempo` and `TempoMap`, porting
     `ruler/timeline.rs` `Timeline::beats` and `ruler/bars.rs` `Bars`.
   - A's ruler draws through `TempoMap`.
   - E implements `ConstantTempo` from `Editor.bpm`/`beats_per_bar`/
     `TimeBase`, and `TempoMap` from `doc.bars` when present.

   Pins: `ruler/tests.rs` (10), `ruler_geometry.rs`.
5. **Grid.**
   - Move `interact/tools.rs:437 Grid` to `timeline::grid::Grid`, and have
     the editor re-export it as `tools::Grid`. `triplet`/`dotted` become
     `Feel`.
   - Change `refit(bar_px)` to `refit(whole_px)`. The editor caller
     (`editor/navigation.rs settle_camera`) passes
     `bar_px / beats_per_bar × 4`. **Behaviour change in non-4/4 only.**
     Add a test.
   - Move `grid_in_force.rs` `InForce` to `timeline::grid::GridRegions`
     (surface units; A converts µs to s at its boundary).
   - Add `grid::lines`. A's `ruler/bars.rs:67 grid()` iterates
     `lines(...)` over `TempoMap` (**fixes the one-bpm lane grid**). E's
     `paint/canvas.rs:109` iterates the same lines.
   - A gets a `Grid` (`widget.rs:438`; `FINEST` → `Grid.division`);
     `options::SNAP`/`GRID` read and write `Grid.enabled`/`visible`.

   Pins: the editing grid tests, the `core` grid tests,
   `the_status_bar_carries_the_grid_and_its_adaptive_setting`,
   `grid_in_force` (2), `ruler/tests`.
6. **Snap.**
   - Add `Snapper` + `SnapRule`.
   - A: replace the three `60/bpm` closures (`arrange_edit.rs:615`,
     `:741`, `:788`) with `Snapper::snap`/`delta`. The item move uses
     `AnchorStart`, trims `Absolute`, razor move `Relative`.
     **Behaviour change:** snaps to the drawn grid rather than quarter
     notes. Update `a_body_drag_moves_the_item_to_the_beat` to say "to
     the grid" and keep a quarter-note grid in its fixture.
   - E: `Grid::snap`, `Editor::snap_time` and `paint/interaction.rs:1810`
     go through `Snapper`.

   Pins: the A move/trim/razor tests, `grid_snapping_uses_the_documents_own_origin`,
   `mouse_actions` move tests.
7. **Camera (the Editor first).**
   - Add `TimeView`, `RowView`, `Camera`, `Bounds` and `constrain`.
   - `interact/camera.rs` keeps its public names as wrappers:
     `Camera::t0`/`units_per_px` map to `time`, and
     `VerticalCamera{center, px_per_row}` gets conversion methods.
   - Move `RowFold` to `timeline::rows`.

   Pins: the `core` zoom tests, `multitrack` (66), `stack` (15),
   `editor_state::the_camera_is_not_part_of_it`, `geometry`.
8. **Camera (the arrangement).**
   - `widget.rs:209 View` → `{camera: Camera, play_at}`.
   - `panel.rs` `scroll`/`down`/`zoom` signals hold a `Camera`.
   - `zoom_about` → `TimeView::zoom_about` / `RowView::zoom_about`.
   - `limits`/`extent` → `Bounds` with `time_slack_screens = 0`,
     `ZoomOut::Fit`.
   - `arrangement.rs:951 Viewport` derives `pps`/`scroll_x` from the
     camera for the painters, so painting is untouched.

   Pins: the step-2 characterisation tests, `zoom.rs` (6), `widget`
   framing tests, `arrange_scroll`, `default_view`.
9. **Rows.**
   - Add `RowModel`, `RowLayout` and `Stack`.
   - A: `Arrangement.offsets` / `row_at` / `visible_rows` delegate to a
     `RowLayout` built in `Arrangement::build` (`arrangement.rs:1129`).
   - E: `Workspace::stack` / `row_at` (`model/tracks.rs:852/942`) →
     `timeline::rows::Stack`; the single roll uses
     `RowLayout::uniform(BottomUp, fold)`.

   Pins: `layout.rs`, `row.rs`, `folds.rs`, `multitrack`, `stack`,
   `mode_drums`, `a_y_coordinate_resolves_to_the_lane_under_it`.
10. **Zoom.**
    - Move A's `zoom.rs` `History`/`frame` to `timeline::zoom::history`
      over `Camera`; A's `Command`/`Request`/`of(id)` stay in A as the
      REAPER-id adapter.
    - Move E's magnets and `memagic.rs` to `timeline::zoom::{magnet,
      contextual}`, over `ItemModel` spans. E's `navigation.rs` calls
      them.
    - **Delete `interact/zoom.rs`** (SmartZoom); `f`/`Shift+f` and the
      toolbar use contextual.
    - Retarget the `smart_zoom_*` `core` tests to `contextual` (same
      expectations).
    - Settle the dual zoom drag in A: keep the `ZoomDrag`-based one
      (`arrange_edit.rs:1097`), wire it from `panel.rs:694`, and delete
      `Drag::Zoom`, so that A and E's stack (`stack_slip` held-z tests)
      share one drag. Q4 decides the vertical sign.
    - A gains `V` reset view and `f` contextual zoom (new actions).

    Pins: `zoom.rs` (6), the `core` zoom/smart/contextual tests,
    `mouse_actions` anchored zoom, `stack_slip` held-z, the A zoom-drag
    tests.
11. **Cursor and selection.**
    - Move A's `cursor.rs` `Playhead`/`Span`/`Edit` and `play_return.rs`
      `Locate`/`Stop` to `timeline::cursor`. A keeps painting and the
      `publish`/`ask` globals.
    - E: `Editor.playhead` → `cursor: EditCursor` + `playhead: Playhead`;
      `time_selection` → `cursor.selection`.
    - Add `Selection<Id>`. A: `Editor.selected` → `Selection<String>`.
      E: `tools::Selection.notes` → `Selection<NoteId>`, `points` →
      `Selection<PointKey>`.

    Pins: `cursor.rs` (21), `play_return` (7), `a_click_selects_and_ctrl_adds`,
    `marquee_*`, `a_drag_belongs_to_the_selection_*`, the `chord_box`
    tests.
12. **Mouse map and tools.**
    - Move `interact/mouse.rs` `MouseMap`/`Binding`/`Gesture` to
      `timeline::mouse` as `MouseMap<P, V>`. The editor's `Context`
      becomes `Context<EditorPlace>`, its `Action` becomes
      `Verb<EditorVerb>`, and its presets are built from `common()` +
      editor rows.
    - A: `mousemap.rs verb()` → `fn arrange() -> MouseMap<ArrangePlace,
      ArrangeVerb>` (a table); `resolve` keeps its signature.
    - `ToolClaims` for E's `Tool`; `Quasi` for both.
    - Feature `input`: `overlay` from `MouseBindDef`, which fulfils A's
      `mousemap.rs:1-14` promise.

    Pins: `mousemap.rs` (8), the `core` modifier and preset tests,
    `mouse_stack` (6), `ui_cursor` (28), `tool.rs` (5), `tools.rs`.
13. **Hit testing and the drag engine.** This is the large step, so it
    lands in three PRs:
    - **13a: marquee, razor, time selection and the zoom tool** through
      `Interaction`. These are already shared logic and only the plumbing
      moves. A: `arrange_edit` `razor_pressed`, `marquee_*` and
      `time_drag`; E: `paint/interaction.rs` `Drag::{Marquee,
      RazorCreate, RazorDrag, RazorEdge, TimeSelect, ZoomTool}`.
    - **13b: move, copy and trim** via `Intent::Move`/`Trim` with
      `Phase`.
      - A: `ItemPress`/`moved`/`release` → intents, mapped to
        `spread_item_edit`/`spread_item_copy`. Rows stay put via
        `accepts_move = false`. The snapback stays in A.
      - E: `Drag::{MoveNotes, Resize}` → intents mapped to
        `Edit::MoveTime`/`Transpose`/`Resize`.
    - **13c: click classification.** E's roll uses `Clicks`, so
      Click/DoubleClick bindings reach the roll. Q3 decides Shift on a
      note.
    - Surface verbs (fade, slip, velocity, pen, handles, multitool, CC,
      hits, swipe) stay surface code that receives `Intent::Surface`.

    Pins: all of `arrange_edit` (65), `mouse_actions` (52), `tools`,
    `stack_slip`, `cc`, `mpe_gestures`, `mode_drums`, `widget` (27),
    `hit_after_mutation`, and the integration tests under
    `apps/session/daw/tests/`.
14. **Undo protocol.**
    - Add `UndoSink` + `SnapshotHistory`.
    - E: `model/edit.rs History` → `SnapshotHistory<ExpressionDoc>`.
      `begin_gesture` is now called on `Phase::Begin` (first change), not
      at press, which **removes the empty steps** in `razor_op`. Add a
      test.
    - A: `bracket`/`as_one_step` implement `UndoSink`.

    Pins: the `core` undo tests, `a_failed_edit_records_no_undo_step`,
    `a_razor_drag_is_still_one_undo`, `undo_routing`, the A
    `*_is_one_undo_step` tests, `sticking_survives_undo_as_one_step`.
15. **Clipboard.** Move `model/clipboard.rs` → `timeline::Clipboard<T>`.
    E: `Clip for Note`. A (new): `Clip for ItemSpan` + copy/paste
    actions. Pins: the `core` clipboard tests; new A tests.
16. **Scroll and touch.**
    - Move `touch.rs` `Fling`/`pinch_step`/`Speed` → `timeline::scroll`.
    - Move `panel.rs:967 follow_cursor` paging → `Follow`.
    - Add `EdgeScroll` (new) through `Interaction::tick`; A's
      `Editor::dragging` finally has a reader.
    - E's standalone roll wheel → `timeline::wheel` (Q6).
    - E's stack gains pinch and follow.

    Pins: `touch.rs` (2), `arrange_scroll`,
    `wheel_alternatives_do_what_their_labels_say` (adjusted per Q6),
    `geometry::the_middle_button_pans_the_strip_too`.
17. **Shared actions.** Feature `actions`: a `TIMELINE_*` group (zoom
    in/out/back/forward/frame/reset/contextual, scroll, grid
    finer/coarser/triplet/dotted/adaptive/toggle, cursor step, razor
    verbs), generic over an `Interaction` accessor trait.
    - A: `keys.rs action_of` maps REAPER ids to them.
    - E: `interact/actions.rs` grid/view/razor groups become aliases.

    Pins: `keys.rs` (13), `the_builtin_bindings_reach_the_actions`,
    `the_razor_help_matches_the_razor_keys`,
    `the_razor_menu_and_the_razor_keys_agree`.

**Progress (2026-10-14).** Steps 1–12 are done. Where the code differs
from the plan above:

- **3:** one `Mods` type for both surfaces (`timeline::input::Mods`; the
  two copies were the same struct), not a `From<ModKey>` wrapper.
  `ModKey` converts both ways.
- **4:** the Editor gets `Editor::time_map()` (a `ConstantTempo`). A
  `TempoMap` from `doc.bars` waits for the host to send tempo and
  signature along with the bar lines. Two tempo points at the same time:
  the first wins. That is what the beat walk did; `Bars::at_time` took
  the last.
- **5:** `Grid` has no `visible`; `options::SNAP`/`GRID` are still the
  arrangement's globals. `GridRegions` keeps whole-note divisions
  (`u32`) and takes a `quantum` for comparing edges.
  `grid::lines`/`grid::bars` return times rather than `GridLine`s, and
  the surfaces pick the weights.
- **6:** the arrangement snaps to the grid it draws (its `Grid` fitted to
  the zoom, the lane tempo map, the quantize grid while editing). Its
  tests pin quarter notes (`arrange_edit::on_quarters`).
- **7:** the Editor's `Camera` keeps its fields (t0, units_per_px,
  vertical, fold; about 200 call sites). Its time maths and `constrain`
  delegate to `timeline::camera`, and `VerticalCamera::row_view`
  converts its centred, bottom-up rows. The clamp now works in folded
  slots, which fixes a folded kit scrolling past its top lane.
- **8:** the arrangement's camera is `session-daw`'s `ArrangeCamera`
  (zoom, scroll, down in the panel's terms), whose every move goes
  through `timeline::Camera` and the arrangement's closed `Bounds`. The
  panel's signals stay its storage. The step-2 tests moved there
  (`arrange_camera::tests`) and run against the real code, not a copy.
  `widget::View` and `arrangement::Viewport` are unchanged, so the
  painters are too. Pinch, scale and history targets still compute in
  zoom/scroll (`panel::zoom_about`) until step 10 moves the history.
- **9:** `timeline::rows` has `RowLayout` (cumulative tops,
  `TopDown`/`BottomUp`, `uniform`, `build` from a `RowModel`) and the
  Editor's weighted `stack`/`lane_at`, ported as they were. The
  arrangement's `offsets` are a `RowLayout`; `Workspace::stack`/`row_at`
  call `stack`/`lane_at` (the Editor stays `f32` at its edges). The
  single roll still maps rows through its `VerticalCamera`, which already
  holds the fold; a `RowLayout::uniform` there would add nothing until
  the hit testing moves (step 13).
- **10:** one zoom drag. `ZoomDrag` zooms rows in when dragged down
  (Q4) and gains `rebase`, which drops travel past a limit. The
  arrangement's live drag is a `ZoomDrag` rebased at its limits, which
  keeps its turn-back-at-once feel; the dead `arrange_edit` zoom tool
  (and its unused Alt sweep) is gone. `ZoomHistory<V>` is generic over
  the surface's view (the arrangement's `Target`). MeMagic is
  `timeline::zoom::contextual` over spans; SmartZoom is deleted, and
  `f`/`Shift+f`/Zoom keep their mode pairs (`Modes::PASSAGE`, `WHOLE`).
  **Not moved:** the Editor's magnets (`blend`, `edge_magnet`, …) stay
  in `interact/camera.rs`. They blend the centre-based vertical camera
  linearly in its centre, and over `timeline::Camera` the same blend
  would run in the top row, which is a different average while the
  row height changes. They move when the Editor's camera stores a
  `timeline::Camera`. **Not added yet:** the arrangement's `V` (reset
  view) and `f` (contextual over items). Contextual now takes spans, so
  this is a key binding and an items adapter away; it is new behaviour,
  so it waits for a decision on the keys.
- **12:** `timeline::mouse` is the shared table (`Gesture`,
  `Binding<C, A>`, the exact-then-plain lookup, `MouseMap<C, A>`,
  `ToolClaims`). The Editor's map uses it; the arrangement's `match` is a
  table, checked chord for chord against the old one. Shift flips snap on
  the arrangement (Q3 revised). Q1 revised: Ctrl is the razor on bodies
  and edges as well as empty ground, on both surfaces, and the
  arrangement has an armed Razor tool like the Editor's. Razor areas keep
  their drags on both. `common()` rows are not written as a shared table:
  the two presets now agree on Ctrl, Alt and the razor rows directly, and
  the Editor's Shift+drag on a note (one axis) is still open against the
  arrangement's Shift (flip snap).
- **11:** `timeline::cursor` holds the arrangement's `Playhead`,
  `TimeSpan`, `EditCursor`, `StopMode`, `TransportStep` and `Locate`, with
  their tests. The arrangement aliases its old names, and keeps the
  painting and the globals. `timeline::select::Selection<Id>` is an
  insertion-ordered set (with `targets` and `as_set`); the arrangement's
  `selected` is one. **The Editor is unchanged here.** Its `playhead` is
  both the edit cursor (ruler clicks, hjkl) and Session's transport
  (`expression.rs` writes it every frame), so splitting it decides whether
  the roll draws one line or two, which is a design question. Its
  `selection.notes` stays a `Vec<NoteId>` (about 80 call sites assign or
  clone it into edits). Its "the selection if any, else the note under
  the pointer" differs from `targets`' "the selection if the note is in
  it".

After step 13 the two `Editor` structs (`arrange_edit::Editor` and
`editor::Editor`) each hold one `timeline::Interaction<Self>`, and they no
longer contain camera, zoom, grid, cursor, marquee or razor code.

---

## 5. Open questions

**Decided 2026-10-13** (the questions are kept below for the reasoning):

1. **Ctrl on a body** (revised 2026-10-14): Ctrl (Cmd on macOS) is the
   razor on both surfaces, wherever a drag can start — empty ground, a
   body, an edge — and both have an armed Razor tool too (the
   arrangement's on its main toolbar, the Editor's ⌗; no letter key, `x`
   is gone). Ctrl-click on a body still toggles selection. The Editor's
   Ctrl+drag on a note was velocity; velocity is the Velocity tool's.
2. **Move snap:** per-surface defaults. Items use `AnchorStart` and notes
   use `Relative`, and a surface can change its own.
3. **Shift on a body** (revised 2026-10-14): Shift flips snapping —
   off when it is on, on when it is off — on everything that can be
   dragged. Done on the arrangement (step 12b). The Editor's Shift+drag on
   a note is still its one-axis move; whether it becomes the snap flip
   too is open.
4. **Zoom drag:** down zooms in (REAPER). `ZoomDrag` and its tests flip.
5. **Editor undo in Session:** the embedded Editor pushes its steps into
   Session's per-peer Loro undo, so `SESSION_UNDO` covers note edits. The
   standalone Editor keeps snapshots.
6. **Standalone wheel:** `timeline::wheel` is the one implementation. The
   `input` `ScrollBindingTable` stays only as an optional override.

1. **Ctrl on a body.** A uses it for razor, and Ctrl-click still toggles
   selection. E uses it for velocity, with Ctrl-click toggling. Both
   presets put the razor on Ctrl over empty ground. Should A's
   item-body-Ctrl-razor survive, or should a razor over items be Ctrl on
   empty ground plus the Razor tool, as in E? The core supports either;
   this decides the `common()` row.
2. **How an item move snaps.**
   - A snaps the item start to the grid (`AnchorStart`, REAPER's default
     for items).
   - E snaps the delta (`Relative`), so an off-grid note keeps its feel.
   - For drum hits and audio slices the relative rule is arguably right
     on A too.

   Keep the per-surface defaults, or make `Relative` universal?
3. **Shift on a note or item.**
   - Once E's roll classifies clicks (13c), Shift+click can mean "add to
     selection" while Shift+drag stays "one axis".
   - On A, Shift+drag means "ignore snap".
   - Should the merged default make Shift+drag one-axis on both sides,
     with snap bypass moved to Ctrl+Alt as E's `MoveNoteNoSnap`, or keep
     A's meaning?
4. **Vertical direction of the zoom drag.** A's live panel drag is "down
   zooms rows in (REAPER)". `ZoomDrag` (tested on both sides, including
   E's stack) is "up zooms in". Pick one.
5. **Undo store for the Editor inside Session.** The protocol is shared.
   But should the Editor embedded in Session push its steps into Session's
   per-peer Loro undo, so that `SESSION_UNDO` covers note edits, rather
   than keeping a separate 10-deep per-track snapshot history? This
   affects `undo_routing.rs` and the Turso + Loro store's "the doc must
   hold everything" rule.
6. **The standalone Editor's wheel.** It is keymap-driven (`input`
   `ScrollBindingTable`: Ctrl+Shift nudges time, `z` zooms both axes),
   while Session's is the hard-wired `gestures::wheel` (`z` = time, `z` +
   Shift = rows). Should the keymap table become the one implementation,
   with `wheel()` as its default profile, or should the table be retired?
