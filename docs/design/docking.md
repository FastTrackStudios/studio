# Docking and screensets

One panel system for all of Studio. Signal and Session are not two UIs:
they are two **screensets** of the same panels. Asked for 2026-10-29;
modelled on REAPER's dockers and screensets and on MaxPane
(github.com/b451c/MaxPane). Builds on `docs/design/unified-shell/spec.md`
(regions, tokens, touch rules) and `docs/design/studio-app-shell.md` (form
factor by size); supersedes the spec's "only sidebars split" limit.

Code: `features/ui/dock` — `dockspace` (the model), `screenset`, `hit`
(drop targets), `builtin` (kinds and shipped sets), `host` (feature
`host`: the Blitz renderer). `examples/dock_shot` draws every built-in set
at every form class to PNGs.

## The model

A window's arrangement is a **`Dockspace`**:

```
            +--------------------- top ----------------------+
            | left |            centre             |  right  |
            +-------------------- bottom --------------------+
   floats:  [ window: its own tree ] [ window: its own tree ]
```

- **Areas:** the centre, four edge docks (left, right, top, bottom: each a
  size and open / collapsed to a rail / hidden; which pair owns the
  corners is a setting) and any number of **floating windows**, each with
  its own tree.
- **Every area is a tree**: a `Split` in two (row or column), sized by a
  ratio *or by a fixed px on one side* — a toolbar, the transport band, the
  TCP against the arrangement keep their size while the rest flexes — or a
  `Stack` of panels shown as tabs (or `Bare`, no header: toolbars, the
  transport).
- **Panels are instances**, not kinds. A `Panel` is an id, a kind
  (`"arrangement"`, open-ended: features add their own), an optional title,
  an optional **link group** and its own **view state** (zoom, scroll,
  which item an editor follows). Two arrangement views at different zooms,
  or a MIDI editor and an audio editor side by side, are two instances.
- **Links:** the TCP and the arrangement are separate panels; attached
  (`Dockspace::attach`, the panel menu's "Attach to Arrangement") they sit
  side by side at a fixed TCP width and share a `LinkId`, which panels use
  to share vertical scroll and the track set. Put them apart and they are
  two panels again.
- **Maximise** (REAPER's / MaxPane's solo) is a flag, ended by any
  structural change.
- **Repair on load** (MaxPane's rule): tabs naming no panel go, a panel in
  two places keeps the first, a panel placed nowhere goes, shares are
  clamped. A layout naming a kind the app does not know keeps it, drawn as
  a placeholder, so a newer layout is not destroyed by an older app.

## Screensets

A `Screenset` keeps a `Dockspace` per **form class**: phone upright, phone
sideways, tablet upright, tablet sideways, desk. Recalling a set shows the
variant for the window's class, or the nearest stand-in
(`FormClass::fallbacks`: a phone borrows the other phone orientation
before a tablet's).

`Screensets` holds the sets, the active one, and the **working copy** per
class — what is on screen, edited live. Save writes the working copy for
the current class into the active set; Save as makes a new set; recalling
throws working copies away. Turning the device round switches working
copy without losing the other orientation's edits. Number slots recall
sets by key.

Shipped (`dock::builtin`), in number-key order:

| Key | Set | What it is |
|---|---|---|
| 1 | **Performance** | progress over chart \| lyrics, transport under (Session's Performance view) |
| 2 | **Overview** | progress over [chart editor] \| chart over lyrics \| tracks (Session's Overview) |
| 3 | **Session** | tracks; mixer / editor / FX below; browser left (the DAW view) |
| 4 | **Mix** | the mixer, FX beside it |
| 5 | **Edit** | tracks over the editor |
| 6 | **Charts** | chart and chart editor; lyrics right |
| 7 | **Signal** | rig; chain and transport below; chart right |

Every variant keeps the transport in the centre or the bottom dock, never
a phone sheet.

**Session's views are screensets.** Session's `shell::View` switch
(Performance, Overview, DAW, Chart, Lyrics, Mixer, FX, Editor, Setup) and
its `Pane` / `Pins` docking become this: Performance, Overview and the DAW
are the sets above; the whole-window views (Chart, Lyrics, Mixer, FX,
Editor) are one panel maximised, from any set; Setup is a panel. The
pinned progress and transport are the `progress` and `transport` panels
each set places. Record mode swapping Performance's chart for the record
view stays a mode's concern (a panel reading the mode), not a layout.

## The host (Blitz)

`dock::host::DockHost` draws the working copy; `use_dock_provider` puts
the screensets, the form class and the app's `Catalog` (kind → title,
rail label, home, chrome, attach rule, render fn) in context — no
globals, so each window or plugin editor hosts its own.

- **Desk / tablet:** edge docks around the centre with dividers (drag to
  resize, double-click collapses to a rail); splits drag; floats are
  absolute boxes over the host with a title bar to move and a corner to
  resize.
- **Phone:** the centre fills the screen; every docked or floating panel is
  a 48 px button on a bar — along the bottom upright, down the side
  sideways (studio-app-shell.md) — that opens it as a sheet.
- **Moving panels:** drag a tab (pointer events; Blitz has no DnD). Near
  the window edge → that edge dock; over a stack, its outer bands (a
  quarter of it, 24–96 px) split it, its middle adds a tab. Always also
  through the panel menu (⋯ or right-click): move to an edge / centre /
  float, attach, open another instance, maximise, collapse, close. Drag is
  never the only way (spec §2.2).
- **Toolbars** are panels (`toolbar`, bare). Put one on the top edge,
  against the arrangement in a fixed-px split, or in a float.

## Not yet

- Panels keep hook state only while their place in the tree is unchanged:
  a split above them remounts them. Their lasting view state belongs in
  `Panel::state` (`Dock::set_state`).
- Floats are boxes inside the window; real OS windows (and loft / niri
  placement) come next, one `Dockspace` per window, using the engine's
  window targeting (`SESSION_WINDOW_TARGET`).
- Peek (a rail icon showing its panel over the centre) opens the dock for
  now.
- Wiring the real panels into Studio, with the per-instance fixes they
  need: the arrangement's restore key (`panel.rs` `KEPT`, per song) must
  become per instance; the editor's `PICKED` is global; the mixer needs
  `Links` without `DawPanels`; the TCP is drawn inside the arrangement
  widget today; there is no lyric editor on Blitz.
