# studio-views

Studio's views in Architect and the toolbar that opens them: the
`studio-views` extension (studio#409). Compiled into Studio Architect (the
`studio` bundle, `apps/architect`), or run as a process extension the
Architect app installs (`src/main.rs`).

## The Views toolbar

`extension.toml`'s `[[toolbars]]` `views`, at the bottom (`dock:bottom`),
as the Studio iPad app's bottom bar has its views. The owner's six, then
the views Studio has in Architect today:

| Button | Action | Today |
|---|---|---|
| Arrangement | `STUDIO_VIEWS_ARRANGEMENT` | needs Studio's engine: refuses, says why |
| Mixer | `STUDIO_VIEWS_MIXER` | needs Studio's engine: refuses, says why |
| Chart | `STUDIO_VIEWS_CHART` | a panel of the song workspace (studio#408): refuses, says why |
| Chart Editor | `STUDIO_VIEWS_CHART_EDITOR` | a panel of the song workspace: refuses, says why |
| Progress Bar | `STUDIO_VIEWS_PROGRESS` | a panel of the song workspace: refuses, says why |
| Editor | `STUDIO_VIEWS_EDITOR` | needs Studio's engine: refuses, says why |
| Song | `STUDIO_VIEWS_SONG` | opens `studio-setlists/song` |
| Setlists | `STUDIO_VIEWS_SETLISTS` | opens `studio-setlists/setlist` |

- **Each button is an action** (a toggle), so it is also in ⌘K, bindable
  and scriptable. Run, it opens or focuses its view (`EXT_VIEW_OPEN`).
- **The lit button is the extension's state.** On every `workspace.changed`
  (a hook), `STUDIO_VIEWS_FOLLOW` reads the focused view (the host's
  `selection`) and writes each toggle's `checked` state. Pressing a
  toolbar button focuses the toolbar's own pane; that leaves the view in
  front lit.
- **A view that is not there yet keeps its button.** Run, it refuses with
  the reason, as an error and as a notification. It cannot be drawn
  disabled with its reason: an extension cannot disable its own action
  (FastTrackStudios/architect#584).

## What is not here yet

- **The Arrangement, the Mixer and the Editor as views.** They are
  Session's panels (`session-ui`: `mixer_panel::DawPanels`, the
  arrangement widget, `editor_view::EditorPanel`). They draw a
  `StudioSession`, the song open in Studio's engine (`session-engine`),
  through Blitz custom widgets that listen to the window's events.
  Registering them as views of this extension needs:
  - that engine in the Architect process, or reachable from it (Studio
    Architect linking `session-engine`, or attaching to a running
    `studio engine`);
  - the panels taking their session from the extension rather than from
    Session's window;
  - a linked view (the widgets cannot be drawn remotely by a process).
- **The Chart, the Chart Editor and the Progress Bar as views of their
  own.** They are the song workspace's panels (studio#408); each becomes a
  view once that lands.
- **The Arrangement's tools on its left side, the Chart Editor's options at
  its bottom.** Architect places a toolbar in a dock or a pane's header
  only (FastTrackStudios/architect#585), and the Arrangement is not a view
  yet.
- **Icons.** Architect's toolbars draw a fixed set of icon names, so the
  buttons use the nearest of them, not Session's own
  (FastTrackStudios/architect#586).
