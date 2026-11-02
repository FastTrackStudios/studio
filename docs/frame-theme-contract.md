# Studio theme contract: the role catalogue

This is the maintained list of what a Frame theme can style in Studio's five
views: the Arrangement View, the Track Control Panel (TCP), the Mixer, the
MIDI Editor and the Audio Editor. For every element it answers two questions:
**which role styles it, and is that implemented?**

- **Checked against the code:** 2026-11-02, on `main` @ `8580c96b`.
- **Design and history:** `docs/frame-themes.md` in FastTrackStudios/frame.
- **Tracking:** STA-675 (the parent), audited in STA-676. Paths are
  relative to `apps/session/ui/` unless they begin with `features/`,
  `crates/` or `themes/`; `themes/` means `apps/session/ui/themes/`.

**Full coverage is not claimed.** No view is fully theme-authored today. The
[coverage summary](#coverage-summary) states what is complete and what is
planned. The issue that brings each view to full coverage owns updating its
rows here (see [Keeping this current](#keeping-this-current)).

## How to read the tables

**Status**

| Status | Meaning |
|---|---|
| **Role** | A Frame role draws it (geometry, style, artwork and, where noted, hit testing). Works today. |
| **Tokens** | The native painter draws it, but the theme's tokens recolour it (`skin::palette`, `skin/mod.rs:550-602`; the editor's `Look`, `expression.rs:1593`). No geometry or artwork from the theme. |
| **Native** | The painter draws it from code or `Theme::dark()` (`widget.rs:986,6109`). A theme cannot reach it. |
| **Planned (STA-n)** | Needs implementation; the issue named owns it. A role ID in *italics* is a proposed name, not an API. |
| **None** | The element does not exist in Studio. |

**Kind** says what sort of theme target the element is (or will be):

- **Role:** a Frame page named by its role ID. The theme owns its layout,
  style, artwork and state visuals. Instances are recorded once per size and
  data, and replayed.
- **Component:** a reusable `<Component>` placed by `<Instance of=…>` inside
  a role, such as `mute` or `volume` in `themes/console/controls.frame`.
  Expanded at compile time (`crates/frame/src/theme/runtime/expand.rs:13`).
- **Slot:** a `%name` frame. The theme places and sizes it; the host paints
  its dense content there (`Theme::record_with`, `draw.rs:161-198`).
  **Waveforms and MIDI notes are slots**, not Frame widgets: they stay on
  the host's cached, culled scene.
- **Token:** a variable in the theme's `<Variables>` that a native painter
  reads.

**Exposed in** says where an editor or agent can reach the target today.
Nothing is on the Action List yet: the only way in is to edit the `.frame`
files, which hot-reload (`FTS_THEME`). See
[Editing surfaces](#editing-surfaces).

## The shared contract

Every role in every view goes through one runtime and one schema.

### Runtime API

All in `crates/frame/src/theme/runtime/`:

| API | Where | Use |
|---|---|---|
| `Theme::from_markup`, `from_sources` | `compile.rs:180,202` | Compile one file, or a stack of layers |
| `has_role`, `roles`, `token`, `with_mode` | `compile.rs:329,324,335,308` | Ask what a theme provides |
| `record`, `record_with` | `draw.rs:124,161` | Record an instance; `record_with` calls the host at each `%slot` |
| `live` | `draw.rs:201` | Draw the live pass (meters, faders, lit buttons) every frame |
| `hit`, `target_rect`, `targets` | `draw.rs:227,244,274` | Hit-test the `@name` targets from the same plan the paint used |
| `slots` | `draw.rs:262` | The rects of the `%slots` |
| `Diagnostic { role, node, message }` | `compile.rs:17-23` | Every compile problem; any one fails the compile (`compile.rs:232-234`) |

### Bindings

The host passes each instance a record of named fields. The schema is one
list for all roles, `skin::schema()` (`skin/mod.rs:62-165`), and each field
has a class (`schema.rs:6-15`):

- **Static:** part of the recording. A change re-records that instance.
  Examples: `track.name`, `track.color`, `track.selected`, `button.label`,
  `key.black`, `tick.bar`.
- **Layout:** selects a cached plan. Examples: `mixer.h`, `ui.mixing`,
  `ui.recording` (`skin/mod.rs:141-143`).
- **Live:** drawn in the live pass only. Examples: `track.mute`,
  `track.solo`, `track.armed`, `track.fader`, `track.pan`, `meter.left`,
  `meter.peak`, `ui.hover`, `ui.pressed` (`skin/mod.rs:144-164`).

The bindable node properties are listed at `compile.rs:37-121`: position,
size, rotation, scale, opacity, the four radii, fill, stroke, dash, inset,
font size, vector geometry, text and `visible`.

### States and size variants

There is no variant construct. A theme expresses them in two ways:

- **States** are sibling nodes switched by `bind:visible`, such as
  `Face Normal/Hover/Pressed` on `ui.hover`/`ui.pressed`
  (`themes/console/controls.frame:9-17`).
- **Size variants** are expressions on `w` and `h`, such as `w >= 26` in
  `strip.frame` or `bind:visible="h >= 11"` in `tcp.frame:6`. Each distinct
  size, plus each Layout field's value, gets its own cached plan
  (`plan.rs:30`).

### Hit testing

- A node named `@name` is a hit target. The host maps the name to its own
  control enum: the strip through `skin::control` (`skin/mod.rs:452-485`),
  the row through `skin::row_control` (`:264-279`).
- Drawing and hit testing read the same cached plan (`draw.rs:227-258`), so
  a theme that moves a control moves where it is clicked.
- **Obligation:** every interactive element a role draws must carry an
  `@name` the host knows. If a theme drops a target, that control cannot be
  clicked.

### Accessibility

**Nothing in the theme runtime or the painted views carries accessibility
data today.** I searched `accesskit`, `a11y`, `aria` and `label` across
`crates/frame/src`, `apps/session/ui/src` and `features/editor/src`.

- The arrangement's own doc says a screen reader sees one element
  (`widget.rs:35-43`).
- The only ARIA is in the Dioxus transport bar (`transport_bar.rs:265-287`)
  and the editor's error banner (`features/editor/src/ui/mod.rs:317`).

The contract still sets these obligations, and STA-692 owns them ("engine,
authorization, undo and accessibility stay authoritative"):

- The **host** owns semantics: the name, role, value and actions of every
  control. A theme cannot remove them.
- A theme's `@name` targets map to those semantics, never replace them.
- Every state a theme shows (muted, armed, selected) is also readable
  through the CLI/RPC surface. That is true today for session state.

### Fallback

- **No theme:** every view draws natively with the base palette
  (`skin/mod.rs:552-554`).
- **A theme without a role:** the native painter draws that element,
  recoloured by the theme's tokens (`skin/mod.rs:542-548`). Each table below
  names the fallback.
- **The strip role** is used only if the theme opts in for live and touch
  strips through the `strip.live` and `strip.touch` tokens (`mcp.rs:545-553`).

## Coverage summary

| View | Theme-authored today | Native today | Planned in |
|---|---|---|---|
| Arrangement | Nothing in the lanes or ruler; the TCP rows beside them (below) | Lanes, grid, ruler, items, waveforms, MIDI previews, fades, cursors, selection, automation, overlays; colours from `Theme::dark()` | STA-680 (`arrange.*` roles, slots), STA-678 (live edit from Frame Editor) |
| Track Control Panel | Full-panel rows: tint, rail, name, number, folder, mute, solo, arm, volume, pan, FX, routing, phase, input; with hit testing | Compact panel; lane pills; automation button; meter; transport bar | STA-687 |
| Mixer | Strip below the rack (`mcp.strip`), rack ground (`mcp.rackback`), folder steps (`mcp.step`); with hit testing | The Tone rack's units unless `rack.*` exist; live and touch strips unless the theme opts in | STA-680 (paint view), STA-687 (meters, transport) |
| MIDI Editor | Toolbar and status bar, keyboard, key groups, ruler, ticks, markers, strip chrome, context menu, stacked-lane headers; `Look` colours through tokens | Notes, rows, grid, velocity stems, CC and expression curves, handles, selection, marquee, razors, playhead | STA-688 |
| Audio Editor | Shares the editor chrome above; stacked-lane headers (`editor.lane`) | Waveform backdrop, blobs, sibilant bands, the stacked view's waveforms, hit markers, ruler | STA-688 |

**Complete:** none of the five. **Initial acceptance** (STA-675) covers
Arrangement, TCP and Mixer (plus the Keyflow chart and text editor, which are
outside this catalogue). The MIDI and Audio Editors are tracked explicitly
under STA-688, and themes for them under STA-690.

## 1. Arrangement View

The lanes draw from `Palette::from_theme(&Theme::dark())` (`widget.rs:986`).
`skin::palette` overrides only the text, accent, mute/solo/rec, pan, meter,
field, combo, rack and rail colours (`skin/mod.rs:566-602`). These palette
fields are **never** reachable from a theme: `surface`, `row_a`, `row_b`,
`divider`, `grid`, `grid_beat`, `grid_sub`, `ruler_bg`, `ruler_fg`,
`ruler_fg2`, `item_edge` (`arrangement.rs:146-156`).

The paint order is `widget.rs` `draw()` (`:6067-6750`). The window and the
bench share `Arrangement::replay_lanes` (`arrangement.rs:4307`).

| Element | Role / token | Kind | Data and bindings | Interaction and states | Size variants | Status | Evidence |
|---|---|---|---|---|---|---|---|
| Track lane background, divider, lane seams | *`arrange.lane`* | Role | rows (`record.rs:810,838`), `lane_stack::Stack::seams` | `row_at` `arrangement.rs:1056`; even/odd stripe; no hover, selected or armed tint | row height | Native; planned (STA-680) | `arrangement/record.rs:1201-1226` |
| Ruler: bars and beats | *`arrange.ruler`* | Role | `tempo`, `Timeline::beats` (`ruler/timeline.rs:35`) | press `arrange_edit.rs:496` (`RulerPress`, `TimeSweep`) | LOD by pixel room (`ruler/bars.rs:457-481`); slim mode (`ruler/mod.rs:167`) | Native; planned (STA-680) | `ruler/bars.rs:247-421` |
| Grid lines | *`arrange.ruler`* or a grid role | Role | `Bars`, `grid_in_force::current()` | toggle `options::GRID` | three weights; fades when crowded (`bars.rs:200`) | Native; planned (STA-680) | `ruler/bars.rs:71`, `widget.rs:6384` |
| Tempo / time-signature lane | *`arrange.ruler`* | Role | `scene.tempo()` | none (not in `ruler/hit.rs`) | grid lane in Edit mode | Native, `tcp_rule`/`text*`/`accent` tokens | `ruler/tempo.rs:16,97,111` |
| Sections (regions) and markers | *`arrange.marker`* | Role | `Section`/`Marker` with CSS colour | `ruler/hit.rs:107` (`EDGE_GRIP`, `MARKER_GRIP`); rename `ruler/hit.rs:73` | lane visibility (`ruler/mod.rs:203`) | Native; own colour else `accent` token; planned (STA-680) | `ruler/lanes.rs:12-145` |
| Chord / key lane | none yet | Role | `ChartMarks` | toggle Ctrl+Shift+J (`widget.rs:4111`) | — | Native, partly hard-coded | `ruler/chords.rs:14,93` |
| Track marks strip, POI marks | none yet | Role | track regions; `crate::poi` | — | only with a selected track | Native, hard-coded colours | `marks_strip.rs:72-75,136`; `poi_paint.rs:11-20` |
| Clip / item body | *`arrange.item`* | Role | `daw_proto::Item`, `ItemBox` (`arrangement.rs:1748`) | `item_at` `:1465`, `ItemZone` `:1734`; `arrange_edit.rs:397,815,942`; muted alpha, non-playing lane ×0.45, outlined mode (`:3057`) | — | Native, item/track colour; planned (STA-680) | `arrangement/record.rs:1234-1367` |
| Item name label | *`arrange.item`* (text node) | Role | `Title`, `Lettering` (`arrangement.rs:3520,3538`) | cycle Ctrl+Shift+K | hidden under 12 px; pinned at left edge | Native, `text` token | `arrangement.rs:4175-4266` |
| **Waveform** | *`%wave`* in `arrange.item` | Slot | `LaneWave` (`arrangement.rs:322`); detail via `wave_detail::get` | — | coarse pass; fine pass past zoom 2.0 (`:569`); sample dots past 6.0 (`:572`); 2 ms budget, 250 ms refresh (`:771-777`) | Native; slot planned (STA-680) | `arrangement.rs:544,596,4392-4426` |
| **MIDI notes in an item** | *`%notes`* in `arrange.item` | Slot | `midi::Note` | — | folder hint strip at 22 % height | Native; slot planned (STA-680) | `arrangement.rs:4066`; `record.rs:1479-1495` |
| Fades and fade handles | *`arrange.item`* (`@fade_in`, `@fade_out`) | Role | `Fades` (`arrangement.rs:1818`) | `ItemZone::FadeIn/FadeOut`, `FadeDrag` (`arrange_edit.rs:104`); shown on hover or drag | — | Native, `text` + hard-coded shade | `arrangement.rs:1730,1910-1962` |
| Trim (edge) handles | *`arrange.item`* (`@left`, `@right`) | Role | — | hit only: `ItemZone::LeftEdge/RightEdge` (`arrangement.rs:1511-1515`); nothing drawn | — | Native hit, no visual | `arrangement.rs:1511-1515` |
| Item volume dial | *`arrange.item`* (`@volume`) | Role | `ItemBox.volume` | `ItemZone::Volume`; `set_item_volume` (`widget.rs:4684`) | hover, turning | Native | `arrangement.rs:1976-1999` |
| Take lanes, swipe comping | none yet | Role | `Stack`, `TakeColors` | `swipe_gesture` (`widget.rs:1571`) | pinned head | Native; replaceable comping is STA-692 | `lane_stack.rs:35,88`; `swipe.rs:96` |
| Kit hits, take marks, comp strip | none yet | Role / Slot | `hit_overlay` | `hit_gesture` (`widget.rs:1383`) | edit vs record | Native, mixed tokens and hard-coded | `arrangement.rs:2281,2667,2838,3003,3027` |
| Automation lanes and envelopes | none yet | Role + Slot (curve) | `automation_draw::on_screen` (`:78`) | `automation_edit.rs:136,518,684`; lit point | — | Native, tokens | `automation_draw.rs:307-1010` |
| Item selection | *`arrange.item`* (a selected state) | Role | `editor.selected` | `Editor::select` (`arrange_edit.rs:1258`) | — | Native, `text` token | `arrangement.rs:3455-3485` |
| Drag ghost (move/trim) | none yet | Role | `Editor::ghost` (`arrange_edit.rs:1217`) | — | — | Native | `arrangement.rs:3500-3517` |
| Rubber-band marquee | none yet | Role | `MarqueeDrag` | `arrange_edit.rs:717,737` | — | Native, `accent` token | `arrangement.rs:2233` |
| Razor areas | none yet | Role | `RazorDrag` | `arrange_edit.rs:627,705` | — | Native, `accent` token | `arrangement.rs:2173` |
| Edit cursor and time selection | none yet | Role | `editor.cursor` | ruler press | — | Native; cursor hard-coded `#3b82f6` | `cursor.rs:17,81` |
| Play cursor (playhead) | none yet | Role (live) | `play_at`, trail | scrub (`widget.rs:3913`); touch handle | trail, glow | Native; hard-coded `#facc15` | `cursor.rs:115-119,282` |
| Loop region on the timeline | — | — | — | — | — | None (only the transport Loop button) | searched arrangement, ruler, cursor, widget |
| Slip preview, outlines, cuts, gaps, stream veil | none yet | Role | — | — | — | Native | `arrangement.rs:2106,3078,3145,3241,3359` |
| Collaborator ghosts and pointers | none yet | Role | `PeerState` | — | — | Native, peer colour | `ghosts.rs:236,389`; `collab_pointers.rs:17` |
| Rename field, notice, automation menu | none yet | Role | — | `rename.rs:18` | — | Native, palette | `widget.rs:6573,6657,6675` |
| Scrollbars | none yet | Role | — | — | — | Native, `Theme::dark()` | `studio.rs:200,239` |
| Main toolbar | none yet | Role | `options` | Dioxus | on/off | Native, hard-coded CSS | `toolbar.rs:17-20,192` |
| Transport bar | see the [TCP table](#2-track-control-panel) | | | | | Native; planned (STA-687) | `transport_bar.rs:26` |

**Fallback after STA-680:** each `arrange.*` role is optional. Without it,
today's painter draws that element, as `tcp.row` does now.

**Hit-testing obligation:** item zones (body, fade handles, edges, volume)
must keep coming from one source with the drawing. Today they come from
`ItemZone` geometry; once `arrange.item` exists, from its `@` targets.

**Performance obligation:** the slots keep the host's scene cache, culling
and fine-pass worker (STA-680 goal). A theme edit must not rebuild the scene
every frame.

## 2. Track Control Panel

The full panel's rows are themed by `tcp.row` (`skin::ROW`,
`skin/mod.rs:217`; page at `themes/console/tcp.frame:2`):

- **Record:** `record_panel` records each row through `record_with`
  (`arrangement.rs:1250-1291`).
- **Live:** `theme.live(ROW, …)` (`overlay.rs:1343-1382`).
- **Hit:** `row_spot_at` asks `theme.hit(ROW, …)` (`arrangement.rs:1118-1127`).
- **Fields:** `row_fields` (`skin/mod.rs:297-352`), plus every strip field.
- **Slots:** `%steps` (folder depth steps, `tcp.frame:8`) and `%icon`
  (`tcp.frame:20`), both painted by the host (`arrangement.rs:1296-1303`).

The **compact panel** (`Tcp::COMPACT`, `tcp.rs:191-218`) is a flag, not a
role: the theme row is skipped when it is set (`arrangement.rs:1250`,
`overlay.rs:1345`).

| Element | Role / target | Kind | Data and bindings | Interaction and states | Size variants | Status | Evidence |
|---|---|---|---|---|---|---|---|
| Row background and tint | `tcp.row` | Role | `track.tint`, `track.band` | selected rows tinted toward the track colour (`tcp.rs:898-922`) | `Density`: Full ≥ 58, Compact ≥ 11, Bar (`tcp.rs:163-174`) | **Role** | `tcp.frame:4-10` |
| Track name | `tcp.row` `@name` | Role | `row.name`, `row.name_size`, `track.selected` | click selects (`widget.rs:3814`); double-click renames (`:3733-3748`) | — | **Role** | `tcp.frame:21,37-38` |
| Track number | `tcp.row` | Role | `row.number*`, `row.has_number` | — | hidden when the folder mark fills the rail | **Role** | `tcp.frame:19`; `tcp.rs:554-594` |
| Track icon | `tcp.row` `%icon` | Slot | icon chosen from the name (`track_icon.rs:68`) | — | only with room | **Role** places it; host paints | `arrangement.rs:1297-1302` |
| Folder, collapse | `tcp.row` `@folder` | Role | `track.folder`, `track.collapsed` | `SESSION_FOLD` (`widget.rs:3821-3829`) | open/shut | **Role** | `tcp.frame:12-17,39` |
| Folder depth steps | `tcp.row` `%steps` | Slot | `folder_steps` (`tcp.rs:508`) | — | 10 px per level, max 60 (`tcp.rs:303-308`) | **Role** places it; host paints | `tcp.frame:8` |
| Volume | `tcp.row` `@volume` (`volume` component) | Component | `track.fader` (live) | drag only (`widget.rs:4669`) | gone below a 20 px band (`row.rs:315`) | **Role** | `tcp.frame:35`; `controls.frame:466` |
| Pan | `tcp.row` `@pan` (`pan`) | Component | `track.pan` (live) | drag (`widget.rs:4670`) | as volume | **Role** | `tcp.frame:36`; `controls.frame:3` |
| Mute | `tcp.row` `@mute` (`mute`) | Component | `track.mute` (live) | `TRACK_SET` (`widget.rs:3754-3763`) | squashed on short rows | **Role** | `tcp.frame:29`; `controls.frame:370` |
| Solo | `tcp.row` `@solo` (`solo`) | Component | `track.solo` (live) | `TRACK_SET` | as mute | **Role** | `tcp.frame:30`; `controls.frame:402` |
| Record arm | `tcp.row` `@arm` (`arm`) | Component | `track.armed` (live); shown when `ui.recording` | `TRACK_ARM` (`widget.rs:3764`) | hidden outside Record mode (`row.rs:290`) | **Role** | `tcp.frame:34`; `controls.frame:297` |
| Phase | `tcp.row` `@phase` (`phase`) | Component | `track.phase` | `TRACK_SET phase` (`widget.rs:3768`) | rows ≥ 94 px only | **Role** | `tcp.frame:33,47` |
| FX | `tcp.row` `@fx` (`fx`) | Component | `track.fx` | no action yet (`widget.rs:3838`) | band ≥ 20 px | **Role** (no action) | `tcp.frame:31` |
| Routing | `tcp.row` `@routing` (`routing`) | Component | `track.parent_send`, `track.sends`, `track.receives` | no action yet | band ≥ 20 px | **Role** (no action) | `tcp.frame:32` |
| Input | `tcp.row` | Role | `row.input` | display only (no control) | height ≥ 58 and armed | **Role** | `tcp.frame:22-28` |
| Monitor | — | — | — | — | — | None in the TCP (`row::Control` has no monitor; mixer only) | `row.rs:21-49` |
| Meter | none yet | Role (live) | meter levels | — | thin bar inside the name field | Native; planned (STA-687) | `overlay.rs:1078-1136` |
| Automation button | none yet | Role | — | opens a menu (`widget.rs:3832-3835`) | full panel only | Native, drawn over the theme row (`arrangement.rs:1309`); planned (STA-687) | `row.rs:62-69` |
| Lane prev/next/name/show pills | none yet | Role | lane label | `TRACK_LANE_STEP`, `TRACK_SHOW_LANES` (`widget.rs:3778-3812`) | pill or `+` form (`row.rs:244`) | Native, drawn over the theme row (`overlay.rs:1330-1338`); planned (STA-687) | `row.rs:131-248` |
| Compact panel rows | *`tcp.compact`* or a `tcp.row` size variant | Role | as `tcp.row` | only Folder and Name hit (`tcp.rs:277`) | compact density | Native; planned (STA-687) | `tcp.rs:191-218`; `arrangement.rs:1250` |
| Selection | `tcp.row` | Role | `track.selected` | name click | — | **Role** | `tcp.frame:21` |
| Hover / pressed visuals | `tcp.row` | Role | `ui.hover`, `ui.pressed` (row names) | — | — | **Role** | `tcp.frame:37`; `overlay.rs:1343-1382` |
| **Transport bar** (Home, Play/Stop, Record, Loop, End, clock, tempo/key, menus) | none yet | Role | `engine::Reading` | Dioxus `onpress` | big/small | Native Dioxus, hard-coded CSS; planned (STA-687) | `transport_bar.rs:15-21,26,102-104,185-322` |

**Hit testing gap:** `row_control` maps only ten theme names
(`skin/mod.rs:264-279`). The lane pills and the automation button are always
hit by Rust geometry (`arrangement.rs:1108-1117`), so a theme cannot move them
yet.

**Possible dead code:** the compact panel only lets Folder and Name through
(`tcp.rs:277`; `row.rs:259`), so its second line of mute, solo and arm
(`row.rs:183-205`) looks unreachable. STA-687 should confirm.

## 3. Mixer

The strip is themed by `mcp.strip` (`skin::STRIP`, `skin/mod.rs:41`; page at
`themes/console/strip.frame:2`):

- **Record:** below the rack (`mcp.rs:871-877`).
- **Live:** `theme.live(STRIP, …)` (`overlay.rs:339-366`).
- **Hit:** `mcp::control_in` asks `theme.hit(STRIP, …)` (`mcp.rs:1078-1086`).
- **Fields:** `skin::fields` (`skin/mod.rs:356-448`).
- **Stacked desktop strips only.** Live and touch strips need the
  `strip.live` and `strip.touch` tokens (`mcp.rs:529-554`).

| Element | Role / target | Kind | Data and bindings | Interaction and states | Size variants | Status | Evidence |
|---|---|---|---|---|---|---|---|
| Strip body | `mcp.strip` | Role | all `track.*`, `meter.*`, `ui.*`, `mixer.h` | — | `Squeeze`: Full ≥ 78, Head ≥ 60, Level (`mcp.rs:171-221`); `STRIP_W` 86, touch 62 | **Role** | `strip.frame:2-85` |
| Rack ground, edge, selection rule | `mcp.rackback` | Role | strip fields | — | — | **Role**; fallback fills (`mcp.rs:821-840`) | `strip.frame:86-92`; `mcp.rs:813-819` |
| Folder steps | `mcp.step` | Role | `folder.tint`, `folder.level` | — | 12 px per ancestor (`mcp.rs:139`) | **Role** | `strip.frame:93-97`; `mcp.rs:852-868` |
| Fader / volume | `mcp.strip` `@fader` (`volume.fader`) | Component | `track.fader`, `track.volume_label` (live) | drag (`mcp.rs:1904-1916`); readout while pressed | — | **Role** | `strip.frame:18,54,79-80` |
| Pan | `@pan` | Component | `track.pan` (live) | drag (`mcp.rs:1917-1921`) | w ≥ 26 | **Role** | `strip.frame:20` |
| Mute | `@mute` | Component | `track.mute` (live) | `TRACK_SET` (`mcp.rs:1842-1849`) | larger on touch | **Role** | `strip.frame:26` |
| Solo | `@solo` | Component | `track.solo` (live) | `TRACK_SET` | larger on touch | **Role** | `strip.frame:27` |
| Record arm | `@arm` | Component | `track.armed` (live) | `TRACK_ARM` | hidden when `ui.mixing` | **Role** | `strip.frame:24` |
| Monitor | `@monitor` | Component | `track.monitor` (live) | cycles mode (`mcp.rs:1876-1888`) | only while armed | **Role** | `strip.frame:25` |
| Meters (L/R, hold, peak) | `mcp.strip` | Role (live) | `meter.left/right/hold_*/peak` | — | — | **Role** | `strip.frame:31-51,81-82` |
| Clip latch | `@clip` | Role | `meter.clip` (live) | click clears (`mixer_panel.rs:2167-2168`) | — | **Role** | `strip.frame:19,53` |
| Name | `@name` | Role | `track.label`, solo/mute/selected | click selects (`mcp.rs:1857`); rename not wired (`mixer_panel.rs:21-24`) | — | **Role** | `strip.frame:16,29` |
| Folder | `@folder` | Role | `track.folder`, `track.collapsed` | `SESSION_FOLD` (`mixer_panel.rs:2147-2160`) | — | **Role** | `strip.frame:14-17` |
| FX | `@fx` (`fx` / `fx.compact` under w 46) | Component | `track.fx` | no action yet (`mcp.rs:1866`) | none on touch | **Role** (no action) | `strip.frame:21-22` |
| Routing | `@routing` | Component | sends, receives | opens a panel (`mcp.rs:1862`) | Full tier only | **Role** | `strip.frame:23` |
| Input | `mcp.strip` | Role | `track.input` | display only | hidden when mixing or w < 78 | **Role** | `strip.frame:8-11` |
| Selection | `mcp.strip`, `mcp.rackback` | Role | `track.selected` | — | — | **Role** | `strip.frame:6,28,90` |
| Tone rack units, rail, phase | `rack.unit`, `rack.rail`, `rack.phase` | Role + Slot (`%body`, `%glyph`) | `unit.*`, `phase.*` | — | — | **Role** (console theme only) | `tone.rs:1726-1895,2657-2700`; `rack.frame` |
| FX phase bar, macros | `fx.phase`, `fx.macro[.<phase>[.<track>]]` | Role | `phase.*`, `macro.*` | — | most specific wins (`macros.rs:209-218`) | Role supported; **no theme defines them** | `fx_view.rs:2274-2293`; `macros.rs:258` |
| Rails (side gutters, rail buttons) | `rails`, `rail.button` | Role + Slot (`%icon`) | `button.*` | — | `Surface::Arrange/Mixer` | **Role** | `rails.rs:202-420`; `rails.frame` |
| Width, phase, sends, I/O on the strip | — | — | — | — | — | None in `mcp::Control` | `mcp.rs:1001-1031` |
| Master strip | *`mcp.master`* | Role | — | — | — | None (no master strip in Studio's UI) | searched `master` in `src` |

**Meter data path** (the engine side of every meter role):

1. The audio thread writes atomic cells (`crates/daw/standalone/src/metering.rs:14-50`).
2. A pump reads them at about 30 Hz (`crates/daw/standalone/src/sync/daw.rs:1131-1139`).
3. The UI keeps only the latest frame (`apps/session/engine/src/engine.rs:2636-2700`).

A theme reads meters in the live pass only, and never touches this path.

## 4. MIDI Editor

There is one **Editor view** for MIDI and audio:

- **Widget:** `EditorPanel` / `EditorView` (`editor_view.rs:278-358`), which
  holds `Expression` (`expression.rs:52-111`). It is built on
  `features/editor`.
- **Opening it:** key `e`, `FTS_EXPRESSION_EDITOR_OPEN` (`keys.rs:393`).
- **Modes:** Midi, Mpe, Vocals, Drums, Guitar, PitchedAudio,
  UnpitchedAudio (`features/editor/src/interact/mode.rs:19-46`).

The roles are read in `expression.rs:1231-1545`, and the pages are in
`themes/console/editor.frame`. Colours come from `Look`
(`features/editor/src/paint/paint.rs:84-123`), built from the arrangement
palette by `base_look` (`expression.rs:1196-1225`). Theme tokens override it:

- `editor.<field>` for the 32 `Look` fields (`expression.rs:1552-1612`).
- `editor.pitch_0`…`pitch_11` and `editor.drum_*` for the note colours
  (`expression.rs:1626-1645`).

| Element | Role / token | Kind | Data and bindings | Interaction and states | Size variants | Status | Evidence |
|---|---|---|---|---|---|---|---|
| Toolbar | `editor.bar` + `editor.button` | Role | `button.label/on/hover/enabled/readout` | `chrome::hit` (`features/editor/src/paint/chrome.rs:356`) | 26 px | **Role** | `expression.rs:1239-1273`; `editor.frame:2-14` |
| Status bar (snap, grid, key, strip, selection count) | `editor.bar` + `editor.button` | Role | as toolbar | as toolbar | 20 px | **Role** | `chrome.rs:228-288` |
| Piano keyboard / drum lane names | `editor.keys`, `editor.key` | Role | `key.piano`, `key.black`, `key.label`, `key.labelled` | `Context::Keys` (`interaction.rs:390`) | label at h ≥ 9 | **Role** (all six roll roles needed) | `expression.rs:1371-1404`; `editor.frame:15-29` |
| Key groups (braces) | `editor.keygroup` | Role | `group.label` | — | — | **Role** | `expression.rs:1405-1413` |
| Ruler, bar/beat ticks | `editor.ruler`, `editor.tick` | Role | `tick.bar`, `tick.label`, `tick.labelled` | `Context::Ruler` (`interaction.rs:387`) | 28 px; beats at ≥ 14 px/beat | **Role** | `expression.rs:1422-1454` |
| Markers | `editor.marker` | Role | `marker.label` | — | — | **Role** | `expression.rs:1455-1463` |
| Velocity / pressure / timbre strip chrome | `editor.strip` | Role | `strip.label` | — | 96 px default | **Role** | `expression.rs:1516-1545` |
| Velocity stems and strip curves | *`%stems`* in `editor.strip` | Slot | `canvas::stems`, `strip_curves` | `strip_write` (`expression.rs:1036`); `Drag::Velocity` | stem 3-9 px | Native (`Look`); planned (STA-688) | `paint.rs:529-565` |
| Context menu | `editor.menu` + `editor.menuitem` | Role | `menu.label/shortcut/enabled/hover/rule` | `Menu::choose` (`chrome.rs:644`) | 210×20 native | **Role** | `expression.rs:1472-1511` |
| Row bands (black/white keys, drum families) | `editor.row_a`, `editor.row_b` | Token | `canvas::rows` | — | — | **Tokens**; role planned (STA-688) | `paint.rs:567-580` |
| Grid lines | `editor.grid_beat`, `editor.grid_sub` | Token | `grid_lines` (`canvas.rs:109`) | — | skipped over 512 lines | **Tokens**; planned (STA-688) | `paint.rs:582-593` |
| **Note rectangles** | *`%notes`* | Slot | `NoteRect` (`canvas.rs:138-171`); colour by pitch class, string or drum, **not velocity** | `tools::hit_test` (`interact/tools.rs:210`) | label at h ≥ 9, w ≥ 18 | **Tokens** (`editor.pitch_*`, `editor.drum_*`); slot planned (STA-688) | `paint.rs:859-1005` |
| Note labels | inside `%notes` | Slot | `note_label` (`model/rows.rs:697`) | — | as above | Native, hard-coded black (`paint.rs:976`) | `paint.rs:967-987` |
| Note handles (pitch, slopes, formant, vibrato) | *`editor.handle`* | Role | `Handle` (`interact/handles.rs:32-48`) | `handles::hit` (`:205`) | selected notes, modes with handles | **Tokens** (`editor.handle`, `accent`); planned (STA-688) | `paint.rs:1022-1051` |
| Note edge resize | hit only | — | `HitConfig.edge_px` | `Drag::Resize` (`interaction.rs:188`) | — | Native hit | `interact/tools.rs:198` |
| CC lanes | *`%cc`* | Slot | `cc_paths` (`canvas.rs:1228`) | `Drag::CcPen/CcLine/CcErase/CcScale` | — | Native, hard-coded `CC_COLORS` (`model/cc.rs:87-96`); planned (STA-688) | `paint.rs:1074-1112` |
| Expression curves | *`%curves`* | Slot | `curve_paths` (`canvas.rs:751`) | `Drag::Curve` | active vs quiet | Native, `lane_color`; planned (STA-688) | `paint.rs:1053-1072` |
| Note selection outline | inside `%notes` | Slot | `Editor.selection` | — | — | **Tokens** (`editor.selected`) | `paint.rs:926-937` |
| Time selection | *`editor.selection`* | Role | `ed.time_selection` | `Drag::TimeSelect` | — | **Tokens**; planned (STA-688) | `paint.rs:759-785` |
| Marquee | *`editor.marquee`* | Role | `Overlay.marquee` | `Drag::Marquee` | — | **Tokens**; planned (STA-688) | `paint.rs:404-414` |
| Razor areas | *`editor.razor`* | Role | `razor_rects` | `Drag::RazorCreate/RazorDrag` | — | **Tokens**; planned (STA-688) | `paint.rs:787-823` |
| Playhead | *`editor.playhead`* | Role (live) | `follow_transport` (`expression.rs:375-397`) | — | — | **Tokens** (`accent`, not `playhead`, `paint.rs:1120`); planned (STA-688) | `paint.rs:1114-1124` |
| Tuning and zone guides, references | none yet | Role | `tuning_guides`, `zone_guides`, `reference_rects` | `separator_press` | — | **Tokens** | `paint.rs:610,825` |
| Scale / key highlighting | — | — | only a "Key" readout | — | — | None | searched `canvas.rs`, `paint.rs` |
| Empty / loading message | none yet | Role | view states (`editor_view.rs:320-350`) | — | — | Native | `editor_view.rs:983-999` |
| Save control | — | — | saves on pointer-up and key-down (`editor_view.rs:677-679`) | — | — | None (no Save button, no `editor.save` role) | `editor_view.rs:837-859` |
| Inspector, chord box, velocity panel, curve editor | — | — | Dioxus only | — | — | Not mounted in Studio | `features/editor/src/ui/{inspector,velocity_panel,curve_editor}.rs` |

**Fallback:** each group falls back to the editor's painter when a role is
missing:

- toolbar and status bar: `chrome::paint` (`expression.rs:435-444,509-519`);
- roll chrome (all six roles or none): `paint.rs:422-425`;
- strip: `strip_chrome`;
- menu: `menu.paint`.

**Defects found while writing this (nothing draws):** the "theme draws it"
flag is set before the font check, so with the roles present and no font
loaded, neither side draws the element.

- **Roll chrome:** keys and ruler (`expression.rs:470-485` vs `:1356`).
- **Strip chrome:** `:491-505`.
- **Stacked-lane headers:** `:448-465`.
- **Gutter edge line:** when the theme draws the roll chrome, the native
  gutter-edge line is skipped (`paint.rs:1247-1253`), so a theme must draw
  it itself (`editor.frame:18`).

**Colours a theme cannot reach yet:** native ruler tick ink
(`paint.rs:1270-1274`), note labels, sibilant bands, the cursor halo
(`paint/cursor.rs:180-183`), and the model's `BAND_COLORS`, `STRING_COLORS`
and `CC_COLORS`. `editor.pitch_*` and `editor.drum_*` cover only the
`DRUM_*` constants. The hex-literal audit `hex_literals`
(`paint/theme.rs:255`) has no callers.

## 5. Audio Editor

**There is no separate Audio Editor.** The Editor view opens an audio item in
`Mode::PitchedAudio` (`expression.rs:201-207`;
`features/editor/src/audio/session.rs:127`). It writes nothing back: "It
writes nothing back" (`expression.rs:198-199`), and `save()` returns early
(`editor_view.rs:844`). A folder of doubles opens the **stacked view**
(`editor_view.rs:208-242`; painter `features/editor/src/paint/stack/paint.rs:73`).

The toolbar, status bar and menu are the shared `editor.*` roles above.

| Element | Role / token | Kind | Data and bindings | Interaction and states | Size variants | Status | Evidence |
|---|---|---|---|---|---|---|---|
| **Take waveform** (pitched view) | *`%wave`* | Slot | `take_waveform` (`canvas.rs:515`) | — | — | **Tokens** (`editor.reference`); slot planned (STA-688) | `paint.rs:658-676` |
| Pitch blobs (notes on audio) | *`%notes`* | Slot | `blob_polygon` (`canvas.rs:583`); colour by cents (`tune_color`) | as notes | — | Native; planned (STA-688) | `paint.rs:892-899` |
| Sibilant bands | inside `%wave` | Slot | `sibilant_bands` (`canvas.rs:553`) | — | — | Native, hard-coded black | `paint.rs:671-673` |
| Stacked-lane headers (name, rail, REF badge) | `editor.lane` | Role | `lane.name/active/reference/tinted/tint` | lane select (`stack/interact.rs:514`) | — | **Role** (console only; missing from `fts-dark.frame`) | `expression.rs:1289-1333`; `editor.frame:36-49` |
| Stacked lane ground and active rail | *`editor.lane`* (body) | Role | — | — | — | Native; planned (STA-688) | `stack/paint.rs:259` |
| Stacked summed waveforms and triggers | *`%wave`* | Slot | `summed_columns` (`waveform.rs:35`) | — | — | Native (`editor.peaks` token); planned (STA-688) | `stack/paint.rs:336` |
| Hit / onset / flam markers | *`editor.hit`* | Role | drum hits | slip, stretch, split, add, remove (`drums/drum.rs:9`) | — | Native; planned (STA-688) | `stack/paint.rs:420` |
| Stacked ruler, sections, beat grid | *`editor.ruler`* reuse | Role | — | — | — | Native; planned (STA-688) | `stack/paint.rs:152,296` |
| Mic chip and mic menu | none yet | Role | — | `stack/interact.rs:466,497` | — | Native | `stack/paint.rs:505,617` |
| Playhead, selected-hit bracket, slip ghost, zoom marquee | none yet | Role | — | — | — | Native (`look.playhead` here, unlike the roll) | `stack/interact.rs:312,325` |
| Stretch markers | — | — | stretch is a gesture with only the slip ghost | — | — | None (no painter) | `stack/interact.rs:412` |
| Gain / dynamics envelopes | — | — | computed (`audio/lanes.rs:35,62`) | — | — | None (no painter; `ui/envelopes.rs:172` has no callers) | `features/editor/src/audio/` |
| Fade handles, clip gain handle | — | — | — | — | — | None in the editor (the arrangement has them) | searched `features/editor/src` |
| Spectral view | — | — | — | — | — | None | searched "spectral" |
| Quantize panel | — | — | Dioxus `QuantizePanelView` | — | — | Not mounted in Studio | `features/editor/src/ui/quantize_panel_view.rs:205` |
| Commit / save | — | — | — | — | — | None (read-only) | `expression.rs:198-199` |

STA-688 owns the choice of which absent elements to build and which to list
as gaps.

## Cross-cutting behaviour

### Layering and overrides

- `FTS_THEME` is a `:`-separated list of files and folders, **base first**
  (`skin/mod.rs:659-670`). Folders are read in name order, `.frame` and `.fm`
  files only (`:705-737`).
- `Theme::from_sources` lets a later layer replace a page, component or
  token **by name** (`layer.rs:26`). A theme on top of `console` is only the
  files it changes.
- **Modes:** `FTS_THEME_MODE` picks one (`skin/mod.rs:665`). Both shipped
  themes define only `dark`.
- **Not supported:** partial overrides inside a page. A layer replaces a
  whole page.

### Live edit and preview

- **Today:** saving a theme file hot-reloads it. `skin::poll` checks mtimes
  at most every 250 ms (`skin/mod.rs:54,675-702`), compiles **inline on the
  UI thread**, and bumps `VERSION` (`:629-637`). Widgets re-record when
  `skin::version()` moves (`mixer_panel.rs:2414,2607`;
  `editor_view.rs:970,1120`; `fx_view.rs:1179`).
- **Not available:** an off-thread compile, an OS file watcher, and a
  preview of an unsaved edit.
- **Planned:** live edit from the Frame Editor and from agents on the
  running arrangement (STA-678, on the Frame chain STA-665..STA-667 and
  STA-670).

### Shared-document update

There is no shared theme document today: each process loads files from disk.
STA-678 owns the live shared-document path (Loro and undo on
`crdt::History`: STA-661).

### Save and revert

Saving means writing the `.frame` file by hand; nothing reverts it. Planned
actions: `THEME_SAVE` and `THEME_REVERT` (Destructive, with a ticket), and
revert restoring a byte-identical theme (STA-685).

### Invalid-theme recovery

- A compile error keeps the **last good theme** and warns once through
  `tracing::warn!` (`skin/mod.rs:692-697`). Any diagnostic fails the whole
  compile (`compile.rs:232-234`).
- **There is no on-screen error.** `THEME_ERRORS` with structured
  diagnostics is planned (STA-685).
- **A role that compiles but draws nothing** is not caught. See the editor
  defects in [MIDI Editor](#4-midi-editor).

### Engine independence

- **Today:** themes compile and draw on the UI thread. The engine's audio
  callback uses preallocated buffers and realtime priority
  (`crates/daw/standalone/src/audio_engine/set_audio.rs:721-780`). Meters
  cross to the UI through atomics and a 30 Hz pump. No theme code runs on
  the audio thread (audit STA-676, section 1e).
- **Required:** a theme edit, a failed reload, or a UI restart must never
  stop or disrupt the engine, playback, meters or transport.
- **Not yet measured:** this is a requirement, not a verified property.
  STA-680 and STA-678 each own a test or a measured run that shows it.

### Performance budgets

**No measured numbers exist yet.** STA-684 (WP0) is recording the native
p50/p99 baselines. This section takes its numbers when they land; until then,
no performance claim is made.

- **Harness:** `src/bin/bench.rs` covers the arrangement and the mixer. It
  runs 240 frames at 5120×1440 over scroll, zoom and fit, and checks
  against a 4.17 ms target (`src/bin/bench.rs:2037`). The recipes are
  `just daw-bench`, `daw-animate` and `daw-studio`.
- **Gate** (from `frame-themes.md` in FastTrackStudios/frame): a themed view
  must be within noise of native p50 and p99.
- **Known costs today:** a theme reload compiles on the UI thread. The
  arrangement's fine waveform pass has a 2 ms build budget per frame
  (`arrangement.rs:777`).
- **No editor bench exists.** STA-688 adds one if STA-684 does not.

### Editing surfaces

| Surface | Today | Planned |
|---|---|---|
| Theme files on disk | Edit `.frame` and save; hot-reloads (`FTS_THEME`) | — |
| Theme picker | None | frame-themes.md phase 5 |
| Studio Action List / MCP | No theme action (searched action ids for THEME, SKIN, STYLE, FRAME_) | `THEME_INSPECT`, `THEME_APPLY`, `THEME_EDIT`, `THEME_RELOAD`, `THEME_ERRORS`, `THEME_SAVE`, `THEME_REVERT`, `VIEW_CAPTURE` (STA-685) |
| Frame MCP (`crates/frame/tools/mcp`) | Edits a Frame *document*: describe, read, write, bind, add_component, `set_theme`, undo, redo (`crates/frame/src/design/session/mod.rs:1110-1234`) | Not wired to Studio's running themes |
| Frame surface actions | `FRAME_SURFACE_LOAD`, `PARAM_SET`, `FEED`, `STATE` act on a surface, not a theme (frame repo, `src/bin/frame-surface.rs:41-78`) | STA-671 |
| Frame Editor | Stub crate only (frame `feat/sta-658-figcraft-dep`, `crates/frame-editor`) | STA-665..STA-667 (editor), STA-663/STA-664 (`FRAME_*` actions) |
| Export | `theme_export` writes `themes/fts-dark.frame` from the painters (`src/bin/theme_export.rs:17`; `skin/export.rs:69`) | — |

## Shipped themes

| Theme | Roles it defines | Missing |
|---|---|---|
| `themes/console/` | `mcp.strip`, `mcp.rackback`, `mcp.step`, `tcp.row`, `rails`, `rail.button`, `rack.unit`, `rack.rail`, `rack.phase`, all 12 `editor.*` | `fx.phase`, `fx.macro*`, every `arrange.*` |
| `themes/fts-dark.frame` (generated) | `mcp.strip`, `tcp.row`, 11 `editor.*` | `editor.lane`, `mcp.rackback`, `mcp.step`, `rails`, `rail.button`, `rack.*`, `fx.*`, `arrange.*` |

Example themes inspired by other DAWs are STA-689 (Arrangement and Mixer)
and STA-690 (the other three views). They describe a look as *inspired by*
another product, never as matching it.

## Keeping this current

This catalogue is maintained, not a snapshot. When a change adds, renames or
removes a role, field, slot or `@` target, update the matching row in the
same change:

1. Replace the proposed *italic* ID with the real one, and set the status to
   **Role**.
2. Cite the code that records it, hit-tests it and supplies its fields.
3. Update the [coverage summary](#coverage-summary). Mark a view complete
   only when every row in its table is **Role** or **None**.

Issues that must update this file when they land: STA-680, STA-685, STA-687,
STA-688, STA-678, STA-689, STA-690 and STA-692.
