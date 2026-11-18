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
- **An ignored note is still an event until its note-off.** A note that a
  script plays and a later script ignores (it plays no voice) stays live in
  Kontakt; its `note_off` still runs every later script's `on release`.
  CSSS trills depend on it: script 0 collects the pair within `$dhhc2`
  (25 ms shipped), plays the lower key tagged half/whole (event par 0), and
  script 1 ignores that note, plays the trill voices and their release
  sample when script 0's `note_off` arrives (null −0.3 → −55.8 dB).

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
- **An instrument overlay's `kontakt_transition` replaces the library's whole
  block**: a field added at library level never reaches an instrument that
  has one. If an A/B comes back byte-identical, check that the change can
  reach the engine before reading anything into it.
- **Decoded is not proof that a path runs**: CSSS's `$1z3x0` phrase ramp
  sits on a shared path, but applied to legato it made 17 of 28 units worse
  (marcato, the same code, got better). Keep a decode only once a null
  confirms it.

## Extraction

- **Use nkx-extract ≥ 45dc0be** (local). Older builds wrote one syscall per
  sample (~95% kernel time, ~2,000 CPU-s/GB; now ~6.5) and left short WAVs
  when killed, which the next run skipped as "already extracted". Now each
  WAV is written as `.wav.part` and renamed.
- **Find short WAVs** from any older run with
  `nkx-extract/scripts/find_truncated_wavs.py <dir> [--delete]`: hound's
  placeholder sizes (RIFF 8, data 0) mean a write never finished.
- **An alphanumeric SNPID** (e.g. CSB `Q46`) is not the archive's set id:
  read the u32 at byte 6 of the `.nkx` header and pass that (CSB: `9426`).

## Kontakt 8.7.1, decoded from the binary (arm64 VST3, C++ symbols kept)

Disassembly and a Ghidra project: /run/media/Development/kontakt-re (nm.txt,
strings.txt, proj/Kontakt8; scripts/DecompileAt.java decompiles by address).

- **KSP `random(a,b)`** (`BScriptParser::EvalExpression`): `v = ((rand()<<30)|
  (rand()<<15)|rand()) & 0x7fffffff` (32-bit), `v % (hi−lo+1) + lo`. macOS
  libc `rand()` = Park–Miller (16807, mod 2^31−1, unseeded state 1).
  Kontakt seeds `srand(time(NULL))`. Reproducible renders: the KSP Seed CLAP
  (signal:features/rigs/orchestra/tests/ksp_seed/) pins it; KONTRA `KONTRA_KSP_SEED`.
- **Group start options are a generated system script**
  (`BEngine::GenerateSysScript`): key select, `in_range(%CC[n],min,max)`, round
  robin, random cycles (`random(0,n−1)`, the same rand()), all under
  `USE_CODE_IF_NOT(NO_SYS_SCRIPT_GROUP_START)`. A script that sets the
  condition switches every start option off; the saved criteria are then data
  for `get_engine_par` (CSSS: articulation/dynamic in CC110/CC111 rows).
- **Classic Limiter**: a fixed 32-sample lookahead delay line (not ms; the index runs 0..=31, wrapping only from 31).
- **Saturation (module 0x1d)** is `BEffectShaper`: Classic is a table curve
  at base rate (no oversampling, no delay).
- **Sample resampler** (KONTRA 46eab28): group `interp_quality` 0 Normal = 4-point
  Lagrange (`FillBlock_ResL`) with the position accumulated as a float inside
  512-frame PCM cache windows — the source of a ±2 ppm, step-dependent drift
  against an exact position; 1 High = 9-tap, 2 Ultra = 25-tap Kaiser sinc
  (`BWindowedSinc`, Up/Down tables, cutoff scaled by the step when downsampling);
  `|step−1| ≤ 1e-4` is a plain copy. The step is a float32 product. Render KONTRA
  with `KONTRA_INSPECT_BLOCK=512` to match REAPER's render blocks. CSSS sustains
  then null at −98…−109 dB.
- **Envelope time modulation** (KONTRA c81bc21): AHDSR knob → ms is
  `e^(C·u + ln2) − 2` (C 8.9228 attack/hold, 9.4336 decay/release;
  `BEnvelopeAHDSR::Modulate`). A CC/velocity route on a time scales the knob,
  not ms: `ms' = 2·((ms+2)/2)^m − 2`, `m = 1 − (1 − x)·I` (generated mod script,
  `BGroup::GenSysModScript`); a cube only with the Kontakt 2 compatibility
  byte. The release is re-read at note-off. Fall: state 1.075 → 0.075 by
  `pow(43/3, −1/N)`, silent exactly at N.
- **Events first, per chunk** (`BInstrument::processAudio` 0x83d78c): Kontakt runs
  a chunk's MIDI events and `wait()` resumes before it renders the chunk
  (chunks ≤ 1024 frames; a longer host block splits into ⌈N/1024⌉ equal
  chunks — REAPER's 512 is one chunk, aligned to the render start). So an
  event's parameter change (`change_tune`, `change_vol`, key-up release, engine
  pars) acts from its chunk's START, not its sample. Earlier "CSS waits 2 ms"
  style fits were probably this. KONTRA catches up from the event (e91a9d3,
  340e5ca); an events-first chunk mode is the exact fix.
- **Script offsets** (`DispatchVoice` 0x3dc128): `(int)((float)µs·(float)rate·1e-6f)`,
  f32 and truncated (447000 µs → 21455 frames). `wait()` is in 1/1024-frame units.
- **Marcato body glide** (CSSS KSP 13033–13077): −13.8 cents → 0 over 60 one-ms
  ticks of relative `change_tune`; `setNoteEventPar` 0x11 multiplies the voice's
  tune by exp2(mcent/1.2e6).
- **KONTRA runs Kontakt instruments events-first** (45bb993): per chunk, every
  MIDI event and `wait()` resume first, then the chunk renders; voice starts
  stay at their own frame. `change_vol` follows Kontakt's law (a target more than
  0.1% away is approached by 1/1800 of the gap per frame, unsmoothed start;
  mB → `exp2(mB/6000)`; `ProcessVoice` 0x45c3cc). The AHDSR releases at the
  note-off's own frame (`BEnvelopeAHDSR::Terminate` keeps the offset); the flex
  envelope releases from the chunk start. `fade_out` starts at its event frame;
  a fade-out with stop does not count as a note-off.
- **Flex envelope = BEnvelopeFM7** (ce0988e): one value per 32-frame step,
  ramped one step late; per stage `v ← a + r·v`, `r = q^(1/N)`
  (`NextStage` 0x670bb0), a 5 ms wait when the first stage is > 100 ms, and a
  scaled first-stage time on release.
- **Volume modulators, per step** (`BContMod::RunForAllTargetsInt` 0x39db1c,
  `GenSysModScript` 0x4ba038): every volume route multiplies into one value per
  32-frame step as `1 − (1 − x)·I`, AHDSR and flex included; `ProcessVoice`
  (0x45aad0) ramps that product linearly, one step late. A lag-0 CC route has
  no per-frame smoothing. A flex point level is cubed.
- **AHD-only AHDSR** (`BGroup::SetParEnv` 0x4be268 → `NoteOffLessMode`, +0x120;
  ni-file's `unknown_flag`): the envelope ignores the key-up and its decay
  falls to silence, not to the sustain. CSSS sets it on every release group.
- **Script tune and volume are f32 products** (`setNoteEventPar` 0x3d3b58:
  tune case 6/0x11, volume 4/0xf, each `(f32)exp2(…)`): relative
  `change_tune` multiplies an f32 ratio, so a glide back to 0 cents ends ~1 ppm
  off — a phase drift that reaches −40 dB if you sum millicents instead.
- **Script fades** (`ScriptFadeOut` 0x3d6aac, `ScriptFadeIn` 0x3d6e14,
  `BDsp::FadeVoice` 0x3a4598): a new fade replaces the running one, longer or
  not. `N = trunc(fma(ms·0.001f, rate, 0.5f))`, ≥ 2; frame 1 outputs the current
  level, then steps by `(target − level)/(N − 1)` in f32; during its event
  offset the level holds. A fade-in is ≥ 1 ms; a fade-out under 1 ms on a voice
  that has not played yet zeroes it (CSSS's legato click).
- **`wait()` is f32 too** (`RunStatementList` 0x2c04c0): units =
  `(int)((f64)((f32)µs·1e-6f)·1024·rate)`, ≥ 1, in 1/1024 frame — `wait(10000)`
  is 1/1024 frame short of 480 frames, so a looping script's volume step lands
  one 512-chunk early every 16th chunk.
- **A voice's first block** (`ProcessVoice` 0x45aad0): only a voice whose
  group has a pitch modulator takes the per-frame pitch kernel
  (`ResampleChannelQuad_L_mod_SSE2`, serial f32 cursor) on its first block;
  script `change_tune` does not route through it.
- **CSSS Violin I vs seeded Kontakt (KONTRA, block 512)**, every unit nulled:
  probe −104.3, legato −102.3, leg2 −102.5, marcato −98.6, marc2 −98.8, shorts
  −103.8, pizz −98.2, trem −106.5, rebow −110.7, attack −108.6 dB median.
