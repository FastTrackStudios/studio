# Actions everywhere

Every control in Studio's Session window — a button, a menu item, a key,
the end of a drag — runs a **named action** on the engine's Action List.
Nothing in the UI changes the song or the window's state by itself.

Why:

- **It is detectable.** The Action List's `watch` stream records
  `Ran { id, ok }` for every action. Generated guides, tests and in-app
  tours (`docs/design/guide-media.md`) advance on it. A click that pushes
  an `Edit` or writes a signal shows up, at best, as an anonymous
  `Changed { topic: "tracks" }`, and often as nothing.
- **It is scriptable.** Anything a click does, `studio run <ID> '{…}'`
  and MCP's `run` can do too: the CLI and MCP reach every id on the list
  (`apps/session/desktop/src/cli/app.rs`, `architect_mcp`). An action's
  description is its MCP and CLI documentation.

## The rules

1. **A handler names an action and runs it.** In the window that is
   `crate::window::press(id, &args)` (`apps/session/ui/src/window.rs`),
   the same path a key takes (`through_list` → `crate::actions::run`).
   `press_behind` runs it on the key-actions thread when it reads the
   whole song or waits on the engine. A strip control builds a
   `crate::mcp::Press` and runs `crate::mcp::run`.
2. **The action does the work.** The handler keeps no logic of its own
   that the action lacks: what a lanes button opens
   (`lanes::opened_with_folders`) or which lane a lane arrow plays
   (`comp::step_lane`) lives in the engine, so the CLI does exactly what
   the click does.
3. **A preview may stay local.** A button may *assume* its result
   (`ArrangementWidget::assume`, `mixer_panel::predict`) so it lights at
   once. A continuous gesture (fader, pan, a drag, scroll, zoom) may send
   live edits while it moves. **It commits through an action when it
   ends** (`MixerPanel::commit_turn` → `TRACK_SET`).
4. **View state is still an action.** Folds, lanes shown, panels,
   screensets, modes, phases and options are window state. Their actions
   are window actions (registered in `apps/session/ui/src/window.rs`) or
   engine actions that forward to the window (`also_in_the_window` with
   the settled state: `TRACK_SHOW_LANES {tracks}`, `SESSION_FOLD
   {folders}`).
5. **A window action run in the window is said on the engine's list.**
   When the window drives an engine in another process
   (`FTS_ENGINE=spawn|attach`), `actions::run` runs a window action
   locally and then calls `view_client::ran_in_window`; the engine runs
   its forwarded entry inside the `RAN_IN_WINDOW` task-local, which
   answers without asking the window again. So the engine's `watch` sees
   `Ran` for keys and clicks alike.
6. **Only pure ephemera are exempt:** hover, a menu or popover opening
   (its *choices* are actions), typing into a field before it commits, a
   drag's in-flight preview, window chrome (drag/maximise the OS window),
   focus, measuring layout.
7. **The browser build runs actions too — no direct-edit fallbacks.**
   Decided 2026-10-30: like the desktop window, the page's controls run
   named actions. The page runs its own engine in-process (`web_engine`,
   daw-standalone on wasm) with no link to a desktop engine, so it has its
   own Action List: `session_engine::web_actions` (`web` only). On it:

   - the actions both lists carry, one description and one argument shape
     per id (`session_engine::shared_actions`): `TRACK_SET`, `TRACK_ARM`,
     `SESSION_SELECT_TRACK`, `SESSION_FOLD`, `TRACK_SHOW_LANES`,
     `SESSION_PHASE` — their decisions (which track a name means, which
     folders a fold opens, which lanes show) shared, carried out through
     the page's facade;
   - the arrangement's gestures (`arrange_actions`: `ARRANGE_*`,
     `ITEMS_SELECT`), which were facade edits already;
   - the window's view actions (`session_ui::view_actions`:
     `SESSION_OPTION`, `SESSION_MIXER_FX`), added by the page's root
     (`press::register_page_actions`).

   A control runs one with `crate::press::press` (the desktop's list
   through `window::press`, the page's in the background), or queues it
   among its edits as `Edit::Run`, which the page's edit worker runs in
   order with them. An action the page's list lacks is refused there and
   said once (`control action refused`), never stood in for.

### Browser: what the page's list still lacks

| control | action it runs | on the page now |
|---|---|---|
| track panel lane arrows | `TRACK_LANE_STEP` | refused (it was a no-op there before) |
| toolbar grouping | `SESSION_GROUPING` | the option flipped and `options::grouping_changed` (toolbar.rs) — debt |
| everything not yet routed on the desktop either | — | as on the desktop |

## The guard

`apps/session/ui/tests/it/actions_everywhere.rs` scans `session-ui`'s
sources (comments out, whitespace squeezed) for the patterns a bypass is
made of — an `Edit` pushed to an edits queue or an applier
(`edits.borrow_mut().push(`, `edits.borrow_mut().extend(`,
`applier.send(`, `Effect::Send(` — the arrangement's planned edits are
`Effect::Edit` now, committed as gestures),
`lanes::set_expanded(`, `folds::toggle(`, `folds::set_tops(`,
`transport_here(`, `request_dock(`, `mode_phases::choose(`,
`set_racks_shown(`, `play_return::set(`, `options::X.toggle/set(` — and
fails on any count above its allowance **or below it** (so a routed
handler must lower its entry). Each allowance is a file, a pattern, a
count and why: an action's own handler, a browser fallback, or debt with
the action it should become. `cargo nextest run -p session-ui --test it
actions_everywhere`.

`session-engine`'s `actions::presses` tests hold the other end: every id
a control runs is on the list, every transport move maps to a registered
action, and the list validates and survives hostile arguments.

## New and extended actions

| id | scope | what |
|---|---|---|
| `TRACK_SET` + `phase`, `monitor`, `name` | engine | the panel's and mixer's buttons, fader/pan commits, renames |
| `SESSION_SELECT_TRACK` + `add` | engine | name / strip clicks, modified click adds |
| `TRACK_LANE_STEP {track, by}` | engine | the panel's lane arrows (by name across a folder and its media-edit group) |
| `TRACK_SHOW_LANES` | engine → window | now opens the folders between, as the button did |
| `SESSION_FOLD {folders}` | engine → window | forwards the settled folders to a window in another process |
| `SESSION_STOP`, `SESSION_GO_TO_END`, `SESSION_LOOP`, `SESSION_SEEK {at}`, `SESSION_PLAY_FROM {at}` | engine | the transport's moves (`actions::transport_action`) |
| `SESSION_OPTION {option, on?}` | window | snap, grid, ripple, razor, auto crossfade, lock |
| `EngineView::ran_in_window` | link | a window action run in the window, said on the engine's list |
| `ARRANGE_MOVE`, `ARRANGE_TRIM`, `ARRANGE_SLIP`, `ARRANGE_FADE`, `ARRANGE_SPLIT`, `ARRANGE_COPY`, `ARRANGE_DELETE`, `ARRANGE_MARKERS`, `ARRANGE_ITEM_VOLUME`, `ARRANGE_TAKE_VOLUME`, `ARRANGE_EDIT`, `ITEMS_SELECT` `{label?, edits}` | engine | an arrangement gesture's edits, in order, one undo step (`session_engine::arrange_actions`); which id is what the gesture did (`gesture_id`) |
| `Edit::Run(id, args)` | window → engine | a control's action run by the window's edit worker in order with the edits around it (`Applier`) |
| `RIG_BLOCK_SET {track, block, set?, bypassed?, level_db?, option?}` | engine (`signal`) | one block of a track's signal rig, by id or name: what the FX view's rig faces and bypass buttons commit |
| `RIG_BLOCKS {track}` | engine (`signal`), read-only | the rig's blocks as they are: what `RIG_BLOCK_SET` changes |

## Inventory (2026-10-30)

Counted by hand and by sweep of every handler in scope. *routed* already
runs a named action; *bypass* changes state without one; *local* is
exempt by rule 6.

| area | routed | bypass | local |
|---|---|---|---|
| Arrangement: track panel (`widget.rs` `act`, rename) | 0 → **9** | 9 → **0** | 1 |
| Arrangement: items, ruler, gestures (`arrange_edit.rs`, `widget.rs`) | 1 → **~29** | ~25 edit families → **0** | — |
| Painted mixer / FX / automation / editor (`mixer_panel`, `mcp`, `fx_view`, `automation_*`, `tone`, `expression`, `editor_view`, `navigator`, `arrange_camera`, `ruler`, `device_audio`) | 13 → **25** → **~39** | 86 → **74** → **~60** | 13 |
| Session UI rsx (`shell`, `transport_bar`, `toolbar`, `quantize_panel`, `collab_bar`, `record_view`, `versions`, `setup`, `compact`, `closeup`, `bar_step`, `studio`, `lyrics_panel`, `perf_meter`, `chart_*`, `browser`, `organize`, `navigator`, `instrument_rail`, `web_host`, `progress`, `performer`, `app_menu`) | 41 → **73** | 97 → **65** | 107 |
| Desktop (`native/{dock,shell,start,library,publish,handoff,mod}.rs`, `session_view.rs`, `lib.rs`, `mixer_view.rs`) | 28 → **34** | 71 → **65** | 68 |
| Dock crate (`features/ui/dock` host + components) | 0 | 50 | 30 |
| Studio welcome (`apps/studio/desktop/src/front.rs`) | 4 | 3 | 3 |
| **total** | **87 → 146 → ~174 → ~188** | **~341 → ~282 → ~254 → ~240** | **222** |

Bold: after the first routing pass, then after pass 2's arrangement area
(every item, ruler and dial gesture; the track knob's commit and the snap
key, which the first count had missed). Routed in the first pass:

- **Track panel**: every control (table below).
- **Strips** (`mcp::click`, mixer and FX view): mute, solo, arm, input
  monitor, name, a strip's ground; the mixer's fold button; fader and
  pan drags commit `TRACK_SET` on release and on a lost release; the mix
  phase rail runs `SESSION_PHASE`.
- **Transport, everywhere**: `engine::transport` — what every transport
  button, the navigator's and progress bar's sections, the record view,
  the performer view, the iOS lock screen, autoplay and leaving the DAW
  call — now runs the move's named action (`transport_action`:
  `SESSION_PLAY_STOP`, `SESSION_GO_TO_START`, `SESSION_STOP`,
  `SESSION_GO_TO_END`, `SESSION_LOOP`, `SESSION_SEEK {at}`,
  `SESSION_PLAY_FROM {at}`, `TRANSPORT_RECORD`) behind the window, linked
  or not. Before, only play and record were actions, and only when linked.
- **Shell**: the mode picker (`SESSION_MODE`), the phase picker
  (`SESSION_PHASE`), the racks switch (`SESSION_MIXER_FX`).
- **Toolbar**: the metronome (`TRACK_SET {mute}` on the click track);
  snap, grid, ripple, razor, auto crossfade and lock (`SESSION_OPTION`).
- **Dock**: the bottom bar's panel buttons (`SESSION_PANEL {kind}`), its
  Arrange button (`SESSION_ARRANGE`), the top bar's navigator
  (`SESSION_PANEL {kind:"set"}`).

### Arrangement: track panel — routed

| file:line | control | was | now |
|---|---|---|---|
| widget.rs `act` | Mute / Solo | `Edit::ToggleMute/Solo` | `TRACK_SET {mute|solo}` |
| widget.rs `act` | Rec arm | `Edit::ToggleArm` | `TRACK_ARM {on}` |
| widget.rs `act` | Phase | `Edit::SetPhase` | `TRACK_SET {phase}` |
| widget.rs `act` | Lanes `+` / lane name | `lanes::set_expanded` (no event at all) | `TRACK_SHOW_LANES {track, show}` |
| widget.rs `act` | Lane ◀ ▶ | `Edit::SetLanePlays` per track, logic in the widget | `TRACK_LANE_STEP {track, by}` |
| widget.rs `act` | Name click | `Edit::Select` | `SESSION_SELECT_TRACK` |
| widget.rs `act` | Folder | `folds::toggle` | `SESSION_FOLD {track, open}` |
| widget.rs rename (Enter) | Rename | `Edit::Rename` | `TRACK_SET {name}` |
| widget.rs `act` | Automation | opens a menu (its choices run `AUTOMATION_*`) | local |

### Arrangement: items and ruler — routed

A gesture is planned in the window (`arrange_edit`: snap, a folded row
standing for its mics, ripple), drawn at once from the window's copy of
the song, and **committed when it ends as one named action**:
`ArrangementWidget::settle` splits its edits into gestures
(`arrange_edit::gestures`: each undo bracket one, named by its label; a
bare selection one), and `commit_edits` names each
(`session_engine::arrange_actions::gesture`) and queues it as
`Edit::Run(id, args)` — run by the window's edit worker through the
Action List, in order with the edits before and after it, so a barrier an
action waits on (`Edit::Done`) still lands after the work. The action's
answer carries `made` (the engine's GUID for each piece a split or copy
named), which the worker remembers as the view link's `apply` did.

What an *action* asks of the arrangement — a key's, already run through
the list (`SESSION_DELETE_ITEMS`, `SESSION_SPLIT`, …), or the engine's
(`QUANTIZE`'s slip, restore, stretch, remove a range) — is that action's
own work: `settle_for_action` sends it as it is, under that action.

| control | was | now |
|---|---|---|
| item drag (body), lane change, joined marks carried | `MoveItem`, `SetItemLane`, `MoveMarker`/`SetRegionBounds` per edit | `ARRANGE_MOVE` |
| item edge, stretch, edge snap offset | `TrimItem`, `SetStretchMarkers`, `SetSnapOffset` | `ARRANGE_TRIM` |
| slip | `SlipItem` | `ARRANGE_SLIP` |
| fade handles | `SetFadeIn/Out` | `ARRANGE_FADE` |
| razor click, split at the pointer | `SplitItem` (+ snap offset) | `ARRANGE_SPLIT` |
| modified drag (copy), paste (`Ctrl+V`, no action of its own) | `CopyItem` | `ARRANGE_COPY` |
| razor area delete | `DeleteItem` | `ARRANGE_DELETE` |
| razor carve, cuts that split and trim | mixed | `ARRANGE_EDIT` |
| marker / region drag, mark made or removed with the pointer | `MoveMarker`, `SetRegionBounds`, `AddMarker`, … | `ARRANGE_MARKERS` |
| item click, marquee, modified click | `SelectItem`, `DeselectAllItems`, … | `ITEMS_SELECT` |
| item volume dial (drawn while turned) | `SetItemVolume` on release | `ARRANGE_ITEM_VOLUME` |
| take volume curve: point made, dragged, Alt-deleted (drawn while dragged) | `BeginUndo`, `SetTakeVolume` per move, `EndUndo` | `ARRANGE_TAKE_VOLUME` on release and on a lost release |
| track panel volume / pan knob | `SetVolume/SetPan` per move, no commit | live edits stay; `TRACK_SET` on release and on a lost release |
| snap key (`grid::TOGGLE`, REAPER 1157) | `options::SNAP.toggle()`, no action | `SESSION_OPTION {option:"snap"}` |

Not a gesture, so not here: `SetVisibility` is `SESSION_SHOW_GROUP` /
`SESSION_HIDE_ALL`'s own work; `Edit::Done` is the barrier an engine
action waits on; `Touch` rides the knob's live turn. The automation
lanes (`automation_edit::press/drag/release`) are area 3.

### Painted mixer, FX view, automation, Editor

| file:line | control | does now | action |
|---|---|---|---|
| mcp.rs `click` (mixer_panel, fx_view) | Mute / Solo / Arm / Monitor / Name / ground | `Edit::Toggle*`, `SetInputMonitor`, `Select` | **routed**: `TRACK_SET`, `TRACK_ARM`, `SESSION_SELECT_TRACK {add}` |
| mixer_panel.rs fold button | Folder | `folds::toggle` | **routed**: `SESSION_FOLD` |
| mixer_panel.rs release / cancel | Fader / pan drag end | last `SetVolume/SetPan` | **routed**: `TRACK_SET {volume_db|pan}` (drag stays a live preview) |
| fx_view.rs `turn_strip` / `release_strip` | strip fader/pan drag | `SetVolume/SetPan` per move, no commit | **routed**: live turn, `TRACK_SET` on release (`commit_turn`) |
| mixer_panel.rs:1286-1311 | rails: scene, focus selected, take focus width, phase, audience | signal writes; `mode_phases::choose` | NEW `MIXER_VIEW {…}`; phase **routed** (`SESSION_PHASE`) |
| mixer_panel.rs:506, 1851, 1924, 1964 | wheel / overview / rack scroll / fling | scroll cells | NEW `MIXER_VIEW {scroll}` at gesture end |
| mixer_panel.rs:1475, 1484 | rack zoom grip, rack phase fold | `closeups.ask`, `folds.toggle` | NEW `SESSION_CLOSEUP`, `MIXER_VIEW {rack_fold}` |
| tone.rs `Shared::edited` / `commit` (every rack edit: the mixer's racks, the FX view's devices, knobs, switches and unit faces, a close-up) | rack switches, grips, EQ dots, macros, faces | `Edit::SetFxParam/SetFxEnabled`, no undo step | **routed**: live edits inside one undo step, then `FX_SET` (each FX's parameters as they ended) and `FX_ENABLE` (an FX it switched on) on the racks' worker when the pointer lifts or is lost |
| fx_view.rs `face_edits` / `press` (Rig) | rig block faces / bypass | the window's copy only — **never reached the engine** | **routed**: `RIG_BLOCK_SET` with what the gesture changed on the block (`rig::block_changes`) when the face lets go; a bypass click at once |
| fx_view.rs:803-810, 908 | lane scroll / step, group fold | view state | NEW `FX_VIEW {…}` |
| automation_edit.rs:164, 186, 214 | hide lane, mode chip, lane height | `ShowEnvelope`, `SetEnvelopeMode`, `SetLaneHeight` | `AUTOMATION_SHOW`, `AUTOMATION_SET_MODE`, `AUTOMATION_SHOW {height}` |
| automation_edit.rs:222-376 | items (make, move, trim, curve, delete), envelope points (add, drag, delete) | `*AutomationItem*`, `SetItemCurve`, `SetEnvelope` | `AUTOMATION_ITEM` (extend: `at`, `move_to`, bounds, `curve`), `AUTOMATION_WRITE` (extend: `replace`) |
| automation_edit.rs (leveler), automation_menu.rs | leveler dials, rides, menu | — | routed (`VOCAL_LEVELER_SET`, `VOCAL_RELEVEL`, `AUTOMATION_*`) |
| expression.rs, editor_view.rs:666 | Editor notes, velocity, menu, keys | edit the doc; `write_take` through the DAW facade | NEW `NOTES_WRITE {item, content}`, `EDITOR_ACTION {id, args}`, `TAKES_ALIGN` |
| navigator.rs:54 | section click | `transport(Seek)` | NEW `SESSION_SEEK` |
| arrange_camera.rs:166-205 | wheel, pinch, hand pan, z-zoom | camera | `SESSION_VIEW` at gesture end |
| ruler/mod.rs:130 | CHORDS lane toggle | atomic | NEW `SESSION_RULER {chords}` |
| device_audio.rs | mic / speaker | host callback | NEW `SESSION_DEVICE_AUDIO` |

**Still to route in the mixer and FX view (view state, rule 4):** the
mixer's rails (scene, focus selected, take focus width, audience), its
rack phase folds, rack zoom grip (`closeups.ask`), scroll (wheel,
overview, pan, rack scroll, fling), and the FX view's lane scroll and
step and group folds — `MIXER_VIEW {…}`, `FX_VIEW {…}`,
`SESSION_CLOSEUP`. They are window actions, and `mixer_panel` and
`fx_view` build for the browser too, so they wait for the browser's
Action List (rule 7) rather than gain a browser fallback.

**The FX view's rig is a fixture.** Its blocks come from a dump picked by
the track's name (`rig::for_track`: Rhythm, Lead under Electric), not
from the rig on the track, so `RIG_BLOCK_SET` names a block by that
dump's name. Where the track's real rig has no block of that name the
action refuses and says which it has — the edit is no longer silently
lost, but it is not made either. Reading the FX view's rig from the
engine (`RIG_BLOCKS`) is the fix.

### Session UI rsx — to route

| file:line | control | does now | action |
|---|---|---|---|
| transport_bar.rs:186, 283; performer.rs:228 | Go to start | `engine::transport(Home)` | **routed** (`engine::transport`): `SESSION_GO_TO_START` |
| transport_bar.rs:197; progress.rs:193; performer.rs:192 | Play/stop | `transport(PlayStop)` (an action only when linked) | **routed**: `SESSION_PLAY_STOP` |
| transport_bar.rs:230; progress.rs:195 | Record | `transport(ToggleRecord)` (an action only when linked) | **routed**: `TRANSPORT_RECORD` |
| transport_bar.rs:232; progress.rs:194; performer.rs:212 | Loop | `transport(ToggleLoop)` | **routed**: `SESSION_LOOP` |
| transport_bar.rs:235, 287; performer.rs:232 | Go to end | `transport(End)` | **routed**: `SESSION_GO_TO_END` |
| progress.rs:150; navigator.rs:52; record_view.rs:115, 315; performer.rs:188 | Seek to a section / bar | `transport(Seek)` | **routed**: `SESSION_SEEK {at}` |
| progress.rs:201, 211; performer.rs:196 | Section back / forward | Seek or song step | seeks **routed** (`SESSION_SEEK`); NEW `SESSION_SECTION_STEP {by}` would name the step |
| transport_bar.rs:213 | Stop behaviour | `play_return::set` + action | move the set into `SESSION_STOP_BEHAVIOUR` |
| shell.rs:769; compact.rs:557; record_view.rs:194 | Mode | `mode.set` | shell.rs **routed**; compact, record view: `SESSION_MODE` |
| shell.rs:834 | Phase | `mode_phases::choose` | **routed**: `SESSION_PHASE` |
| shell.rs:546 | bottom-bar views | `Pane::press` / `view.set` | `SESSION_TOGGLE_MIXER/FX/EDITOR`; NEW `SESSION_SHELL_VIEW {view}` |
| shell.rs:577, 587, 904, 1183 | pins: transport, progress, inspector, navigator | `pins.*.toggle` | `SESSION_PANEL {kind}` (extend to the undocked shell's pins) |
| shell.rs:914 | Quantize pin | `pins.quantize.toggle` | `SESSION_QUANTIZE_PANEL` |
| shell.rs:613 | pane docked | `Pane::set_docked` | NEW `SESSION_PANE_DOCK {pane, docked}` |
| shell.rs:941; toolbar.rs:263; compact.rs:574 | Lock | `lock.toggle`, `options::LOCKING` | NEW `SESSION_LOCK {on?}` |
| toolbar.rs:252 | auto crossfade, ripple, grid, snap, razor | `options::*.toggle` | **routed**: `SESSION_OPTION {option, on}` |
| toolbar.rs:241, 246 | follow, compact track panel | prefs | NEW `SESSION_FOLLOW`, `SESSION_COMPACT_TRACKS` |
| toolbar.rs:255 | grouping | option flipped, then `SESSION_GROUPING` | let the action flip it |
| toolbar.rs:232 | metronome | `Edit::ToggleMute(click)` | **routed**: `TRACK_SET {mute}` |
| toolbar.rs:225 | mixer toggle | `MixerToggle::press` | `SESSION_TOGGLE_MIXER` |
| shell.rs:1015 | fold every folder | `folds::set_tops` | NEW `SESSION_FOLD_TOPS {folded}` |
| shell.rs:1052 | racks | `set_racks_shown` | **routed**: `SESSION_MIXER_FX` |
| shell.rs:972-1000; chart_panel.rs:595, 739 | chart fit / page / follow | file / signal | NEW `SESSION_CHART_FIT`, `SESSION_CHART_PAGE` |
| shell.rs:1399; setup.rs:191 | audio mode | `open::request_mode` | NEW `SESSION_AUDIO_MODE` |
| setup.rs:261 | streaming quality | pref | `SESSION_STREAM_QUALITY` |
| setup.rs:159, 168 | speaker / mic | host callback | NEW `SESSION_DEVICE_AUDIO` |
| setup.rs:440; record_view.rs:213 | touch mode | signal | NEW `SESSION_TOUCH` |
| setup.rs:89; compact.rs:417-635 | setup tab, phone views | signals | NEW `SESSION_SETUP_TAB`, `SESSION_PHONE_VIEW` |
| record_view.rs:345-421 | take ★ / ✕ | DAW facade markers | `TAKES_RANK {by, at|near}` |
| versions.rs:259, 282, 291 | open a version | `projects::open_next` + Back | `SESSION_OPEN {name}` |
| collab_bar.rs:404-518 | share, join, together/apart, leave, lock | `collab::*` (an action only when linked) | `SESSION_TOGETHER_*` |
| closeup.rs:192-267 | close-up tabs, close | signal | NEW `SESSION_CLOSEUP {track, which|none}` |
| lyrics_panel.rs:526, 544 | lyrics view / layer | signals | NEW `SESSION_LYRICS` |
| perf_meter.rs:246, 342 | sort, close | signal, `toggle` | `SESSION_PERF_METER {sort?}` |
| chart_editor.rs:186, 329 | editor toggle, text apply | signal; file write (action only when linked) | `SESSION_PANEL {kind:"chart-editor"}`, `CHART_APPLY` |
| organize.rs:82 | marker / section / time signature | REAPER commands (action only when linked) | `ORGANIZE_PRESS` |
| web_host.rs:693, 956 | song pick, multitracks (browser) | direct | browser build: no list (rule 7) |
| mix_guide.rs:58-91 | Mix mode mutes the guide (no handler) | `Edit::ToggleMute` | `TRACK_SET` from inside `SESSION_MODE` |

### Desktop, dock and Studio's welcome — to route

| file:line | control | does now | action |
|---|---|---|---|
| native/dock.rs:193 | bottom-bar panel buttons | `dock.toggle_kind` | **routed**: `SESSION_PANEL {kind}` |
| native/dock.rs:229; dock host.rs:1108, 1608 | Arrange | `toggle_arrange` | native/dock.rs **routed**; dock host: `SESSION_ARRANGE` |
| native/shell.rs:371 | navigator button | `request_dock` | **routed**: `SESSION_PANEL {kind:"set"}` |
| native/shell.rs:313; start.rs:202 | iOS play/pause; autoplay | `engine::transport` | **routed**: `SESSION_PLAY_STOP` |
| native/shell.rs:587 | chart editor toggle | signal | NEW `SESSION_CHART_EDITOR` |
| dock host.rs:528, 1443, 1589 | screenset recall (⌘1-9, menu, bar) | `dock.recall` | `SESSION_SCREENSET` |
| dock host.rs:1597, 1602 | screenset save / save as | `sets.save*` | `SESSION_SCREENSET_SAVE{,_AS}` |
| dock host.rs:674-1456 | panel show / move / attach / open / close / maximise / tabs / sheet; edge and split sizes | `Dock` methods; drags write the layout per move | NEW `SESSION_PANEL_{SHOW,MOVE,ATTACH,OPEN,CLOSE,MAXIMIZE,TABS,SHEET}`, `SESSION_DOCK_{EDGE,SPLIT}`; drags commit on release |
| dock ui/components/* | the older tile dock (`features/session`, not this window) | `DockActions` callbacks | host-installed `DockActions` running the host's `DOCK_*` |
| start.rs:838-876, 1232, 1693 | sign in / out | `acct.*` | NEW `ACCOUNT_SIGN_IN`, `ACCOUNT_SIGN_OUT` |
| start.rs:990 | reconnect to REAPER | `set_mode(Remote)` + attach | NEW `SESSION_CONNECT {address}` |
| start.rs:996-1303 | start screen sections, editors | signals | NEW `SESSION_START_SECTION`, `SESSION_LIBRARY_EDITOR` |
| start.rs:1100, 1134, 1889, 1898, 1162 | download / remove | `stream_set::*` | NEW `LIBRARY_DOWNLOAD`, `LIBRARY_REMOVE_DOWNLOAD` |
| start.rs:1663 | org pick | signal + pref | NEW `LIBRARY_ORG` |
| library.rs:424-988 | lists and songs in Task | `l.*` | NEW `LIBRARY_LIST_{CREATE,RENAME,DUPLICATE,DELETE,EDIT}`, `LIBRARY_SONG_{SAVE,DELETE}` |
| handoff.rs:280; front.rs:342, 352; start.rs:757; front.rs:301 | leave the DAW, back to library, open a rig | `transport(Stop)`, `view.set` | stop **routed** (`SESSION_STOP`); NEW `STUDIO_VIEW {view, rig?}` |
| session_view.rs:310-639 | REAPER connect, guide/metronome, output routing | direct | NEW `SESSION_REAPER_CONNECT`, `GUIDE_SET`, `AUDIO_ROUTING_SET` |
| mixer_view.rs:72 | open setlist in REAPER | writes an RPP, spawns REAPER | NEW `SESSION_OPEN_IN_REAPER` |
| lib.rs (older webview shell) 570-1417 | workspaces, setlist load/save, organize, record | direct | `SESSION_WORKSPACE`, `SESSION_OPEN`, `SETLIST_CREATE`, `ORGANIZE_PRESS`, `TRANSPORT_RECORD` |
| apps/studio/desktop/src/mac_menu.rs:56-119; window.rs `dock_request`; keys.rs:575 | menu and pane keys | `request_dock` | `SESSION_PANEL` / `SESSION_SCREENSET*` |

**The dock crate stays Session-free.** `features/ui/dock` gets a
serialisable `DockIntent` and an optional router installed by the app
(`use_dock_provider(.., router)`): each committing handler calls
`dock.ask(intent)`; with a router, the app runs the matching action, which
applies the intent back (`Dock::apply`, moved in from
`native/dock.rs::apply`); without one, the dock applies it itself.
