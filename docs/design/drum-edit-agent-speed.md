# Drum editing by an agent: what slowed it, and what Studio needs

## The goal

An agent edits a whole song's drums in under a minute, model time included. Applying the edits takes under a second.

## The baseline

On 2026-11-23 an agent (Claude, over Studio's MCP) edited the drums of Crescendum's "Unbreakable". The edit covered bars 9–177, about 1,400 played hits over 44 takes, comped. It took about 80 minutes of wall-clock time and a few hundred action calls. The owner approved it section by section.

- **The engine was not the bottleneck.**

  | Action | Time |
  |---|---|
  | `HITS_PLACE` | 8–25 ms |
  | `QUANTIZE` on one span | 30–400 ms |
  | `DRUMS_EDIT_LANES {reset}` | 211 ms |
  | `HITS_DETECT {all_takes}` | 2.5–5.4 s |

- **Almost all the time went to:**
  1. reading and re-reading hit data, in pages;
  2. working out the targets by hand (per-bar lag, MIDI matching, flams, doubles);
  3. working around quantize and placement rules the agent couldn't see;
  4. checking every step;
  5. undoing the mistakes those hidden rules caused.

One action slowed things directly: `EDIT_REVERT` took 30 s, through the window-queued slip path.

What follows is every barrier met, grouped. The proposals are at the end.

## 1. Reading the state

1. **`HITS_LIST` returns the hits of every take, not only those that play.**
   - 300 hits per page, mostly with `now: null`. The agent pages through all of them and filters them itself.
   - The MCP answer is capped at about 100 KB, so a big read fails mid-JSON (`DRUMS_REFERENCE` and `HITS_LIST` both did).
   - **Need:** `playing: true`, and a compact, columnar result.
2. **No single view of a bar.** For every decision the agent stitched together:
   - `HITS_LIST` (the hits);
   - `DRUMS_REFERENCE` (the MIDI);
   - `ZONES_LIST` (the fills);
   - `SESSION_SECTIONS` (the sections);
   - `COMP_CANDIDATES` (which take plays);
   - its own target arithmetic.
3. **Hits don't say which take they're from.**
   - The agent had to decode it from the id suffix (`~420134`).
   - The id code didn't match the comp's names (`260907_1321-01`), so it was matched by recorded time.
4. **The drum MIDI map was wrong. `DRUMS_REFERENCE_MAP` learned it badly:**
   - kick was taken as pitch 23/46;
   - pitches 24 (kick) and 26 (snare) were unmapped;
   - agreement with the hits was 0.04–0.26.

   So `target: Guided` sent hits to wrong lines, and `by_written` stayed empty. There's no way to set the map by hand.
5. **Quantize results are truncated.** `biggest` lists 30 moves at most. A dry run can't be checked against intended targets without reading every hit again.
6. **No playback speed per slice.** A slice stretched to 0.11 (bar 128) was found only by the owner listening. Neither `DRUMS_REVIEW` nor `HITS_AUDIT` reports stretch.

## 2. How the quantize chooses lines

1. **Nearest-line choice with no model of timing.**
   - The drummer was 20–60 ms early or late, varying bar to bar and section to section. Any hit more than half a line from its grid line went to the wrong line.
   - The agent built its own lag model three times over:
     - per bar from the beat-3 snare;
     - per bar from the MIDI matches;
     - per half-bar from the snares on 2 and 4.
2. **One hit per line.** Doubles, flams and two takes sounding at a comp seam each push a second hit onto the next 16th, 60–100 ms away. The agent wrote rules for these:
   - a hit within 30 ms after another drum's hit rides with it;
   - a same-drum hit within 30 ms rides (a flam);
   - a second kick of a double goes one 16th after the first.
3. **Span edges.**
   - A hit within half a 16th before a span's start belongs to that span. One within half a 16th of its end belongs to the next. Targets within `SEAM_ROOM` of the end are excluded.
   - Results:
     - a pickup snare went to the wrong 16th (bar 52);
     - a triplet snare was pulled into the wrong span (bar 67).
   - None of this shows in the result.
4. **Plan steps sometimes leave their first hit unmoved** (bars 25, 43, 44). The dry run listed the move, the apply didn't make it, and the check still said `ok`.
5. **`lock` overrides `lead`.** With `lock: Kick, lead: Snare` the snares stayed 9–91 ms off beat 3 (bars 121–128). It took a second pass.
6. **`window_ms` (30) silently excludes hits.** Bars 9–10 found 0 hits because the drummer was 31–55 ms late.
7. **`tolerance_ms` (3) leaves hits 1–3 ms off.** It took a second pass at 0.05.
8. **Hidden no-quantize zones.** `EDIT_REVERT` laid a no-quantize zone over Verse 1 without saying so. Every later quantize there found 0 hits, and only reading the engine's code explained it.

## 3. Placing hits (`HITS_PLACE`)

1. **The order of moves matters, and nothing says so.**
   - Moving a run of close strokes earliest-first corrupted fill 1 (bar 44): kicks moved into the middle of the fill and doubles came up midway.
   - The rule had to be learned by trial: later moves go last stroke first, earlier moves first stroke first.
2. **A move and its reverse don't cancel.** +5 ms then −5 ms left a kick doubled.
3. **Placing onto another hit deletes one of them, and which one is unpredictable.**
   - Bar 116: the kick underneath was deleted.
   - Bar 128: the moved snare was deleted.
   - Bar 92: the kick, a unison partner of the snare, was lost.
4. **Refusals without a reason.** "sent to 89.625, the nearest hit plays at 89.6337" left hits 9–16 ms off (bars 59 and 116). Sometimes a second try worked.
5. **Silent stretching.** When neighbours moved apart, a short slice was stretched (0.11× in bar 128) instead of slipped.
6. **No batch placement.** Every hit is its own call, its own check and its own undo step. A section's 20–40 fixes took 20–40 calls, plus re-reads to catch side effects.

## 4. Markers, flams and false detections

1. **False doubles needed hand rules.**
   - The cases:
     - snare trigger double-fires, trigger-only, 24–46 ms after the stroke, 7–15 dB down;
     - toms bleeding onto Tom 4's mic;
     - a snare setting off the kick trigger.
   - The agent removed 15 or so. The owner caught several others by ear: bars 11–12, and the phantom kick in bar 150.
2. **No way to mark a grace note or a flam.**
   - `HITS_MARK` takes `ghost`/`bleed` only; the engine supports `flam_with` internally.
   - A grace before its main stroke stays a cut. Moving the main stroke then makes the grace play twice (bar 120).
3. **Marking `bleed` didn't stop the cut.** The cut stayed where the marker was, so the audio after it played twice: the stutter in bars 11–12.
4. **Removing a marker doesn't remove its sound.** The only way to delete a hit's audio was to place another hit onto it, which is unpredictable (3.3).

## 5. Comp seams

1. **Two takes sound at a take switch.** Bars 85, 129 and 150 had a double downbeat 7–9 ms apart: each take's kick plays in the crossfade.
2. **Nothing trims the old take at a seam.**
   - `COMP_CHOOSE` over a 50 ms stretch moved the switch to the take's nearest cut point. At bar 85 that was a whole beat early, replacing an approved edit with the other take, unedited.
   - `COMP_SEAMS` only moves a seam to where it judges it least heard, then re-edits the stretch.

## 6. History, saving and backups

1. **Steps merge.** Several `HITS_PLACE` calls in quick succession became one history step ("Place a drum hit; Place a drum hit; …"). Undoing one bad move meant restoring an earlier step and redoing the good ones.
2. **The history is the only record of an edit.** When a fix went wrong, finding the step to go back to took a read of `SESSION_HISTORY`.
3. **No backup action.** Backups were made with SQLite's online backup, straight from the shell.
4. **One copy only.** The session is one SQLite file on one drive. A copy to another machine was done by `scp`.

## 7. Telemetry

- **Teley hung.** Its worker wedged on 574 MB of stored rooms. Spans stopped for 3 hours and ingest timed out. Restarting it with `--new` silently moved the follower to a new room.
- **Edit-to-screen time measured 26–86 ms** (`edit.shown`), against a 10 ms target.

## What would make it fast

The pattern that worked, done by hand:
1. read a section;
2. compute a target for every hit (lag-corrected, MIDI-first, with ride rules for unisons and flams, doubles as runs);
3. quantize on the best grid per bar;
4. place the misses;
5. check;
6. fix what the owner hears.

That planning belongs in the engine (or in rhythm, as the `daw.edit` provider). The agent's work then shrinks to reviewing the few hits the plan is unsure of.

### A. One plan call: `DRUMS_PLAN {sections | from/to}`

Returns, for every playing hit (compact, columnar, no 100 KB problem):
- its id, piece and take (by comp name);
- where it plays and where it was recorded;
- the bar's measured lean;
- the matched MIDI note;
- the planned target, and why (`MIDI`, `grid`, `double-run`, `rides-with <id>`, `flam-grace`, `fill-grid 24`, `false-double`);
- a confidence.

How it plans:
- **Lag per beat or bar,** anchored on the backbeat and on MIDI matches. It goes back to `DRUMS_GROOVE` when there's no MIDI.
- **One-to-one, in order,** against the drum MIDI. It also gets a settable map (`DRUMS_REFERENCE_MAP {set: {Kick: 24, Snare: 26}}`).
- **The grid per beat:** straight 16ths or triplets, chosen from spacing.
- **Rides:** unisons within 30 ms, flams and their grace notes, and the second take at a comp seam.
- **Double-kick runs locked in order,** a bar at a time.
- **False doubles flagged,** with a reason. The cases:
  - trigger-only quiet repeats;
  - tom bleed;
  - a snare in the kick trigger.

It lists only the uncertain spots: ties between lines, conflicts, moves over 47 ms. The agent (or the owner) settles them by id.

### B. One atomic apply: `DRUMS_APPLY {targets: {id: seconds}, remove: [ids], rides: {...}}`

- **The whole set is solved at once:** order, overlaps and rides, so the cuts never step on each other.
- **One history step with a named mark.**
- **Stretch policy:** never slower than 0.8×. Otherwise move up what follows (the bar 128 rule), and report each slice's speed.
- **Explicit conflict handling:** when two hits land on one line, say which survives, or refuse.
- **Under a second for a whole song.** Today it's about 20 ms per hit, called one at a time.
- **Returns the full check:** doubled, lost, gaps, every hit's landing error, and every slice's speed.

### C. Read actions an agent needs

- `HITS_LIST {playing: true, bars: [a, b]}`, compact, and including the take.
- `DRUMS_BARS {from, to}`: a per-bar table of played hits beside the MIDI, the zones and the take.
- `QUANTIZE` dry runs that return every move, not 30.
- `SLICES_LIST {from, to}`: the edit lane's slices, with take, offset and playback rate.

### D. Edits the agent couldn't make

- **`HITS_MARK {grace_of: id}` / `{flam_with: id}`:** a grace rides its main stroke, with no cut.
- **`HITS_DELETE {ids}`:** remove a stroke's sound, the cut and the slice, deterministically.
- **`COMP_SEAM_TRIM {at, before_ms}`:** end the old take a few milliseconds before the new take's attack at a switch, without moving the rest of the comp.
- **`EDIT_REVERT` returns at once** (no 30 s window-queued path), and never adds a hidden zone unless asked (`hold: true`).
- **`ZONES_LIST` shows Exception zones prominently,** and `QUANTIZE` names the zones that held hits.

### E. History, saving

- **Every action is one undo step and can take a `mark`.** Back-to-back calls don't merge.
- **`SESSION_BACKUP {to?}`:** a consistent copy with an integrity check, optionally to another machine through Drive.

### F. Detection

- **Flag trigger-only repeats** under 50 ms and more than 6 dB below the stroke as `false-double` at detection.
- **Flag a tom heard only on another tom's mic as `bleed`.**
- **Report triggers that disagree with their mic per take,** as detection already does, and use the mic alone there.

### The minute

| Step | Who | Time |
|---|---|---|
| `DRUMS_PLAN` over the song | engine/rhythm | 1–3 s |
| Review of 10–30 flagged spots | model | 20–40 s |
| `DRUMS_APPLY` | engine | < 1 s |
| Check (in the apply's result) | engine | 0 s |
| Owner listens, sends fixes by bar and beat (`"118.4.25 → 4.5"`) | owner + one apply each | < 1 s each |

Owner feedback should take bar.beat addresses directly: `DRUMS_FIX {at: "118.4.25", to: "118.4.5"}` or `{remove: "150.1"}`. That's how every correction in this session was phrased.

## Related

- The drum edit's guide: `.claude/skills/comping-editing-drums`.
- Placement goes through rhythm, the `daw.edit` contract (daw-edit).
- The edit-to-screen time (`edit.shown`) and the redraw work: the engine split.
