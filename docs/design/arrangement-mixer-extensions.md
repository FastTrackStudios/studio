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
- **One session (done, studio#522).** The song's session document is
  `<song stem>.fts-session` beside the song (Studio's `Song.session` is
  its project and is never opened as one): an Architect document file of
  kind `fts.session` (FastTrackStudios/session `SessionFile`), opened or
  made, never imported. The engine writes its timing into it on every
  open (tempo changes with their meters, markers, sections as regions,
  `bar_one`: a `SONGSTART` marker, else the first section after an intro
  or count-in, else `FTS_DAW_BAR_ONE`), provides `fts.session` for it
  (`DAW_READ/WRITE/STRUCTURE`), and leads its transport as presence
  (`session::lead::Leader` on `FTS_DAW_LEAD_PORT`, 4142: `lead/play` in the
  host's clock, on every move the deck gets). `DAW_STATE.session = {path,
  id, url}`; a launcher then runs `EXT_SERVICE_USE {"service":
  "fts.session", "provider": "daw"}`, `KEYFLOW_SESSION_FOLLOW {url, id,
  chart_path}`, the chart on `session:<id>`, `IGNITION_SESSION_SONG {…,
  "tempo": false}` and `IGNITION_SESSION_ROOM {url, doc, song, lane}`.
  Proved in one 5120 px window: play, seek to 66 s and 93 s, and the
  arrangement playhead, the mixer meters, the chart's playhead (bar 1 the
  Verse), the room and the cue list land on the same bar
  (`pr-media/arrangement-mixer`, `one-transport/`).
- **Bar 1 of the recital songs** is a launcher setting today
  (`FTS_DAW_BAR_ONE=5` for Drive To LA): the project marks it with neither
  a `SONGSTART` marker nor named regions (studio#524).

## 5. Measured

**The arrangement** (`extensions/arrangement/tests/frames.rs`, the golden
template played at 60 Hz through the layer protocol; live: Drive To LA in
one 5120 px window with the chart, room and cue list, the machine at a
load of ~148 from other builds):

| | 1280 px | 2 924 px |
|---|---|---|
| Opening batch | 1.22 MB | 1.29 MB |
| A playing frame, before `paint_layers` | 462 KB | 535 KB |
| A playing frame, now (test) | 694 B | 694 B |
| The view's work a frame, p50 / p99 (test) | 0.33 / 1.1 ms | 0.65 / 2.9 ms |
| Live, the picture's changes over 8 s of playback | 0 | |
| Live, the view's work a frame, p50 / p99 | 2.1 / 65 ms (loaded machine) | |

**The mixer** (`extensions/mixer/tests/parity.rs`, 30 playing frames
through the protocol, levels from the simulated signal):

| | mixer 1280×400 | strip 120×640 |
|---|---|---|
| A playing frame (tracks with values) | 0 B | 0 B |
| Stopped: pixels past a step of 24 / largest step | 0 / 3 | 0 / 7 |
| Playing: pixels past 24 / largest step | 260 of 512 000 / 63 | 18 of 76 800 / 52 |
| Live: the picture's changes over 8 s of playback | 0 | |
| Live: work a frame p50 / p99 | 3.8 / 80 ms (loaded machine; 2.2 / 23 alone) | |

The meters are the engine's values (`engine.meter.<track>.{pl,pr,hl,hr,pk}`,
written by `daw` into its shared table): the pane draws them once as
parts the host moves (`overlay::Apart`). What is left playing is where a
cover's soft edge meets the level and the order two transparent paints
blend in a cap's pane. The table carries 146 tracks (`VALUE_TRACKS`:
five meter values and two buttons a track, 1022 of its 1024 slots); a
strip past them keeps its meter in the picture (the golden template's
208 tracks: about 71 KB a playing frame for those strips).

## 6. Behaviour gaps, biggest first

1. **The arrangement and the mixer are two processes.** In the window, one
   `Links` ties them: an edit in either shows in the other on the same
   frame, `x` opens the docked mixer, `MIXER_VIEW` reaches the mixer, and
   keys pressed with the mixer focused go to the arrangement's keymap.
   Here each follows the engine, and the engine routes the mixer's window
   actions to the mixer:
   - **Window actions by name.** `studio.mixer` claims `MIXER_VIEW`,
     `SESSION_MIXER_FX` and `SESSION_TOGGLE_MIXER` as the window
     `studio.mixer` (`window_queue::claim_as`, the offer's `window`); the
     engine asks them of it alone, and it takes no other ask. The
     arrangement's process hands them over (`window_queue::hand_over`), so
     its `x` runs the toggle on the engine, which asks the mixer. The
     toggle closes the mixer's pane and opens it again where it was (a
     dock, a float, or split under the pane that was above it). After the
     layout changes, the arrangement keeps being the active pane but gets
     no keys until it is clicked (architect#889).
   - **An edit in the arrangement** reaches the mixer as the engine's
     track event. A process view records on its own 16.7 ms timer, so the
     mixer waited for its next tick; it now draws as the event lands
     (`engine::on_track_event` wakes its pane). Selecting a track in the
     arrangement, 60 fps recording, frames until the mixer shows it:

     | | mean | same frame |
     |---|---|---|
     | before | 1.50 | 5 of 26 |
     | after | 0.92 | 9 of 25 |

     What is left is the round trip: the arrangement draws its own
     prediction on the click's frame, and the mixer the engine's answer
     (apply, then the event), which lands after the next vsync about two
     times in three.
   - **The same frame, by design.** Two processes meet only where the
     host composes. One process for both views would not do it: each
     view sends its own frames, and a vsync falls between them. So:
     - *State that is a value* is drawn from one value both panes read
       at compose. The `daw` extension is the engine's process: it writes
       each track's mute and solo into its shared table as the engine's
       track events say (`engine.track.<i>.{mute,solo}`, 1 lit), before
       any other process hears of them. Every pane draws the button
       unlit and lays the lit one over it as a part that value shows
       (`overlay::Apart::buttons`, `overlay::panel_buttons`), so every
       pane lights it in the same compose, whichever was clicked. The
       clicked pane's own button lights at the engine's answer (one call,
       about a millisecond), not at its prediction. Measured live (a
       mute clicked in the mixer 16 times, 60 fps, the strip pane showing
       the same track): the strip lit in the same frame 16 of 16 times
       (on #519's binaries, 12 of 16, up to 3 frames apart). The parity
       tests compare both panes, with some tracks muted and soloed, with
       the window's pictures (a step of 6 at most, the lit button's soft
       edge over the unlit one).
     - *A change of layout* (a selected strip opened wide, a row's tint
       in a recorded panel) is no value: it needs the host to hold a
       group of panes' frames until each has drawn the same edit (a frame
       barrier, Architect's to build). Selection is the one that shows.
     - Arm, phase, volume and pan can join mute and solo the same way;
       each costs a slot a track, and the table's 1024 are spent, so the
       next step is Architect's `MAX_SLOTS`.
   - The arrangement's track panel starts compact beside the mixer, as
     docked: its rows show no mute or solo until expanded, and only a
     touch swipe or the window's toolbar switch expands it, neither of
     which a pane has.
   - **A key in the mixer pane** runs Studio's keymap, as the window's
     arrangement reads the keys pressed in its mixer
     (`HostedMixer::key`, `window::key_alone`): Space plays, `x` hides
     the mixer, the arrangement's own actions are asked of it through the
     engine. Each runs behind the window, so none holds a frame.
   The visibility manager is no longer one of them: a mixer on its own
   keeps every track and plans its strips again when one is shown or
   hidden (`SESSION_SHOW_GROUP` takes a group out of both panes).
2. **Rack close-ups.** A close-up is Studio's rack widget as a native
   Blitz widget (`CustomWidgetAttr`), its Escape a winit window event:
   neither exists in a process view's document. It needs the rack widget
   hosted in a pane of its own, as the mixer is.
3. **Meters and buttons past 146 tracks** stay in the panes' pictures (a
   shared value table's 1024 slots, seven values a track).
4. **Out by decision:** Studio's window chrome beyond the views (the
   browser, navigator, editor, chart, lyrics and quantize panels, the
   bottom bar). The transport and Folded are `STUDIO_ARRANGEMENT_*`
   actions, Racks is `STUDIO_MIXER_RACKS`.
5. **Peers' ghosts and the touch grip and handle** are drawn after the
   play cursor in the window and before it in the pane: they differ only
   where they overlap the cursor.
6. **The mixer is as wide as its strips.** At 5120 px it fills about half
   its dock: `studio-desktop` does the same (fixed-width strips from the
   left).

## 7. Building and loading

From the repo root, in the dev shell:

```sh
ARCHITECT=/path/to/architect-desktop FTS_DAW_OPEN="song.rpp" extensions/dev.sh --hide all
```

- `ARCHITECT`: `architect-desktop` at Architect `main` `e976a706` or later (`5016300a` has `EXT_SERVICE_USE`).
- `dev.sh` builds `daw-extension`, `studio-arrangement` and `studio-mixer`
  (the `desktop-dev` profile for the host, as studio-desktop's recipes
  build) and installs each into its package (`extensions/{daw,arrangement,mixer}`).
- It then starts an isolated Architect with only those and the playhead
  on. `EXTENSIONS="daw arrangement"` limits the set.
- To load them into another Architect:
  `architect-desktop dev --extension extensions/daw --extension extensions/arrangement --extension extensions/mixer`.
- Open the views with ⌘K or `EXT_VIEW_OPEN`.
