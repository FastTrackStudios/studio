---
name: song-project
description: Build a song's project from its stems and tempo map — tracks, the reference mix, a click, one tempo point per bar, sections as regions and drum/band events as markers — as a REAPER project built by script, and bring it into a Studio session with a top-level Reference folder. Use when asked to build or rebuild a song's project, add or fix sections and markers, or as step 3 of `song-prep`.
---

# The song's project

Input: the `<Song> Reference/` folder, `<song>_beats.json` and `bar1` from
`song-tempo-map`. Output: `<Song> - Tempo Mapped.RPP` in that folder.

## 1. Config

`<song>_cfg.json` (see `scripts/example_cfg.json`):

```json
{"rpp": "<folder>/<Song> - Tempo Mapped.RPP", "folder": "<folder>",
 "base": "<basename>", "mix": "<folder>/<basename> (Mix).wav", "bar1": 0,
 "sections": [[1, "Intro", "c8c8c8"], [9, "Chorus 1", "e65a5a"]],
 "markers": [[69, "Perc in"]]}
```

- Bars are **song bars**. Optional: `"one_countin": true` (a single slow count-in
  bar holding all audio before bar 1), `"raw_from_bar": N` (ritard ending).
- Colours: verse `5aaae6`, chorus `e65a5a`, bridge/interlude `e6963c`,
  breakdown/solo `aa6edc`, intro `c8c8c8`, ending `787878`.

## 2. Sections and markers

Where the form comes from, best first: a matching score's rehearsal marks; the
owner's chart; otherwise the audio — per-bar stem levels and half-bar bass roots
(`form.py <beats.json> <bar1>`), repeated 8-bar blocks, and vocal timestamps
(faster-whisper `small` on the Vocals stem).

- **A section starts on the downbeat after a vocal pickup.** Lyric timestamps
  therefore point a bar early; move the section to the next downbeat.
- Sections are normally 4, 8 or 16 bars. An intro of 7 means bar 1 is wrong.
- Markers are what the band needs to know: drums in/out, breaks, hits, fades.
- A section named `End` becomes a plain marker.

## 3. Build

```bash
uv run --quiet --with numpy python mkproj.py <song>_beats.json <song>_cfg.json <song>.lua
```

It prints the grid error and writes a Lua script. Run it in REAPER through the
`reaper` MCP server's `run_lua`: `return dofile("<path>/<song>.lua")`. The script
finds the project's tab or opens a new one, writes the tempo points, tracks,
items, click, regions and markers, verifies every beat against the grid and that
song bar 1 is project bar 2, and saves.

What it builds: CLICK, MIX, VOCALS, a DRUMS folder (Kick, Snare, Hi-Hat, Toms,
Cymbals, Drums Other), BASS, GUITAR, PIANO, OTHER. All items at 0, stems muted,
MIX and CLICK playing. Regions numbered from 1, markers from 101.

Traps:
- Never close the owner's other tabs. Each song gets its own tab.
- A `run_lua` refused once: send the identical call again.
- A never-saved new tab opens Save As on save: open the path in that tab with
  `Main_openProject("noprompt:"..path)`, then save.
- Check the click's level; it has come out at +12 dB.

## 4. Into Studio

The app's own form is a Studio session (`.session` + `.fts-session`) with a
**top-level `Reference` folder** holding the mix, so the Stems/Reference switch
(`SESSION_HEAR`) works for every song of a set, and a `.setlist` that lists the
songs in order. Importing the REAPER project is being built on branch
`feat/merry-and-bright-sessions`; when it lands, write the command here.
Until then: the RPP is the deliverable, and the reference mix goes beside the
song's note in the vault as a compressed attachment.

Whole-set edits in Studio are scripted, never done by switching songs:
`DAW_RUN {id, args, song}` or a Luau batch through `SCRIPT_RUN`
(`TRACK_MOVE {track, into, every_song}` moved every song's reference in 0.05 s).

## Report

The project path, tracks, tempo points, the sections table (bar, name), the
markers, the grid error, and that it is open in its own tab and saved.
