---
name: comping-editing-drums
description: Comp and edit a multitrack drum recording in Studio, from the takes to a final edit. Use when asked to comp drums, choose drum takes, mark fills, quantize or tighten drums, or check a drum edit.
title: Comping and Editing Drums
kind: guide
type: guide
category: drums
---

# Comping and editing drums

This guide takes a drum recording of several takes to a finished edit. You choose the best performance first, then tighten it. Each step names its key or click, for working in the window, and its action, for driving Studio over MCP (`actions`, `describe`, `run`). Each step ends on **Done when**. Finish one step before starting the next.

The lane system works for comping and editing any instrument. This guide covers drums.

## The lanes

Every drum track holds the same stack of lanes:

| lane | holds |
|---|---|
| the takes | each pass as recorded, one lane each |
| `COMP` | the comp: the best take for each stretch of the song. Usually one; keep more if you want to compare. |
| `EDIT` | a copy of `COMP` that the quantize and your hand edits change. `COMP` is never touched, so the edit can always be made again from it. |
| `FINAL` | which of `EDIT` and `COMP` plays, stretch by stretch: the edit where it works, the comp where the edit went too far. **Not built yet**; today you switch the whole kit with `DRUMS_PLAY_LANE`. |

**Comp first, then edit.** The edit is made from the comp. If a spot will not edit well, comp it again from another take and quantize that stretch again. Hand edits are kept through this (step 7).

## 1. Make the kit one unit

Every mic and trigger under the `Drums` folder belongs to one group, so the whole kit is cut, slipped and comped together. Then you edit from the folder alone, with the folder shut.

- **Window:** turn on Grouping in the toolbar.
- **Action:** `DRUMS_GROUP` (`dry_run` first to see the tracks it takes), then `SESSION_GROUPING {on: true}`.

The kit is found from the golden template. Each piece (Kick, Snare, Tom 1…) has close mics, a `Trig` track (the trigger, which keys the gate and is never heard) and a `Sample` track. Faders, mutes and solos stay each track's own.

**Done when** `DRUMS_GROUP` names every recorded drum track, and no Sample, Process or Verb track.

## 2. Open the lanes with the folder shut

Leave the `Drums` folder closed and show its lanes. Each lane is then one row, folded from every track under it, so you see all the takes of the whole kit together.

- **Window:** the **Lanes** button on the `Drums` folder's header.
- **Action:** `TRACK_SHOW_LANES {track: "Drums", show: true}`.

**Done when** the folder's row shows a lane per take.

## 3. Detect the hits on every take

Hit detection finds every stroke of every piece once, on the triggers and the close mics together, and marks it on each recording. The comp ranking, the fill finder, the quantize and the checks all read these markers, so run it on every take, not only the one that plays.

- **Action:** `HITS_DETECT {all_takes: true}`.

Detection keeps every hit placed by hand or approved. A piece whose trigger recorded nothing is read from its mics alone. If a take's trigger file is silent where its mics heard hits, `DRUMS_TRIGGERS_FROM_MICS` (after `HITS_DETECT`) makes a trigger from those hits, so the gate still opens.

**Done when** `HITS_DETECT` reports counts for every piece. Look at the `unsure` count; `HITS_EXPLAIN` and `HITS_LIST` show those hits.

## 4. Mark the fills

A fill is where the drummer leaves the groove, and it is comped and edited on its own. Mark each one as a zone on the ruler's Drums lane.

- **Action:** `ZONES_FIND` proposes the fills. Approve or reject each with `ZONES_EDIT`. Add one it missed with `ZONES_ADD {start, end, kind: "Fill"}`. `ZONES_LIST` shows them all.
- **Window:** in Edit mode the zones are shaded, and each piece's hit markers sit on their own rows.

A fill is left as played when the song is quantized, unless you include it (step 6).

**Done when** every fill in the song has an approved zone, checked against the song by ear or by the drum MIDI (`DRUMS_REFERENCE`).

## 5. Comp

Choose the take for each stretch of the song before you edit anything.

1. **The main take.** The best overall pass becomes the default, so the comp changes as little as it can. Run `COMP_MAIN {take: "Teal"}` (a take by pass, colour or lane name, as `COMP_CANDIDATES` names them).
2. **The best of the rest.** Run `COMP_BEST`, or press **Ctrl+T** over a selection. It ranks every 4-bar phrase from the takes' hits: how alike a take is to the part (a practice take is not alike, however tight), then how tight. It leaves the main take only where another plays clearly better.
3. **Each fill by ear.** Select across a fill (its hit markers, a razor or a time selection) and press **T** / **Shift+T** to step through the takes, best ranked first. **Ctrl+Shift+T** keeps the take you chose for that stretch, and the later comp and seam passes leave it alone. `COMP_CANDIDATES` shows the ranking; `COMP_CYCLE` is what T runs.
4. **Rank takes while listening.** Mouse button 5 or **↑** up-ranks the take under the mouse or selected; button 4 or **↓** down-ranks it (`TAKES_RANK`). A down-ranked take is skipped by T.
5. **Hide the seams.** `COMP_SEAMS` moves each switch between takes to where it is heard least, just before an attack (`dry_run` first).

**Done when** every stretch plays the take you want, every fill's choice is kept, and `COMP_SEAMS {dry_run: true}` proposes no move you would hear.

## 6. Set up the edit lane and quantize

1. Run `DRUMS_EDIT_LANES`. It names each track's comp lane `COMP` and adds `EDIT` holding a copy of it, playing.
2. Quantize: **Q** over a selection (`EDIT_QUANTIZE`), or `QUANTIZE` by `sections` or `from`/`to` with a `dry_run` first. Different sections can be quantized different ways. **Shift+Q** opens the settings.

How the quantize moves a hit:

- **It cuts and moves, and stretches last.** Each hit is cut a little before its attack, and the whole kit moves together with crossfaded seams. Audio is stretched only when nothing else closes a gap, which in practice means hand edits that ask for something no take played.
- **It takes the nearest line, guided by the drum MIDI when you have it.** With the song's drum MIDI (the part as written, `DRUMS_REFERENCE`), a hit near a written note goes to the line that note is on, straight or triplet. The default `target` is `Guided`; `Reference` puts every hit on its nearest written note, and `Grid` ignores the MIDI. The MIDI is a guide: a hit the chart does not have (a fill played differently) goes to the nearest line only when it is plainly near one, and is otherwise left for you to hear.
- **Fills are left as played** unless you say `fills: Include` (with the fill's own `grid`, such as `12` for 8th triplets) or `fills: Only`.

**Done when** the quantize's own check is clean and `DRUMS_REVIEW` shows no gap where the drums cut out.

## 7. Go through the whole song

Step through the song a page at a time and listen to every page.

- **`[` and `]`** move a page of bars back and forward (`SESSION_STEP`). The transport bar sets the page size (4 or 8 bars for a pass over the drums) and the instrument it frames (the kit, by default). While stopped, the edit cursor goes to the page's start, so Play plays the page.
- **On each page:**
  - Listen to the edit against the comp: `DRUMS_PLAY_LANE {lane: "COMP"}`, then `{lane: "EDIT"}`.
  - Move a hit by hand: drag its marker, or `HITS_PLACE`.
  - Take an edit back: **Alt+Q** over a selection.
  - **Backspace** on selected hits clears the automated edits first and keeps your placements; a second press clears those too.
  - If a stretch will not edit well, go back to step 5 for it: T to another take, then **Q** again. `DRUMS_REQUANTIZE {keep_manual: true}` starts the whole edit over and puts back what you placed by hand.
- **The checks:**
  - `DRUMS_REVIEW` lists hits off their written note, worst first, and any gap.
  - `HITS_AUDIT` counts hits played twice at a seam or cut away by the edit.

**Done when** every page from the first bar to the last has been played, and `DRUMS_REVIEW` lists nothing you have not heard and accepted.

## After the edit

- `DRUMS_HITS_MIDI` writes the edited hits as MIDI, on a `Drum Hits` track and on each piece's Sample track. Editing those notes edits the hits.
- `DRUMS_SAMPLES` and `DRUMS_SAMPLES_ALIGN` set up the sample reinforcement from the edited hits, lined up with the close mics.
- `DRUMS_LEVEL` (**Alt+L**) evens each drum's strokes out.
