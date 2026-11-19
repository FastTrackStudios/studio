---
name: song-tempo-map
description: Tempo-map a recording from its separated drum stems — beats from the kit pieces, snapped to hits, checked by snare phase, bar 1 found (by a score's bass line or the owner's ear), one tempo point per bar, in any meter. Use when asked to tempo map a song, fix a drifting or off-by-a-beat map, find bar 1, measure or compare maps, or as step 2 of `song-prep`.
---

# Tempo map from the drum stems

Input: a `<Song> Reference/` folder from `song-stems`. Output: the song's
analysis (every beat's time, the kit's hits, bar 1) and its map, one tempo
point per bar, applied to the song's session.

**Use the extension, `studio.tempo-map`** (`extensions/tempo-map`; its
algorithms are the `tempo-map` crate, `features/tempo-map`). It replaces
`dpbeats.py`, `dpsnap.py`, `snare_check.py` and the tempo fit in
`mkproj.py`, gives the same beats as those scripts, and does what they
could not: meters other than 4/4, meter changes, and the repair of the
beats the tracker adds or drops. The scripts stay in
`song-prep/scripts/` as the fallback (below) until a song says the
extension is worse; `bass_match.py` (bar 1 from a score) and the REAPER
project build in `mkproj.py` have no replacement yet.

## Running it

Every step is an action: from a shell, from MCP, or from a Luau script.
Each names its song (`song`: its number in the open set or its title;
`name`: the name the analysis is kept under; neither: the song Studio is
on), so a script maps a whole set without going to any song.

```bash
studio actions --app "$sock" run STUDIO_TEMPO_MAP_ANALYSE \
  --json '{"stems":"<folder>","song":"Last Christmas","bpm":107}'
```

```lua
-- Every song of the set whose stems are known, in one run (SCRIPT_RUN).
for _, s in ipairs(state.query("DAW_SONGS", {}).songs) do
  actions.run("STUDIO_TEMPO_MAP_ANALYSE", { stems = stems[s.title], song = s.number, bpm = bpm[s.title] })
end
```

Without Studio running, the same library from a shell:
`cargo run -p tempo-map --features cli --release -- analyse <folder> --bpm 107 -o song.json`
(`candidates`, `map`, `evaluate`, `compare`, `hits`, `bench`).

Analyses are kept as JSON under `$XDG_DATA_HOME/fts/tempo-map/` (or
`STUDIO_TEMPO_MAP_DIR`); `STUDIO_TEMPO_MAP_STATE` lists them.

## 1. Beats

`STUDIO_TEMPO_MAP_ANALYSE {stems, bpm, song|name}`.

- Sums the onset envelopes of every audible drum stem (Kick, Snare, Hi-Hat,
  Toms at full weight; Cymbals and Drums Other 0.6; Bass 0.5; a stem under
  −60 dB is left out), runs one dynamic-programming beat track over the
  whole song, and snaps each beat to the strongest hit of the merged kit
  (a kit piece within 120 ms, else a cymbal within 60 ms), refusing a snap
  that makes neighbouring beats differ by more than about 25%.
- `bpm` is the setlist note's tempo. Get the half/double right: a wrong
  octave shows as the snare on every beat or every fourth.
- `from: T` drops junk beats before the music starts. It is needed far less
  than with the scripts: beats before the first drum hit keep their spacing
  now. (The scripts stacked them on that hit and spread them into bar 1 as
  extra beats; that was the hand fix on four of the five Christmas songs.)
- `meter: "3/4"` when you know it; left out, the accents decide between
  three and four (`meter_guess` in the answer says how clearly).

## 2. Check before believing it

The answer's `summary` has what `snare_check.py` printed, and
`STUDIO_TEMPO_MAP_EVALUATE {bars: true}` has it per bar.

- **Phase line:** one character per bar, `+` for the backbeat where the
  meter puts it (2 and 4 in four; 2 and 3 in three; 2 in 6/8), `X` turned,
  `.` no snare, `?` unclear. A run of `X` after `+` means a beat was added
  or dropped at the change.
- **Repair:** the analysis already re-spaced the spans it could prove
  miscounted (`repair.fixed`: a run of intervals off the tempo, between
  steady beats at one tempo, holding one beat more or fewer than its length
  says; kept only when the backbeat then reads the same both sides).
  `repair.left` is what it would not touch: a change of phase with the
  right number of beats is the music's (an odd bar: set a meter change), a
  held-back intro and a ritard are tempo changes, not miscounts.
- **By hand:** `STUDIO_TEMPO_MAP_EDIT {respace: {from, to, beats}}` puts
  `beats` even intervals between two good beats (indices), as the
  `np.linspace` fix did, and snaps again.
- **Irregular intervals** (`summary.irregular`): any beat more than 4% off
  the local median; look at each. `flagged` in the evaluation groups the
  bars to look at (`backbeat turned`, `no hits`, `off the hits`,
  `tempo jump`) and every bar has a `confidence`.
- **Endings with a ritard:** the tracker holds its tempo through one and
  miscounts it. Place those beats by hand from the hits
  (`STUDIO_TEMPO_MAP_EDIT {place: {from, times}}`) and take them as given
  (`{raw_from_bar: N}`).
- Songs with no snare on the record (brushes): the backbeat is read from
  the kit piece whose hits alternate most; `summary.backbeat` says which
  stem that was. Say it in the report.

## 3. Bar 1

`bar1` is the beat that is the downbeat of song bar 1. Beats before it are
the pickup and count-in.

- **With a score (MusicXML):** `python3 -I mx.py score.xml > mx.json`, then
  `bass_match.py <beats.json> <mx.json> <bass.wav>` (still a script; it is
  written for one score's part name and repeats, adapt both). A clear
  winner (0.86 against 0.67 for the runner-up on Last Christmas) settles
  bar 1 and shows if the record is in another key than the score.
- **Without a score:** `STUDIO_TEMPO_MAP_CANDIDATES` lists the beats it
  could be, best first, each with what speaks for it and the stems heard
  there. On the test set the best candidate was the accepted bar 1 in 8 of
  10 runs: **it is still a guess, and it has been exactly one beat late
  twice.** Tell the owner which beat (time, and what is heard there) and
  have it confirmed by ear **before** sections and markers are placed.
- A pickup is not bar 1. A two-beat vocal pickup means `bar1` is the beat
  after it.
- `STUDIO_TEMPO_MAP_SET_BAR1 {candidate: 1}` (or `beat`, or `time`).

## 4. Meters

- `STUDIO_TEMPO_MAP_EDIT {meter: "3/4"}` sets the song's meter;
  `{changes: [{bar: 17, meter: "2/4"}, {bar: 18, meter: "4/4"}]}` the bars
  where it changes (song bars). Compound meters are tracked in their beat:
  give `bpm` as the dotted quarter's for 6/8 (two beats a bar).
- Meter changes are not found by themselves: take them from the chart.
  This Christmas (Donny Hathaway) alternates 4/4 and 3/4 in every hook:
  `changes` for bars 2, 4, 24, 44, 64, 84, 86, 88 (3/4, each followed by
  4/4 again) puts the kick on 64 of the chart's 90 downbeats, against 46
  read as plain 4/4. That count (a kick within 45 ms of the bar line) is
  the check that a layout fits a recording.
- A counted-in recording (This Christmas: two clicks) fools the bar-1
  guess: the best candidate was the first click, bar 1 the third beat.
- A tempo point's bpm is always in quarter notes, whatever the meter, and a
  point carries the time signature where it is set or changes.
- A score's odd bar may not be on the record (Rockin' Around the Christmas
  Tree bar 52 is written 3/4; the record plays a full 4/4 bar). Trust the
  bass match and the phase line.

## 5. The map

`STUDIO_TEMPO_MAP_MAP`: one tempo point per bar. Each downbeat is a
weighted line fit through the beats four either side of it, so a fill or a
single late hit cannot drag a bar line; the audio before bar 1 is one
count-in bar. `grid_error_ms` in the analysis is the script's
`beats vs per-bar grid` (medians of 1–8 ms are what good maps gave).
Report that number.

- `STUDIO_TEMPO_MAP_APPLY {song}` writes it into the song's session
  (the engine's `TEMPO_MAP_SET`): the tempo points with their time
  signatures, and SONGSTART moved to bar 1, so the session counts its bars
  from there. `offset` is where the song's audio starts in the session,
  seconds (0 when its items sit at the session's start; a project built by
  `mkproj.py` has them later: the items' position). The map's
  `count_in_offset` is the offset that makes the count-in one whole bar at
  bar 1's tempo; with the audio at 0 and bar 1 less than a bar in, the
  count-in is one short bar (1/4 or 1/8) so the grid still lands on bar 1.
- `STUDIO_TEMPO_MAP_HITS {song, write: true}` puts each kit piece's hits on
  its track as track markers (`hit`; `tracks: {"Kick": "DRUMS/Kick"}` says
  which track, `offset` as for APPLY, `min_strength: 0.3` leaves the ghost
  notes out); written again it replaces them. Read without `write`, each
  hit comes with its bar and beat (1.0 is on the downbeat, 2.5 the and of
  two). Charts and lighting read them through the
  `studio.tempo-map` service (`hits`, `map`).
- `STUDIO_TEMPO_MAP_COMPARE {a, b}` sets two maps side by side (a song's
  name, a `.RPP`, a saved analysis): downbeats in ms, the bars that
  differ, where one has a beat more or fewer.
- No stretch markers and no flexing to a constant tempo: the map follows
  the recording.

## The benchmark

Any change to an algorithm is measured before it is kept:

```bash
cargo run -p tempo-map --features cli --release -- bench \
  "<vault>/Projects/Merry and Bright 2026/songs" \
  --alt /run/media/Development/tools/stem-sep/out2 [--scripts <beats from the scripts>]
```

Every song against the map the owner accepted (its `*Tempo Mapped*.RPP`),
from both stem sets: downbeat error, bars more than 25 ms off, beats added
or dropped, the grid error, whether bar 1 was guessed, and the two stem
sets against each other. `TEMPO_MAP_BENCH=all cargo test -p tempo-map
--test reference_set` holds the numbers the branch was left at.

Where it stands (2026-11-18, ten runs: five songs, two stem sets): the
scripts' own beats give the accepted map in 3 runs; the extension in 9,
with no beat added or dropped except in Go Tell It's ritard ending. Left:
that ritard (placed by hand), and one extra beat in the held-back intro of
What Christmas Means to Me from the second separator's stems.

## Changing an algorithm

The extension is a process so that this needs no rebuild of Studio:

1. Change `features/tempo-map`, run the benchmark, keep the change only if
   the numbers say so (and move the lines in `tests/reference_set.rs`).
2. `cargo build --profile desktop-dev --target <host> -p studio-tempo-map`
   (two crates, seconds), then put the program over the package's:
   `install -m 755 target/<host>/desktop-dev/studio-tempo-map <package>/.new && mv <package>/.new <package>/studio-tempo-map`.
3. The running Studio restarts that extension alone within a second
   (`STUDIO_TEMPO_MAP_STATE`'s `build` says which program answers; bump
   `BUILD` in `extensions/tempo-map/src/lib.rs` with the change). Analyses
   are files and are still there.

An action's arguments are another matter: the host keeps the schema it
first read, so after changing `params` in `extension.toml` end the
development copy and start it again (`EXT_DEV_END {folder}`, `EXT_DEV
{folder}`), still without restarting the app.

## The scripts (fallback)

`<studio>/.claude/skills/song-prep/scripts/`, run with
`uv run --quiet --with librosa --with numpy python <script> …`:

```bash
python dpbeats.py "<folder>" "<mix.wav>" <song>_dp.json --bpm <estimate> --tightness 400
python dpsnap.py <song>_dp.json <song>_s.json [--win 0.06] [--from T]
python snare_check.py <song>_s.json [start_beat]
```

4/4 only; an added or dropped beat is re-spaced by hand with `np.linspace`
between two good beats. Their beats can be measured beside the
extension's: `--scripts` on the benchmark.

## Report

Beat count, tempo range and median, the phase line, what was repaired and
the hand fixes made and where, the per-bar grid error, the flagged spans,
the meter and its changes, and bar 1 with how it was decided.
