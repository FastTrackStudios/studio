---
name: song-prep
description: Set up a new song for a setlist end to end — reference recording, stem separation, tempo map, sections, the project with a Reference folder, synced lyrics, and the Master Rhythm chart. Use when asked to "set up", "prep", "add" or "do" a song or a whole setlist, or to bring a song to the standard the Christmas and recital sets are at. Routes to one skill per step.
---

# Song prep: one song, start to charted

A song is **done** when it has all of these, and each agrees with the recording:

| # | Deliverable | Step skill | Done when |
|---|---|---|---|
| 1 | Reference recording + 11 stems | `song-stems` | Title and length said back to the owner; 11 stems + Mix, each the source's length |
| 2 | Tempo map (one point per bar) | `song-tempo-map` | Snare check passes, grid error reported, **bar 1 confirmed** |
| 3 | Project: tracks, Reference, sections, markers | `song-project` | Opens, saved, song bar 1 = project bar 2, the owner has seen the regions |
| 4 | Synced lyrics | `song-lyric-sync` | A lyrics track whose lines sit on the right bars |
| 5 | Master Rhythm chart | `master-rhythm-chart` (+ `keyflow`) | Bar numbers equal the project's; the owner approved it from a screenshot |

Load the step's skill before doing the step. Scripts for steps 1–3 are in
`scripts/` beside this file (`.claude/skills/song-prep/scripts/` in the studio repo).

## Order, and what runs in parallel

1. **Ask or find which recording.** The setlist note names the artist and version
   (key, tempo, meter). Without a link from the owner, find the recording and say
   the title and length back before anything else. A wrong recording wastes every
   later step.
2. Stems (step 1). While they split, start **research in a parallel subagent**:
   form, chords and lyrics from published sources (MusicXML or Musicnotes score,
   Songsterr via `songsterr-ref`, LRC via `spotify-lrc`).
3. Tempo map, then bar 1. **Stop for the owner's ear on bar 1** unless a score's
   bass line matched (see `song-tempo-map`). Everything after hangs on it.
4. Project with sections and drum/band markers. Show the owner the regions.
5. Lyrics onto the map.
6. Chart last, against the project: chart bar N is project bar N+1.

Steps 1–3 for different songs are independent: several songs can be at
different steps at once. Stem splitting on voyager is one Logic at a time.

## One folder per song

```
<Org>/Projects/<Set>/songs/<slug>/
  <id> - <title> (Mix).wav            the reference, copied in
  <id> - <title> (<Stem>).wav         11 stems
  <Song> - Tempo Mapped.RPP           the REAPER project
  <song>_cfg.json  <song>_beats.json  the tempo map's inputs (keep them)
  <Song>.lrc                          synced lyrics
```

The chart lives with the song's note in the vault (`Libraries/<Library>/<Song>/`),
with a compressed reference beside it as an attachment. Multitracks stay under
`Projects/`. Working copies on voyager are in `~/Downloads/<Song> Reference/`.

## Rules that came from corrections

- **Report, then wait, at two points:** the recording's identity, and bar 1.
- **The stems exist so the drums can be read.** Beats come from the kit stems,
  never from the mix alone.
- **One tempo point per bar**, not per beat: per-beat points drift through fills.
- **A section starts on the downbeat after a vocal pickup**, not on the pickup.
- **One count-in bar**, so song bar 1 is project bar 2.
- Never save, close or replace the owner's open Logic or REAPER projects. Add
  tabs and tracks; say what was left open.
- Audio is the owner's working material: it never goes into git or a public place.

## What is proven and what is not (2026-11-18)

- Proven on five songs (Feliz Navidad, Last Christmas, Rockin' Around the
  Christmas Tree, What Christmas Means to Me, Go Tell It): steps 1–3 on voyager
  with Logic Pro 13 and REAPER, and step 5.
- **Not yet done on any song:** step 4 (lyrics on the map). `song-lyric-sync` is
  the intended procedure, to be corrected the first time it is run.
- **Not handled by the scripts:** meters other than 4/4 (Silent Night is 3/4),
  meter changes, stem separation off voyager. Say so and handle by hand rather
  than forcing a song through.
- **Importing the REAPER project into a Studio session** (the app's own format,
  with the top-level Reference folder) is being built on branch
  `feat/merry-and-bright-sessions`; until it lands, the RPP is the project.

When a step goes differently from its skill, fix the skill in the same change.
