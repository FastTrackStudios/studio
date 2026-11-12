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
- **The setlist's page is a playlist** (Architect's `[data-playlist]`):
  each set (a heading) a ```` ```studio-setlist ```` fence, the first with
  one big Play (the setlist through), the others "Play this set", then each
  song a row: its number, title, artist · key · tempo · meter, the length
  of its recording, and a link to its chart. A tap on a row plays it there;
  the songs follow each other with no page load, the playing row marked.
  The bar at the foot opens the **now-playing view**: the playing song's
  chart (its `[data-now-playing]`, fetched from its page), following beat by
  beat, with the controls under it (Architect's until Studio's progress bar
  and transport are on the page: #445). Each row is also a follow box whose
  one line is the song's start, so an Architect before playlists (v0.12.2)
  plays a tapped row from the setlist's page as it did.
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
