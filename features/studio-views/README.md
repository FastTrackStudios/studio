# studio-views

Studio's views in Architect and the toolbar that opens them: the
`studio-views` extension (studio#409). Compiled into Studio Architect (the
`studio` bundle, `apps/architect`), or run as a process extension the
Architect app installs (`src/main.rs`).

## The Views toolbar

`extension.toml`'s `[[toolbars]]` `views`, at the bottom (`dock:bottom`),
as the Studio iPad app's bottom bar has its views. The buttons are the
owner's six (studio#409), and no others:

| Button | Action | Today | Becomes a view in |
|---|---|---|---|
| Arrangement (with its TCP) | `STUDIO_VIEWS_ARRANGEMENT` | refuses: needs Studio's engine | #433 |
| Mixer | `STUDIO_VIEWS_MIXER` | refuses: needs Studio's engine | #434 |
| Chart | `STUDIO_VIEWS_CHART` | refuses: a song workspace panel (#408) | #435 |
| Chart Editor | `STUDIO_VIEWS_CHART_EDITOR` | refuses: a song workspace panel (#408) | #436 |
| Progress Bar | `STUDIO_VIEWS_PROGRESS` | refuses: a song workspace panel (#408) | #437 |
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
  be drawn disabled with its reason: an extension cannot disable its own
  action (FastTrackStudios/architect#584).
- The lyrics display stays part of the song workspace (#408), with no view
  or button of its own, unless the owner says otherwise.

## What each view needs

Each issue says what it needs, what works at the Reference layer (a
recording and the chart's clock) and what needs the Full engine:

- **Arrangement, Mixer, Editor (#433, #434, #438).** Session's panels
  (`session-ui`) draw a `StudioSession` through Blitz custom widgets
  driven by the window's events. As views they need:
  - to be linked views (not drawable by a process copy);
  - their session supplied by the extension;
  - `session-ui` linked into Studio Architect.

  At the Reference layer the Arrangement can show a read-only song (its
  recording as one track, the chart's sections, the playhead), and the
  Editor the chart's notes. The Mixer only makes sense with the engine.
- **Chart, Chart Editor, Progress Bar (#435, #436, #437).** These need
  #408's panels, then a song state the open views share (one clock, one
  engraving). Everything they do works at the Reference layer.
- **Toolbars inside a view** (the Arrangement's tools on its left, the
  Chart Editor's options at its bottom): FastTrackStudios/architect#585.
- **Icons.** Architect's toolbars draw a fixed set of icon names, so the
  buttons use the nearest of them, not Session's own
  (FastTrackStudios/architect#586).
