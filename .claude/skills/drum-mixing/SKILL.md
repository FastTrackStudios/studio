---
name: drum-mixing
description: Mix a multitrack drum kit in Studio over MCP, step by step, from a finished edit to a balanced, gain-staged kit — polarity and alignment, drum leveling, the -23 LUFS lock, gates, then tone. Use when asked to mix drums, align drum mics, level drum hits, gain stage drums, or continue a drum mix in Studio. Maintained: add every finding as the mix goes.
---

# Mixing drums in Studio

The order is fixed. Each step makes the next one easier, and a later step never fixes an earlier one (a smeared kick is an alignment problem, however much EQ is thrown at it). Finish a step, check its **Done when**, then move on. Comp and edit first (the `comping-editing-drums` skill); this skill starts from a finished edit. The phase method behind it (Rescue → Balance → Tone → Polish → Relational → Depth, every phase gain neutral) is the `signal-mixing` skill: read it once.

Drive Studio over its native MCP server `studio` (stdio bridge `scripts/studio-mcp` on the control socket; registered at user level; guide `apps/studio/docs/mcp.md`): `session_tree`, `sections`, `mix_state`, `signal_block`, `signal_block_set`, `listen`, `block_measure` (a block's in vs out: gain, crest, GR in one call), `listen_ab` (with a level-matched `matched` diff), `listen_hits`, `listen_align`, `balance` (every folder vs a reference track), `mix_measure` (as a job), `track_set`, `mix_snapshot`/`mix_restore`/`mix_diff`, `drums_level`, `hits_*`, `job`/`more`. Answers are compact and paged, long calls become jobs. The `/tmp/*.py` scripts below were stopgaps from before it existed (2026-11-25). Never drive the desktop.

## 0. Before anything

- `SESSION_MODE {mode: "Mix"}`.
- `MIX_SNAPSHOT {name: "before-drum-mix"}` — the way back for every fader, pan, polarity, send and plugin state. Take another snapshot before each step (`before-<step>`).
- Edits to items (leveling envelopes, edits) are not in a snapshot; those actions set a history mark instead (`SESSION_HISTORY_RESTORE {mark}`).

## 1. Polarity and time alignment (Rescue, offline)

For each piece with more than one source (close mics, a sample, a Fund track): `LISTEN_ALIGN {piece, max_ms: 8}` dry first, then `apply: true`. The apply runs through the Signal `align` block (added to the instance if missing; bypassable later), re-measures, and keeps the fix only if the sum got better.

- Pieces are named as the hit markers name them: `Kick`, `Snare`, `Tom 1`… (`Toms` is not a piece; the error lists them).
- **Widen `max_ms` to 8.** At the 3 ms default the kick's best offset sat on the window's edge (scores still rising at 2.94 ms); at 8 ms the real optimum was 4.35 ms. If the chosen delay is within ~0.1 ms of `max_ms`, widen again.
- **A piece with few hits in the default span** (the first ~16 s) gives a 0 dB "no change": pass `from: "1", to: "999"` (the whole song). A piece with ~2 hits in the whole song can't be aligned; leave it.
- **Expect the samples to be wrong.** On Unbreakable every Sample track (kick, snare, toms 2–3) was polarity-inverted and/or late against the mics. Check the samples first.
- Around each piece, save a variant group so the owner can hear it: `MIX_VARIANTS_CREATE {group: "<piece>-align", scope: ["Drums/<Piece>"]}`, `MIX_VARIANT_ADD {name: "as-is", from: "current"}`, apply, `MIX_VARIANT_ADD {name: "aligned", from: "current"}`.
- Look at the PNG (`/tmp/listen/align-<piece>-*.png`): the aligned sum's first peak should be taller and the close mic's low swing should line up with it.

**Done when** every piece with two or more sources has been aligned or explicitly skipped, `measured` ≈ `predicted`, and the variants are saved. (Unbreakable: kick +2.3 dB level / +2.1 dB low; snare +2.05 / +1.75 dB attack; toms +0.4…+1.15 dB.)

## 2. Drum leveling — consistency compression, offline (Rescue)

Even out the hits before any compressor: `DRUMS_LEVEL`. Each stroke is brought toward the drum's median (kick 75 %, snare 60 %, toms 50 % of the way; at most `range_db` 6 dB), written as the take volume envelope of each drum's close mics (the same gain on every mic of the piece), a few ms before the attack and held through the ring. It replaces any earlier leveling, rides with the audio through edits, and sets a history mark first (`Before DRUMS_LEVEL <date time>`, in `checkpoint`) — revert with `SESSION_HISTORY_RESTORE {mark}`.

- `dry_run: true` first: per piece, strokes raised/lowered and the most moved.
- Many strokes at the 6 dB cap means the drum is very uneven (or ghost notes are being counted as strokes): consider `consistency` per piece or a wider `range_db` only on purpose.
- Leveling changes loudness a little; don't re-measure for it (step 3 is set once).

**Done when** each played piece is leveled and the checkpoint name is noted. (Unbreakable 2026-11-25: kick 857 of 974 strokes moved, snare 306/338, toms up to ~5 dB.)

## 3. Gain stage: the kit at -23 LUFS, then locked (Balance)

The owner's rule: the whole drum kit hits **-23 LUFS integrated** over the whole song, and then the drums are **locked** — anything that should be louder or quieter relative to the drums moves, the drums don't. The full mix then peaks around -6 dBFS instead of clipping.

- `MIX_MEASURE {from: "IN", to: "OUT end", tracks: ["Drums", "Drums/Kick", "Drums/Snare", "Drums/Toms", "Drums/Cymbals", "Drums/Rooms"]}` (sections by their names in the song: `IN`, `VS 1`, `CH 1`…; `"<section> end"`). Whole song ≈ 4–5 min of render.
- Set the `Drums` folder fader by the difference (`TRACK_SET {track: "Drums", volume_db}`) — after the folder's own processing, so its sound doesn't change. Check where `Drums` goes (Unbreakable: a post-fader send to `MIX BUS/INST BUS/DRUM BUS`, which has no active FX) and measure again.
- **Don't run a whole-song MIX_MEASURE again during the drum mix** (owner, 2026-11-25; studio issue "perf(engine): MIX_MEASURE over a song takes 5–10+ min and freezes Studio"): after DRUMS_LEVEL it ran >10 min and froze the window throughout. Use LISTEN / LISTEN_AB over a section (`exact: true` where gates matter) for decisions until that issue is fixed.
- **Set once, not a hard target.** It fixes the general level at the start. Don't re-measure after each change (leveling, gates, Tone): come back to it only at the final overall-mix steps, when every bus's volume is set. (Whole-song MIX_MEASURE takes 5+ min and freezes the UI's first ~40 s: another reason not to repeat it.)
- Inside the kit, pieces can still peak over 0 dBTP (floating point: not clipped, but hotter than their compressor/saturator settings expect). Trim inputs when dialling Tone (step 6), and keep the kit total at -23.

**Done when** `Drums` measures -23.0 ± 0.1 LUFS over the whole song. (Unbreakable: -15.0 → -23.00 LUFS with the fader at -9.24 dB; true peak +7.1 → -0.8 dBTP; master -22.7 LUFS, -1.4 dBTP.)

## 4. Balance inside the kit (Balance)

Start from the reference study's balance (each piece relative to the drum sum, LU: `/run/media/AudioHaven/fts-corpus/mix-references/research/drum-balance-study.md`, when it exists), then trust ears and the owner. Balance on the piece folders' faders, not with blocks. Re-check the -23 lock after.

Unbreakable before balancing, relative to the drum sum: kick +1.2, snare -4.1, toms -11.0, cymbals -18.0, rooms -8.2 LU (the kick hot against the snare).

## 5. Gates (Rescue, live)

Each piece folder's `rescue.gate` is keyed by the piece's trigger (`key.from: .../Trig`, `via: midi`: the session's hit markers). Check every folder's key in `MIX_STATE {track, depth: 0, params: false}` → `signal.phases[].blocks[].key`.

- **A dead trigger fails open.** A piece hit rarely (Unbreakable's Tom 4: 2 hits) keys `via: self` with `fallback: "no hits for 10 s"`: after 10 s without a hit the gate opens on its own input and passes all the bleed. On Unbreakable that bleed was nearly the whole Toms bus in the chorus (Tom 4 -27 LUFS of the Toms' -26.5). Fix: key it from the hit markers, or keep it closed outside its hits.

## 6. Tone, then Polish, Relational, Depth

Per the `signal-mixing` skill and its references (compression styles, parallel filters, metal drums). After each phase: gain neutral. The -23 level is revisited only at the final bus-level step.

## Samples and their mics

Each piece's `Sample` track is a Signal `drum` block (kind `sampler`) playing one piece engine of a Signal-format library (GGD Modern & Massive 2: `/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2/Engines/*.signalengine`), keyed from the piece's `Trig` (the session's hit markers, `via: midi`), `dynamics` 50 %, `velocity` 120, `level` -6 dB (the hardest hit normalized to 0 dBFS).

- The per-piece engines have one layer, `mic "default"`: a stereo mix of the library's mics, room and overheads included. So the sample's room/OH sound plays inside the piece's bus (`Kick/Sum`), not on `Cymbals/OHs` or `Rooms`. Not realistic, and not separately EQ-able.
- The engine format already has multi-mic layers and ports (`MM2 Bubinga Kick MultiMic`: layer `in` = mic `In 1`, layer `room` = `Room Close`, ports `main` and `room`). The `drum` block outputs only one stereo bus today (`sources: &[]`), so a port can't reach another track yet. **To do:** the block exposes each port, and the kit routes `in/out/sub` to the piece, `oh` to `Cymbals/OHs` and `room` to `Rooms`; per-piece engines with those layers.

## Sources (the vault)

Sourced notes on drum mixing, one per video, in the Signal repo: `/run/media/Development/fasttrackstudio/signal/docs/drum-mixing/` (README + `sources/`; untracked on `stack/studio-distribution`; raw transcripts in `/run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/`). Each note ends with a "Signal mapping" table (inference) and open questions (garbled captions). Use them at the Tone and Polish steps:
- **Nolly, Periphery "Wildfire"**: gates shape the envelope (kick/snare wide 50 dB, narrow 13 dB ranges); saturation then opto on the kick (≤3 dB); snare 4:1, 7–10 dB; Distressor 20:1 parallel on the bus with a filtered detector; soothe on cymbals; a limiter on the tom bus; snare verb fed from an unprocessed top duplicate; kick-keyed ducking of the bass.
- **Nolly, EQing any snare**: listen with sustain at 0; cut the leading zone (flappy 400–500 Hz, clacky ~1–1.3 kHz, ticky 5 kHz+); low shelf with high Q for punch; bottom = a cut near 500 Hz and a lower fader; then 4–5:1 comp, a wide ~4 kHz lift, saturation.
- **Colt Capperrune, overheads**: spectral-dynamic cuts at ~2.5 and ~6 kHz on cymbals; steep HPF under the snare thump; a dynamic cut at ~450 Hz and a dynamic boost at the snare's fundamental (180–250 Hz) **only on snare hits** — in Signal, key both from the snare's trigger instead of detecting the snare in the OHs.
- **Andy Wallace bass EQ (secondhand)**: for the kick/bass relationship.

## Tools and their traps

- **LISTEN / LISTEN_AB / LISTEN_HITS / LISTEN_ALIGN** render only what feeds the point, and replay unchanged tracks (`rendered.answered`: `replayed` / `partly` / `rendered`). Fast (0.3–1 s replayed).
- **The fast path is not exact for gates yet** (2026-11-25): Tom 4's LISTEN said -150 LUFS (gate shut) while `exact: true` and MIX_MEASURE said -27 LUFS (gate open, as playback). Use `exact: true` for anything involving gates, keys or triggers until that's fixed.
- **MIX_STATE answers are cut at 100 KB.** A folder at `depth: 1` with Signal EQ blocks already overflows (every block parameter is listed). Read one track at a time: `{track, depth: 0, params: false, from: "1", to: "1"}` (the `from`/`to` keeps automation points out).
- **Setting Signal block parameters** (fixed 2026-11-25): use Studio's `SIGNAL_BLOCK_SET {track, block, params, bypass?, lane?}` — `block` the chain id (`tone.eq`) or the preset's (`eq`), `params` by id in plain units or text (`{"band.3.used": true, "band.3.freq": 450, "band.3.gain": -6, "band.3.shape": "Bell"}`; a band beyond the preset needs `band.N.used` on), any parameter (not just the ~44 host slots), playing or stopped, one undo step; it also lets go of values FX_SET was holding. FX_SET now reaches a stopped CLAP too (flushed). The old state-editing route (`/tmp/sigstate.py`) is no longer needed. Phase level trims: `SIGNAL_RUN {action: SIGNAL_SET, args: {phase, makeup_db}}`.
- **Two Studio stacks on one machine can cross-link**: another session's isolated instance (`STUDIO_INSTANCE=agent`, `wt/studio-instance`) started its engine the same second ours restarted, and our window linked to its engine (LISTEN then said "no song open"). Check `STUDIO_ENGINE`'s pid against `pgrep -af 'daw-extension --engine'`; restart only our stack (`stack-dev/stack/restart-all.sh`, which now uses `/tmp/smcp_mix.py`).
- **Reference shapes**: `/tmp/refspec.py` (median 1/3-octave shape of the corpus's split kit pieces → `/tmp/refspec.json`) and `/tmp/compare.py <track> <piece>` (ours over CH 1, exact, against it). Use the broad shape only below ~6 kHz (the drum splitter cuts a kick's top), and don't chase a difference that's tuning (a snare's fundamental) or bleed a split reference never has.
- **LISTEN_AB `change`** is a list of objects `{action, args}`, not JSON strings. A Signal block's parameter is set with `FX_SET {track, fx: "Signal Dev", set: {"@<clap_id>": value}}`, the `clap_id` from the block's params in MIX_STATE.
- **SIGNAL_RUN** runs the instance's own actions (`action: "list"` lists them): `SIGNAL_SET` (bypass, phase make-up, algorithm), `SIGNAL_BLOCK_ADD/REMOVE/MOVE`, `SIGNAL_ANALYZE`. It has no key-routing action.
- **Renders no longer freeze the UI** (fixed 2026-11-25, Studio `f4134ca22` + daw `fix/offline-render-pinned`): sync actions used to run inside vox's per-connection loop, so a render blocked every call behind it; now they run off it. Measured live: a trivial action answers in 2–57 ms during a MIX_MEASURE (was 45.9 s). LISTEN-family calls still run one at a time (a shared cache lock). Whole-song MIX_MEASURE is still slow (studio#685): keep to sections.
- **Never overwrite a loaded `.clap` in place**: copy to a temp name in `~/.clap`, then `mv` over (an in-place `cp` SIGBUS-crashed the engine).
- **Telemetry**: OpenObserve on 127.0.0.1:5080 (`/tmp/oo.py`: `sql(...)`). Spans per action (`LISTEN`, `action`…); `audio.dropouts` spans every 2 s window with a dropout (worst block, its fx/items/lock/snapshot ms, the worst track); metrics `audio_overruns`, `audio_xruns`, `audio_gap`, `audio_worst_block_duration`, `audio_load`, `audio_rt_cpu`, `audio_track_fx_cpu`…
- **Variants** (`MIX_VARIANTS_*`, `MIX_VARIANT_*`): the owner auditions with `MIX_VARIANT_NEXT/PREV/PICK` (bindable), blind if they like; keep the winner with `MIX_VARIANT_KEEP` (one undo step).

## Log

- 2026-11-25, **reference balance (supersedes the 13-song numbers)**: `research/reference-balance-study.md` in the corpus — 33 mixes, 23 releases, each release one vote. Vs the drum sum: kick −2.4, snare −4.0 (kick leads on 17/23), toms ≈ −4.4 (by ear: fills ~2 dB over the snare), hi-hat −12.2, ride+crash −11.2 (the splitter's "hh" includes the hat in the OHs: set the OHs to the cymbal target and the close hat by ear, not to −12), bass −0.5, guitars +3.7, vocals +2.3, drums −8.7 LU under the full mix. Applied → snapshot `ref-balance-v1`: snare fader 3.5 → 0.45 (kick 1.6 LU over snare in CH 1), OHs 10 → 8.96, close hat kept at +12, Bass −3.2 → about +0.2 (bass −0.5 LU vs drums in CH 1), Electric GTR −6.3 → about −5.2 (+3.7 LU).
- 2026-11-25, polish pass 1 → snapshot `drum-polish-v1` (plan: signal `docs/drum-mixing/plans/unbreakable-polish.md`). **The kit's `rescue_eq` (eq, algorithm `channel`) did nothing** — pre == post on every mic, so no low cut was ever applied (Signal bug, fix delegated). Workaround applied on every drum mic: `SIGNAL_SET {block: "rescue.rescue_eq", algorithm: "parametric"}` then `band.1` Low Cut at the template's cutoff (OHs 220 Hz 24 dB/oct, rooms 120), levels re-matched on the Rescue trim. **Always verify a block with `block:<id>:pre` vs `:post` once.** Moves applied: OHs crash cut 6.3 kHz −4, OH parallel smash comp 8:1 0.5/80 ms sc_hpf 150 mix 0.4 (~3 dB), OH snare-keyed `dyn_eq` (450 Hz Q4 −5 / 251.5 Hz Q1.4 +3, source Hits, key snare Trig; static 450 cut removed), rooms EQ (1.5 kHz +2, 6.3 kHz −3, 12 kHz shelf +1) + comp sc_hpf 150 (~4.4 dB) + snare-keyed broad lift (1.2 kHz Q0.3 +3), bus glue 4:1 30/100 ms sc_hpf 100 (~2.6 dB, crest up), snare Nonlin keyed by the snare Trig (`SIGNAL_SET {block: "depth.verb", key}`; still muted). Drums over CH 1: crest 25.4 → 27.0. Remaining moves: 7 hat, 9 ride, 10 cymbal-bus parallel, 11 crash dyn, 15 room transient A/B, 17–19 bus sat/smile/clip-limit, 20–22 parallel lanes, 23 automation, 24 re-lock.
- 2026-11-25, Tom 4 fixed: its two real hits were flagged `bleed,unsure` (Studio sends only unflagged hits) → `HITS_MARK {ids, bleed: false, approve: true}`; Signal now treats an empty placed list as "known" (no fallback) and re-subscribes on activation (FX_RELOAD race). After `FX_RELOAD`: Tom 4 −150 LUFS over CH 1, opens on its hits at 250–254 s (−30.3 LUFS, peak −0.1). Status may still read `via: hits`. **After any Signal CLAP rebuild, FX_RELOAD and re-check the gates.**
- 2026-11-25, full kit pass → snapshot `drum-mix-v1` (from `before-snare-power` → `snare-power` → `before-kit-balance` → `before-snare-verbs`). Snare: DRUMS_FUNDAMENTAL {trigger, mics, range: "Snare"} = 251.5 Hz; the Fund band-pass was the template's guess (200 Hz) → retuned, Fund fader −12 → −4 (≈12 LU under the snare; careful: TRACK_SET sets absolute volume, read the fader first), snare sat 3 → 6 dB, snare folder +3.5 dB. Reference balance (`/tmp/refbal.py`, 13 split songs, LU vs drum stem, median): snare −3.4, kick −4.7, hh −7.6, crash −9.1, drums −7.4 vs mix (toms unreliable: the splitter dumps kick/snare into them). Ours set to match on folder faders (`/tmp/bal.py`, vs the kit sum `Drums pre-fx`): kick −3.7 dB (snare now 1.3 LU over kick, as the refs), OHs +10, hi-hat close +12, rooms kept at −7.5 LU. Depth: snare Plate (decay 0.62, predelay 20 ms, 350 Hz–9 kHz, size 0.6) at 14 LU under the dry snare sum; Hall short (decay 0.32, 300 Hz–8 kHz) at 16 LU under for size. The **non-linear verb never opens** on `Snare/Sum`: it opens only on a +12 dB jump over its 150 ms follower, and the sum's steady bleed never allows it — it should be keyed from the snare trigger (left muted). A muted verb is muted twice: the track and the Signal lane's `mute` (set both). Verbs sit inside the piece folder, so its gate and comp see the tails (not cut in practice; TRACK_MOVE only moves to a top-level folder). Drums post-fader over CH 1 is now −25.5 LUFS (was −24.4): re-trim to −23 only at the final bus step.
- 2026-11-25, Tone (compression + rest), snapshots `before-tone-comp` → `tone-v1` → `tone-v2`. The template's compressors (all threshold -18) were set for nothing in particular; their detector follows average level (RMS-like), so set thresholds by measuring `block:tone.comp:pre`/`:post` over CH 1 (`/tmp/gr.py`), not from peaks. Per-mic comps bypassed (folders compress). Kick folder 4:1, 30/120 ms, thr -14 → ~3 dB RMS GR, crest up (punch). Snare 4:1, 20/80 ms, thr -38 → ~6 dB, crest 28→32. Drums bus 2:1, 30/150 ms, thr -16 → ~2 dB. Rooms 4:1, 3/120 ms, thr -24 → ~5 dB, crest down (sustain up). Cymbal comps idle (left). Every change level-matched on the lane's **Tone phase make-up** (`SIGNAL_RUN {action: SIGNAL_SET, args: {phase: "tone", makeup_db}}` — the gain-neutral trim; a block's own `makeup` set through the state didn't stick). Compression brought snare box and dulled the kick click: EQ re-touched after (kick 4.5 kHz +8; snare 340 −3, 490 −11, 1.25 k −7). OHs: 450 Hz −3, 10 kHz shelf +1.5. Toms: 400 Hz −5, 3.5 kHz +3 (unverified: no tom hits in CH 1). Tom 4 bleed passes though its gate meters −80 dB: under investigation.
- 2026-11-25, Tone (EQ) on kick and snare folders' `eq`, against the reference shapes, level-matched, variants `kick-tone` and `snare-tone` (A flat, B eq). Kick: low shelf 55 Hz +3.5, 170 Hz −7, 450 Hz −8, 750 Hz −5, 2 kHz −3, 4.5 kHz +5, out +2 → sub and click within ~1 dB of the refs, box −10…12 dB (rest is snare bleed). Snare: low cut 75 Hz, 200 Hz +3, 480 Hz −8, 1.25 kHz −5, 3.2 kHz −2, high shelf 6.5 kHz +2, out +1.5 (its fundamental sits ~200–250 Hz, refs' ~160: tuning, left). Engine restarted once mid-session (clean exit after the long measure); nothing was lost (autosave).
- 2026-11-25, Rescue check: corrective low cuts already set by the template (kick 30, snare top 60 / bottom 100, toms 70/55/42.5/35, OHs 150, hat 250, ride 200, rooms 60 Hz) — kept. No raw recording clips (kick In +0.75 dBFS only from clip gain + leveling, float). Tom 4's raw mic is ~12 dB hotter than the other toms (mostly bleed). Gates: Signal bug — a folder's gate keyed by another track's trigger never learns the song's hits are known and fails open after 10 s (toms 1/3/4 open most of the song; Toms bus ~10 LU of bleed in CH 1); fix delegated (Signal: planned hits for remote keys, live and in render copies).
- 2026-11-25, Unbreakable (Crescendum): steps 1–3 done (alignment variants `kick-align`, `snare-align`, `toms-align`; leveling checkpoint `Before DRUMS_LEVEL 2026-11-24 19:28`; Drums fader -9.24 dB for -23 LUFS). Open: Tom 4 gate (step 5), balance (step 4) waiting on the reference study.
