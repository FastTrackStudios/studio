# setlists

Song libraries and setlists as notes of an Architect vault, the
`studio-setlists` extension, and a setlist's share page. The model is in
`src/lib.rs`; the extension in `src/ext.rs`.

## A setlist

`Setlists/<Set>.md`, `type: setlist`. Its `songs` property is the running
order (a wikilink per song, an arrangement as a heading: `…/Song#Acoustic`).
A setlist may have **sets**: named runs of its songs ("Presidents Songs",
then "Tree Lighting Concert Set"), kept as `sets: [{name, songs}]` beside
`songs`. With sets, `sets` is the running order and `songs` is written
beside it for whoever reads only that; a setlist without sets is valid as
it always was. `STUDIO_SETLISTS_CREATE`, `_ADD` and `_MOVE` take an
optional `section` (the set), and the navigator shows the sets under
their setlist.

## A song's workspace (the song view)

A song note opens (`SongView`, `src/ext.rs`) as Session's Overview
(`src/workspace.rs` the model, `src/workspace_view.rs` the view):

```
┌──────────────────────────────────────────────┐
│ progress bar (sections, playhead, seek)      │
├──────────────────────┬───────────────────────┤
│ chart editor (.kf)   │                       │
├──────────────────────┤ chart (engraved,      │
│ lyrics (timed lines) │ the full height)      │
└──────────────────────┴───────────────────────┘
```

- **Session's panels**, not second ones: the progress bar is
  `session::ui::components::progress::SongProgressBar` (what Session's
  `ProgressBar` draws), the lyrics `session::ui::components::lyrics`
  (Session's lyrics panel, in its Performer view; moved there from
  `session-ui` so both draw it), the editor
  `session::ui::components::chart_text` (Session's chart editor's pane).
  The chart is the share page's engraving (`follow::follow_pages`), a page
  at a time: Session's chart panel is a Blitz widget painting into the
  window's own scene from the window's events, which a process
  extension's view (drawn remotely) cannot carry.
- **One playhead** (`workspace::Clock`): seconds of the song's recording.
  The bars are timed on the chart's own clock, the lyrics are the LRC's,
  the progress bar's sections are the chart's sections as played. The bar
  and the lyric line at the playhead are marked; a click on a section, a
  bar or a line seeks. This is the Reference layer (a recording and the
  chart's clock): the clock runs on its own, silent, because Architect's
  app plays no media for an extension's view (`architect.playhead` is a
  share page's). Studio's engine, the Full layer, is what adds the sound
  and drives the clock from its transport.
- **The editor**: each pause in typing engraves the chart again on a
  thread of its own (the parts, the pages and a lyric lane with it), and
  saves the `.kf` back to the song through the host's `write_file` (the
  vault's actions write notes only). A vault under the home folder is in
  reach (`capabilities.folders`); one elsewhere asks for its folder once a
  run ("Allow saving").
- **The `.kf` is the source of truth** (`workspace::ChartAccess`). When it
  cannot be read, the editor shows the copy in the song's note (the
  ```` ```kf ```` fence, written at import and never updated on save)
  **read-only**, saying why, with "Read the .kf again"; that text is never
  written over the `.kf`. A song naming no `.kf` edits its note's copy and
  never saves.
- **Every save reads the `.kf` again first** (`workspace::Saving`): written
  only while it still holds what the workspace last read or wrote there,
  never a chart with no bars over one with bars without "Save anyway". The
  file as it was opened is backed up once per session before the first
  write, in the extension's own folder (`backups/<vault path>.<opened>.kf`,
  the host's `write_file` with a relative path), not in the vault, so a
  backup is never synced as a song's file. The write itself truncates then
  writes (FastTrackStudios/architect#590).
- **A shared song**: its share page draws the chart its note carries, which
  a save does not update (FastTrackStudios/architect#492); the workspace
  says so while the song is in a shared setlist.
- **Lyrics never wait on the chart**: a song with an LRC and no chart (or a
  chart that does not engrave) shows its lyrics, the chart pane saying why
  it is empty.
- **Colours** are the app's theme tokens (`var(--sheet)`, `--ink`, `--rule`,
  `--brand`, … over `architect_theme::use_resolved`), the editor's on its
  pane only. The lyrics stay Session's stage display, dark in any theme.
- **On a phone** (narrower than 720 px): the progress, the chart and the
  lyrics stacked, the editor behind "Edit chart".
- **Linked or a process**: the same view both ways. Compiled into Studio
  Architect it is drawn in the app's own document; as an installed
  process its view is drawn remotely. Where a view cannot measure itself,
  it stays in its wide form, the chart fits the pane's width and the
  lyrics take a usual pane's size.

`cargo run -p setlists --features extension --example workspace_shot --
/tmp/ws.png [wide|narrow] [seconds]` draws it headless to a PNG.

## The share page (layer 0 of a share link: the page, no engine)

`STUDIO_SETLISTS_SHARE` shares the setlist's note and its songs'. The node
builds them as an Architect site, and Studio draws its own blocks into it
(`src/share.rs`, feature `share`, `setlists::share::blocks()`):

- **A song's chart** is a ```` ```kf media=<recording> ```` fence of its
  Keyflow source on its page, drawn as the engraving the PDF prints, every
  page stacked in one SVG, a transparent `kf-bar` rect per bar played with
  its time (`data-at`, seconds, on the chart's own clock: `\downbeat`, its
  tempo, tempo changes and meters; Keyflow's `bar_times`), tied to the
  song's recording (`data-media`). It follows **beat by beat** with
  Session's cursor (Keyflow's `CursorConfig::playback()`): a wash over the
  bar and a line at the beat, where Keyflow's `ChartCursor` puts the
  playhead, a `kf-beat` rect per beat played timed on the same clock (the
  bar's time split by its beats; a chord, rest or hold over several beats
  gets a mark at each of them). Architect's page player
  marks the bar and the beat at the playhead, keeps them in view and seeks
  on a click on a bar. The text is drawn as outlines: a block cannot load
  the engraver's fonts on a share page. A glyph drawn more than once is
  defined once and placed with `<use>`, under ids of the chart's own (a
  song page shows its arrangements' charts too) (This Christmas: 11.1 MB of page to
  2.4 MB, 300 KB gzipped).
  The PNG pages and the PDF stay as downloads.
- **Its lyrics: one source a song.** A song with an LRC shows the LRC
  (`![[Song.lrc]]`, drawn by Architect as timed lines). A song with no LRC
  whose chart has a timed lyric lane (`[lyrics lrc]`) shows the LRC made
  from the lane on the chart's clock (a ```` ```lrc ```` fence). Never both.
- **The setlist's page is a playlist** (Architect's `[data-playlist]`),
  laid out as a streaming app's: ONE ```` ```studio-setlist ```` fence,
  each set's name a line of it (`## Presidents Songs`) drawn as a quiet
  label in the list, the numbers running on through the sets. Each song is
  a row: its number, a square tile (its initials on a colour from its
  title; the song's art over it when the link has some), its title over
  its artist · key · tempo · meter, the length of its recording, and a
  small chart icon to its page. No play buttons: a tap on a row plays it
  there, the songs follow each other with no page load, the playing row
  marked. A song with no recording stays in the list, quieter, and opens
  its page. A page written before (a fence a set, each under a `##`
  heading) still draws: Architect's player joins the fences into one list.
  The card at the foot (art, song, Click, Loop, play) opens the
  **now-playing view**: the playing song's chart (its `[data-now-playing]`,
  fetched from its page), following beat by beat, with Architect's
  progress bar over it and its controls under it.
- **A song's art** is its note's `art:` property, a link to an image
  beside it (`art: "[[Libraries/Christmas/Sleigh Ride/cover.jpg|cover.jpg]]"`),
  kept when the song is imported again (`Song::art`). Architect publishes
  it as one of the note's files (only when the note is in the link and the
  file in its layer) and draws it on the setlist's rows and the card.
- **The song's sections as played** ride on the now-playing view as
  `data-sections`: a JSON list `[{"name", "start", "end", "color"}]`,
  seconds on the chart's clock (Keyflow's `chart_section_timeline`, a
  section played twice listed twice, in playing order) and the section
  mark's colour (Keyflow's `get_section_theme`, the colour the chart
  prints; `follow::played_sections`). The page player hands them to the
  views that draw the song.
- **Studio's island** (`apps/architect/island`, `just island`;
  docs/architect-distribution.md): on a node that ships it, the
  now-playing view's bar is Session's own progress bar (its sections from
  `data-sections`, a tap on one seeks there, a song without sections one
  fill the whole length) and its controls Session's transport (previous,
  play / pause, next; no record, no loop), from `features/ui/transport-ui`,
  the crate Session draws them from. It runs in the page's extension
  sandbox and may only ask the page to seek, play, pause or skip.
  Architect's bar and controls show until its first frame, and stay
  where it cannot run.
- **A song's length** is its recording's (`length` on its note, seconds,
  read from the file's header at import and re-engrave: MP4, WAV, AIFF,
  FLAC, Ogg Vorbis/Opus, MP3; `src/media.rs`), and a set's line carries it
  and the artist (`… | <recording> | <artist> | <length>`, a `|` in a field
  written `\|`). A setlist page written before shows its rows without them
  until it is written again, and nothing writes it again when a song
  changes yet (#451, beside FastTrackStudios/architect#492).
- **Playing through** is the playlist on its page, and elsewhere the link's
  queue: every song's recording in running order; when one ends the next
  starts (on its own page, with Follow on).

## What Architect does not do yet

- The setlist's page is written when it is made; an edit through the app
  (`_ADD`, `_MOVE`, `_REMOVE`) updates its properties, not its page, until
  the host can rewrite a note's body (FastTrackStudios/architect#492).
- The hand-over between songs is a page load, not gapless, and a browser
  may ask for a tap to go on (FastTrackStudios/architect#546).
- A node draws Studio's blocks only with `setlists::share::blocks()`
  linked in (`Distribution::block`); installed as a process extension, the
  fences show as text (FastTrackStudios/architect#547). Studio Architect
  (`apps/architect`, docs/architect-distribution.md) is the node with them
  linked in.
