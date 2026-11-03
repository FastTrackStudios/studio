# Studio theme catalogue

Everything a Studio theme has to cover, surface by surface: the token each
element should read, the Frame role that draws it (if one does), where it is
drawn, and how far a theme reaches it **today**. The status of every row comes
from reading the painter, not from `docs/frame-theme-contract.md`. Where the
two disagree, this file follows the code.

- **Checked against:** branch `integration/theming` (origin/main, the portable
  Frame-theme work, `1a034e4e` "resolve theme slot indices once per schema",
  and the in-tree Frame copy removed).
- **Companion documents:** `docs/theme-architecture.md` (the design that
  closes the gaps), `docs/frame-theme-contract.md` (role, field and hit-target
  contract), `docs/frame-theme-examples.md` (the example themes).
- **Paths** are repo paths. A symbol after a path names the function, const
  or type that paints the element.

## Status

| Status | Meaning |
|---|---|
| **Role** | A Frame role draws it: layout, geometry, colour and, where interactive, its `@name` hit target. A `%slot` inside a role counts here when the role places it and the host fills it. |
| **Palette·T** | The native painter draws it from a palette field, and a theme token reaches that field at runtime: `skin::palette` in `apps/session/ui/src/skin/mod.rs`, or an `editor.<field>` token through `themed` in `apps/session/ui/src/expression.rs`. Colour only; no geometry. |
| **Palette·F** | The native painter draws it from a palette field that is **fixed**: it comes from `daw_ui::theming::Theme::dark()` through `Palette::from_theme` in `apps/session/ui/src/arrangement.rs`, and `skin::palette` never overwrites it. No theme, token or import reaches it. |
| **Hard-coded** | A literal colour in the painter (or a constant table next to it), or a colour taken straight from session data with a hard-coded derivation and no token. |
| **Not drawn** | The element does not exist in Studio's views yet. |

**The fixed palette fields.** `skin::palette` overwrites 17 of
`Palette`'s colour fields and 10 of its `chrome` fields. These are never
overwritten, so every element drawn from them is **Palette·F**: `surface`,
`row_a`, `row_b`, `divider`, `grid`, `grid_beat`, `grid_sub`, `ruler_bg`,
`ruler_fg`, `ruler_fg2`, `item_edge`, `tcp_tint`, `tcp_column`, the
`track_tint` metric, and `chrome.surface_raised` / `chrome.selected`.
Note that `Palette.surface` (from `theme.arrange.bg`) and the `surface` token
(which sets `chrome.surface` only) are different fields.

**Unused import data.** `daw_ui::theming::ArrangeTheme`
(`features/ui/audio-controls/src/theming/arrange.rs`) has 45 arrange-view
fields, and the REAPER import (`theme_from_reaper` in
`features/ui/daw-ui/src/theming/reaper_import.rs`) fills about 35 of them,
including `edit_cursor`, `play_cursor`, `peaks`, `timesel`, `fade_line`,
`marker`, `region` and `sel_row_bg`. `Palette::from_theme` reads 11 of them,
and nothing in `apps/session` calls the importer. Every Studio view is built
from `Theme::dark()`.

**Token names** in this file are the proposed `StudioTokens` names
(`docs/theme-architecture.md`). A name that a shipped theme already defines in
`apps/session/ui/themes/console/tokens.frame` is written with its `$`
(`$accent`).

## Summary

| Surface | Entries | Role | Palette·T | Palette·F | Hard-coded | Not drawn |
|---|---:|---:|---:|---:|---:|---:|
| Arrangement | 88 | 0 | 27 | 19 | 31 | 11 |
| Track control panel | 27 | 19 | 5 | 2 | 0 | 1 |
| Mixer | 27 | 16 | 2 | 0 | 5 | 4 |
| MIDI editor | 32 | 9 | 14 | 1 | 6 | 2 |
| Audio editor | 15 | 1 | 7 | 0 | 3 | 4 |
| FX view and plugin faces | 10 | 0 | 3 | 2 | 5 | 0 |
| Transport, rails and toolbar | 12 | 3 | 1 | 1 | 7 | 0 |
| **Total** | **211** | **48** | **59** | **25** | **57** | **22** |

**In one sentence per surface:**

- **Arrangement:** no Frame role covers anything. Under a third of it (27 of
  88) follows the theme's colours through `skin::palette`. The lane grounds, grid and
  ruler are fixed to `Theme::dark()`. The cursors, take, comp and kit
  colours, and the fades are literals.
- **TCP:** the full panel is role-themed end to end. The compact panel, the
  row meter, the lane pills and the automation button are native.
- **Mixer:** the stacked strip below the rack is role-themed. The rack's
  unit bodies, phase tints, the overview and the dock ground are literals.
  There is no routing panel, send list or master strip.
- **MIDI editor:** the chrome is role-themed, and every `Look` colour is
  token-reachable. CC lanes, note labels, guitar strings and native ruler ink
  are literals.
- **Audio editor:** it borrows the MIDI editor's `Look`. Its pitch blobs and
  sibilant bands are literals. Stretch markers, gain envelopes, fades and a
  spectral view are not drawn.
- **FX view:** no shipped theme defines the `fx.*` roles. Device inks and the
  plugin faces are fixed artwork.
- **Transport, rails and toolbar:** only the rails are themed. Every Dioxus
  bar (transport, toolbar, shell, Record view) uses its own hard-coded CSS
  palette.

---

## 1. Arrangement

The lanes are recorded once per change (`Arrangement::build`,
`Arrangement::rebuild_hiding`) and replayed per frame through
`Arrangement::replay_lanes` (`apps/session/ui/src/arrangement.rs`). The
overlays are drawn per frame from `ArrangementWidget`'s `draw`
(`apps/session/ui/src/widget.rs`). The palette is rebuilt, and the scene
re-cut, when `skin::version()` moves (`ArrangementWidget::recut`).

### 1.1 Ruler and timeline

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Ruler ground | `arrange.ruler.bg` | none (*`arrange.ruler`* proposed) | `apps/session/ui/src/ruler/bars.rs` `ruler` (`ruler_bg`) | Palette·F |
| Bar numbers and bar ticks | `arrange.ruler.ink` | none | `apps/session/ui/src/ruler/bars.rs` `ruler` (`ruler_fg`) | Palette·F |
| Beat and subdivision ticks | `arrange.ruler.ink-2` | none | `apps/session/ui/src/ruler/bars.rs` `ruler` (`ruler_fg2`) | Palette·F |
| Song-start and count-in ticks | `$accent` | none | `apps/session/ui/src/ruler/bars.rs` `ruler` (`accent`) | Palette·T |
| Ruler bottom rule and lane rules | `$rail.rule` → `arrange.ruler.rule` | none | `apps/session/ui/src/ruler/bars.rs` `ruler`; `apps/session/ui/src/ruler/lanes.rs` `lanes` (`tcp_rule`) | Palette·T |
| Lane labels (SECTIONS, MARKERS, TEMPO) | `$text.faint` | none | `apps/session/ui/src/ruler/lanes.rs` `lanes`; `apps/session/ui/src/ruler/tempo.rs` `tempo` | Palette·T |

### 1.2 Grid

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Bar lines | `arrange.grid.bar` | none | `apps/session/ui/src/ruler/bars.rs` `grid` (`grid`, faded by `crowded`) | Palette·F |
| Beat lines | `arrange.grid.beat` | none | `apps/session/ui/src/ruler/bars.rs` `grid` (`grid_beat`) | Palette·F |
| Subdivision lines | `arrange.grid.sub` | none | `apps/session/ui/src/ruler/bars.rs` `grid` (`grid_sub`) | Palette·F |
| Grid-in-force / quantize regions | `arrange.marks.grid`, `arrange.marks.no-quantize` | none | `apps/session/ui/src/marks_strip.rs` `tint` | Hard-coded |

### 1.3 Rows

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Arrange ground (below the last row) | `arrange.bg` | none | `apps/session/ui/src/widget.rs` `draw` (`palette.surface`) | Palette·F |
| Row stripe, even and odd | `arrange.row.a`, `arrange.row.b` | none (*`arrange.lane`*) | `apps/session/ui/src/arrangement/record.rs` `record_row_head` | Palette·F |
| Row divider | `arrange.row.divider` | none | `apps/session/ui/src/arrangement/record.rs` `record_row_head` | Palette·F |
| Fixed-lane seams | `arrange.lane.seam` (divider × 0.6) | none | `apps/session/ui/src/arrangement/record.rs` `record_row_head` | Palette·F |
| Selected-row tint in the lanes | `arrange.row.selected` | none | nowhere (`ArrangeTheme::sel_row_bg` is never read) | Not drawn |
| Pinned lane head shadow | `arrange.lane.pinned-shadow` | none | `apps/session/ui/src/widget.rs` `paint_pinned` | Hard-coded |

### 1.4 Track colours and tint

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| A track with no colour | `track.none` (today `$text.faint`) | none | `apps/session/ui/src/tcp.rs` `track_color` | Palette·T |
| Item colour from its track or take | data; `track.tint-strength` | none | `apps/session/ui/src/arrangement.rs` `item_color` | Hard-coded |
| Row tint toward the track colour | `tcp.row.ground`, `track.tint-strength`, `track.selected-lift` | `tcp.row` reads it as `track.tint` | `apps/session/ui/src/tcp.rs` `row_tint` (`tcp_tint`, `track_tint`, `SELECTED_LIFT`) | Palette·F |
| Folder band (rail) colour | `tcp.rail.ground`, `track.band-strength` | `tcp.row` reads it as `track.band` | `apps/session/ui/src/tcp.rs` `folder_band` (`tcp_tint`, `tcp_gutter`) | Palette·F |
| Item colour while mixing (always the track's) | data | none | `apps/session/ui/src/arrangement.rs` `item_color` | Hard-coded |

### 1.5 Items

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Item body (fill alpha 0.42, outlined 0.12) | `arrange.item.body-alpha`, `arrange.item.body-alpha-outlined` | none (*`arrange.item`*) | `apps/session/ui/src/arrangement/record.rs` `record_item`; `apps/session/ui/src/arrangement.rs` `body_alpha` | Hard-coded |
| Item edge (outline) | `arrange.item.edge` | none | `apps/session/ui/src/arrangement.rs` `outlines_overlay` (Outlined option only, in the item's colour); `Palette.item_edge` is read only by the editor | Hard-coded |
| Item name | `arrange.item.label` (today `$text`) | none | `apps/session/ui/src/arrangement.rs` `titles`, `letter` | Palette·T |
| Item name plate | `arrange.item.title-plate` | none | `apps/session/ui/src/arrangement.rs` `letter` (`FLOOR`) | Hard-coded |
| Selected item | `arrange.item.selected` (today `$text`) | none | `apps/session/ui/src/arrangement.rs` `selection_overlay` | Palette·T |
| Muted item (alpha 0x66) | `arrange.item.muted-alpha` | none | `apps/session/ui/src/arrangement.rs` `item_color` | Hard-coded |
| Locked item | `arrange.item.locked` | none | nowhere (`daw_proto::Item::locked` is never read by the UI) | Not drawn |
| Item on a lane that does not play (× 0.45) | `arrange.lane.idle-alpha` | none | `apps/session/ui/src/arrangement/record.rs` `record_item` (`faint`) | Hard-coded |
| Take lanes: body in the take's colour | `take.0` … `take.11` (12 hues, then light and dark rounds); `arrange.take.body-alpha` | none | `apps/session/engine/src/take_colors.rs` `HUES`, `swatch`; `apps/session/ui/src/arrangement/record.rs` `muted`; `apps/session/ui/src/arrangement.rs` `TAKE_BODY` | Hard-coded |
| Fixed-lane line in the TCP (prev, name, next, show) | `tcp.lane.ink`, `$accent` | none | `apps/session/ui/src/overlay.rs` `lane_line`, `lanes_plus`; `apps/session/ui/src/swipe.rs` `tcp_lanes` | Palette·T |
| Swipe-comp sweep | `arrange.swipe` (today `$accent`) | none | `apps/session/ui/src/swipe.rs` `overlay` | Palette·T |
| Comp strip: kept (gold), pending, ranks | `arrange.comp.kept`, `arrange.comp.pending`, `arrange.take.rank-up`, `arrange.take.rank-down` | none | `apps/session/ui/src/arrangement.rs` `comp_strip`, `comp_pending_marks`, `RANK_UP`, `RANK_DOWN` | Hard-coded |
| Comp source rings | `arrange.comp.source-ring` | none | `apps/session/ui/src/arrangement.rs` `comp_source_outlines` | Hard-coded |
| Take rating stars | `arrange.take.star` | none | `apps/session/ui/src/arrangement.rs` `take_mark`, `take_marks_overlay` | Hard-coded |
| Folder item (a shut folder's summary item), alpha 0.4 | `arrange.item.folder-alpha` | none | `apps/session/ui/src/arrangement.rs` `FOLDER_BODY`; `apps/session/ui/src/folder_item/draw.rs` `side_colour` | Hard-coded |

### 1.6 Waveforms

The peaks are `LaneWave`s indexed by item body and drawn at replay (coarse,
then the fine pass, then sample dots) in `replay_command`
(`apps/session/ui/src/arrangement.rs`). Every pass uses `LaneWave::color`,
which is the item's ink.

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Peak body | `arrange.wave.peak` (default: the item ink) | none (*`%wave`* slot in *`arrange.item`*) | `apps/session/ui/src/arrangement.rs` `replay_command`, `lane_wave_paths`, `fine` | Hard-coded |
| RMS body | `arrange.wave.rms` | none | nowhere | Not drawn |
| Clip indicator | `arrange.wave.clip` | none | nowhere | Not drawn |
| Zero line | `arrange.wave.zero` | none | nowhere | Not drawn |
| Selected item's waveform | `arrange.wave.selected` | none | nowhere (selection is an outline only) | Not drawn |
| Kit layers on a shut drum folder (kick, snare, toms) | `arrange.kit.kick`, `arrange.kit.snare`, `arrange.kit.toms` | none | `apps/session/ui/src/arrangement.rs` `kit_color` | Hard-coded |

### 1.7 MIDI items

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Note bars in a MIDI item | `arrange.midi.note` (default: the item colour) | none (*`%notes`* slot) | `apps/session/ui/src/arrangement.rs` `midi_preview`; `apps/session/ui/src/arrangement/record.rs` `record_item` | Hard-coded |
| Note hint strip on a folder of audio (alpha 0.35) | `arrange.midi.hint-alpha` | none | `apps/session/ui/src/arrangement/record.rs` `record_item` | Hard-coded |
| Velocity in the item | `arrange.midi.velocity` | none | nowhere | Not drawn |

### 1.8 Fades and crossfades

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Fade shading | `arrange.fade.shade` | none | `apps/session/ui/src/arrangement/record.rs` `record_item`; `apps/session/ui/src/arrangement.rs` `FADE_SHADE` | Hard-coded |
| Fade curve and handles (hover, drag) | `arrange.fade.line` (today `$text`) | none | `apps/session/ui/src/arrangement.rs` `fade_overlay` | Palette·T |
| Crossfade (as its own shape) | `arrange.crossfade` | none | nowhere: two overlapping items each draw their own fade | Not drawn |
| Item volume dial | `arrange.item.dial`, `arrange.item.dial-ground` | none | `apps/session/ui/src/arrangement.rs` `volume_overlay` (`text`, `surface`) | Palette·T |

### 1.9 Envelopes and automation lanes

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Automation lane ground, divider, grid | `arrange.env.ground`, `arrange.env.divider`, `arrange.grid.bar` | none | `apps/session/ui/src/automation_draw.rs` `bodies` (`row_b`, `divider`, `grid`) | Palette·F |
| Envelope curve and points, by kind | `env.volume` = `$meter.safe`, `env.pan` = `$pan`, `env.mute` = `$mute`, `env.width` = `$solo`, `env.param` = `$accent`, leveler stages | none | `apps/session/ui/src/automation_draw.rs` (the `Kind` match), `points`, `square` | Palette·T |
| Automation lane headers | `$rail.ground`, `$rail.rule`, `$text*`, `$field` | none | `apps/session/ui/src/automation_draw.rs` `headers` | Palette·T |
| Lit point and value readout | `$text` | none | `apps/session/ui/src/automation_draw.rs` `points`, `windows` | Palette·T |
| Take gain lines | `$text` | none | `apps/session/ui/src/automation_draw.rs` `take_lines` | Palette·T |
| Leveler lines | `$text`, `arrange.bg` | none | `apps/session/ui/src/automation_draw.rs` `leveler_lines` | Palette·T |

### 1.10 Cursors, selection and loop

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Play cursor (line, trail, glow) | `arrange.cursor.play` | none | `apps/session/ui/src/cursor.rs` `Look::default`, `paint` | Hard-coded (`#facc15`) |
| Edit cursor | `arrange.cursor.edit` | none | `apps/session/ui/src/cursor.rs` `EDIT_CURSOR`, `paint_edit` | Hard-coded (`#3b82f6`) |
| Time selection wash and edges | `arrange.selection.time`, `arrange.selection.edge` (today `$accent` × 0.22, × 0.85) | none | `apps/session/ui/src/cursor.rs` `paint_edit` | Palette·T |
| Loop region | `arrange.loop`, `arrange.ruler.loop` | none | nowhere (only the transport's Loop button) | Not drawn |
| Touch grip on the panel edge | `arrange.touch.grip` | none | `apps/session/ui/src/widget.rs` `paint_grip` | Hard-coded |
| Touch scrub handle in the bars lane | `arrange.touch.handle` | none | `apps/session/ui/src/widget.rs` `paint_handle` | Hard-coded |

### 1.11 Markers, regions, chords and marks

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Regions (sections): band and name | `arrange.region` (fallback `$accent`), `$text` | none (*`arrange.marker`*) | `apps/session/ui/src/ruler/lanes.rs` `lanes` | Palette·T |
| Markers: flag and name | `arrange.marker` (fallback `ruler_fg`), `$text` | none | `apps/session/ui/src/ruler/lanes.rs` `lanes` | Palette·F |
| Region and marker lines over the lanes | `arrange.region.line`, `arrange.marker.line` | none | `apps/session/ui/src/ruler/lanes.rs` `lane_lines` | Palette·F |
| Chord lane | `$text`, `$text.faint`, `$rail.rule` | none | `apps/session/ui/src/ruler/chords.rs` `chord_lane` | Palette·T |
| Key-change tag | `arrange.ruler.key-tag` | none | `apps/session/ui/src/ruler/chords.rs` `chord_lane` (violet) | Hard-coded |
| Track marks strip | `arrange.marks.fill`, `arrange.marks.note` | none | `apps/session/ui/src/marks_strip.rs` `tint`, `paint` | Hard-coded |
| Points of interest | `arrange.poi.open`, `arrange.poi.done` | none | `apps/session/ui/src/poi_paint.rs` `paint` (`OPEN`, `DONE`) | Hard-coded |

### 1.12 Tempo lane

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Tempo and time-signature lane | `$rail.rule`, `$text.faint`, `$accent`, `$text` | none | `apps/session/ui/src/ruler/tempo.rs` `tempo` | Palette·T |
| Tempo grid lane (Edit mode) | as above | none | `apps/session/ui/src/ruler/tempo.rs` `grid_lane` | Palette·T |

### 1.13 Record preview

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Item growing while recording | `arrange.record.item`, `arrange.record.wave` | none | nowhere: a take appears when the engine publishes it | Not drawn |
| Armed-row lane tint | `arrange.record.armed` | none | nowhere | Not drawn |

### 1.14 Drag, ghost and edit previews

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Move or trim ghost | `arrange.ghost` (today `$text`, fill × 0.12) | none | `apps/session/ui/src/arrangement.rs` `selection_overlay` | Palette·T |
| Slip preview | `arrange.row.a/b`, `arrange.fade.shade`, item colour | none | `apps/session/ui/src/arrangement.rs` `slip_overlay` | Palette·F |
| Razor areas | `arrange.razor` (today `$accent`) | none | `apps/session/ui/src/arrangement.rs` `razor_overlay` | Palette·T |
| Rubber-band marquee | `arrange.marquee` (today `$accent`) | none | `apps/session/ui/src/arrangement.rs` `marquee_overlay` | Palette·T |
| Drum hit markers: as played, quantized, by hand, doubt | `arrange.hit.flag`, `arrange.hit.flag-hand`, `$meter.warn`, `$accent` | none | `apps/session/ui/src/arrangement.rs` `hits_overlay` | Hard-coded |
| Hit drag and hit marquee | `$accent` | none | `apps/session/ui/src/arrangement.rs` `hit_drag_overlay`, `hit_marquee_overlay` | Palette·T |
| Stretch rate strip | `arrange.stretch.extreme`, `arrange.stretch.audible`, `arrange.stretch.subtle` | none | `apps/session/ui/src/arrangement.rs` `stretch_tint`, `stretch_strip` | Hard-coded |
| Stream veil | `arrange.stream.veil`, `arrange.stream.track` | none | `apps/session/ui/src/arrangement.rs` `stream_overlay` | Hard-coded |
| Outlines mode | `arrange.item.edge`, `$meter.danger` | none | `apps/session/ui/src/arrangement.rs` `outlines_overlay` | Palette·F |
| Cuts | `arrange.row.divider` | none | `apps/session/ui/src/arrangement.rs` `cuts_overlay` | Palette·F |
| Gap marks | `arrange.gap.mark` | none | `apps/session/ui/src/arrangement.rs` `gaps_overlay`, `gap_mark` (`NOTCH`) | Hard-coded |
| Collaborator ghosts and pointers | peer colour (data), `collab.pointer.ink` | none | `apps/session/ui/src/ghosts.rs` `paint`, `paint_pointer`, `color_of` | Hard-coded |
| Rename field | `$field`, `$text`, `$accent` | none | `apps/session/ui/src/rename.rs` `paint` | Palette·T |
| Notice | `$field`, `$text`, `$meter.warn` | none | `apps/session/ui/src/notice.rs` `paint` | Palette·T |
| Automation menu (popup) | `$text*`, `$accent`, `$rail.rule` | none | `apps/session/ui/src/popup.rs` `Popup::paint` | Palette·T |
| Scrollbars | `arrange.scrollbar` | none | `apps/session/ui/src/studio.rs` (`daw_ui::studio::lanes::Colors::from_theme(Theme::dark())`) | Palette·F |

---

## 2. Track control panel

The full panel's rows are recorded through the `tcp.row` role
(`record_panel` in `apps/session/ui/src/arrangement.rs`, with the `%steps`
and `%icon` slots filled by the host). The live pass is `theme.live(ROW, …)`
in `control_row` (`apps/session/ui/src/overlay.rs`), and hit testing is
`row_spot_at` through `theme.hit`. `skin::row_control` maps ten `@name`s.
The role is skipped when `Tcp::compact` is set, and the zoomed-out `Bar`
density is the native `panel_bar` scene. The token column names the console
theme's token. In `apps/session/ui/themes/console/tcp.frame`, several of these
are still literals (see the audit in `docs/theme-architecture.md`).

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Row ground and tint | `tcp.row.ground` → field `track.tint` | `tcp.row` (Tint) | `apps/session/ui/themes/console/tcp.frame`; fields `skin::row_fields` | Role |
| Button-column gutter | `$surface` (literal `#09090b` in console) | `tcp.row` (Gutter) | `apps/session/ui/themes/console/tcp.frame` | Role |
| Indent column and rules | `$field`, `$border` (literals in console) | `tcp.row` (Column, Rule, Edge) | `apps/session/ui/themes/console/tcp.frame` | Role |
| Folder rail (band) | field `track.band` | `tcp.row` (Rail) | `apps/session/ui/themes/console/tcp.frame` | Role |
| Folder open/shut mark, `@folder` | `ink.on-light`, `ink.on-dark` | `tcp.row` (Fold) | `apps/session/ui/themes/console/tcp.frame` | Role |
| Folder depth steps | data (ancestor bands) | `tcp.row` `%steps` | `apps/session/ui/src/tcp.rs` `folder_steps` | Role |
| Track number | `ink.on-light`, `ink.on-dark` | `tcp.row` (Number) | `apps/session/ui/themes/console/tcp.frame` | Role |
| Track icon | `ink.on-light`, `ink.on-dark` | `tcp.row` `%icon` | `apps/session/ui/src/track_icon.rs` `paint`, ink from `apps/session/ui/src/tcp.rs` `ink_on` | Role |
| Name, `@name`, selected ink | `$text`, `$text.dim` | `tcp.row` (Name) | `apps/session/ui/themes/console/tcp.frame` | Role |
| Name hover underline | `$accent` (literal in console) | `tcp.row` (Name hover) | `apps/session/ui/themes/console/tcp.frame` | Role |
| Mute, `@mute` | `$mute`, `$hardware*` | `tcp.row` → component `mute` | `apps/session/ui/themes/console/controls.frame` | Role |
| Solo, `@solo` | `$solo`, `$hardware*` | `tcp.row` → `solo` | `apps/session/ui/themes/console/controls.frame` | Role |
| Record arm, `@arm` | `$rec`, `$field` | `tcp.row` → `arm` | `apps/session/ui/themes/console/controls.frame` | Role |
| Phase, `@phase` | `$accent`, `$hardware` | `tcp.row` → `phase` | `apps/session/ui/themes/console/controls.frame` | Role |
| FX count, `@fx` | `$text*`, `$field` | `tcp.row` → `fx` | `apps/session/ui/themes/console/controls.frame` | Role |
| Routing, `@routing` | `$route.out`, `$route.send`, `$route.recv` | `tcp.row` → `routing` | `apps/session/ui/themes/console/controls.frame` | Role |
| Volume knob, `@volume` | `$accent`, `$hardware*` | `tcp.row` → `volume` | `apps/session/ui/themes/console/controls.frame` | Role |
| Pan knob, `@pan` | `$pan`, `$hardware*` | `tcp.row` → `pan` | `apps/session/ui/themes/console/controls.frame` | Role |
| Record input (row two) | `$field`, `$text.dim`, `$text.faint` | `tcp.row` (Row two) | `apps/session/ui/themes/console/tcp.frame` | Role |
| Monitor | `tcp.monitor` | none | nowhere (`row::Control` has no monitor) | Not drawn |
| Row meter | `$meter.safe`, `$meter.warn`, `$meter.danger` | none | `apps/session/ui/src/overlay.rs` `row_meters` | Palette·T |
| Automation button (drawn over the role) | `$field`, `$text.dim` | none | `apps/session/ui/src/tcp.rs` `automation_button` | Palette·T |
| Lane pills (prev, name, next, show) | `$accent`, `$text`, `$text.dim` | none | `apps/session/ui/src/overlay.rs` `lane_line`, `lanes_plus` | Palette·T |
| Compact panel rows (role skipped) | `$field`, `$combo`, `$text*`, `palette.chrome` | none | `apps/session/ui/src/tcp.rs` `draw_row`, `row_one`; `apps/session/ui/src/overlay.rs` `control_row` | Palette·T |
| Built-in row (no theme), M/S art | `palette.chrome` (`$hardware*`, `$mute`, `$solo`) | none | `apps/session/ui/src/tcp.rs` `draw_row`, `mute_lit`, `solo_lit` | Palette·T |
| Bar-density rows (zoomed out) | `tcp.row.ground`, `tcp.rail.ground` | none | `apps/session/ui/src/arrangement.rs` `record_panel` (`panel_bar`) | Palette·F |
| Panel column behind indents | `tcp.column` | none | `apps/session/ui/src/tcp.rs` `draw_row` (`tcp_column`) | Palette·F |

---

## 3. Mixer

`Mixer::build` (`apps/session/ui/src/mcp.rs`) records each stacked desktop
strip through `themed`: `mcp.rackback`, the rack, `mcp.step` per ancestor, then
`mcp.strip`. Live and touch strips use the role only when the theme sets
`strip.live` / `strip.touch`. The live pass is `theme.live(STRIP, …)` in
`controls` (`apps/session/ui/src/overlay.rs`), and hit testing is
`mcp::control_in`.

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Strip body and ground | `mixer.strip.ground` → field `track.ground`, `track.edge` | `mcp.strip` | `apps/session/ui/themes/console/strip.frame`; `apps/session/ui/src/mcp.rs` `strip_ground`, `strip_edge` | Role |
| Rack ground, edge, selection rule | `mixer.strip.ground`, `$accent` | `mcp.rackback` (fallback fills in `apps/session/ui/src/mcp.rs` `themed`) | `apps/session/ui/themes/console/strip.frame` | Role |
| Folder steps under nested strips | field `folder.tint` | `mcp.step` | `apps/session/ui/themes/console/strip.frame` | Role |
| Folder button, `@folder` | `$text*` | `mcp.strip` | `apps/session/ui/themes/console/strip.frame` | Role |
| Fader and cap, `@fader` | `$hardware*`, `$accent` | `mcp.strip` → `volume.fader` | `apps/session/ui/themes/console/controls.frame` | Role |
| Volume readout | `$text*` | `mcp.strip` | `apps/session/ui/themes/console/strip.frame` | Role |
| Meters L/R, hold, peak | `$meter.safe`, `$meter.warn`, `$meter.danger` (plus a literal 7-step ladder in console) | `mcp.strip` (live) | `apps/session/ui/themes/console/strip.frame` | Role |
| Meter scale | `$text.faint` | `mcp.strip` | `apps/session/ui/themes/console/strip.frame` | Role |
| Clip latch, `@clip` | `$mute` / `meter.clip` | `mcp.strip` | `apps/session/ui/themes/console/strip.frame` | Role |
| Pan, `@pan` | `$pan` | `mcp.strip` → `pan` | `apps/session/ui/themes/console/controls.frame` | Role |
| Mute, solo, arm, monitor | `$mute`, `$solo`, `$rec`, `$monitor.tape` | `mcp.strip` → components | `apps/session/ui/themes/console/controls.frame` | Role |
| Name plate, `@name` | `$text`, `$plate` | `mcp.strip` | `apps/session/ui/themes/console/strip.frame` | Role |
| FX count, `@fx` | `$field`, `$text*` | `mcp.strip` → `fx` / `fx.compact` | `apps/session/ui/themes/console/controls.frame` | Role |
| Routing (counts), `@routing` | `$route.out`, `$route.send`, `$route.recv` | `mcp.strip` → `routing` | `apps/session/ui/themes/console/controls.frame` | Role |
| Record input display | `$field`, `$text.dim` | `mcp.strip` | `apps/session/ui/themes/console/strip.frame` | Role |
| Live and touch strips without the opt-in tokens | `palette.chrome`, `$meter.*` | none | `apps/session/ui/src/mcp.rs` `strip`; `apps/session/ui/src/overlay.rs` `controls` | Palette·T |
| Built-in strip (no theme) | as above | none | `apps/session/ui/src/mcp.rs` `strip`, `bottom`, `tinted_band` | Palette·T |
| Tone rack unit ground, head, bypass veil | `$rack.well`, `$text.faint`, `$text.dim` | `rack.unit`, `rack.rail`, `rack.phase` (console only) | `apps/session/ui/themes/console/rack.frame`; `apps/session/ui/src/tone.rs` | Role |
| Tone rack unit bodies (curves, knobs, meters) | `fx.ink.eq`, `fx.ink.gate`, `fx.ink.saturate*`, `fx.ink.deess`, `fx.ink.delay`, `fx.ink.reverb`, `fx.ink.decay`, `arrange.grid.*` | `%body` slot | `apps/session/ui/src/tone.rs` `EQ_INK`, `GATE_INK`, `SAT_INK`, `DEESS_INK`, `DELAY_INK`, `REVERB_INK`, `DECAY_INK` (plus `grid`, `grid_beat`) | Hard-coded |
| Mix-phase tints (Rescue, Balance, Tone, Polish …) | `phase.rescue` … (8) | field `phase.tint` | `apps/session/ui/src/tone.rs` `phase_tint` | Hard-coded |
| Unselected rack dimming | `mixer.rack.dim` | none | `apps/session/ui/src/overlay.rs` `controls` | Hard-coded |
| Mixer overview (mini-map) | `mixer.overview.*` | none | `apps/session/ui/src/mixer_panel.rs` `overview` | Hard-coded |
| Mixer dock ground (Dioxus) | `desk` | none | `apps/session/ui/src/mixer_panel.rs` `DawPanels`, `WebDawPanels` | Hard-coded |
| Send list, send levels | `mixer.send.*` | none | nowhere (the strip shows counts only) | Not drawn |
| Routing panel | `mixer.routing.*` | none | nowhere (`mcp::click` returns `None` for `Control::Routing`) | Not drawn |
| FX slot list on the strip | `mixer.fx-slot.*` | none | nowhere (one `@fx` count button) | Not drawn |
| Master strip | `mixer.master.*` (*`mcp.master`*) | none | nowhere | Not drawn |

---

## 4. MIDI editor

One Editor view (`EditorPanel` / `EditorView`, `apps/session/ui/src/editor_view.rs`)
holds `Expression` (`apps/session/ui/src/expression.rs`) over
`features/editor`. Colours come from `Look`
(`features/editor/src/paint/paint.rs`), built by `base_look` from the
arrangement palette and then overridden by `editor.<field>` tokens in
`themed`. Pitch and drum colours are remapped by hex key through
`set_recolour`. The `editor_view.rs` palette is
`Palette::from_theme(&Theme::dark())` **without** `skin::palette`, so `Look`
starts from fixed values and only the `editor.*` tokens change it.

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Toolbar | `editor.surface_bar`, `editor.control*` | `editor.bar`, `editor.button` | `apps/session/ui/src/expression.rs` `themed_bar`; `apps/session/ui/themes/console/editor.frame` | Role |
| Status bar | as toolbar | `editor.bar`, `editor.button` | `apps/session/ui/src/expression.rs` `themed_bar` | Role |
| Piano keys | `editor.key_white`, `editor.key_black`, `editor.key_label` | `editor.keys`, `editor.key` (all six roll roles, or none) | `apps/session/ui/src/expression.rs` (`ROLL_CHROME`) | Role |
| Drum lane names | as keys | `editor.key` | `apps/session/ui/src/expression.rs` | Role |
| Key groups | `editor.key_label` | `editor.keygroup` | `apps/session/ui/src/expression.rs` | Role |
| Ruler and bar/beat ticks | `editor.surface_bar`, `editor.text_dim`, `editor.gold` | `editor.ruler`, `editor.tick` | `apps/session/ui/src/expression.rs` | Role |
| Ruler ink when no theme draws the roll chrome | `editor.ruler.tick`, `editor.ruler.tick-bar` | none | `features/editor/src/paint/paint.rs` `ruler` (`theme::TEXT_DIM`, `theme::GRID_SUB`) | Hard-coded |
| Markers | `editor.gold` | `editor.marker` | `apps/session/ui/src/expression.rs` | Role |
| Velocity strip chrome | `editor.panel`, `editor.text_dim` | `editor.strip` | `apps/session/ui/src/expression.rs` | Role |
| Context menu | `editor.panel`, `editor.control_hover` (shadow literal `#00000066`) | `editor.menu`, `editor.menuitem` | `apps/session/ui/themes/console/editor.frame` | Role |
| Row bands (white/black keys, drum families) | `editor.row_a`, `editor.row_b`, `editor.band_tint` | none | `features/editor/src/paint/paint.rs` `rows` | Palette·T |
| Beat and subdivision grid | `editor.grid_beat`, `editor.grid_sub` | none | `features/editor/src/paint/paint.rs` `grid` | Palette·T |
| Octave line | `editor.octave_line` | none | `features/editor/src/paint/paint.rs` | Palette·T |
| Notes by pitch class | `editor.pitch_0` … `editor.pitch_11` | none (*`%notes`*) | `features/editor/src/paint/paint.rs` `notes`; `apps/session/ui/src/expression.rs` `themed` | Palette·T |
| Drum notes | `editor.drum_kick`, `_snare`, `_tom`, `_hat`, `_ride`, `_cymbal`, `_other` | none | as above | Palette·T |
| Note outline (border) | `editor.border_strong` | none | `features/editor/src/paint/paint.rs` `notes` | Palette·T |
| Selected notes | `editor.selected` | none | `features/editor/src/paint/paint.rs` `notes` | Palette·T |
| Note labels | `editor.note.label` | none | `features/editor/src/paint/paint.rs` `notes` (`Color::BLACK`) | Hard-coded |
| Guitar string and band colours | `editor.string.0…`, `editor.band.0…3` | none | `features/editor/src/model/rows.rs` `STRING_COLORS`, `BAND_COLORS` | Hard-coded |
| Velocity stems (in the note's colour) | `editor.pitch_*`, `editor.selected` | none (*`%stems`*) | `features/editor/src/paint/paint.rs` `strip_stems` | Palette·T |
| Velocity-coloured notes | `editor.velocity.low`, `editor.velocity.high` | none | nowhere (colour is by pitch class only) | Not drawn |
| CC lanes | `editor.cc.0` … `editor.cc.7` | none (*`%cc`*) | `features/editor/src/model/cc.rs` `CC_COLORS`; `features/editor/src/paint/paint.rs` `controllers` | Hard-coded |
| Expression curves | `editor.curve.*` | none (*`%curves`*) | `features/editor/src/paint/paint.rs` `curves` (lane colour from the model) | Hard-coded |
| Note handles | `editor.handle`, `editor.accent` | none | `features/editor/src/paint/paint.rs` `handles` | Palette·T |
| Time selection | `editor.selected` | none | `features/editor/src/paint/paint.rs` `time_selection` | Palette·T |
| Razor areas | `editor.razor` | none | `features/editor/src/paint/paint.rs` `razors` | Palette·T |
| Marquee and draft | `editor.accent`, `editor.text_dim` | none | `features/editor/src/paint/paint.rs` `draft` | Palette·T |
| Playhead | `editor.playhead` (read as `editor.accent` today) | none | `features/editor/src/paint/paint.rs` `playhead` | Palette·T |
| Tuning and zone guides | `editor.gold`, `editor.zone` | none | `features/editor/src/paint/paint.rs` `guides` | Palette·T |
| Cursor halo | `editor.cursor.halo` | none | `features/editor/src/paint/cursor.rs` | Hard-coded |
| Scale and key highlighting | `editor.scale` | none | nowhere | Not drawn |
| Empty and loading message | `$text.dim` | none | `apps/session/ui/src/editor_view.rs` (fixed `Palette::from_theme`) | Palette·F |

---

## 5. Audio editor

There is no separate audio editor: the Editor view opens an audio item in
`Mode::PitchedAudio`, and a folder of doubles opens the stacked view
(`features/editor/src/paint/stack/paint.rs` `stack_scene`). It shares the
MIDI editor's toolbar, status bar, menu and `Look`.

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Stacked-lane headers (name, rail, REF badge) | `editor.text*`, `editor.control_selected` | `editor.lane` | `apps/session/ui/src/expression.rs`; `apps/session/ui/themes/console/editor.frame` | Role |
| Take waveform (pitched view) | `editor.reference` | none (*`%wave`*) | `features/editor/src/paint/paint.rs` `audio` | Palette·T |
| Stacked lane ground and active rail | `editor.row_a`, `editor.row_b`, `editor.gutter_bg` | none | `features/editor/src/paint/stack/paint.rs` `lane_ground` | Palette·T |
| Stacked summed waveforms | `editor.peaks` | none | `features/editor/src/paint/stack/paint.rs` `lane_material` | Palette·T |
| Stacked ruler, sections, beat grid | `editor.surface_bar`, `editor.grid_*`, `editor.text_dim` | none | `features/editor/src/paint/stack/paint.rs` `ruler`, `timebase` | Palette·T |
| Hit, onset and flam markers | `editor.accent`, `editor.text*` | none | `features/editor/src/paint/stack/paint.rs` `hits` | Palette·T |
| Playhead, selected-hit bracket, slip ghost | `editor.playhead`, `editor.accent` | none | `features/editor/src/paint/stack/interact.rs` `overlays` | Palette·T |
| Mic chip and mic menu | `editor.panel`, `editor.text*` (one literal ground) | none | `features/editor/src/paint/stack/paint.rs` `lane_labels`, `mic_menu` | Palette·T |
| Pitch blobs (notes on audio), by cents | `editor.tune.0` … `editor.tune.4` | none | `features/editor/src/paint/theme.rs` `tune_color` | Hard-coded |
| Sibilant bands | `editor.sibilant` | none | `features/editor/src/paint/paint.rs` `audio` (`Color::BLACK`) | Hard-coded |
| Pitched-view references | `editor.text_dim` (and a literal shadow) | none | `features/editor/src/paint/paint.rs` `references` | Hard-coded |
| Stretch markers | `editor.stretch` | none | nowhere | Not drawn |
| Gain and dynamics envelopes | `editor.env.*` | none | nowhere (computed, not painted) | Not drawn |
| Fade and clip-gain handles | `editor.fade`, `editor.gain` | none | nowhere | Not drawn |
| Spectral view | `editor.spectral.*` | none | nowhere | Not drawn |

---

## 6. FX view and plugin faces

The FX view (`apps/session/ui/src/fx_view.rs`) lays out the selected track's
processors as devices. It reuses the rack painters (`apps/session/ui/src/tone.rs`)
and frame's face documents (`apps/session/ui/src/faces.rs`,
`apps/session/ui/src/plugin_faces.rs`). Its palette is
`skin::palette(Palette::from_theme(Theme::dark()))`.

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| FX view ground (Dioxus) | `desk` | none | `apps/session/ui/src/fx_view.rs` `FxPanel` | Hard-coded |
| Device header text | `$text`, `$text.dim`, `$text.faint` | none | `apps/session/ui/src/fx_view.rs` `device_header` | Palette·T |
| Device on lamp | `fx.device.on` | none | `apps/session/ui/src/fx_view.rs` `device_header` | Hard-coded |
| Device row bands and dividers | `arrange.row.a`, `arrange.row.b`, `arrange.row.divider` | none | `apps/session/ui/src/fx_view.rs` | Palette·F |
| Phase bar | `phase.*`, `$text*` | `fx.phase` (supported; no shipped theme defines it) | `apps/session/ui/src/fx_view.rs` (`has_role("fx.phase")`) | Palette·T |
| Macros | `$text*`, `arrange.row.divider` | `fx.macro[.<phase>[.<track>]]` (supported; none defined) | `apps/session/ui/src/macros.rs` | Palette·T |
| Device bodies (rack painters at device size) | `fx.ink.*` | none | `apps/session/ui/src/tone.rs` (`*_INK`, `ACROSS`, `ess_trace`) | Hard-coded |
| Rack grids inside devices | `fx.grid`, `fx.grid.beat` | none | `apps/session/ui/src/tone.rs` (`grid`, `grid_beat`) | Palette·F |
| Plugin faces (FET, opto, console EQ, tube, tape, chorus …) | none: each face document carries its own colours | the face documents in `features/ui/plugin-faces/generated` (not theme roles) | `apps/session/ui/src/faces.rs` `design_dir`, `FrameFace`; `apps/session/ui/src/plugin_faces.rs` `face` | Hard-coded |
| Reverb and delay picture faces | none | none | `apps/session/ui/src/faces.rs` (`reverb_ui`, `delay_ui` painters) | Hard-coded |

---

## 7. Transport, rails and toolbar

| Element | Token(s) | Frame role | Drawn in | Status |
|---|---|---|---|---|
| Rails frame (top and side gutters, rules) | `$rail.ground`, `$rail.rule` | `rails` | `apps/session/ui/src/rails.rs`; `apps/session/ui/themes/console/rails.frame` | Role |
| Rail buttons | `$rail.button`, `$accent`, `ink.on-*` | `rail.button` | `apps/session/ui/themes/console/rails.frame` | Role |
| Rail button icons | `$text.dim` | `rail.button` `%icon` | `apps/session/ui/src/rails.rs` | Role |
| Rails without a theme | `$rail.*` | none | `apps/session/ui/src/rails.rs` (`tcp_gutter`, `tcp_rule`) | Palette·T |
| Transport bar (ground, text, Play, Rec, Loop) | `sheet`, `rule`, `ink`, `ink-2`, `transport.play`, `transport.rec`, `transport.loop` | none | `apps/session/ui/src/transport_bar.rs` `BG`, `RULE`, `TEXT`, `DIM`, `PLAY`, `REC`, `LOOP` | Hard-coded |
| Main toolbar | `toolbar.on`, `toolbar.on-ink`, `ink-2`, `rule` | none | `apps/session/ui/src/toolbar.rs` `ON_BG`, `ON_FG`, `OFF_FG`, `HOVER_RULE`, `MainToolbar` | Hard-coded |
| Shell bars, mode and phase switches, song tabs | `sheet`, `rule`, `ink`, `ink-2`, `brand`, `pill`, `scrim` | none | `apps/session/ui/src/shell.rs` `BAR_BG`, `RULE`, `TEXT`, `DIM`, `ACCENT`, `SongTabs`, `Modes` | Hard-coded |
| Record view (rating pad, take prompt) | `sheet`, `ink`, `record.star`, `record.miss` | none | `apps/session/ui/src/record_view.rs` `BG`, `PANEL`, `StarButton`, `MissButton`, `TakePrompt` | Hard-coded |
| Collaboration bar | `safe`, `warn`, `danger`, `desk` | none | `apps/session/ui/src/collab_bar.rs` `CollabBar` | Hard-coded |
| Frame-rate and performance HUD | `hud.*` | none | `apps/session/ui/src/fps.rs` `INK`, `UNDER`, `OVER`, `WAY_OVER`; `apps/session/ui/src/perf_meter.rs` | Hard-coded |
| Navigator | `ink`, `ink-2` | none | `apps/session/ui/src/navigator.rs` `Bar` | Hard-coded |
| Arrangement dock and panel grounds (session lanes colours) | `arrange.scrollbar`, `desk` | none | `apps/session/ui/src/panel.rs`, `apps/session/ui/src/web_host.rs` (`lanes::Colors::from_theme(Theme::dark())`) | Palette·F |

---

## Keeping this current

When a change adds, removes or themes an element, update its row, the
[summary](#summary) and the [appendix](#appendix-hard-coded-colours) in the
same change. The rule for status stays the same: read the painter. A token that a
theme sets but a painter never reads does not move a row out of
**Palette·F**.

---

## Appendix: hard-coded colours

Every literal colour in `apps/session/ui/src/**` and
`apps/session/ui/themes/**` that should be a token, with the token proposed
for it. **How it was collected:** a scan for `Color::from_rgb8` /
`from_rgba8` / `Color::new([…])` with literal arguments (multi-line calls
included), `Color::WHITE` / `BLACK`, `css::*`, and `#hex` / `rgb(a)(…)`
literals inside strings (the Dioxus CSS), skipping comments, `tests.rs`,
`skin/examples.rs` and `#[cfg(test)]` modules (3 test literals left out). Packed
`0xRRGGBB` track colours are data, not literals. For Frame files it counts
every `#hex` outside a comment.

**Proposed names** follow `docs/theme-architecture.md`. A bare name (`desk`,
`sheet`, `rule`, `ink`, `ink-2`, `ink-3`, `pill`, `brand`, `brand-ink`,
`brand-soft`, `scrim`, `safe`, `warn`, `danger`, `info`) is architect's own
semantic token (`architect_theme`'s `names::SHELL` and `names::STATUS`): the
Dioxus chrome has been hand-copying exactly that palette (`#17181b` sheet,
`#2a2c31` rule, `#e5e7eb` ink, `#8b9099` ink-2, `#3aa0ff` brand, `#0b0c0e`
desk) in every file. A dotted name is a Studio domain token. "(new)" marks a
name made up only to place the literal; the owning view should pick a better
one when it migrates.

**Headline.** 279 literals in 43 Rust files. 923 literals inside pages and
components across the Frame themes, 658 of them in the generated
`fts-dark.frame`. Of the 265 in hand-written theme pages, 109 are exactly a value the
theme (or console beneath it) already names as a token, so they should be a `$`
reference, and 14 more are such a token at an alpha. Most of the rest are
shading ladders inside one piece of control art.

#### Rust: counts per file (non-test)

| File | Literals |
|---|---:|
| `apps/session/ui/src/lyrics_panel.rs` | 30 |
| `apps/session/ui/src/arrangement.rs` | 22 |
| `apps/session/ui/src/record_view.rs` | 20 |
| `apps/session/ui/src/tone.rs` | 20 |
| `apps/session/ui/src/shell.rs` | 19 |
| `apps/session/ui/src/collab_bar.rs` | 16 |
| `apps/session/ui/src/setup.rs` | 14 |
| `apps/session/ui/src/perf_meter.rs` | 13 |
| `apps/session/ui/src/transport_bar.rs` | 13 |
| `apps/session/ui/src/closeup.rs` | 10 |
| `apps/session/ui/src/mixer_panel.rs` | 9 |
| `apps/session/ui/src/quantize_panel.rs` | 7 |
| `apps/session/ui/src/toolbar.rs` | 7 |
| `apps/session/ui/src/compact.rs` | 6 |
| `apps/session/ui/src/web_host.rs` | 6 |
| `apps/session/ui/src/browser.rs` | 5 |
| `apps/session/ui/src/chart_editor.rs` | 5 |
| `apps/session/ui/src/fps.rs` | 5 |
| `apps/session/ui/src/chart_panel.rs` | 4 |
| `apps/session/ui/src/loading.rs` | 4 |
| `apps/session/ui/src/marks_strip.rs` | 4 |
| `apps/session/ui/src/performer.rs` | 4 |
| `apps/session/ui/src/versions.rs` | 4 |
| `apps/session/ui/src/bar_step.rs` | 3 |
| `apps/session/ui/src/tcp.rs` | 3 |
| `apps/session/ui/src/widget.rs` | 3 |
| `apps/session/ui/src/cursor.rs` | 2 |
| `apps/session/ui/src/fx_view.rs` | 2 |
| `apps/session/ui/src/navigator.rs` | 2 |
| `apps/session/ui/src/poi_paint.rs` | 2 |
| `apps/session/ui/src/progress.rs` | 2 |
| `apps/session/ui/src/skin/export/tcp.rs` | 2 |
| `apps/session/ui/src/bin/bench.rs` | 1 |
| `apps/session/ui/src/collab_pointers.rs` | 1 |
| `apps/session/ui/src/editor_view.rs` | 1 |
| `apps/session/ui/src/folder_item/draw.rs` | 1 |
| `apps/session/ui/src/ghosts.rs` | 1 |
| `apps/session/ui/src/organize.rs` | 1 |
| `apps/session/ui/src/overlay.rs` | 1 |
| `apps/session/ui/src/ruler/chords.rs` | 1 |
| `apps/session/ui/src/skin/export.rs` | 1 |
| `apps/session/ui/src/which_key.rs` | 1 |
| `apps/session/ui/src/window_cursor.rs` | 1 |
| **Total** | **279** |

#### Rust: every literal

Grouped by file and symbol; a value repeated inside one symbol is listed once with its count.

| File | Symbol | Value(s) | Proposed token |
|---|---|---|---|
| `apps/session/ui/src/arrangement.rs` | `CLEAR` | `#ffffff`, `#fbbf24` | `arrange.hit.flag (white) / arrange.hit.flag-hand (#fbbf24)` |
| `apps/session/ui/src/arrangement.rs` | `FADE_SHADE` | `#0000005c` | `arrange.fade.shade` |
| `apps/session/ui/src/arrangement.rs` | `FLOOR` | `#00000073` | `arrange.item.title-plate` |
| `apps/session/ui/src/arrangement.rs` | `NOTCH` | `#ff5a5a` | `arrange.gap.mark` |
| `apps/session/ui/src/arrangement.rs` | `RANK_DOWN` | `#e5484d` | `arrange.take.rank-down` |
| `apps/session/ui/src/arrangement.rs` | `RANK_UP` | `#f2c14e` | `arrange.take.rank-up` |
| `apps/session/ui/src/arrangement.rs` | `STAR` | `#ffffff`×2 | `arrange.take.star` |
| `apps/session/ui/src/arrangement.rs` | `comp_source_outlines` | `#fffffff0` | `arrange.comp.source-ring` |
| `apps/session/ui/src/arrangement.rs` | `comp_strip` | `#f2c14e`, `#ffffff`×2 | `arrange.comp.kept (gold), arrange.comp.rank-down, arrange.comp.pending` |
| `apps/session/ui/src/arrangement.rs` | `kit_color` | `#e5484d`, `#3e8ef7`, `#9b6be0` | `arrange.kit.kick / .snare / .toms` |
| `apps/session/ui/src/arrangement.rs` | `stream_overlay` | `#00000070`, `#ffffff22` | `arrange.stream.veil / arrange.stream.track` |
| `apps/session/ui/src/arrangement.rs` | `stretch_tint` | `#ef4444`, `#f59e0b`, `#60a5fa` | `arrange.stretch.extreme / .audible / .subtle` |
| `apps/session/ui/src/arrangement.rs` | `take_marks_overlay` | `#ffffff` | `arrange.take.star` |
| `apps/session/ui/src/bar_step.rs` | `Stepper` | `#34d399`, `#34d3991f`, `rgba(0,0,0,0.5)` | `bar-step.stepper (new)`; `safe`; `scrim` |
| `apps/session/ui/src/bin/bench.rs` | `shot` | `css::MAGENTA` | (bench fallback; keep magenta) |
| `apps/session/ui/src/browser.rs` | `BrowserBody` | `#e5e7eb` | `ink` |
| `apps/session/ui/src/browser.rs` | `HEADING` | `#8b9099` | `ink-2` |
| `apps/session/ui/src/browser.rs` | `row_style` | `#34d399`, `#34d3991f`, `#e5e7eb` | `browser.row-style (new)`; `ink`; `safe` |
| `apps/session/ui/src/chart_editor.rs` | `ChartEditor` | `#4ac26b`, `#e3b341`, `#101114` | `desk`; `safe`; `warn` |
| `apps/session/ui/src/chart_editor.rs` | `EditorToggle` | `#0b0c0e`, `rgba(16,17,20,0.85)` | `desk`; `scrim / overlay` |
| `apps/session/ui/src/chart_panel.rs` | `Chart` | `#1a1b1e` | `sheet` |
| `apps/session/ui/src/chart_panel.rs` | `NoChart` | `#8b9099`, `#1a1b1e` | `ink-2`; `sheet` |
| `apps/session/ui/src/chart_panel.rs` | `WebChart` | `#1a1b1e` | `sheet` |
| `apps/session/ui/src/closeup.rs` | `CloseupLayer` | `#3b82f6`, `#1e3a5f`, `#e5e7eb`×2, `#2a2c31`×3, `#1c1e22`, `#9ca3af`, `#17181b` | `closeup.closeup-layer (new)`; `ink`; `ink-2`; `pill`; `rule`; `sheet` |
| `apps/session/ui/src/collab_bar.rs` | `Avatar` | `#101114` | `desk` |
| `apps/session/ui/src/collab_bar.rs` | `CollabBar` | `#4ac26b`, `#e3b341`×6, `#0f1012`×2, `#5a2a2a`, `#f28b82`, `rgba(0,0,0,0.55)` | `danger`; `danger (wash)`; `desk`; `safe`; `scrim`; `warn` |
| `apps/session/ui/src/collab_bar.rs` | `PeerDots` | `#101114` | `desk` |
| `apps/session/ui/src/collab_bar.rs` | `half` | `#0b0c0e` | `desk` |
| `apps/session/ui/src/collab_bar.rs` | `primary` | `#0b0c0e` | `desk` |
| `apps/session/ui/src/collab_pointers.rs` | `Arrow` | `rgba(255,255,255,0.7)` | `scrim / overlay` |
| `apps/session/ui/src/compact.rs` | `CompactShell` | `#0f1012` | `desk` |
| `apps/session/ui/src/compact.rs` | `More` | `#2563eb`, `#3a3d44`, `#f3f4f6` | `brand`; `brand-ink`; `pill` |
| `apps/session/ui/src/compact.rs` | `SongLine` | `#3a3d44`×2 | `pill` |
| `apps/session/ui/src/cursor.rs` | `EDIT_CURSOR` | `#3b82f6` | `arrange.cursor.edit` |
| `apps/session/ui/src/cursor.rs` | `default` | `#facc15` | `arrange.cursor.play` |
| `apps/session/ui/src/editor_view.rs` | `EditorPanel` | `#0b0c0e` | `desk` |
| `apps/session/ui/src/folder_item/draw.rs` | `side_colour` | `#ffffff` | `arrange.folder-item.side` |
| `apps/session/ui/src/fps.rs` | `INK` | `#f2f2f2` | `hud.ink` |
| `apps/session/ui/src/fps.rs` | `OVER` | `#e3b341` | `hud.over (= warn)` |
| `apps/session/ui/src/fps.rs` | `UNDER` | `#5bc88c` | `hud.under (= safe)` |
| `apps/session/ui/src/fps.rs` | `WAY_OVER` | `#dc267f` | `hud.way-over (= danger)` |
| `apps/session/ui/src/fps.rs` | `draw` | `#000000` | `hud.ground` |
| `apps/session/ui/src/fx_view.rs` | `FxPanel` | `#0b0c0e` | `desk` |
| `apps/session/ui/src/fx_view.rs` | `device_header` | `#22c55e` | `fx.device.on (= safe)` |
| `apps/session/ui/src/ghosts.rs` | `paint_pointer` | `#101014` | `collab.pointer.ink` |
| `apps/session/ui/src/loading.rs` | `LoadingFrame` | `#f87171`, `#9aa0a6` | `danger`; `ink-3` |
| `apps/session/ui/src/loading.rs` | `Sweep` | `#1f2228`, `#3aa0ff` | `brand`; `pill` |
| `apps/session/ui/src/lyrics_panel.rs` | `Audience` | `#0c0e16`, `#040507`, `#ffffff`×2, `#9aa3b5` | `ink (or ink @alpha)`; `lyrics-panel.audience (new)` |
| `apps/session/ui/src/lyrics_panel.rs` | `Confidence` | `#000000`×2, `#ffffff`, `#3a3d44` | `ink (or ink @alpha)`; `lyrics-panel.confidence (new)`; `pill` |
| `apps/session/ui/src/lyrics_panel.rs` | `LyricsPanel` | `#07080b`, `#6b707a`, `rgba(12,13,17,0.86)`, `rgba(0,0,0,0.55)` | `desk`; `lyrics-panel.lyrics-panel (new)`; `scrim`; `scrim / overlay` |
| `apps/session/ui/src/lyrics_panel.rs` | `MenuRow` | `#1f2a3a`, `#ffffff`, `#c7cad1`, `#4b4f57` | `ink (or ink @alpha)`; `lyrics-panel.menu-row (new)` |
| `apps/session/ui/src/lyrics_panel.rs` | `NEXT_YELLOW` | `#f5c542` | `lyrics.next (= warn)` |
| `apps/session/ui/src/lyrics_panel.rs` | `Performer` | `#1d2027`, `#5a5f69`, `#ffffff`, `#cfd3da`, `#a9afba`, `#474c55`, `#767c87`, `#07080b`, `#0b0c0e`, `#1b1d23`, `#8b9099` | `desk`; `ink (or ink @alpha)`; `ink-2`; `lyrics-panel.performer (new)`; `pill` |
| `apps/session/ui/src/lyrics_panel.rs` | `SectionRail` | `#0b0c0e` | `desk` |
| `apps/session/ui/src/marks_strip.rs` | `tint` | `#2dd4bf`, `#ef4444`, `#f59e0b`, `#9ca3af` | `arrange.marks.grid / .no-quantize / .fill / .note` |
| `apps/session/ui/src/mixer_panel.rs` | `DawPanels` | `#000`×2, `#101113` | `desk` |
| `apps/session/ui/src/mixer_panel.rs` | `WebDawPanels` | `#000` | `desk` |
| `apps/session/ui/src/mixer_panel.rs` | `overview` | `#0c0d0f`, `#0c0d0fb0`, `#4ade80`, `#ffffff14`, `#ffffff66` | `mixer.overview.ground / .veil / .meter / .window / .window-edge` |
| `apps/session/ui/src/navigator.rs` | `Bar` | `#ffffff`, `#c9ccd2` | `ink (or ink @alpha)`; `ink-2` |
| `apps/session/ui/src/organize.rs` | `OrganizeToolbar` | `#141518` | `sheet` |
| `apps/session/ui/src/overlay.rs` | `controls` | `#000000` | `mixer.rack.dim-unselected` |
| `apps/session/ui/src/perf_meter.rs` | `AHEAD_INK` | `#e8a23a` | `hud.ahead` |
| `apps/session/ui/src/perf_meter.rs` | `CARD` | `rgba(0,0,0,0.45)` | `scrim` |
| `apps/session/ui/src/perf_meter.rs` | `CPU_INK` | `#4c9aff` | `hud.cpu` |
| `apps/session/ui/src/perf_meter.rs` | `HEAD` | `#8a8f98` | `ink-3` |
| `apps/session/ui/src/perf_meter.rs` | `Header` | `#e6e6e6` | `ink` |
| `apps/session/ui/src/perf_meter.rs` | `MUTED` | `#8a8f98` | `ink-3` |
| `apps/session/ui/src/perf_meter.rs` | `PerfMeter` | `#101113`, `#2a2d33`×2, `#5d626b`, `#1f2227`, `#202328` | `desk`; `ink-3`; `pill` |
| `apps/session/ui/src/perf_meter.rs` | `RT_INK` | `#45c46b` | `hud.realtime` |
| `apps/session/ui/src/performer.rs` | `SessionPerformer` | `#e4e4e7` | `ink` |
| `apps/session/ui/src/performer.rs` | `sections` | `#4b5563` | `ink-3` |
| `apps/session/ui/src/performer.rs` | `setlist` | `#8e8e98` | `performer.setlist (new)` |
| `apps/session/ui/src/performer.rs` | `view` | `#7dd3fc` | `info` |
| `apps/session/ui/src/poi_paint.rs` | `DONE` | `#34d399` | `arrange.poi.done (= safe)` |
| `apps/session/ui/src/poi_paint.rs` | `OPEN` | `#f59e0b` | `arrange.poi.open (= warn)` |
| `apps/session/ui/src/progress.rs` | `ProgressBar` | `#8b9099` | `ink-2` |
| `apps/session/ui/src/progress.rs` | `of` | `#4b5563` | `ink-3` |
| `apps/session/ui/src/quantize_panel.rs` | `HEAD` | `#8a8f98` | `ink-3` |
| `apps/session/ui/src/quantize_panel.rs` | `QuantizePanel` | `#c9ccd1`×2 | `ink` |
| `apps/session/ui/src/quantize_panel.rs` | `chip` | `#3a6df0`, `#ffffff`, `#24272c`, `#c9ccd1` | `brand`; `ink`; `ink (or ink @alpha)`; `pill` |
| `apps/session/ui/src/record_view.rs` | `BG` | `#0f1012` | `desk` |
| `apps/session/ui/src/record_view.rs` | `DIM` | `#8b9099` | `ink-2` |
| `apps/session/ui/src/record_view.rs` | `MissButton` | `#1f2228`, `#f87171` | `record.miss (= danger) / pill` |
| `apps/session/ui/src/record_view.rs` | `PANEL` | `#17181b` | `sheet` |
| `apps/session/ui/src/record_view.rs` | `RULE` | `#2a2c31` | `rule` |
| `apps/session/ui/src/record_view.rs` | `RecordMenu` | `#fca5a5`, `#93c5fd`×2, `#24272d`×2, `#262a31`, `rgba(0,0,0,0.6)` | `brand-soft`; `danger (soft)`; `pill`; `scrim` |
| `apps/session/ui/src/record_view.rs` | `StarButton` | `#1f2228`, `#fbbf24`, `#a38a4a` | `record.star / pill` |
| `apps/session/ui/src/record_view.rs` | `TEXT` | `#e5e7eb` | `ink` |
| `apps/session/ui/src/record_view.rs` | `TakePrompt` | `#3aa0ff`, `#0b0c0e`, `rgba(0,0,0,0.6)` | `brand`; `desk`; `scrim` |
| `apps/session/ui/src/ruler/chords.rs` | `TAG` | `#7c3aed` | `arrange.ruler.key-tag` |
| `apps/session/ui/src/setup.rs` | `Settings` | `#e3b341` | `warn` |
| `apps/session/ui/src/setup.rs` | `SetupView` | `#0f1012`, `#131417` | `desk`; `sheet` |
| `apps/session/ui/src/setup.rs` | `SongTable` | `#6b7280`×3, `#c9ccd2` | `ink-2`; `ink-3` |
| `apps/session/ui/src/setup.rs` | `Switch` | `#2563eb`, `#3a3d44`, `#f3f4f6` | `brand`; `brand-ink`; `pill` |
| `apps/session/ui/src/setup.rs` | `Toggle` | `#2563eb`, `#3a3d44`, `#f3f4f6` | `brand`; `brand-ink`; `pill` |
| `apps/session/ui/src/setup.rs` | `row_button` | `#34373d` | `pill` |
| `apps/session/ui/src/shell.rs` | `ACCENT` | `#3aa0ff` | `brand` |
| `apps/session/ui/src/shell.rs` | `AudioBadge` | `#e3b341`, `rgba(0,0,0,0.5)` | `scrim`; `warn` |
| `apps/session/ui/src/shell.rs` | `BAR_BG` | `#17181b` | `sheet` |
| `apps/session/ui/src/shell.rs` | `BottomBar` | `rgba(0,0,0,0.5)` | `scrim` |
| `apps/session/ui/src/shell.rs` | `ColorMenu` | `rgba(0,0,0,0.5)` | `scrim` |
| `apps/session/ui/src/shell.rs` | `DIM` | `#8b9099` | `ink-2` |
| `apps/session/ui/src/shell.rs` | `Modes` | `rgba(0,0,0,0.5)` | `scrim` |
| `apps/session/ui/src/shell.rs` | `PhaseSwitch` | `rgba(0,0,0,0.5)` | `scrim` |
| `apps/session/ui/src/shell.rs` | `RAISED` | `#26292f` | `pill` |
| `apps/session/ui/src/shell.rs` | `RULE` | `#2a2c31` | `rule` |
| `apps/session/ui/src/shell.rs` | `SongTabs` | `#3a3d44`×3, `#ffffff14` | `ink (or ink @alpha)`; `pill` |
| `apps/session/ui/src/shell.rs` | `TEXT` | `#e5e7eb` | `ink` |
| `apps/session/ui/src/shell.rs` | `mode_option` | `#2a2d33` | `pill` |
| `apps/session/ui/src/shell.rs` | `option` | `#23262c` | `pill` |
| `apps/session/ui/src/shell.rs` | `segment` | `#0b0c0e` | `desk` |
| `apps/session/ui/src/skin/export.rs` | `editor_tokens` | `#ffffff12` | `sheen (emit $sheen)` |
| `apps/session/ui/src/skin/export/tcp.rs` | `tcp` | `#000000`, `#e8e8ea` | `ink.on-light / ink.on-dark (emit $ink.*)` |
| `apps/session/ui/src/tcp.rs` | `icon_ink` | `#ffffff` | `tcp.icon.ink-lift (white mix)` |
| `apps/session/ui/src/tcp.rs` | `ink_on` | `#000000`, `#e8e8ea` | `ink.on-light / ink.on-dark` |
| `apps/session/ui/src/tone.rs` | `ACROSS` | `#8c8c96` | `fx.rack.across` |
| `apps/session/ui/src/tone.rs` | `DECAY_INK` | `#38bdf8` | `fx.ink.decay` |
| `apps/session/ui/src/tone.rs` | `DEESS_INK` | `#f472b6` | `fx.ink.deess` |
| `apps/session/ui/src/tone.rs` | `DELAY_INK` | `#3b82f6` | `fx.ink.delay` |
| `apps/session/ui/src/tone.rs` | `EQ_INK` | `#d4a932` | `fx.ink.eq` |
| `apps/session/ui/src/tone.rs` | `GATE_INK` | `#34d399` | `fx.ink.gate` |
| `apps/session/ui/src/tone.rs` | `REVERB_INK` | `#8b5cf6` | `fx.ink.reverb` |
| `apps/session/ui/src/tone.rs` | `SAT_GLOW` | `#ff2e0c` | `fx.ink.saturate-glow` |
| `apps/session/ui/src/tone.rs` | `SAT_HOT` | `#ffb347` | `fx.ink.saturate-hot` |
| `apps/session/ui/src/tone.rs` | `SAT_INK` | `#ff4a3d` | `fx.ink.saturate` |
| `apps/session/ui/src/tone.rs` | `ess_trace` | `#9a9aa0` | `fx.ink.ess-trace` |
| `apps/session/ui/src/tone.rs` | `hex` | `#888888` | (fallback for unparseable hex; keep) |
| `apps/session/ui/src/tone.rs` | `phase_tint` | `#ef4444`, `#22c55e`, `#facc15`, `#06b6d4`, `#3b82f6`, `#8b5cf6`, `#ec4899`, `#6366f1` | `phase.rescue / .balance / .tone / .polish / … (8)` |
| `apps/session/ui/src/toolbar.rs` | `HOVER_RULE` | `#3a3f47` | `rule` |
| `apps/session/ui/src/toolbar.rs` | `MainToolbar` | `#1c1e22`, `rgba(0,0,0,0.6)` | `pill`; `scrim` |
| `apps/session/ui/src/toolbar.rs` | `OFF_FG` | `#9aa0a8` | `ink-2` |
| `apps/session/ui/src/toolbar.rs` | `ON_BG` | `#1f5f55` | `toolbar.on` |
| `apps/session/ui/src/toolbar.rs` | `ON_FG` | `#e8fff9` | `toolbar.on-ink` |
| `apps/session/ui/src/toolbar.rs` | `menu_row_style` | `#d1d5db` | `ink` |
| `apps/session/ui/src/transport_bar.rs` | `BG` | `#17181b` | `sheet` |
| `apps/session/ui/src/transport_bar.rs` | `Button` | `#0b0c0e` | `desk` |
| `apps/session/ui/src/transport_bar.rs` | `DIM` | `#8b9099` | `ink-2` |
| `apps/session/ui/src/transport_bar.rs` | `LOOP` | `#3aa0ff` | `transport.loop (= brand)` |
| `apps/session/ui/src/transport_bar.rs` | `PLAY` | `#34d399` | `transport.play (= safe)` |
| `apps/session/ui/src/transport_bar.rs` | `REC` | `#ef4444` | `transport.rec (= rec)` |
| `apps/session/ui/src/transport_bar.rs` | `RULE` | `#2a2c31` | `rule` |
| `apps/session/ui/src/transport_bar.rs` | `TEXT` | `#e5e7eb` | `ink` |
| `apps/session/ui/src/transport_bar.rs` | `TransportBarView` | `#0b0c0e`, `rgba(0,0,0,0.5)`×3 | `desk`; `scrim` |
| `apps/session/ui/src/transport_bar.rs` | `stop_option` | `#34d3991f` | `transport-bar.stop-option (new)` |
| `apps/session/ui/src/versions.rs` | `NO` | `#e5a442` | `warn` |
| `apps/session/ui/src/versions.rs` | `OK` | `#4ac26b` | `safe` |
| `apps/session/ui/src/versions.rs` | `Versions` | `#3aa0ff`, `#0b0c0e` | `brand`; `desk` |
| `apps/session/ui/src/web_host.rs` | `ListeningBadge` | `#e3b341` | `warn` |
| `apps/session/ui/src/web_host.rs` | `LoadMultitracks` | `rgba(0,0,0,0.55)`, `rgba(0,0,0,0.6)` | `scrim` |
| `apps/session/ui/src/web_host.rs` | `WebDemo` | `#3aa0ff`, `#0b0c0e` | `brand`; `desk` |
| `apps/session/ui/src/web_host.rs` | `WebPerformance` | `#2a2c31` | `rule` |
| `apps/session/ui/src/which_key.rs` | `Panel` | `rgba(0,0,0,0.45)` | `scrim` |
| `apps/session/ui/src/widget.rs` | `paint_grip` | `#9a9fa8c0` | `arrange.touch.grip` |
| `apps/session/ui/src/widget.rs` | `paint_handle` | `#c9ccd1` | `arrange.touch.handle` |
| `apps/session/ui/src/widget.rs` | `paint_pinned` | `#000000` | `arrange.lane.pinned-shadow` |
| `apps/session/ui/src/window_cursor.rs` | `passes` | `#000000be` | `scrim` |

#### Frame themes: counts per file

"Literal" is a `#hex` inside a page or component (it should be a `$token`); "Token defs" are `<Var>` values in `<Variables>`, which are where a literal belongs.

| File | Literals in pages | Token defs |
|---|---:|---:|
| `apps/session/ui/themes/console/controls.frame` | 53 | 0 |
| `apps/session/ui/themes/console/editor.frame` | 1 | 0 |
| `apps/session/ui/themes/console/rails.frame` | 2 | 0 |
| `apps/session/ui/themes/console/strip.frame` | 105 | 0 |
| `apps/session/ui/themes/console/tcp.frame` | 18 | 0 |
| `apps/session/ui/themes/console/tokens.frame` | 0 | 54 |
| `apps/session/ui/themes/control-room/controls.frame` | 7 | 0 |
| `apps/session/ui/themes/control-room/strip.frame` | 34 | 0 |
| `apps/session/ui/themes/control-room/tcp.frame` | 3 | 0 |
| `apps/session/ui/themes/control-room/tokens.frame` | 0 | 33 |
| `apps/session/ui/themes/fts-dark.frame` | 658 | 70 |
| `apps/session/ui/themes/graphite/controls.frame` | 3 | 0 |
| `apps/session/ui/themes/graphite/strip.frame` | 15 | 0 |
| `apps/session/ui/themes/graphite/tokens.frame` | 0 | 30 |
| `apps/session/ui/themes/workbench/controls.frame` | 3 | 0 |
| `apps/session/ui/themes/workbench/strip.frame` | 15 | 0 |
| `apps/session/ui/themes/workbench/tcp.frame` | 6 | 0 |
| `apps/session/ui/themes/workbench/tokens.frame` | 0 | 30 |
| **Total** | **923** | **217** |

#### Frame themes: every literal in a page

Proposed token: the theme's own `<Var>` of the same value when it has one (else console's), marked `=`; otherwise a new name. `fts-dark.frame` is generated by `theme_export` (`apps/session/ui/src/skin/export.rs`), so its literals are fixed in the exporter, not by hand; it is summarised by value.

| File | Page / component | Value ×count | Proposed token |
|---|---|---|---|
| `apps/session/ui/themes/console/controls.frame` | `pan` | `#151517`, `#15151b` | `$$control.pan.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/console/controls.frame` | `arm` | `#ceced3`, `#d5d5d9`, `#ccccd1`, `#bbbbbf`, `#acacb1`, `#f26666`, `#f47d7d`, `#f16060`, `#da5f5f`, `#d23c3c` | `$$control.arm.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/console/controls.frame` | `arm` | `#c3c3c9` | = `$hardware.mark` |
| `apps/session/ui/themes/console/controls.frame` | `arm` | `#ef4444` | = `$mute` |
| `apps/session/ui/themes/console/controls.frame` | `phase` | `#18181b`×2 | = `$hardware` |
| `apps/session/ui/themes/console/controls.frame` | `phase` | `#d3d3d7`×12, `#1e1e22`, `#46ecff` | `$$control.phase.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/console/controls.frame` | `phase` | `#38bdf8`×2 | = `$accent` |
| `apps/session/ui/themes/console/controls.frame` | `volume` | `#00000026`, `#00000000` | `$shadow` / `$plate` |
| `apps/session/ui/themes/console/controls.frame` | `volume` | `#0f0f10`, `#232324`, `#212124`, `#17171a`, `#29292d`, `#1f1f23`, `#1e1e24`, `#14141a` | `$$control.volume.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/console/controls.frame` | `volume` | `#27272a`×3 | = `$hardware.edge` |
| `apps/session/ui/themes/console/controls.frame` | `volume` | `#38bdf8` | = `$accent` |
| `apps/session/ui/themes/console/controls.frame` | `volume.fader` | `#26262b`, `#d4d4d8` | `$$control.volume.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/console/controls.frame` | `volume.fader` | `#38bdf88c` | = `$editor.control_active` |
| `apps/session/ui/themes/console/controls.frame` | `pan.line` | `#26262b`, `#3f3f46`, `#d4d4d8` | `$$control.pan.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/console/controls.frame` | `pan.line` | `#eab3088c` | `alpha($solo, 0.55)` |
| `apps/session/ui/themes/console/editor.frame` | `editor.menu` | `#00000066` | `$shadow` / `$plate` |
| `apps/session/ui/themes/console/rails.frame` | `rail.button` | `#000000`, `#e8e8ea` | `$ink.on-light` / `$ink.on-dark` |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#0a0a0c`×2 | = `$field` |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#a1a1aa`×5 | = `$text.dim` |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#71717a`×9 | = `$text.faint` |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#38bdf8` | = `$accent` |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#eab308` | = `$solo` |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#e4e4e7`×2 | = `$text` |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#f16430c8`, `#f16430`, `#f3841cc8`, `#f3841c`, `#f19f0dc8`, `#f19f0d`, `#bfa820c8`, `#bfa820`, `#8eb134c8`, `#8eb134`, `#5cba47c8`, `#5cba47`, `#2ac35bc8`, `#2ac35b`, `#f58585`×5, `#19191b`, `#262629`×4, `#18181a`×4, `#101012`×4, `#0c0c0e`×4, `#59595b`, `#3d3d3f`, `#5e5e60`, `#606063`, `#626265`, `#656568`, `#67676a`, `#3c3c3e`, `#6c6c6f`, `#6e6e71`, `#707074`, `#737376`, `#757579` | `$$mixer.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#71717a5a`×7 | `alpha($text.faint, 0.35)` |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#050506`, `#050506d2`, `#00000033` | `$shadow` / `$plate` |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#22c55e`×6 | = `$meter.safe` |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#f59e0b`×8 | = `$meter.warn` |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#ef4444`×6 | = `$mute` |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#22c55e82`×2 | `alpha($meter.safe, 0.51)` |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#f59e0b82`×2 | `alpha($meter.warn, 0.51)` |
| `apps/session/ui/themes/console/strip.frame` | `mcp.strip` | `#ef444482`×2 | `alpha($mute, 0.51)` |
| `apps/session/ui/themes/console/tcp.frame` | `tcp.row` | `#09090b` | = `$surface` |
| `apps/session/ui/themes/console/tcp.frame` | `tcp.row` | `#0a0a0c`×3 | = `$field` |
| `apps/session/ui/themes/console/tcp.frame` | `tcp.row` | `#27272a`×2 | = `$hardware.edge` |
| `apps/session/ui/themes/console/tcp.frame` | `tcp.row` | `#18181b`×2 | = `$hardware` |
| `apps/session/ui/themes/console/tcp.frame` | `tcp.row` | `#e4e4e7`×3 | = `$text` |
| `apps/session/ui/themes/console/tcp.frame` | `tcp.row` | `#000000`, `#e8e8ea` | `$ink.on-light` / `$ink.on-dark` |
| `apps/session/ui/themes/console/tcp.frame` | `tcp.row` | `#a1a1aa`×2 | = `$text.dim` |
| `apps/session/ui/themes/console/tcp.frame` | `tcp.row` | `#71717a`×2 | = `$text.faint` |
| `apps/session/ui/themes/console/tcp.frame` | `tcp.row` | `#38bdf8` | = `$accent` |
| `apps/session/ui/themes/control-room/controls.frame` | `mute` | `#ffffff33` | `$sheen` / `$bevel.light` |
| `apps/session/ui/themes/control-room/controls.frame` | `solo` | `#ffffff33` | `$sheen` / `$bevel.light` |
| `apps/session/ui/themes/control-room/controls.frame` | `arm` | `#ffffff33` | `$sheen` / `$bevel.light` |
| `apps/session/ui/themes/control-room/controls.frame` | `arm` | `#fff4f0`×2 | `$$control.arm.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/control-room/controls.frame` | `pan` | `#ffffff22` | `$sheen` / `$bevel.light` |
| `apps/session/ui/themes/control-room/controls.frame` | `volume` | `#ffffff22` | `$sheen` / `$bevel.light` |
| `apps/session/ui/themes/control-room/strip.frame` | `mcp.strip` | `#ffffff0d` | `$sheen` / `$bevel.light` |
| `apps/session/ui/themes/control-room/strip.frame` | `mcp.strip` | `#05080c`×2, `#3b4350`, `#bcc5d1`, `#2a1310`×2, `#1f8f4a`×2, `#b8e04a`×2, `#bfe0fa`, `#f2f5f9`×3 | `$$mixer.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/control-room/strip.frame` | `mcp.strip` | `#00000066` | `$shadow` / `$plate` |
| `apps/session/ui/themes/control-room/strip.frame` | `mcp.strip` | `#e9edf2` | = `$cr.cap.top` |
| `apps/session/ui/themes/control-room/strip.frame` | `mcp.strip` | `#8f9aab` | = `$cr.cap.bottom` |
| `apps/session/ui/themes/control-room/strip.frame` | `mcp.strip` | `#5d6a7e`×2 | = `$cr.button.bottom` |
| `apps/session/ui/themes/control-room/strip.frame` | `mcp.strip` | `#2b3442` | = `$surface` |
| `apps/session/ui/themes/control-room/strip.frame` | `mcp.strip` | `#35c46a`×2 | = `$meter.safe` |
| `apps/session/ui/themes/control-room/strip.frame` | `mcp.strip` | `#f2d32c`×2 | = `$solo` |
| `apps/session/ui/themes/control-room/strip.frame` | `mcp.strip` | `#e5352b`×4 | = `$rec` |
| `apps/session/ui/themes/control-room/strip.frame` | `mcp.strip` | `#6d7a8c` | = `$text.faint` |
| `apps/session/ui/themes/control-room/strip.frame` | `mcp.strip` | `#10151c`×3 | = `$hardware.edge` |
| `apps/session/ui/themes/control-room/strip.frame` | `mcp.step` | `#ffffff26` | `$sheen` / `$bevel.light` |
| `apps/session/ui/themes/control-room/tcp.frame` | `tcp.row` | `#ffffff0d` | `$sheen` / `$bevel.light` |
| `apps/session/ui/themes/control-room/tcp.frame` | `tcp.row` | `#bfe0fa` | `$$tcp.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/control-room/tcp.frame` | `tcp.row` | `#6d7a8c` | = `$text.faint` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#0a0a0c`×2 | = `$field` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#a1a1aa`×5 | = `$text.dim` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#71717a`×9 | = `$text.faint` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#151517`, `#202024`, `#15151b`, `#6b6b6f`×3, `#d7d7db9c`×3, `#f2f2f3de`×3, `#212124`×8, `#838387`×40, `#29292d`×8, `#1e1e24`×8, `#ceced3`×2, `#d5d5d9`×2, `#ccccd1`×2, `#bbbbbf`×2, `#acacb1`×2, `#f26666`×2, `#f47d7d`×2, `#f16060`×2, `#da5f5f`×2, `#d23c3c`×2, `#b0b0b5`×8, `#d3d3d7`×12, `#1e1e22`×2, `#ff5555`, `#ffe00a`, `#f16430c8`, `#f16430`, `#f3841cc8`, `#f3841c`, `#f19f0dc8`, `#f19f0d`, `#bfa820c8`, `#bfa820`, `#8eb134c8`, `#8eb134`, `#5cba47c8`, `#5cba47`, `#2ac35bc8`, `#2ac35b`, `#f58585`×5, `#19191b`, `#262629`×4, `#18181a`×4, `#101012`×4, `#0c0c0e`×4, `#59595b`, `#3d3d3f`, `#5e5e60`, `#606063`, `#626265`, `#656568`, `#67676a`, `#3c3c3e`, `#6c6c6f`, `#6e6e71`, `#707074`, `#737376`, `#757579` | `$$mixer.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#27272a`×49 | = `$hardware.edge` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#eab308`×5 | = `$solo` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#18181b`×5 | = `$hardware` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#c3c3c9`×3 | = `$hardware.mark` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#00000059`×6 | = `$plate` (console) |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#38bdf8`×16 | = `$accent` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#ffffff12`×24 | = `$sheen` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#f59e0b`×20 | = `$meter.warn` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#ef4444`×22 | = `$mute` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#e4e4e7`×11 | = `$text` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#f08c2e`×6 | = `$monitor.tape` (console) |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#71717a5a`×7 | `alpha($text.faint, 0.35)` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#050506`, `#050506d2`, `#00000033` | `$shadow` / `$plate` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#22c55e`×6 | = `$meter.safe` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#22c55e82`×2 | `alpha($meter.safe, 0.51)` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#f59e0b82`×2 | `alpha($meter.warn, 0.51)` |
| `apps/session/ui/themes/fts-dark.frame` | `mcp.strip` | `#ef444482`×2 | `alpha($mute, 0.51)` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#09090b` | = `$surface` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#0a0a0c`×9 | = `$field` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#27272a`×54 | = `$hardware.edge` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#a1a1aa`×3 | = `$text.dim` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#000000`, `#e8e8ea` | `$ink.on-light` / `$ink.on-dark` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#e4e4e7` | = `$text` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#71717a`×2 | = `$text.faint` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#18181b`×7 | = `$hardware` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#d3d3d7`×24, `#1e1e22`×3, `#ff5555`, `#ffe00a`, `#6b6b6f`×3, `#d7d7db9c`×3, `#f2f2f3de`×3, `#8a8a8f7d`×36, `#46ecff`, `#ceced3`, `#d5d5d9`, `#ccccd1`, `#bbbbbf`, `#acacb1`, `#f26666`, `#f47d7d`, `#f16060`, `#da5f5f`, `#d23c3c`, `#0f0f10`, `#232324`, `#212124`, `#17171a`, `#29292d`, `#1f1f23`, `#1e1e24`, `#14141a`, `#151517`, `#202024`, `#15151b` | `$$tcp.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#ef4444`×15 | = `$mute` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#eab308`×4 | = `$solo` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#00000059`×30 | = `$plate` (console) |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#38bdf8`×19 | = `$accent` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#ffffff12`×24 | = `$sheen` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#f59e0b`×12 | = `$meter.warn` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#c3c3c9`×2 | = `$hardware.mark` |
| `apps/session/ui/themes/fts-dark.frame` | `tcp.row` | `#00000026`, `#00000000` | `$shadow` / `$plate` |
| `apps/session/ui/themes/fts-dark.frame` | `editor.menu` | `#00000066` | `$shadow` / `$plate` |
| `apps/session/ui/themes/graphite/controls.frame` | `mute` | `#101012` | `$$control.mute.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/graphite/controls.frame` | `solo` | `#101012` | `$$control.solo.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/graphite/controls.frame` | `arm` | `#101012` | `$$control.arm.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/graphite/strip.frame` | `mcp.strip` | `#00000073` | `$shadow` / `$plate` |
| `apps/session/ui/themes/graphite/strip.frame` | `mcp.strip` | `#ffffff`, `#ffffff59` | `$sheen` / `$bevel.light` |
| `apps/session/ui/themes/graphite/strip.frame` | `mcp.strip` | `#2fb866`×2, `#f4f4f6` | `$$mixer.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/graphite/strip.frame` | `mcp.strip` | `#46d17a`×2 | = `$meter.safe` |
| `apps/session/ui/themes/graphite/strip.frame` | `mcp.strip` | `#f5c518`×2 | = `$solo` |
| `apps/session/ui/themes/graphite/strip.frame` | `mcp.strip` | `#f0443a`×4 | = `$rec` |
| `apps/session/ui/themes/graphite/strip.frame` | `mcp.strip` | `#121214` | = `$hardware.edge` |
| `apps/session/ui/themes/workbench/controls.frame` | `mute` | `#1c1c1c` | `$$control.mute.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/workbench/controls.frame` | `solo` | `#1c1c1c` | `$$control.solo.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/workbench/controls.frame` | `arm` | `#ffffff55` | `$sheen` / `$bevel.light` |
| `apps/session/ui/themes/workbench/strip.frame` | `mcp.strip` | `#00000059` | = `$wb.bevel.dark` |
| `apps/session/ui/themes/workbench/strip.frame` | `mcp.strip` | `#ffffff80` | `$sheen` / `$bevel.light` |
| `apps/session/ui/themes/workbench/strip.frame` | `mcp.strip` | `#3ccf6e`×4 | = `$meter.safe` |
| `apps/session/ui/themes/workbench/strip.frame` | `mcp.strip` | `#e8c547`×4 | = `$solo` |
| `apps/session/ui/themes/workbench/strip.frame` | `mcp.strip` | `#e2463c`×4 | = `$rec` |
| `apps/session/ui/themes/workbench/strip.frame` | `mcp.strip` | `#00000080` | `$shadow` / `$plate` |
| `apps/session/ui/themes/workbench/tcp.frame` | `tcp.row` | `#1c1c1c`×2 | `$$tcp.*` (new: ladder step / art shade) |
| `apps/session/ui/themes/workbench/tcp.frame` | `tcp.row` | `#e2e4e7`×2 | = `$text` |
| `apps/session/ui/themes/workbench/tcp.frame` | `tcp.row` | `#000000`, `#e8e8ea` | `$ink.on-light` / `$ink.on-dark` |

