# Kontakt behaviours decoded so far

Each entry: what Kontakt does, where it showed up, and the mechanism or
data that now models it. Check new libraries against every entry — most
recur.

## Values

- **Shipped values beat the source.** A `$var := N` in the KSP is a
  compiled default; the instrument's persistent snapshot
  (`persistent_N.tsv`) holds the value it ships with. Resolve every
  behavioural variable there.
- **The rendering instance can differ again.** CSB Taka's end hit ships as
  `$vwzan = 2` (RR 20) but every Kontakt reference plays `random(22,24)`.
  Measurement wins; the spec says "measured" next to the number.
- **Per-instrument numbers come from the generator**, not the shared spec:
  the same script ships different knob values per patch (CSB Tuba's Taka
  tick is 110 BPM, the others 166).

## Zones and round robins

- **`play_note`'s velocity argument is a velocity index**, selecting the
  zone whose velocity range holds it (index = RR + layer × RRs). Some NKIs
  map files out of suffix order: `tests/vel_rr.py` relabels slots from the
  zone map.
- **Pick zones by `rr_index`, not by position.** Our `find_layer_zone` takes
  the n-th matching zone in file order (`_1, _10, _11, … _2 …`); a hit that
  names a recording must go through the RR-index lookup (`phrase_rr`,
  `short_sequence_rr`).
- **Random RRs avoid the last one or two picks** (`rr_random_avoid_last`);
  with fewer RRs than that the script would spin — check what it really does.
- **Groups, not sample folders, decide what plays**: a group can borrow
  another articulation's recordings (`sync-tsv` emits them `borrowed`).
  Group start criteria (instrument ID, articulation family on CC110/111…)
  are in `groups.json`.

## Levels and envelopes

- **Velocity within a layer**: `dB = (band_top − vel) · delta / (span − 1)`
  (`vel_thresholds` / `vel_layer_db`). An articulation with that law gets
  nothing else — no velocity² on top, even with one layer (CSB Rips).
- **Instrument level** = program volume × the Mix bus fader × its gain
  stages (bus effect kind 4); a **balance stage** (kind 9, one parameter)
  pans every voice (`instrument_pan`; CSB Solo Trombone 0.22).
- **Group flex envelopes with CC-modulated times**: a short family's fall
  after the note-off is `7381 ms × table(cc/127)^3.2` from the CC the
  script sets per family (`set_short_note_CC`; the 3.2 is measured).
  Strings set 127 (no change), CSW 75/62/50/50, CSB 75.
- **CC-driven group envelopes** (marcato swells, filter sweeps): the script
  sets the CC (`set_controller`) before the voice starts; model the CC
  write (`note_on_cc`, `cc19_controller`) and the route (`body_env_*`,
  `group_fx` env routes).

## Timing

- **`wait()` before `play_note`** shifts the body (CSW marcato 40 ms).
- **Tick loops interlock**: a note arriving while a crossfade loop runs waits
  for its next 1 ms tick (`busy_tick_ms`) — a whole-ms lag in fast runs.
- **Releases start on the note-off** in CSW/CSB; CSS waits 2 ms.
- **Timed phrases**: a held key can play a scripted chain of hits on a timer
  (CSB Taka "Hold": delay, tick, fade, end hit) — `PhraseSpec`.

## Pitch

- **Random per-strike pitch envelopes** (CSW marcato) and **random detunes**
  (shorts struck after a gate; brass bodies struck with another key down)
  are draws to fit, not constants.
- **Per-key tuning tables** on short groups (CSB trumpets, `key_pitch_cents`).
- **A detune can belong to one play mode only**: CSB's held-key detune sits
  in the chord-mode path (`$ra4sw = 0`); legato bodies never take it.

## Keyswitches and modes

- **A keyswitch to an articulation an instrument doesn't record is
  ignored**: Kontakt keeps playing what it had (CSS Basses harmonics, CSB
  trombone trills, oboe flutter).
- **A keyswitch target with no articulation of that name** resolves through
  a keyswitch note group (velocity picks HTrills / WTrills); a trill key pair
  then picks half / whole by interval (`trill_pair`).
- **Everything a key can trigger must be warmed** before a render: a side
  only reachable through a pair or overlay starts cold and silent.

## Test harness

- **References can be cut short** when copied: check the WAV header length.
- **A refit is needed** after any change that moves the draws; stored fits
  can carry a past experiment.
- **Fitting one draw kind at a time can settle on the wrong pair**: refit
  RR + detune together (`fit_joint.py`), check a pitch-dip depth by
  rendering (`fit_pitch_env.py`).
