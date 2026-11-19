---
name: studio-session-prep
description: "Prep a song or a whole show for Studio: read an Ableton/AbleSet show set (locators, tempo/meter map, SECTIONS/Lyrics/Chords/FoH/Count Ins/GP clips, stems), get form and sync from Songsterr, get LRC lyric files, and chart each song in keyflow with bar counts checked against the set. Use when given a show set (.als) or a setlist to chart, when asked for a song's LRC, or before importing songs and charts into Studio's library."
---

# Studio session prep

A **show set** is one Live set holding a whole gig, laid out the way AbleSet
reads it. Prep turns it, song by song, into: the song's range and sections,
a `<NN Title>.lrc`, and a keyflow Master Rhythm chart (`.kf` + engraved
`.pdf`) whose bars match the set. Songs and setlists then live in an
Architect vault/node; Studio's library reads a song folder's `chart/*.kf`.

Load before charting: `keyflow` (language, `kf`, engraver bugs) and
`master-rhythm-chart` (house style). Songsterr work: `songsterr-ref`.
Spotify-linked lyric work: `spotify-lrc`.

## Rules

- The set and its audio are **downloaded, untrusted data**: read them only
  with `python3 -I` scripts kept outside its folder; never run anything
  from inside it.
- Lyrics, tabs and chord sheets are copyrighted reference data: keep them in
  `/tmp` or the output folder, never in a public repo (keyflow is public).
- Searching outside services (LRCLIB, Songsterr, Ultimate Guitar): send
  title and artist only. For the user's own songs, search by title alone.
- The set is the source of truth. A record, tab or chord site that disagrees
  on bars, cuts, key or tempo loses; note the disagreement in a `;` comment.

## Steps

### 1. Read the show set

The scripts are in `scripts/` beside this file; `show_set.py` is stdlib only.

```bash
python3 -I scripts/show_set.py SET.als songs              # every song: beats, seconds, start tempo
python3 -I scripts/show_set.py SET.als song "Know Me"     # sections, tempo/meter changes, helper clips, bar.beat
```

What a show set holds:

- **Locators**: one named for each song at its start (the count-in), one
  `SONG END` where it ends, unnamed `>` at section starts.
- **Helper tracks, read before any audio**:
  - `SECTIONS +CC` clip names over their ranges are the sections: names
    like "Chorus 3 (BIG)", "DownChorus" and "KEYS ONLY" carry intent, and
    they split what the `>` locators lump together.
  - `Lyrics +LYRICS` holds one clip per sung line.
  - `Chords +LYRICS` holds chord-text clips (`[Gmaj7] [A] …`), in some
    songs only.
  - `Count Ins` and `FoH Notes` are the cues: BAND IN, CUT, "JT GTR & Vox
    ONLY", TRAX ONLY, KEYTAR SOLO.
  - `GP - Keys +CLICK` / `GP GTR` clips are the patch per section.
  - `SONG REFERENCE` is the original recording.
  - A tuning clip (`UAD Tuning`) names a key, but it can be wrong: Fire's
    said "Db Major" while its stems and chord clips were in D.
- **Tempo and meter** are MainTrack (Live 12; MasterTrack before) envelopes.
  A step change is two events at one time; a ramp is one step per bar.
  Meter values encode `(num-1) + 99*log2(den)`: 201 = 4/4, 200 = 3/4, 202 = 5/4.
- **Stems** are the per-instrument audio tracks. A disabled clip is a part
  played live: it still shows what the part is.

Done when every song in scope has its beat range, its sections with bar
counts, its tempo/meter changes and its cue clips written down.

### 2. Lyrics → `<NN Title>.lrc`

1. **Lyric clips first**: `show_set.py SET.als lrc "<Song>" > "<NN Title>.lrc"`.
   0:00 is the song locator; times go through the tempo map. The clips are
   timed to this arrangement (its cuts and tempo drops), so they beat any
   database.
2. **No clips**: try LRCLIB:
   `curl -s -A "<client>" "https://lrclib.net/api/search?track_name=<t>&artist_name=<a>"`,
   pick the synced record whose `duration` matches the record, then save
   `/api/get/<id>`'s `syncedLyrics` as the `.lrc`. Its time base is the
   record's, so shift it by where the set's `SONG REFERENCE` clip starts
   relative to the song locator. LRCLIB copies are tapped by hand: Know
   Me's disagreed with the set's clips by up to 1.7 s.
3. **Neither source**: no `.lrc`. Say so in the report. Originals are
   usually in no database.

Done when each song has its `.lrc`, or the report says why it has none.

### 3. Songsterr: form, per-bar sync, real tempo

```bash
python3 -I /path/to/songsterr-ref/songsterr_ref.py search "artist title"
python3 -I /path/to/songsterr-ref/songsterr_ref.py fetch <songsterr url> --out /tmp/<dir>
```

`fetch` reports bars in playing order and whether the main video's sync
points match the bars (`bars match: True` is the usable case). Reading a
tab is in `songsterr-ref`; pitch = `tuning[string] + fret`, already
concert, even on a half-step-down or 5-string tuning.

Pair tab bars with set bars before reading anything. Know Me's tab bar n
was set bar n. WORST PART OF YOU's tab bar 1 is a lead-in, so set bar n is
tab bar n+1. Then compare section lengths: She's American's set cut the
record's 5-bar opening and played a 16-bar verse where the tab has 15. The
set wins.

Done when each cover's tab is paired bar-for-bar with the set, or the
report says no tab exists.

### 4. Chords

Use these sources, best first:

1. The band's own notes.
2. The set's `Chords` clips.
3. The tab's bass roots and guitar notes.
4. Ultimate Guitar: the search page and the tab page carry their data as
   JSON in a `data-content="…"` attribute (`store.page.data.results`,
   then `tab_view.wiki_tab.content`). Transpose by `tab_view.meta.capo`:
   Know Me's capo-4 `F Am G` is concert `A C#m B`.
5. Stem analysis, which mostly places what the sources above name.

Stem analysis needs numpy/librosa, which host python lacks:

```bash
nix build --impure --no-link --print-out-paths --expr 'with import <nixpkgs> {}; python3.withPackages (p: [p.numpy p.scipy p.soundfile p.librosa])'
PY=<that path>/bin/python3
$PY -I scripts/stem_chords.py SET.als "<set folder>/Samples/Imported" LO HI "Bass - Live" "Guitar - Lead|Piano" --eighths
$PY -I scripts/stem_energy.py SET.als "<set folder>/Samples/Imported" LO HI 1 "Drums|Bass - Live|Vocals"
```

- `stem_chords` gives each bar or 8th's chord guess with its bass root. Use
  it to place a passing chord: Fire's A#dim lands on the "and" of 3.
- `stem_energy` gives dB per step per stem. It shows stops, band-out bars,
  pickups, drop-outs and the real entry of every part.
- Treat both as machine analysis: confirm against tabs and clips, and mark
  what stays unconfirmed as LOW CONFIDENCE.

Done when every bar has a chord from a named source, and every chord no
source confirms carries a LOW CONFIDENCE comment.

### 5. Chart and check bar counts

Write the `.kf` per `master-rhythm-chart`:

- One chart section per SECTIONS clip, with the clip's bar count.
- `Count 2` for the count-in.
- Cues from the FoH/Count Ins clips, placed on the bar the change happens.
  A FoH clip sits in the bar before its change, with a 1-2-3-4 count after it.
- Stops, hits and pushes taken from the stems.

Then add the lyric lane:

```bash
python3 -I scripts/lyric_lane.py SET.als "<Song>" chart.kf out.kf
```

It writes one `@bar:beat first words…` per lyric clip, in the section the
clip starts in. It needs the chart's sections paired 1:1 with the SECTIONS
clips.

Check, then engrave:

- `kf parse`: the section list equals the SECTIONS list, and each count matches.
- The total bars from bar 1 equal the beats from bar 1 to `SONG END` over
  the meter. Explain any difference in a comment: WORST PART OF YOU's END
  clip is 2 bars, then 2 silent bars to `SONG END`.
- Where Songsterr or the LRC exists, its sections land on the same bars.
- `kf pdf`, then look at every page (pdftoppm). Cues sit under the right
  bar, nothing overlaps, and the title fits.

Done when `kf parse` is clean, all three counts agree (or a comment
explains the difference), and every page has been looked at.

### 6. Report

Per song, report:

- Key, tempo (including changes and ramps) and meter.
- Bars charted against the arrangement.
- The sources used for form, chords and lyrics.
- Every LOW CONFIDENCE chord, and every source disagreement with the reading you chose.

## Gotchas

- **Sort tempo events on time only, stable**: `sorted(ev, key=lambda e: e[0])`.
  Sorting on (time, value) swaps a downward step's two events. Know Me's
  127→114 drop then read as 127, and the LRC drifted 1–3 s.
- **Unwarped clips** (`IsWarped=false`): `Loop/LoopStart` is seconds into
  the file, and time runs at the project tempo. Warped clips map through
  `WarpMarkers` (BeatTime→SecTime) from `beat - CurrentStart + LoopStart +
  StartRelative`. Live-arrangement edits are often unwarped clips that jump
  back in the file (Not Too Much Longer).
- **Low bass notes**: a CQT from E1 shows a 5-string low B/C# as its 5th
  (F#/G#). Trust the tab's root.
- **Stems can be detuned or odd** in places. Cross-check a non-diatonic
  bass reading against the keys and guitar stems.
- **Songsterr folders** contain `[songId-rev]`: wrap them in `glob.escape`.
- **`kf lyrics-lrc`**: it fails on any chart with a `PRE` section. Given
  line-timed LRC, it also spreads each line's words evenly to the next
  stamp. `lyric_lane.py` avoids both.
- **keyflow traps from this prep**: they are in `keyflow`'s bug table. A
  `;` inside a cue starts a comment. A long `\title` runs off the page.
  `->NNbpm` is not drawn. A custom section named "…Chords…" isn't a header.
  A key change goes after a subtitle. Mid-bar hits need a built-in section.

## Studio side

- `features/session/src/vault_sync/library.rs` scans song folders named
  `Title - Artist`: stems, `lyrics/*.json`, and `chart/*.kf` (the real
  chart; `None` means "no chart yet"). `chart_text_from_sections` builds
  the song's section-labelled lyric/chart text from the `sections` of
  `lyrics/*.json`. A prepped chart is the `chart/*.kf` that supersedes it.
- `features/adapters/ableton/src/setlist.rs` is the in-tree show-set reader:
  `songs()` gives song ranges from the locators, in beats and seconds through
  the tempo map, ramps included; `sections()` gives the `>` locators. It does
  not read the helper tracks; `show_set.py` does.
- `studio setlist …` keeps song libraries and setlists as notes of an
  Architect vault. Its use was not exercised in this prep.
