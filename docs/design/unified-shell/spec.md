# Unified studio shell — design spec

Status: draft 1 for review · Owner: Designer (STA-152) · Parent: STA-142
Mockups: `docs/design/unified-shell/mockups/` (follow-up commit, static HTML/CSS)
Audit input (A): not used yet, this spec stands on the code and on `docs/signal/DESIGN.md`. Revisit when A lands.

One shell for Session and Signal (Signal is the Session engine with a live-guitar-rig face), on desktop and iPad, with Keyflow charts as a first-class panel in both. This doc covers the regions, how panels dock/collapse/persist, desktop vs iPad, Signal's live-rig ("Stage") mode, the setlist/practice view, how Session and Signal show the same song, and the states every region needs.

## 0. What exists today (so this is an evolution, not a rewrite)

- Session already has a top bar (`BAR_H` 40), bottom bar, four-plus views (`shell::View`: Setup, Performance, Daw, Overview, Chart, Lyrics, Mixer, Fx, Editor), pinned progress strip and transport (`shell::Pins`), a left navigator/browser column, a right inspector, and docking of Mixer/FX under the arrangement (`shell::Pane`: press = own view, docked = pinned under host views). `Density` is Narrow/Compact/Full by bar width (1250/1700).
- Docking tree, presets, persistence and history exist in `features/ui/dock` (`dock-proto`); the Blitz renderer for it is not built (docs/app-on-blitz.md, phase 3).
- Signal has its own design system, "Signal Touch" (`docs/signal/DESIGN.md`): 48px hit, 56px bar, 64px rows, 104px chain strip, green = sounding, blue = picked. It has a Perform/Setlist prototype (`docs/signal/prototypes/touch`) with NOW / NEXT / MAP, and Mute / Panic safety buttons.
- Constraints from `CLAUDE.md` that this spec respects: Blitz renders everything (inline styles, no external CSS, no `position: fixed/sticky`, no `overflow: auto`, no container queries, no `text-overflow`, no `text-shadow`), and root components take no props. The mockups use none of those either.

Design stance: the unified shell is the Signal Touch design system applied to Session's region model. Tokens are unified (section 9); Session's hex constants (`BAR_BG #17181b`, `RULE #2a2c31`, `ACCENT #3aa0ff`) are retired in favour of Signal Touch tokens.

## 1. Regions

```
+--------------------------------------------------------------+
| APP BAR  (56)  views · song/crumbs · mode · safety · settings |
+------+----------------------------------------------+--------+
|      | TOP DRAWER   (progress / song strip / cues)  |        |
|  L   +----------------------------------------------+   R    |
|  e   |                                              |   i    |
|  f   |                    MAIN                      |   g    |
|  t   |         (the view: chart, rig, arrangement)  |   h    |
|      +----------------------------------------------+   t    |
|      | BOTTOM DRAWER (transport · practice · chain)  |        |
+------+----------------------------------------------+--------+
```

- **App bar** (not a drawer, never collapses): wordmark/app switcher, view tabs, crumbs (song, rig), mode, Mute / Panic (always, both apps), settings. 56px on iPad and Signal, 40px allowed on dense desktop Session until it adopts the Touch bar (see section 9).
- **Left sidebar (L)**: what you choose from. Setlist/songs, browser, navigator, rig/preset library. Default 280px (min 220, max 420).
- **Right sidebar (R)**: what you inspect. Inspector (selected track/block), song info and notes, chart outline (sections), rig scenes for the song. Default 320px (min 260, max 480).
- **Main (M)**: one view at a time (Perform, Chart, Arrangement, Rig, Overview...). Never collapses; takes all remaining space; minimum 480px wide, below that sidebars become overlays (section 4).
- **Top drawer (T)**: slim, glanceable, full main width. Song progress with sections (Session's `PinnedProgress`), Signal's NOW / NEXT strip. Heights: closed 0, strip 48 (desktop) / 64 (iPad), tall 144 (cue list, section map). Not resizable freely: it snaps to those three.
- **Bottom drawer (B)**: what you operate. Stacked bands, top to bottom: **Practice panel** (when open) → **Transport band** (always visible in Perform/Setlist/Stage, 64px) → **Chain strip** (Signal only, 104px, always visible per Signal Touch rule). Mixer/FX dock here as a tabbed panel above the transport (today's `Pane` docking under the arrangement). Heights: collapsed (transport band only), standard (+ practice, ~176), expanded (+ timeline/mixer, ~340, max 60% of main).

Sidebars run the full height between app bar and window bottom; drawers live inside the main column. Reason: sidebars are navigation context that should not jump when a drawer opens; the transport belongs to the song in main, and aligns under it.

### 1.1 Panel catalogue

Every panel is a prop-less component (context in via `use_context`), so any panel mounts in any region (docs/app-on-blitz.md, "panels are components").

| Panel id | Default region | Allowed regions | Min size | Both apps? |
|---|---|---|---|---|
| `setlist` | L | L, R | 220w | yes |
| `browser` | L | L, R | 220w | yes (sources differ: samples/FX vs presets/profiles) |
| `chart` (Keyflow) | M | M, R (narrow, "reference") | 360w main / 280w right | yes |
| `lyrics` | M | M, R | 320w | yes |
| `arrangement` | M | M | — | Session |
| `rig` (scene grid + stage header) | M | M | — | Signal |
| `inspector` | R | R, L | 260w | yes |
| `song-info` (key, bpm, notes, links) | R | R, L | 260w | yes |
| `outline` (sections of the chart) | R | R, L, T (as chips) | 200w | yes |
| `progress` | T | T | 48h | yes |
| `now-next` | T | T | 64h | Signal (Session optional) |
| `transport` | B | B | 64h | yes |
| `practice` | B | B | 112h | yes |
| `mixer`, `fx` | B | B, M (maximised) | 200h | Session; Signal gets a compact mixer in R |
| `chain` | B | B | 104h | Signal |

A region holds one or more panels as **tabs**. A sidebar can also **split vertically into two stacks** (e.g. Setlist above, Outline below); nothing else splits. Main holds exactly one view, but a view may declare an internal split (Overview: chart left, arrangement right). This keeps the model small enough to build on `dock-proto` and to explain.

## 2. Docking, collapsing, persisting

### 2.1 Panel states

| State | Meaning | How |
|---|---|---|
| **Pinned** | Visible in its region, takes space | default |
| **Collapsed** | Region shrinks to a **rail** (44px desktop, 56px iPad) of panel icons with labels on hover/long-press; tap an icon to reveal the panel as an **overlay** (peek) over main, tap away to dismiss | chevron in region header, `Cmd+B` (L), `Cmd+Alt+B` (R), `Cmd+J` (B), `Cmd+Shift+J` (T) — proposed, to reconcile with the existing keymap (`b` is already "browser") |
| **Hidden** | No rail, no space; reachable from the View menu and the command palette | rail "x" or panel menu → Hide |
| **Maximised** | Panel fills main as its own view; press again to return (today's `Pane::press_full`) | header button, double-tap header |
| **Docked/undocked (Mixer, FX)** | Existing behaviour kept: undocked = its own view; docked = pinned under its host views | right-click / long-press the panel's button → "Dock under Arrangement" |

Peek overlays dismiss on tap-outside, Escape, or switching view. They never push main (no layout shift), so the chart does not reflow while a player peeks at the setlist.

### 2.2 Moving panels

- **Desktop:** drag a panel tab to a region edge. Drop zones light up as 2px blue (`focus-fg`) outlines with a 12% wash; the five regions are the only targets. Dropping on a tab strip adds it as a tab. Escape cancels.
- **iPad / touch:** long-press the tab (400ms) → the panel lifts (the "lifted row" look, `sheet-2`), regions show drop zones. Plus a non-drag path that is always available: panel menu → **Move to ▸ Left / Right / Bottom / Top**. Drag is never the only way (accessibility and fat-finger safety).
- A panel can only go where the catalogue allows. Disallowed regions do not light up.
- Resize: a divider with an 8px visible hairline and a **44px invisible hit area**; double-click/double-tap resets to default size. Sidebars resize freely within min/max; drawers snap to their heights.

### 2.3 Persistence

One layout document, versioned, stored through `utils::prefs` (already used for `signal-engine-mode`), key `shell.layout.v1.<app>.<class>.<mode>`:

```
{ v: 1,
  regions: { L: {open, w, stacks:[{panels:[id..], active:id}]},
             R: {...}, T: {open, h:"strip"|"tall"}, B: {open, bands:{practice:bool, chain:bool}, h} },
  main: { view: "perform" },
  pins: { ... existing Pins ... } }
```

- **Keyed by size class and mode**, not by device: `desktop-wide`, `desktop-compact`, `ipad-landscape`, `ipad-portrait`, and `stage` (Signal live-rig mode, section 5). Resizing a window across a class boundary switches to that class's saved layout; it does not squash one layout.
- **Saved:** region open/collapsed/hidden, sizes, tab order and active tab, split position, current view, drawer heights, fit mode of the chart (Page/Width, already kept in `chart_panel::keep_fit`), chart zoom per class.
- **Not saved:** peek overlays, maximised state, scroll positions, the active song and playhead (those are engine state — "the rig never depends on the screen").
- **Presets:** built-in layouts per app (Session: Perform, Arrange, Overview, Setup. Signal: Perform, Setlist, Edit, Stage). "Reset layout" restores the preset for the current class. "Save layout as..." stays out of v1.
- **Migration:** unknown panel ids are dropped; panels new to a preset appear in their default region collapsed to the rail (never pop open and move the user's layout); a layout with a version newer than the app falls back to the preset.
- **Remote screens:** layouts are per-remote (an iPad and a laptop attached to one engine keep separate layouts). The *stage* layout can be marked "use on all remotes of this rig", because a bandmate's view of the same rig should match.

## 3. Desktop vs iPad

Layout adapts to **window width**, not to device, so Stage Manager, Split View, Slide Over and desktop windows all work. Size classes (logical px, window width):

| Class | Width | Left | Right | Top | Bottom | Main |
|---|---|---|---|---|---|---|
| **XL** desktop wide | ≥1700 | pinned | pinned | strip/tall | all bands | ≥ 700 |
| **L** desktop / iPad 13" landscape (1366) | 1250–1699 | pinned | pinned or rail (user) | strip | transport + practice | ≥ 500 |
| **M** iPad landscape 11" (1194/1024), half-screen desktop | 900–1249 | pinned or rail | rail, peek overlay | strip | transport + practice | ≥ 480 |
| **S** iPad portrait (820/744), Split View 1/2 | 600–899 | rail, peek overlay | rail, peek overlay | strip, 64 | transport; practice as **sheet** | full |
| **XS** Slide Over, phone landscape | <600 | one sidebar at a time as a full-height **sheet** from the edge | same | hidden (NOW/NEXT folds into app bar) | transport band only; practice as sheet | full |

`Density` (Narrow/Compact/Full) stays as the app bar's own detail level; its thresholds move to these classes (XL=Full, L/M=Compact, S/XS=Narrow). One source of truth for "how wide am I".

### 3.1 Touch rules (apply on any touch input, not just iPad)

- **Hit targets:** 48px minimum (Signal's `hit`), 44px absolute floor only for secondary icons that have a 48px padded hit area. Transport primary (play/pause) 64px. Adjacent targets 8px apart minimum; destructive ones 16px.
- **No hover-only affordances.** Everything on hover has a long-press or visible equivalent. Right-click = long-press (400ms), with a haptic tick on iOS.
- **Gestures (each has a button equivalent):** edge-swipe from the left/right opens that sidebar's peek; swipe up on the drawer handle opens the next height; swipe down closes. Horizontal swipes inside the chart are chart panning, never region gestures; region edge-swipes only start within the outer 20px. The bottom drawer handle sits 24px above the home indicator safe area so the system gesture does not fight it.
- **Safe areas:** honour insets; the app bar extends under the status bar; the transport band sits above the home indicator.
- **Keyboard:** Magic Keyboard / desktop use the same shortcuts. `F6`/`Shift+F6` cycle focus between regions. Visible focus ring: 2px `focus-fg` offset 2px.
- **Split View:** the shell reflows live as the width changes; the song, playhead, and loop are engine state so nothing resets. Dropping from M to S collapses the right sidebar to its rail first, then the left.
- **Apple Pencil:** not required; Pencil taps behave as finger taps. (A later chart-annotation feature may use it.)
- **Dynamic Type (non-Stage):** body text scales with the system setting up to AX1; at larger sizes the shell steps down a size class (sidebars become overlays) rather than truncating. Stage uses its own fixed large scale (section 5).

## 4. Collapse order when space shrinks

Always in this order, so behaviour is predictable: (1) practice panel → sheet, (2) right sidebar → rail, (3) left sidebar → rail, (4) top drawer strip stays, tall → strip, (5) chain strip stays (Signal), (6) sidebars rail → overlay-only (rail itself hidden below 600). The **transport band and the app bar never disappear**.

## 5. Signal live-rig ("Stage") mode

Purpose: the rig is at your feet on a dark stage, the screen is read standing, from 1–1.5 m, mid-song. Stage mode is a **mode of the same shell**, not a separate app: same regions, same engine, a different layout preset and a stage theme. Entered from the app bar "Stage" switch (and optionally auto-entered when a setlist is started in Live mode, `audio_mode`). Exit needs a **hold** (see Lock).

### 5.1 Layout (landscape, reference 1194×834; scales to 1366 and 1024)

- **App bar** 56px, reduced to: rig crumb (read-only), **Mute house** and **Panic** (the Signal safety buttons, 72px), engine health pill, lock, exit-Stage (hold).
- **Top drawer — NOW / NEXT** (tall, 144px): left, song number in its song colour + title (Display, 44px/750), key + BPM pills; centre, **the current section** very large (64px, section colour) with its patch name below; right, **NEXT** card (what a tap goes to, next section or next song) with a back chevron. Below, the **MAP**: the song's sections as one bar, played ones dimmed, the one playing lit green. Same model as the existing `SetlistStrip` prototype.
- **Main — Rig:** a grid of **scene tiles** (snapshots/variations of the current patch): 4 columns × 2 rows on 1194w, each ≥ 200×140, 96px minimum height. Label 28px/780, stack-colour 10px band at the top (a band, never a large fill), a **footswitch badge** ("FS 3") top-right. The tile that sounds has the green bar under it; a picked-but-not-sounding tile has the 2px pale-blue border (Green plays, blue picks). Tapping a tile switches scenes immediately (no hold delay: latency matters on stage); the Lock below protects against accidental structural changes instead.
- **Right sidebar:** closed by default in Stage. When open (a toggle "Chart"), it is the **Keyflow chart** at 38% width in Stage type scale (below). A second toggle swaps roles: chart in Main, scene grid collapses to a single row of tiles (96px) along the bottom. Two Stage sub-layouts: **Rig-first** and **Chart-first**, one tap to switch, remembered per setlist.
- **Left sidebar:** hidden. Setlist navigation is the NEXT/back controls and, via the hold-to-open song list, a full-screen picker.
- **Bottom drawer:** the **chain strip** (104px, always) with bypass dots enlarged to a 56px target. Transport band is replaced by a slim **backing/practice control** only if the song has a backing track (Play/Pause, 64px; loop and speed are not in Stage — those are practice features, not performance).

### 5.2 Legibility at arm's length (rules, with numbers)

| Rule | Value |
|---|---|
| Reference viewing distance | 1 m (design), usable to 1.5 m |
| Smallest text anywhere in Stage | 20px (Meta 14px is not allowed) |
| Song title | 44–56px / 750; current section 64–96px; patch/scene name 28px |
| Numerals | tabular, 28px minimum (BPM, key, bar count) |
| Contrast | Primary text on background ≥ 12:1; secondary ≥ 7:1; no text below 4.5:1. Text colour `#f4f4f5` on `#000000` ≈ 19:1 |
| Primary targets | ≥ 96px tall (scene tiles, NEXT, section map segments), 72px for safety and chain bypass |
| Gaps between adjacent targets | ≥ 16px; Panic and Mute are separated from scene tiles by the app bar |
| Colour is never the only signal | each state has a word or glyph too: "PLAYING", "NEXT", "BYPASSED" (struck + dimmed to 55%), a lock glyph |
| Motion | none except meters and the 140ms press; no toasts over targets; honour Reduce Motion |

### 5.3 Stage theme

True-black desk for OLED and low glare (`stage-desk #000000`), panes `#0a0a0c`, hairlines slightly brighter than Signal Touch (`#2b2b31` → `#3a3a42`) so edges survive a bright backlight. **Brightness is the user's**: a Stage-only dimmer (50–100%) in the app bar is a simple opacity scrim, not a theme. No gradients, no shadows (as Signal Touch). Red appears only for Panic / Mute-engaged / engine errors.

### 5.4 Lock and accident protection

- **Lock** (the existing `Pins.lock` idea, promoted): when on, the screen ignores touches in the grey zone around targets and ignores drags (no accidental drag-reorders, no resize). A locked Stage still lets scene tiles, NEXT/back, Mute, Panic, and the transport through. Everything that changes structure (exit Stage, switching rig, editing) needs **press and hold 600ms** with a visible ring filling; the label says "Hold to exit".
- Destructive/high-impact actions (Panic) keep **one tap** (emergency), with an in-progress state ("Restarting engine…") and no confirmation; Mute is a tap, hold for full mute (as in the prototype).

### 5.5 Stage states

- **Empty (no setlist):** centre card "No setlist loaded" with a primary **Choose setlist** button (72px) and a secondary "Play free" that opens the scene grid for the current patch with no NOW/NEXT.
- **Loading (setlist/song loading):** the NOW card shows the title with a determinate progress bar and "Loading samples 12 / 40" in 28px; scene tiles are skeleton placeholders at their final size (no layout shift); the previous song keeps sounding until the new one is ready ("Previous song still active" label).
- **Error — engine offline / dropout:** a full-width red banner pinned under the app bar, 72px, 32px text: "ENGINE OFFLINE — playing through dry signal" (or whatever the bypass really is), with a primary **Reconnect** (96px). Never a toast, never dismissible while true. Tiles stay visible but dimmed to 55% and disabled.
- **Error — missing sound/patch:** the affected scene tile is struck (Out Red strike) with "Missing: <name>"; the rest of the set is playable.
- **Long content:** song titles clamp to 2 lines at the size above (no ellipsis support in Blitz: the component cuts at a character budget with "…" itself and exposes the full title on long-press); patch names to 1 line (28px, ~22 chars budget per tile); a setlist of 40+ songs scrolls only in the picker. NEXT card is a fixed height; its text clamps the same way.

## 6. Setlist / practice view

Used in Session (preparing, rehearsing) and Signal (pre-gig practice and, in Stage, a reduced form). Same view in both: **Setlist** layout preset.

### 6.1 Layout

| Region | Content |
|---|---|
| L | **Setlist panel:** song list, current and next marked, add/reorder in edit mode |
| T | Progress strip (Session) / NOW-NEXT strip (Signal), strip height |
| M | **Chart** (Keyflow) for the current song, Fit Page / Fit Width, follows playhead |
| R | Song info + **Outline** (sections; click to jump/loop). Signal adds "Scenes for this song". Collapses to rail at class M and below |
| B | **Practice panel** over **Transport band** (over **Chain strip** in Signal) |

### 6.2 Setlist panel

Row (64px; 72px on touch): `[drag handle 24 / hit 48] [index] [4px song colour bar] Title (Body 17/560; current = 820, no box) · artist (Meta) ... [key] [bpm] [duration] [status glyphs]`. Second line (Meta, faint): sections count or "Chart ready". Status glyphs: chart (Keyflow) ready / missing, audio ready / missing, rig scene linked (Signal).

- **Current song:** heavier weight, `up` wash, green bar under the row growing with progress (like `SongTabs` fill) while playing; label "NOW". **Next:** label "NEXT". A **played** song is dimmed 70% with a check.
- Tap = select and load (does not start playback). Double-tap/Enter = load and play from top. In Stage, only NEXT/back.
- Header: set name (Headline 28), total time ("14 songs · 62 min"), Edit button; footer: "+ Add song" outline button.
- **Edit mode:** drag handles active (long-press on touch), swipe-left reveals Remove (out red outline, confirmation inline), tap title to rename the display name.
- Song colour: SONG-region colour else `setlist::title_color` — so a song is recognised by its colour first (existing behaviour, kept).
- **Keyboard:** Up/Down moves, Enter loads, Cmd+Up/Down reorders in edit mode, `/` filters.

### 6.3 Chart in main

The existing Keyflow chart panel (`session_daw::chart_panel`), no new renderer. Header: title, artist, key · bpm · meter, version. Fit toggle (Page / Width) as a segmented control in the main header. **Follow playhead** is on by default: the current bar gets the "now" highlight; a manual pan pauses following for 4s then a **Follow** pill returns it. Section labels in section colours; the looped section gets a blue outline. A tap on a bar places the cursor; long-press on a section label sets the loop to that section.

### 6.4 Bottom drawer: transport and practice

**Transport band (64px, always)**: `⏮ Back | ▶/⏸ Play (64) | ⏭ Next | bar.beat · clock | song progress | tempo · key pills | Loop toggle | Metronome toggle`.

**Practice panel (standard ~176px; opens above transport; sheet on S/XS)**: three rows.

1. **Timeline (56px):** the whole song as a bar, sections as coloured segments labelled (sizes proportional to bars, minimum 48px), playhead, and **A** and **B** markers (draggable handles with a 44px hit area, snap to bar lines; hold Alt/Shift for beat snap). Loop region shaded blue 12%. Tap a section = select it; double-tap = loop it.
2. **Loop & A/B:** `[Loop section ▾]` toggle (when on, loops the selected section; A/B follow section bounds), `[A] [B]` set-at-playhead buttons (tap A sets A to playhead, tap B sets B; tap again on a set marker clears), `[Clear]`. **Loop section** and **A/B** are one loop with two ways to set its ends, shown with the same shaded region — there is exactly one loop at a time. Loop count readout "pass 3" and optional "× N then continue" (off by default).
3. **Speed, count-in:**
   - **Speed** stepper with slider, 50%–125%, 1% step, presets 50 / 60 / 70 / 80 / 90 / 100. Shows effective tempo: `92 BPM (80% of 115)`. **Keep pitch** toggle (default on). Reset = tap the percent.
   - **Count-in** segmented: Off · 1 bar · 2 bars · 4 beats, plus **Click** toggle and "Also on loop restart" checkbox (default off). During count-in the transport band shows "Count-in 3 2 1" in 28px; the click plays regardless of the metronome toggle.
   - Click volume (small slider) lives in the overflow menu, not the main row.

Keyboard: Space play/pause · `L` loop toggle · `[` set A · `]` set B · `-`/`=` speed ∓5% · `C` count-in cycle · `Home` back to loop start.

**Engine needs** (to check with the Session engine owners before build; not designed around assumptions): arbitrary play rate 50–125% with pitch preservation (time-stretch), A/B loop region with bar snap, count-in with bars/beats and click routing, loop pass counter. `transport_bar.rs` today has play/stop, loop and tempo; speed, count-in and A/B are new.

### 6.5 Practice sheet (S/XS)

Practice panel opens as a bottom sheet (60% height) with the same three rows stacked, 56px controls; the transport band stays visible beneath it so play/pause is never covered.

## 7. Same song, same chart, in Session and Signal

- **One song model.** Both apps read the same `setlist::Song` (name, project, span, colour) and the same `keyflow::Chart` for it, from the one engine. Selecting a song in either app is the same engine action, so a second remote follows.
- **One Chart panel**, one Setlist panel, one Practice panel, one Transport band, mounted from `session_daw` into both apps. An app contributes only what is its own: Session → `arrangement`, `mixer`, `fx`, `inspector` (track); Signal → `rig`, `now-next`, `chain`, scenes. Shared panels may never be forked per app; a difference is a prop-less variant selected by context (`AppKind`).
- **Same look:** the chart renders with the same engraver and fonts; chrome uses the same tokens. Differences allowed: Stage type scale (larger sizes via a scale factor on the same chart, not a different layout) and the extra Signal-only rows (scenes).
- **Same state:** current song, playhead, loop A/B, speed and count-in are engine state. If Signal (iPad) and Session (laptop) are attached to one engine, a loop set on one shows on the other. View state (fit, zoom, layout) is per remote.
- **Cross-links:** the song info panel has "Open in Session" / "Open in Signal" when the other app is available; the Signal scene for a song is linked from the Session setlist row's glyph.

## 8. States for every region

| Region | Empty | Loading | Error | Long content |
|---|---|---|---|---|
| Setlist (L) | "No songs yet" + **Add song** + **Open setlist** (primary). Never a blank list | 5 skeleton rows at 64px; header shows set name + "Loading..." | Row-level: red outline "Audio missing" / "Chart missing" with Relink; panel-level: "Couldn't read setlist" + Retry | Titles clamp to 1 line (budgeted cut + "…") and show in full on long-press / in the tooltip on hover; 100+ songs → filter field and virtualised list |
| Chart (M) | "This song has no chart" + **Add chart** / **Import .kf** (reuses `NoChart`) | Page-sized skeleton: title block + 8 faint bar lines; progress "Engraving…" | "Couldn't engrave: bar 24, bad chord `Gm7b5#`" with **Open source** and Retry; previous chart not kept | Pages: Fit Page shows next page beside it where room; long lyrics lines wrap, never clip; 60+ bar songs scroll with Follow |
| Outline (R) | "No sections" (chart absent) | skeleton chips | n/a (derived from chart) | chips wrap; names clamp at 18 chars |
| Progress / NOW-NEXT (T) | Hidden when no song; NOW shows "—" | Bar with indeterminate shimmer *off* when Reduce Motion | "Playhead lost" amber label + Resync | Title clamps; section name 1 line |
| Transport (B) | Disabled controls at 40% when no song, play disabled with reason "No song" | Spinner in the play button, controls disabled, "Loading audio" in the clock slot | Red dot + "Engine offline" + Reconnect; the band is never removed | Tempo/key pills never wrap (min width, tabular) |
| Practice (B) | Panel shows "Select a song to practise" | Timeline skeleton with no markers | "Can't time-stretch this audio" disables speed with reason; others still work | Timeline with 30+ sections: min 48px segments → horizontal scroll with fixed playhead window |
| Rig (M, Signal) | "No rig loaded" + Choose rig | tile skeletons at final size | struck tile "Missing: <name>"; whole rig: banner (5.5) | patch names budgeted, 2 lines at 18+ chars in non-Stage |
| Sidebar rail / peek | Rail shows icons only; a peek with nothing shows the panel's own empty state | n/a | n/a | Rail labels never shown; tooltips |

Error copy rule: say what failed, then what to do, in one sentence each. No error codes as the headline.

## 9. Tokens and components

Adopt Signal Touch tokens for both apps (hex, because Blitz reads hex everywhere). Session's constants map as: `BAR_BG #17181b → sheet #0e0e11`, `RULE #2a2c31 → rule-strong #2b2b31`, `TEXT #e5e7eb → ink #e4e4e7`, `DIM #8b9099 → ink-3 #80808a`, `ACCENT #3aa0ff → focus-fg #bfdbfe` for outlines and primary `#2563eb` for the one solid button. Session's existing blue `#3aa0ff` for "loop" becomes `primary`/`focus-fg` (blue = picked/active selection) so green and blue keep their single meanings.

New tokens (Stage and shell): `stage-desk #000000`, `stage-rule #3a3a42`, `stage-ink #f4f4f5`, `scrim rgba(0,0,0,0.55)` for peek overlays, `drop-wash rgba(191,219,254,0.12)`, `warn #f59e0b` (stale/resync only), region sizes as in section 1, motion 160ms for drawers (0 under Reduce Motion), 140ms for presses.

Components to build (each prop-less where it holds a panel; shared in a `shell` module used by both apps): `Region`, `PanelTabs`, `Rail`, `Peek`, `Divider` (8 visible / 44 hit), `DrawerHandle`, `DropZone`, `AppBar` (+ `SafetyButtons`), `SetlistRow`, `SectionChip`, `TransportBand`, `PracticePanel` (`LoopBar`, `SpeedStepper`, `CountInSelector`), `SceneTile`, `NowNext`, `StatusBanner`, `EmptyState`, `Skeleton`. Each has states: default, hover (pointer only), pressed (inset), focus (2px ring), disabled (faint ink, no fill), and for lists: picked (weight + wash), playing (green bar), played (dim + check).

## 10. Hand-off: build order and acceptance criteria

1. **Shell module + regions** (layout only, on Blitz, no drag): `Region`/`Rail`/`Peek`/`Divider`, size classes, persistence document and migration. *Accept:* resizing a window across XL→S switches layout without losing the current song/playhead; layout survives restart; unknown ids in a stored layout don't crash; no `position: fixed` / `overflow: auto` used.
2. **Setlist view in Session** on the new shell: Setlist panel, Chart main, Transport band, Outline. *Accept:* states in section 8 reachable via a debug fixture; 100-song fixture scrolls smoothly; keyboard path works.
3. **Practice panel:** loop section, A/B, speed, count-in. *Accept:* the engine requirements in 6.4 confirmed or filed as separate engine issues first; loop survives a song change by clearing; speed/pitch state shown correctly on a second remote.
4. **Signal on the shell:** Perform/Setlist presets, NOW/NEXT, chain strip, Mute/Panic in the app bar. *Accept:* the same Chart/Setlist/Practice components are used (no fork); song selected on one remote follows on the other.
5. **Stage mode:** layout, theme, Lock, hold-to-exit, error banner. *Accept:* on a 12.9" iPad at 1 m the song, section and scene names are readable (checked in person, not by screenshot); no text under 20px; every target ≥ 72px; engine-offline shows the banner with Reconnect.
6. **Drag-to-dock** (desktop mouse, then touch long-press) on top of `dock-proto`, last because the menu path ("Move to...") already covers it.

Open questions for the Captain / engine owners: (a) engine support for playrate with pitch preservation, count-in and A/B; (b) whether Stage mode auto-enters on "start setlist" or only manually; (c) whether to retire Session's 40px app bar for the 56px Touch bar now or after docking lands; (d) the proposed keyboard shortcuts versus the existing keymap.
