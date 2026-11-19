---
name: song-tempo-map
description: Tempo-map a recording from its separated drum stems — beats from the kit pieces, snapped to hits, checked by snare phase, bar 1 found (by a score's bass line or the owner's ear), one tempo point per bar. Use when asked to tempo map a song, fix a drifting or off-by-a-beat map, find bar 1, or as step 2 of `song-prep`.
---

# Tempo map from the drum stems

Input: a `<Song> Reference/` folder from `song-stems`. Output: `<song>_beats.json`
(every beat's time) and a `bar1` index, which `song-project` turns into one
tempo point per bar.

Scripts: `<studio>/.claude/skills/song-prep/scripts/`. Run them with
`uv run --quiet --with librosa --with numpy python <script> …`.

## 1. Beats

```bash
python dpbeats.py "<folder>" "<mix.wav>" <song>_dp.json --bpm <estimate> --tightness 400
python dpsnap.py <song>_dp.json <song>_s.json [--win 0.06] [--from T]
```

- `dpbeats.py` sums the onset envelopes of every audible drum stem (Kick, Snare,
  Hi-Hat, Toms at full weight; Cymbals and Drums Other 0.6; Bass 0.5; a stem
  under −60 dB is left out), runs one dynamic-programming beat track over the
  whole song, and snaps each beat to the strongest hit within ±30 ms.
- `dpsnap.py` re-snaps to the merged hit list of all kit pieces, refuses a snap
  that makes neighbouring beats differ by more than about 25%, and re-spaces
  unsnapped beats evenly. `--from T` drops junk beats before the music starts.
- `--bpm` is the setlist note's tempo. Get the half/double right: a wrong octave
  shows as the snare on every beat or every fourth.

## 2. Check before believing it

```bash
python snare_check.py <song>_s.json [start_beat]
```

- **Snare phase:** the printout is one character per bar, `+` for snare on 2 and
  4, `X` for 1 and 3, `.` for no snare. A run of `X` after `+` means the tracker
  **added or dropped a beat** at the change. Re-space that span by hand with one
  beat more or fewer (`np.linspace` between two good beats) and check again.
  Common at drum fills, band entrances and bridges.
- **Irregular intervals:** any beat more than 4% off the local median is listed;
  look at each.
- **Endings with a ritard:** place those beats by hand from the hits, and tell
  `song-project` to take them as given (`"raw_from_bar": N` in the config).
- Songs with no snare on the record (brushes) need the same check done on
  whatever carries the backbeat; say which stem you used.

## 3. Bar 1

`bar1` is the index of the beat that is the downbeat of song bar 1. Beats before
it are the pickup and count-in.

- **With a score (MusicXML):** `python3 -I mx.py score.xml > mx.json`, then match
  the score's bass pitch per beat against the Bass stem for every start beat and
  transposition (`bass_match.py <beats.json> <mx.json> <bass.wav>`; it is written
  for one score's part name and repeats, adapt both). A clear winner (0.86
  against 0.67 for the runner-up on Last Christmas) settles bar 1 and also shows
  if the record is in another key than the score.
- **Without a score: it is a guess, and it has been exactly one beat late twice.**
  Tell the owner which beat (time, and what is heard there) and have it confirmed
  by ear **before** sections and markers are placed.
- A pickup is not bar 1. A two-beat vocal pickup means `bar1` is the beat after it.

## 4. What the map is

One tempo point per bar: each downbeat is a weighted line fit through the beats
four either side of it, so a fill or a single late hit cannot drag a bar line.
`song-project`'s `mkproj.py` computes this and prints
`beats vs per-bar grid: median … ms`; medians of 1–8 ms are what good maps gave.
Report that number.

## Limits

- **4/4 only.** The scripts assume four beats a bar. For 3/4, 6/8 or a meter
  change, the beat steps still work, but bars must be grouped by hand and the
  project step needs changing; do not push such a song through unchanged.
- A score's odd bar may not be on the record (Rockin' Around the Christmas Tree
  bar 52 is written 3/4; the record plays a full 4/4 bar). Trust the bass match.
- No stretch markers and no flexing to a constant tempo: the map follows the
  recording.

## Report

Beat count, tempo range and median, the snare line, the hand fixes made and
where, the per-bar grid error, and bar 1 with how it was decided.
