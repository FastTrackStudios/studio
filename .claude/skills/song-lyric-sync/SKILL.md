---
name: song-lyric-sync
description: Put a song's lyrics on its tempo map — fetch time-synced lyrics (LRC), check them against the Vocals stem, place each line on its bar and beat, and write the lyrics track Studio's lyric view and confidence monitor read. Use when asked to sync lyrics, add a lyrics track, fix lyrics that run early or late, or as step 4 of `song-prep`.
---

# Lyrics on the map

**Status: not yet run end to end on any song (2026-11-18).** The pieces below
exist and were used separately. The first run should correct this file.

Output: `<Song>.lrc` in the song's folder and a lyrics track in the project
whose lines start on the right bar and beat.

## 1. Get timed lines

Best first:
1. `spotify-lrc` skill: `spotify_lrc.py fetch <spotify link>` writes `lyrics.lrc`
   and `lines.json` (timings from LRCLIB). Needs the **same recording** as the
   reference; a different master or edit will be offset or cut differently.
2. `studio lyrics fetch <Song>.RPP` writes `<Song>.lrc` beside the project.
3. No published timing: faster-whisper `small` on the **Vocals stem** for line
   start times, with the words taken from a lyrics source, not from the
   transcription.

## 2. Check against the recording

- LRC times are for the published track. The reference may have a different
  lead-in: find the offset from the first sung line against the Vocals stem's
  first entry, apply one offset to all lines, then spot-check a line in each
  section. A drift that grows means a different edit; do not stretch, re-time
  the affected sections from the stem.
- Line-level LRC is only good to the line. Do not claim word timing from it.

## 3. Lines to bars

With the project's tempo map, each line gets a bar and beat:
`spotify_lrc.py bars --sync …` or `kf lyrics-lrc chart.kf song.lrc --sync`
(see the `spotify-lrc` and `keyflow` skills for the exact arguments).

- A line that starts with a pickup belongs to the bar it lands in; the
  **section** it opens still starts on the next downbeat (`song-project`).
- Use the result to check the section table: every chorus's first line should
  fall at the same place in its section. One that does not is a wrong section
  boundary or a wrong bar count, and is worth more than the lyrics themselves.

## 4. The lyrics track

Studio reads a track named `Lyrics` (tags may follow, as AbleSet writes them:
`Lyrics +LYRICS +CC [large]`), one item per line, the item's name or text the
line, starting at the line's time. The lyric view and the confidence monitor
under the chart show them (48 lines for Drive To LA in the recital set, the
model to match). Add it to the project by script, not by hand.

## 5. In the chart

Charts carry only the essential lyric cues, written by hand at `@bar:beat`
(`keyflow` skill). Do not pour the whole LRC into a chart.

## Report

Where the lines came from, the offset applied, lines per section, any section
whose first line is out of place, and whether the lyrics track shows in Studio.
