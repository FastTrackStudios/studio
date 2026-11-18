# Studio's arrangement, mixer and track strip as Architect runtime extensions

studio#519. Studio's own views, the ones `studio-desktop` shows, run in a
stock Architect as process extensions, installed and never compiled in, on
Studio's own engine. Nothing is redrawn or re-modelled. (The PR's first
plan, a separate data model and views drawn again from it, was rejected
for that reason; it is in this file's history.)

## 0. In one page

- **`daw`** (`extensions/daw`) runs the engine `studio engine` runs
  (`session_desktop::engine_host`): one `Standalone`, the set, the Action
  List and the engine's window protocol on a Unix socket, published where
  `studio engine` publishes itself. It opens `FTS_DAW_OPEN` (a song, a
  `.session` or a setlist) or what `DAW_OPEN` names. `DAW_RUN` runs any
  of the engine's actions. It draws nothing.
- **`studio.arrangement`** (`extensions/arrangement`) and
  **`studio.mixer`** (`extensions/mixer`, views `mixer` and `strip`) are
  each a Studio window in a process of its own. They attach to the engine
  as `studio-desktop`'s window attaches to a `studio engine`
  (`session_engine::studio_engine::link`), read the song from the window
  protocol's replicas (`session_engine::view_client`), and mount Studio's
  own panels with an Architect pane as their host.
- **`studio-view-link`** (`extensions/link`) is what those processes share
  at build time: the attach, the song as a signal, Architect's pane input
  as the Blitz events Studio's widgets take, and a widget's scene as a
  pane layer that crosses only when it changes. Neither view needs the
  other installed, and neither needs anything but the `daw` extension.
- **One transport in the app:** the engine leads Architect's playhead deck
  and follows it (§4).

## 1. The hosts

Studio's panels already had two hosts: dioxus-native (the window) and the
browser. The extensions are the third, an Architect `PaintPane`:

| View | The window | The browser | The pane |
|---|---|---|---|
| Arrangement + track panel | `studio::Arrangement` (`use_arrangement_panel`, `CustomWidgetAttr`) | `web_host::WebArrangement` | `studio_arrangement::host::Hosted` |
| Mixer | `mixer_panel::Mixer` | `mixer_panel::WebMixer` | `studio_mixer::host::Hosted` over `mixer_panel::HostedMixer` |
| Selected-track strip | `mixer_panel::Inspector` | (none) | the same, `Links::inspector` |

Each frame the pane's paint view does what the window does: it tells the
arrangement panel the frame and where the play cursor is
(`PanelEvent::Frame`, from the same `engine::Transport`), asks the widget
for its picture, and hands pointer, wheel and keys to the panel and the
widget as winit and Blitz would. The panel's DOM chrome (`PanelChrome`:
the ☰ toolbar, the scrollbars) renders in the view's own document.

What a process view changes, and how the host makes up for it:

- **The panel's rectangle.** A process view's nodes are in the host's
  document, which the guest cannot measure, so the panel's own
  measurement never returns. The pane hands it its geometry instead
  (pane-local, as its pointer is).
- **`box-sizing`.** Studio's window sets `*{box-sizing:border-box}`; a
  process view is a Blitz sub-document with Blitz's `content-box`. The
  view carries the rule for its own subtree (a `style` element:
  `document::Style` needs a document provider the guest has none of).
- **Window actions.** A window offers its own actions (`SESSION_ZOOM`, …)
  to the engine's list, and the engine asks them of a window. Only a
  window with window actions of its own takes them
  (`view_client`, `Stream::Queued`): the mixer's process would otherwise
  answer "no action" in place of the arrangement's.

## 2. Smallest changes to Studio's code

All additive. The window draws as before.

- `session_ui::panel::StartCompact`: a pane beside a mixer it is not
  linked to opens with the compact track panel, as a docked panel does.
- `session_ui::mixer_panel::{MixerWidget (now pub), HostedMixer}`: the
  mixer widget for a third host.
- `ArrangementWidget::paint_layers`: the picture and, apart, what moves
  while the song plays (the play cursor with its trail, the track panel's
  level meters). Composed in that order they are `paint_scene`'s picture.
- `session_engine::view_client`: window asks go to a window with window
  actions.

## 3. Same pixels

- **Parity tests** (`extensions/arrangement/tests/parity.rs`,
  `extensions/mixer/tests/parity.rs`): Studio's panels on the golden
  template. Each compares the widget's picture as the window hands it to
  Blitz with the pane's layers sent through Architect's layer protocol
  (`ops::Sender` → `Receiver` → `compose_at`), both rasterised on the CPU.
  0 pixels differ for the arrangement with its track panel (1280×780),
  the mixer (1280×400) and the strip (120×640).
- **Window against window** (`pr-media/arrangement-mixer`,
  `in-app-vs-runtime/`): `studio-desktop` (origin/main 7c2b7ac0) and the
  extension's pane, Drive To LA in the same state at 1280×764. 182 of
  977 920 pixels differ, none by more than 13/255: text anti-aliasing.
- **Theme:** dark only, as the session window is.

## 4. One transport

- **In the app (done).** The current song is the `daw` extension's
  source on Architect's playhead deck (`architect.playhead`). Every move
  of the engine's transport is told to the deck: a play, a pause or a
  seek from any view, key or action, plus a re-anchor every 4 s while
  playing. The deck's own moves (its Now Playing bar, another extension's
  `PLAYHEAD_*`) reach the engine through a hook on `app:playhead.changed`
  (`DAW_DECK`). Measured:
  - the deck follows the engine's play (3.04 against 3.15 s, read in
    turn);
  - the engine follows the deck's pause (6.031 against 6.0306 s), its
    seek (60 s) and its play.
- **Across machines (not yet).** The transport is the session document's
  `lead/play` presence (session#186), stamped in the shared clock
  (architect#845's `Here::play_now`; at this pin,
  `session::clock::SharedClock::play_state`). At the Architect pin
  (`runtime-compositor` 2f2fb1d1) a process extension has no presence API,
  and FastTrackStudios/session is not in this build: the `session` crate
  here is Studio's own `features/session`. What it takes:
  1. an Architect host service through which an extension publishes and
     reads presence on a document it holds;
  2. FastTrackStudios/session pinned by rev in `daw`;
  3. `lead/play` published from the same transport moves the deck gets;
  4. `session::sync::Follower` driving the engine when another peer leads.
- **The chart's offset** (the chart's bar 1 is the verse; the tracks have
  a 4-bar intro) is the session's structure, not a view's (the session
  thread).

## 5. Measured

| | 1280 px | 2 924 px |
|---|---|---|
| Opening batch | 1.22 MB | 1.29 MB |
| A playing frame, before `paint_layers` | 462 KB | 535 KB |
| A playing frame, now (test) | 694 B | 694 B |
| The view's work a frame, p50 / p99 (test) | 0.23 / 0.86 ms | 0.35 / 0.67 ms |
| Live, the picture's layer changes over 5 s of playback | | 0 |
| Live, the view's work a frame, p50 / p99 | 0.55 / 1.8 ms | 1.6 / 3.8 ms |
| The mixer's work a frame, p50 / p99 (live; a 639 px pane, then 2 924) | 0.55 / 2.5 ms | 0.62 / 1.8 ms |

`extensions/arrangement/tests/frames.rs` plays the golden template at 60
Hz through the layer protocol. The live numbers are
`STUDIO_ARRANGEMENT_STATE` / `STUDIO_MIXER_STATE` in a dev Architect on a
headless GPU session. The setlist window's sizes hold: the arrangement at
2 924 × 759, and the mixer docked under it at 400 px (its pane 360 px
under the dock's header).

## 6. Behaviour gaps, biggest first

1. **The transport across machines** (§4).
2. **The arrangement and the mixer are two processes.** In the window,
   one `Links` ties them together:
   - the visibility manager hides a group in both;
   - an edit in either shows in the other on the same frame;
   - `x` opens the docked mixer;
   - `MIXER_VIEW` reaches the mixer;
   - keys pressed with the mixer focused go to the arrangement's keymap.

   Here each follows the engine on its own. An edit reaches the other
   view through the engine's change stream, a frame or more later. A
   hidden group stays in the mixer, and `MIXER_VIEW` is asked of the
   arrangement's process, which has no mixer.
3. **Rack close-ups.** The mixer pane provides no `closeup::Closeups`, so
   asking for a rack's close-up does nothing.
4. **Out by decision:** Studio's window chrome beyond the views (the
   browser, navigator, editor, chart, lyrics and quantize panels, the
   bottom bar). The transport and Folded are `STUDIO_ARRANGEMENT_*`
   actions, Racks is `STUDIO_MIXER_RACKS`.
5. **Peers' ghosts and the touch grip and handle** are drawn after the
   play cursor in the window and before it in the pane: they differ only
   where they overlap the cursor.

## 7. Building and loading

From the repo root, in the dev shell:

```sh
ARCHITECT=/path/to/architect-desktop FTS_DAW_OPEN="song.rpp" extensions/dev.sh --hide all
```

- `ARCHITECT`: `architect-desktop` at `runtime-compositor` 2f2fb1d1.
- `dev.sh` builds `daw-extension`, `studio-arrangement` and `studio-mixer`
  (the `desktop-dev` profile for the host, as studio-desktop's recipes
  build) and installs each into its package (`extensions/{daw,arrangement,mixer}`).
- It then starts an isolated Architect with only those and the playhead
  on. `EXTENSIONS="daw arrangement"` limits the set.
- To load them into another Architect:
  `architect-desktop dev --extension extensions/daw --extension extensions/arrangement --extension extensions/mixer`.
- Open the views with ⌘K or `EXT_VIEW_OPEN`.
