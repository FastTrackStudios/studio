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
- **The setlist's page** lists each set (a heading) as a
  ```` ```studio-setlist ```` fence: "Play this set", then each song a row
  that plays its recording (marked while it plays) and links to its chart.
- **Playing through** is the link's queue: every song's recording in
  running order; when one ends the next starts (on its own page, with
  Follow on).

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
