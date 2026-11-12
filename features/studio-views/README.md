# studio-views

Studio's views in Architect and the toolbar that opens them: the
`studio-views` extension (studio#409). Compiled into Studio Architect (the
`studio` bundle, `apps/architect`), or run as a process extension the
Architect app installs (`src/main.rs`).

## The toolbars

Three, where the owner placed them (studio#409). Each is a toolbar of
Architect's own (`extension.toml`'s `[[toolbars]]`); a person edits or
overrides any of them with `toolbars/<name>.toml` (id `studio-views/views`,
`studio-views/arrangement`, `studio-views/chart-editor`), as any
extension's.

| Toolbar | The owner's place | Declared at | Waits on |
|---|---|---|---|
| `views` | the bottom of the window | `dock:bottom` (the main area's bottom dock) | a strip across the window's bottom, FastTrackStudios/architect#587 |
| `arrangement` | the left side of the Arrangement view | `view:studio-views/arrangement` (its panes' header) | a view toolbar on its left edge, FastTrackStudios/architect#585; the Arrangement view, #433 |
| `chart-editor` | the bottom of the Chart Editor | `view:studio-views/chart-editor` (its panes' header) | a view toolbar on its bottom edge, FastTrackStudios/architect#585; the Chart Editor view, #436 |

### Views (the bottom)

The owner's six and no others. The four the Studio iPad app's bottom bar
has keep its order and words (`session_ui::shell::BottomBar`: Perform,
Chart, Lyrics, Arrange, Mixer, FX, Editor); the song's two other panels
follow the Chart:

| Button | Action | Today | Becomes a view in |
|---|---|---|---|
| Chart | `STUDIO_VIEWS_CHART` | refuses: a song workspace panel (#408) | #435 |
| Chart Editor | `STUDIO_VIEWS_CHART_EDITOR` | refuses: a song workspace panel (#408) | #436 |
| Progress | `STUDIO_VIEWS_PROGRESS` | refuses: a song workspace panel (#408) | #437 |
| Arrange (with its TCP) | `STUDIO_VIEWS_ARRANGEMENT` | refuses: needs Studio's engine | #433 |
| Mixer | `STUDIO_VIEWS_MIXER` | refuses: needs Studio's engine | #434 |
| Editor (MIDI) | `STUDIO_VIEWS_EDITOR` | refuses: needs Studio's engine | #438 |

- **Each button is an action** (a toggle), so it is also in ⌘K, bindable
  and scriptable. Run, it opens or focuses its view (`EXT_VIEW_OPEN`) once
  that view is registered (`Opens::View`).
- **The lit button is the extension's state.** On every `workspace.changed`
  (a hook), `STUDIO_VIEWS_FOLLOW` reads the focused view (the host's
  `selection`) and writes each toggle's `checked` state. Pressing a
  toolbar button focuses the toolbar's own pane; that leaves the view in
  front lit.
- **A view that is not there yet keeps its button.** Run, it refuses with
  the reason and its issue, as an error and as a notification. It cannot
  be drawn disabled with its reason (FastTrackStudios/architect#584).
- **Icons:** the nearest names Architect's toolbars draw, not the iPad's
  own Lucide icons (`file-music`, `chart-no-axes-gantt`,
  `sliders-vertical`, `audio-waveform`): FastTrackStudios/architect#586.
- The lyrics display stays part of the song workspace (#408), with no view
  or button of its own, unless the owner says otherwise.

### Arrangement tools (its left side)

What Session's arrangement rail carries (`session_ui::rails::profile`,
`Surface::Arrange`):

- **The eight mix phases,** a group: `STUDIO_VIEWS_PHASE_<PHASE>`, presets of
  `STUDIO_VIEWS_PHASE {phase}`.
- **Takes:** `STUDIO_VIEWS_FOLDED_TAKES`.

They refuse until the Arrangement is a view on the engine (#433). The
rail's scenes are the current mode's for the instrument in front, not a
fixed list: they become a widget of the Arrangement view's (`kind =
"widget"`) once it is one. The mode selector (`rails::main_toolbar`) is not
on the rail.

### Chart options (the Chart Editor's bottom)

Keyflow's own transposer (`keyflow::transpose_source`, `src/chart.rs`);
Studio writes none.

- **Transpose:** `−1` / `+1` (`STUDIO_VIEWS_CHART_TRANSPOSE_DOWN|UP`, presets
  of `STUDIO_VIEWS_CHART_TRANSPOSE {by, source?}`).
- **Chords:** Letters, Nashville, Roman (`STUDIO_VIEWS_CHART_NOTATION_*`,
  presets of `STUDIO_VIEWS_CHART_NOTATION {notation, source?}`).

Given `source` (from ⌘K with arguments, a script, an agent), they answer
the rewritten text today. From the toolbar (no `source`) they need the
Chart Editor's chart, once it is a view (#436). Keyflow rewrites the key
on the header line only when it leads the line
(FastTrackStudios/keyflow#63). Capo is Keyflow's too (`ChartView::capo`),
but it is a display setting of the Chart, not an edit of the source; it
waits for the Chart view's display state (#435).

## What each view needs

Each issue says what it needs, what works at the Reference layer (a
recording and the chart's clock) and what needs the Full engine:

- **Arrangement, Mixer, Editor (#433, #434, #438).** Session's panels
  (`session-ui`) draw a `StudioSession` through Blitz custom widgets
  driven by the window's events. As views they need:
  - to be linked views (not drawable by a process copy);
  - their session supplied by the extension;
  - `session-ui` linked into Studio Architect.
- **Chart, Chart Editor, Progress Bar (#435, #436, #437).** These need
  #408's panels, then a song state the open views share (one clock, one
  engraving).
