# Studio theme architecture: one token namespace

How Studio's colour models become one runtime-themeable namespace, layered
over architect's semantic tokens, so that the arrangement, track panel, mixer,
MIDI and audio editors, FX view and transport can all be restyled at runtime
by a person or an agent. The goal is herdr's desktop app, where a theme is
picked, previewed or edited live and every surface follows in the next frame.

- **What exists today, element by element:** `docs/theme-catalogue.md`
  (211 entries; 48 role-themed, 59 token-reachable, 25 fixed, 57 hard-coded,
  22 not drawn) and its appendix of literal colours.
- **The Frame role contract** (fields, `@name` targets, slots) stays in
  `docs/frame-theme-contract.md`. This file decides where colour comes from.
- **Other repos** are written `architect:…`, `frame:…`, `herdr:…`.

## 1. Today: four colour models and a fixed palette

| Model | Where | What it holds | Who builds it in Studio | Runtime-reachable? |
|---|---|---|---|---|
| `audio_controls::theming::Theme`, re-exported as `daw_ui::theming::Theme` | `features/ui/audio-controls/src/theming/theme.rs`, `features/ui/audio-controls/src/theming/arrange.rs` | `Tokens` (18 colours), `Metrics`, `ArrangeTheme` (45 arrange-view fields), `McpTheme` / `TransTheme` / `EnvcpTheme` skins | Only `Theme::dark()`, at 16 non-test call sites (`apps/session/ui/src/widget.rs` `ArrangementWidget::for_session` and `draw`, `apps/session/ui/src/mixer_panel.rs`, `apps/session/ui/src/fx_view.rs`, `apps/session/ui/src/editor_view.rs`, `apps/session/ui/src/studio.rs` …) | No. The REAPER import (`theme_from_reaper`) fills it, but nothing in `apps/session` calls the importer. |
| `daw_ui::theme::palette::Theme` | `features/ui/daw-ui/src/theme/palette.rs`, `features/ui/daw-ui/src/theme/defaults.rs` | `Chrome`, `Signal`, `Editor`, `Metrics`, REAPER `overrides`; styx in and out; REAPER and SWELL export | `Palette.chrome` (`chrome` in `apps/session/ui/src/arrangement.rs`), from `Theme::default().chrome` | Partly: `skin::palette` overwrites 10 `Chrome` fields from Frame tokens |
| Frame `<Variables>` | `apps/session/ui/themes/*/tokens.frame` | Flat named values, per mode, layered by file | `skin::ThemeHost` (`apps/session/ui/src/skin/mod.rs`) | Yes, for the 27 names `skin::palette` copies and the 51 `editor.*` names `themed` (`apps/session/ui/src/expression.rs`) copies |
| Hand-copied CSS palettes and literals | `apps/session/ui/src/transport_bar.rs` `BG`/`TEXT`/…, `apps/session/ui/src/shell.rs`, `apps/session/ui/src/toolbar.rs`, painters' consts | 279 literals in 43 files | Compiled in | No |

On top of these sit **three projections** that already have the right shape
and the wrong source:

- `Palette` (`apps/session/ui/src/arrangement.rs`): "colours resolved once, so
  the draw loop never parses a hex string". It is built from `Theme::dark()`,
  and `skin::palette` patches 17 of its fields.
- `Look` (`features/editor/src/paint/paint.rs`): 32 colours. `base_look`
  builds it from `Palette`, then `editor.<field>` tokens overwrite it.
- **The note recolour table:** `set_recolour` keyed by the *hex string* of
  the colour being replaced (`"#ef4444"` → the theme's `editor.pitch_0`). It
  is a fourth, string-keyed colour model inside the editor.

**What this costs.** Of `Palette`'s fields, 15 are fixed from `Theme::dark()`
(lane stripes, divider, grid, ruler, item edge, panel tint, track tint). That
makes 25 catalogue entries **Palette·F**, and no theme can reach them.
REAPER-import data for the edit cursor, play cursor, peaks, time selection,
markers, regions and fades is built and then dropped, because `Palette` reads
11 of `ArrangeTheme`'s 45 fields. A Frame theme edit bumps one global
`skin::version()`, and every listener responds by rebuilding everything. The
arrangement re-cuts its whole scene (`ArrangementWidget::recut`), even when
the edit only touched a mixer role.

## 2. The target

```text
 architect-theme                         Studio (owns every bridge)
 ───────────────                         ──────────────────────────
 Palette ──derive──▶ semantic tokens ──▶ StudioTokens defaults (expressions over semantic)
                     (background, ink,    │
                      brand, safe, …)     ├─ layer: REAPER import      (.ReaperTheme → arrange.*, tcp.*, mixer.*)
 ThemeStore cascade                       ├─ layer: Frame <Variables>  (the picked Frame theme's own tokens)
 Builtin < App < Org < Person < Device    ├─ layer: person / device / workspace edits
 < Workspace < Pick < Preview             └─ layer: pick / preview (THEME_SET, THEME_PREVIEW, agents)
        │                                        │
        ▼                                        ▼
   Resolved (revisioned, flat, magenta for unknown)
        │
        ├──▶ Projected<ArrangePalette>  ── arrangement lanes, ruler, grid, cursors, overlays
        ├──▶ Projected<TcpPalette>      ── built-in rows, compact rows, lane pills
        ├──▶ Projected<MixerPalette>    ── strips, rack inks, overview
        ├──▶ Projected<EditorLook>      ── features/editor Look + note/CC/string tables
        ├──▶ Projected<FxPalette>       ── FX view, rack bodies
        ├──▶ CSS variables              ── Dioxus transport, toolbar, shell (architect-ui's)
        └──▶ Frame overlay <Variables>  ── recompiles Frame roles that read a changed $token
```

There are three rules:

1. **One namespace, one resolution.** Every colour Studio paints is a named
   token in `StudioTokens`, resolved once by architect's cascade. Nothing else
   chooses a colour. Literals are allowed only in token *defaults*.
2. **Imports are layers.** A REAPER theme, a Frame theme's `<Variables>`, the
   styx `fts-theme.styx`, a person's edit and an agent's preview are all token
   overrides on a cascade layer. None of them is a separate model.
3. **Views read projections, not tokens.** Each view owns a typed slice
   (`FromResolved`) cached in architect's `Projected<T>`. A view redraws or
   re-records only when `Projected::update` says *its* slice changed.

### 2.1 Token names

The names are dotted namespaces of lowercase kebab segments:
`arrange.item.edge`, `tcp.row.ground`, `mixer.meter.safe`, `editor.pitch-0`.

- **Architect's own semantic names stay bare** (`background`, `ink`,
  `ink-2`, `brand`, `rule`, `sheet`, `desk`, `pill`, `scrim`, `safe`, `warn`,
  `danger`, `info`). Studio *reads* these and never redefines them.
- **Studio's domain namespaces:** `arrange.*`, `track.*`, `take.*`, `tcp.*`,
  `mixer.*`, `rack.*`, `phase.*`, `env.*`, `editor.*`, `fx.*`, `transport.*`,
  `toolbar.*`, `record.*`, `hud.*`, `collab.*`, `ink.*` (contrast inks).
- **The names Frame themes use today** are kept as aliases for one release:
  `text` → `ink`, `text.dim` → `ink-2`, `text.faint` → `ink-3`, `accent` →
  `brand`, `meter.safe` → `mixer.meter.safe`, `rail.*`, `field`, `combo`,
  `hardware*`, `plate`, `sheen`, `editor.<snake>` → `editor.<kebab>`. The
  alias table is one `const` in the bridge and is covered by the names test.
- **Theme-private names** (`wb.bevel.light` in `apps/session/ui/themes/workbench/tokens.frame`)
  need a theme prefix of at least two segments that is not a Studio namespace.
  They never enter the cascade.

### 2.2 `StudioTokens`: one typed struct, one source of names

This follows herdr's split (`herdr:vendor/gpui-kit/crates/base/src/theme_tokens.rs`
`SemanticThemeTokens` for the typed values;
`herdr:vendor/gpui-kit/crates/shell/src/theme_tokens.rs` `COLOR_TOKEN_NAMES`
and the test `token_names_match_gpui_base_fields`), with one change: the
names, the field and the default are written **once**, in a table macro, so
they cannot drift apart.

```rust
// apps/session/ui/src/theme/tokens.rs (new)
studio_tokens! {
    // field path              token name               default (over architect's semantic tokens)
    arrange.bg:          Rgba = "arrange.bg"          => "mix($desk, $background, 0.5)",
    arrange.row_a:       Rgba = "arrange.row.a"       => "mix($background, $ink, 0.03)",
    arrange.row_b:       Rgba = "arrange.row.b"       => "mix($background, $ink, 0.05)",
    arrange.grid_bar:    Rgba = "arrange.grid.bar"    => "alpha($ink, 0.18)",
    arrange.cursor_play: Rgba = "arrange.cursor.play" => "$warn",
    arrange.cursor_edit: Rgba = "arrange.cursor.edit" => "$info",
    arrange.item_body_alpha: f32 = "arrange.item.body-alpha" => "0.42",
    take.hues: [Rgba; 12] = "take.{0..11}"            => ["#e5484d", "#3e8ef7", …],
    // … every token in docs/theme-catalogue.md
}
```

The macro generates:

- `StudioTokens` (nested `ArrangeTokens`, `TcpTokens`, `MixerTokens`,
  `EditorTokens`, `FxTokens`, `ChromeTokens`), all `Copy + PartialEq` plus
  `Rgba` / `f32`;
- `StudioTokens::NAMES: &[&str]` and `StudioTokens::DEFAULTS: &[(&str, &str)]`;
- `impl FromResolved for StudioTokens`, which calls `Resolved::color` /
  `number` per name (magenta and a logged miss for an unknown name).

**Tests, in the same file:**

| Test | Asserts |
|---|---|
| `token_names_match_fields` | facet's field paths of `StudioTokens` map one-to-one onto `NAMES`, sorted, with no duplicates (herdr's `token_names_match_gpui_base_fields`) |
| `default_theme_has_no_magenta` | resolving the built-in dark and light themes yields no token equal to architect's `MAGENTA`, so every default resolves |
| `default_matches_today` | with no layers, the projections equal today's `Palette::from_theme(&Theme::dark())` and `base_look`, field for field. This is the pixel-identity gate for migration step 1. |
| `frame_themes_name_known_tokens` | every `$name` in `apps/session/ui/themes/**` is in `NAMES`, in the alias table, or theme-private |
| `aliases_resolve` | every alias names a real token |
| `reaper_import_covers_arrange` | every `ArrangeTheme` field the REAPER import sets lands on a `StudioTokens` name (the mapping table in [section 4](#4-what-happens-to-each-model)) |

### 2.3 Typed per-view projections

Each view keeps the pattern `Palette` already has (colours resolved once,
`PartialEq`), but the slice comes from `Resolved`:

| Projection | Replaces | Re-records on change | Repaints only |
|---|---|---|---|
| `ArrangePalette` | `Palette` fields read by `apps/session/ui/src/arrangement.rs`, `apps/session/ui/src/arrangement/record.rs`, `apps/session/ui/src/ruler/*`, `apps/session/ui/src/automation_draw.rs` | `RecordedSlice` (row stripes, divider, item body alphas, fade shade, take hues, kit colours, peak colour rule): these are baked into the recorded `lanes` scene | `OverlaySlice` (cursors, selection, razor, marquee, ghosts, hits, grid, ruler): drawn per frame, so no rebuild |
| `TcpPalette` | `Palette.tcp_*`, `track_tint`, `chrome` | the panel scene (`Arrangement::panel`) | meters, lane pills, hover |
| `MixerPalette` | `Palette` and `chrome` in `apps/session/ui/src/mcp.rs`, rack inks in `apps/session/ui/src/tone.rs` | `Mixer::build` | `overlay::controls` |
| `EditorLook` | `base_look` + `themed` + `set_recolour` | the editor's roll scene | playhead, selection |
| `FxPalette` | `fx_view.rs` / `tone.rs` inks | device scenes | — |

Splitting each projection into a *recorded* slice and an *overlay* slice is
what makes "repaint only when the slice changes" real. Editing
`arrange.cursor.play` costs nothing beyond the next frame. Editing
`arrange.row.a` re-records the lanes once (an `rebuild_hiding` with every row
dirty, about one build). Editing `editor.pitch-3` touches neither. The widgets
stop comparing `skin::version()` and call `Projected::update` instead
(`ArrangementWidget::draw` today: `if skin::version() != self.skin_seen {
self.palette = …; self.recut(); }`).

**Colour types.** `Rgba` (architect) is converted to `vello::peniko::Color`
once, inside `from_resolved`. Painters never see `Rgba`, `daw_ui::theme::Color`
or `audio_controls::theming::Color`. The note table moves from hex-string keys
to indices (`EditorLook.pitch[12]`, `EditorLook.drum[7]`, `EditorLook.cc[8]`,
`EditorLook.string[n]`), so `features/editor` no longer parses CSS strings at
draw time (`color` and `recoloured` in `features/editor/src/paint/paint.rs`).

### 2.4 Frame roles for layout, host slots for dense content

Frame stays the layout and geometry engine for bounded, per-row chrome. Dense
content stays on the host's culled, recorded scene, coloured by tokens:

| Content | Instances at 2,000 tracks / 20,000 items | Drawn by | Theme controls |
|---|---|---|---|
| TCP rows, mixer strips, rack units, lane headers, ruler chrome, editor bars/keys/menus, rails, transport | ≤ rows (visible ≤ ~100) | **Frame role** (`tcp.row`, `mcp.strip`, *`arrange.ruler`*, *`arrange.lane`*, `editor.*`, `rails`, *`transport.bar`*) | Layout, geometry, artwork, state visuals, `@name` hits |
| Item bodies, edges, names, fades, handles | 20,000 | **Host**, recorded once per change in `record_item` | `arrange.item.*` colours **and geometry numbers** (`arrange.item.radius`, `arrange.item.edge-width`, `arrange.item.label-inset`, `arrange.fade.handle-size`) |
| Waveforms (coarse, fine pass, sample dots) | 20,000 × chunks | **Host** `%wave`-style, `replay_command` + `fine` with `FINE_BUDGET` | `arrange.wave.peak` / `.rms` / `.clip` / `.zero` / `.selected`; a colour rule (`item`, `track`, `token`) |
| MIDI notes, velocity, CC, curves | thousands | **Host** (`%notes`, `%stems`, `%cc`, `%curves` in the editor roles) | `editor.pitch-*`, `editor.cc.*`, `editor.velocity.*` |
| Grid, ruler ticks | hundreds per frame | **Host** `ruler::grid`, `ruler::ruler` | `arrange.grid.*` colours and line weights |

**Why items are not a Frame role:** Frame caches one plan per distinct size
(`frame:crates/frame/src/theme/runtime/plan.rs`), and items come in
continuous widths, so a per-item role would plan almost once per item. A
theme still reaches every visual property of an item, through typed
geometry tokens the host painter reads. If a future Frame "template" form
compiles a role once into a size-independent host primitive (rects and
strokes as linear functions of `w`/`h`), `arrange.item` can become a role
without changing the token names.

**Performance obligations** (each one comes with a bench phase):

1. A token or theme edit never runs on the audio thread and never blocks a
   frame. Resolution and Frame compiles happen on the existing
   `theme-reload` worker (`start_poll_worker` in
   `apps/session/ui/src/skin/mod.rs`) and publish with `set_if_version`.
2. **The live pass reads only Live fields.** Today `control_row`
   (`apps/session/ui/src/overlay.rs`) rebuilds the *whole* `row_fields` record
   for every visible row on every frame: about 100 `Value`s, the strings, two
   `Font::elide` calls and `rail_layout`. That is used for the 20 Live slots
   only. The host should keep each row's static record from recording time
   and pass `theme.live` a small live overlay (Frame: a `Fields` impl over
   "static record + live patch").
3. **No full panel re-record per zoom frame.** `Arrangement::repanel`
   re-records every row through `Theme::record_with` whenever `zoom_y`
   changes. That is the measured themed regression (section 5). Record the
   visible rows first and finish the rest under a budget, as the fine
   waveform pass does. Better still, cache recorded rows keyed by (row
   fingerprint, height to the half pixel, theme revision), so a zoom back and
   forth replays.
4. **Gate:** with `FTS_THEME` set, the bench at 2,000 tracks / 20,000 items
   stays within noise of the default at p50 and p99 in every phase.

### 2.5 The Frame bridge (Studio-owned)

Frame inlines `$tokens` at compile time (`resolve_tokens` in
`frame:crates/frame/src/theme/runtime/compile.rs`; "tokens cannot read the
box or the host's data"), so the bridge runs in two directions, and neither
feeds the other:

- **Up (Frame → cascade).** When a Frame theme is picked, its own
  `<Variables>` (`Theme::tokens()` of the theme compiled from its files
  alone) become the token layer of a registered architect `Theme` (id
  `frame-<stack>`), applied as `LayerKind::Pick` (or `Workspace` for a
  screenset). Studio names, after aliasing, enter the cascade. Theme-private
  names do not.
- **Down (cascade → Frame).** After every resolve whose `Projected<FrameTokens>`
  changed, the bridge writes one synthetic `<Variables>` document with the
  resolved value of every Studio token the theme references. It hands that
  document to `ThemeHost` as the **top** layer, and compiles on the worker.
  A person's edit to `brand` therefore recolours the console's `$accent`
  inside `mcp.strip` without touching the theme's files.
- **Cycle-free by construction:** the up layer is computed from the files
  without the overlay, and the overlay is computed from the resolved cascade.
  An edit made in the Frame editor to a `<Var>` is a file edit (up). An edit
  made through `THEME_SET_TOKEN` is a cascade edit (down).
- **Recompile only when needed:** the overlay holds only the tokens some
  role references (`$name`s collected at compile), so editing
  `arrange.row.a` does not recompile Frame at all.

## 3. The architect-side contract

What Studio needs from `architect-theme` (`architect:features/theme`). Every
item keeps the rule that **architect never depends on Frame or Studio**: the
Frame bridge, `StudioTokens` and the REAPER mapping live in Studio.

| # | Need | Status in `architect:features/theme/src` today | Ask |
|---|---|---|---|
| C1 | **Namespaced app token names**, such as `arrange.item.edge` | `valid_token` = `valid_id`: `[a-z0-9-]` only | Accept dot-separated segments of `[a-z0-9-]`. When emitting CSS variables, emit only undotted names, or emit dotted ones as `--<a>-<b>-<c>` behind an opt-in. |
| C2 | **App defaults that follow the palette:** a token whose value references other tokens | overrides are literal strings in a `BTreeMap<String, String>` | A layer value may be `$name`, `mix($a, $b, t)` or `alpha($a, t)`, resolved after derivation in cascade order. Unknown references give magenta and a `Problem`. (An alternative is a `fn(&Resolved) -> BTreeMap` app-derive hook registered with the `Registry`. Expressions are preferred: agents can write them.) |
| C3 | **Numbers as well as colours** (`arrange.item.body-alpha`, `track.tint-strength`, geometry) | `Resolved::value` returns `&str`; `color` parses | `Resolved::number(name) -> f32`, with a fallback rule like magenta's (`NaN` plus a logged miss) |
| C4 | **Typed projections with change detection** | `FromResolved`, `Projected<T>` with `update`: present | Keep. Add `Projected::revision()` so a recording can stamp what it was cut with. |
| C5 | **A revisioned store readable from a worker** | `ThemeStore::current`, `revision`, `set`, `set_token`, `preview`: present | `ThemeStore: Send + Sync`, plus a cheap `revision()` (atomic) that the UI thread polls each frame without locking |
| C6 | **Layers fed by the app** (REAPER import, Frame `<Variables>`) | `Registry::insert(Theme)`, `ThemeStore::set(LayerKind, Layer)`: present | `Theme::from_tokens(id, scheme, BTreeMap)` for a theme built in memory (no file), and `Registry` replacement without a restart |
| C7 | **Provenance** for agents and the inspector | not present | `Resolved::source(name) -> Option<(LayerKind, theme id)>`, the layer and theme that set a token |
| C8 | **Unknown-name reporting** | `color` logs once | `Resolved::misses() -> Vec<String>`, the names asked for and not found, for tests and `THEME_INSPECT` |
| C9 | **Actions** on the Action List and MCP | `THEME_SET` / `THEME_PREVIEW` named in comments | `THEME_SET`, `THEME_PREVIEW`, `THEME_END_PREVIEW`, `THEME_SET_TOKEN {layer, name, value}`, `THEME_RESET_TOKEN`, `THEME_INSPECT {prefix}` (value plus provenance), `THEME_ERRORS`. Each acts on the store only; apps add their own (`STUDIO_THEME_IMPORT_REAPER`, `FRAME_*`). |
| C10 | **Magenta fallback** | `MAGENTA`, `Resolved::color`: present | Keep; it is how a missing default shows on screen and in `default_theme_has_no_magenta` |

Studio-side counterparts, none of which architect sees:

- a `features/ui/studio-theme` crate (or `apps/session/ui/src/theme/`) with
  `StudioTokens`, the alias table, the projections, the REAPER-to-token map
  and the Frame bridge;
- Frame asks, filed against Frame rather than architect: `ThemeHost`
  accepting an in-memory top layer (`frame:crates/frame/src/theme/host.rs`
  has `edit(file, markup)` for files it already loaded, but no overlay); the
  set of `$name`s each compiled role references; a `live` entry point that
  takes a static record plus a live patch.

## 4. What happens to each model

| Today | Becomes |
|---|---|
| `daw_ui::theming::Theme::dark()` at 16 non-test call sites | gone from `apps/session`. Views take their projection from the store. |
| `audio_controls::theming::Theme` (`Tokens`, `ArrangeTheme`, `McpTheme` …) | **an import format.** `theme_from_reaper` still produces it, then `reaper_layer(&Theme) -> Layer` maps it onto tokens. The Dioxus `audio-controls` widgets read `ChromeTokens` through the CSS variables. |
| `daw_ui::theme::palette::Theme` (`Chrome`, `Signal`, `Editor`, styx, REAPER/SWELL export) | **the default values.** `features/ui/daw-ui/src/theme/defaults.rs` constants become the `DEFAULTS` literals where they are not derivable. `fts-theme.styx` loads as a layer. REAPER export reads `StudioTokens`. |
| Frame `<Variables>` | a layer (up), plus the synthetic overlay (down) |
| `Palette`, `Look`, `set_recolour` | `ArrangePalette`, `TcpPalette`, `EditorLook` projections |
| Dioxus CSS consts (`BG`, `RULE`, `TEXT`, `DIM`, `ACCENT` …) | architect-ui's CSS variables (`var(--sheet)`, `var(--rule)`, `var(--ink)`, `var(--ink-2)`, `var(--brand)`); the values are already the same palette |

**The import mapping** (from `features/ui/daw-ui/src/theming/reaper_import.rs`
and `features/ui/audio-controls/src/theming/arrange.rs`), abridged:

| `ArrangeTheme` / `Tokens` field | Token |
|---|---|
| `bg`, `empty_bg` | `arrange.bg` |
| `row_bg[0]`, `row_bg[1]` | `arrange.row.a`, `arrange.row.b` |
| `sel_row_bg[0/1]` | `arrange.row.selected-a/b` |
| `row_divider[0]` | `arrange.row.divider` |
| `grid_measure`, `grid_beat`, `grid_sub`, `vgrid` | `arrange.grid.bar`, `.beat`, `.sub`, `.vertical` |
| `ruler_bg`, `ruler_fg`, `ruler_fg2`, `ruler_sel_bg`, `ruler_loop_bg` | `arrange.ruler.bg`, `.ink`, `.ink-2`, `.selection`, `.loop` |
| `timesel` | `arrange.selection.time` |
| `edit_cursor`, `play_cursor` | `arrange.cursor.edit`, `arrange.cursor.play` |
| `item_bg[0/1]`, `item_bg_sel[0/1]`, `item_blend` | `arrange.item.body`, `arrange.item.body-selected`, `arrange.item.body-alpha` |
| `item_label`, `item_label_sel`, `item_edge`, `selitem_tag` | `arrange.item.label`, `.label-selected`, `.edge`, `.selected` |
| `peaks[0/1]` | `arrange.wave.peak`, `arrange.wave.selected` |
| `fade_line`, `fadezone`, `mute_overlay` | `arrange.fade.line`, `arrange.fade.shade`, `arrange.item.muted` |
| `marker*`, `region*`, `tsig`, `ts_lane_*` | `arrange.marker.*`, `arrange.region.*`, `arrange.tempo.*` |
| `env_default`, `env_vol`, `envlane_divider` | `env.default`, `env.volume`, `arrange.env.divider` |
| `sel_fill`, `marquee_fill` | `arrange.razor`, `arrange.marquee` |
| `Tokens.mute/solo/rec`, `meter_*`, `route_*` | `mixer.mute`, `mixer.solo`, `mixer.rec`, `mixer.meter.*`, `mixer.route.*` |

## 5. Performance: measured

The headless arrangement benchmark (`apps/session/ui/src/bin/bench.rs`) was
run on THEBATTLESHIP's RTX 4080 with a dev build and `FTS_BENCH_SIZE=1600x1000`,
on the orchestral fixture (2,000 tracks, 20,000 items; the scene reports
1,250 rows). The themed runs used `FTS_THEME=apps/session/ui/themes/console:apps/session/ui/themes/workbench`.
There were three interleaved rounds per configuration, 240 frames per phase.
p50 was printed by a local, uncommitted change to the bench's row format; the
bench already computes it (`Summary::p50` in
`apps/session/ui/src/profile.rs`).

**Conditions:** this was not a quiet box. Load average was 14 to 38, and
another process kept the GPU 73 to 91% busy during every run. Treat the
absolute numbers as an upper bound. The comparisons are interleaved, so they
hold.

| Build | Theme | Worst phase p99 (median of 3; range) | Its p50 | Vertical zoom p50 / p99 |
|---|---|---|---|---|
| HEAD (with `1a034e4e`) | default | 10.75 ms (10.10–13.96) | 10.09 ms | 9.93 / 10.16 ms |
| HEAD (with `1a034e4e`) | console + workbench | 41.20 ms (40.27–42.08) | 33.68 ms | 32.43 / 40.27 ms |
| `1a034e4e` reverted | default | 10.90 ms (9.77–12.04) | 10.03 ms | 10.03 / 10.90 ms |
| `1a034e4e` reverted | console + workbench | 35.65 ms (35.11–37.33) | 33.17 ms | 33.17 / 35.11 ms |

- **Themed versus default:** about 3.3× at p50 and 3.3–3.8× at p99 on the
  worst phase (vertical zoom and zoom-both). That matches #377's 9.77 vs
  32.00 ms. Scrolling is unaffected (scroll-both p50 1.1–1.7 ms in every
  configuration).
- **The slot-index commit (`1a034e4e`) made no measurable difference** here:
  p50 is within 0.5 ms either way, and the p99 spread overlaps under this
  contention. It removed `Schema::slot` lookups, but the cost is elsewhere.
- **Where the cost is:** vertical zoom re-records the whole panel every frame
  (`Arrangement::repanel` → `record_panel` → `Theme::record_with` per row,
  with `skin::row_fields` per row). The live pass then rebuilds `row_fields`
  again for every visible row (`control_row`). Obligations 2 and 3 in
  [section 2.4](#24-frame-roles-for-layout-host-slots-for-dense-content)
  target exactly these two loops.

## 6. Migration order

Each step lands on its own, keeps the default render identical
(`default_matches_today` plus the bench's `FTS_BENCH_SHOT`), and updates
`docs/theme-catalogue.md`.

1. **Foundation:** `StudioTokens` with defaults equal to today's colours, the
   names tests, the projections and the store bridge. No view changes yet:
   `Palette::from_theme` is reimplemented as `ArrangePalette::from_resolved`
   and the pixels are unchanged. *Why first:* everything else is a one-line
   change of where a colour comes from once this exists, and the identity
   test guards it.
2. **Arrangement colours (Palette·F and hard-coded):** 50 of its 88 entries
   cannot be reached by any theme today, and no Frame role touches it. The
   work is colour-only: rows, grid, ruler, item alphas, fades, cursors, take
   and kit hues, comp and stretch colours. This step also lets the REAPER
   import through. *Why second:* it is the surface asked for first, it is
   the largest gap, and it carries no geometry or performance risk, because
   recorded colours change only on a token edit.
3. **The themed-panel performance fix** (obligations 2 and 3), then the
   remaining TCP roles: compact rows, lane pills, automation button, row
   meter. *Why before more Frame roles:* the one measured regression lives
   here, and every new per-row role would multiply it.
4. **Dioxus chrome** (transport, toolbar, shell, Record view, FX and editor
   dock grounds): swap the CSS constants for architect's CSS variables. This
   is mechanical and visible everywhere, and it removes the 214 CSS literals among the
   279 Rust literals.
5. **The editor:** an indexed `EditorLook` replaces the hex-keyed recolour
   table, and CC, string, band, note-label, tune and native ruler colours
   become tokens.
6. **The rest of the mixer:** rack inks, phase tints, overview, dimming. A
   `transport.bar` role, and routing, sends and master strip roles as those
   views get built.
7. **FX view:** `fx.*` tokens and the first shipped `fx.phase` / `fx.macro`
   roles. Plugin faces receive a small `face.*` token subset as Frame
   variables, so a face can follow the theme where its artwork allows.
8. **Arrangement Frame roles** (*`arrange.ruler`*, *`arrange.lane`* header,
   item geometry tokens), after steps 3 and 4 hold the bench gate.
