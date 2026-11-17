---
name: nkx
description: Bring a Kontakt library into our sampler and null-test it against Kontakt until it matches — decode its NKIs and samples, model its script, write its library spec and per-instrument overlays. Use when starting a new Kontakt library (e.g. MOJO 2 Horn Section), adding an instrument to one, or chasing why our playback of a Kontakt instrument differs from Kontakt.
---

# nkx — a Kontakt library in our sampler

Goal: an instrument played through our sampler sounds like the factory
Kontakt patch, note for note, measured by a null test. Method: **decode
first, measure second.** Read the instrument's script and shipped values
and model what they say; when the decode runs out, measure Kontakt's render
and write the measured number into the spec with a note saying so.

The Cinematic Studio libraries (strings, woodwinds, brass) went through this
loop; their generators, specs and test tools are the templates. The
CS-specific procedure (chains on voyager, score A/B) is
`.claude/skills/orchestra-kontakt-match`; the decode commands are the
`kontakt-extract` skill.

## Hard rules

- **Keys stay in-process.** Native Access JDX/HU are read from
  `/run/media/AudioHaven/Kontakt/NativeAccess.xml` by the decode scripts and
  passed straight to the tools; print names and SNPIDs only.
- **Library data stays off git**: samples, decoded KSP/zones (`_ksp/`),
  patch folders (`_patches/`), Kontakt renders, scores. It lives on
  AudioHaven, edited in place with dated backups (`file.pre-<what>`).
- **REAPER renders use their own `-cfgfile`** (`~/orchestra-ab/cfg/<copy>.ini`
  on voyager), never the user's `reaper.ini`; kill only PIDs you started.
- **Every long command runs in the background**: cargo builds, `null_batch.py`,
  `null_suite.sh`, the fitters, batch renders — even ones expected to be cached.
- **The engine holds no library numbers.** A library needs something the
  spec can't express → add a generic mechanism with a neutral default
  (`features/sampler`), put the values in the spec or overlay. After any
  engine change, rerun `null_batch.py strings` and compare: every strings
  pair unchanged (mean −38.7 dB as of 2026-11-16).
- **Commit each measured step** through `nix develop --offline -c git commit`
  (hooks may reformat: `git add -u`, commit again), with the before → after
  numbers in the message and a `.changes/*.md` for crates touched.
  nkx-extract commits stay local until the user OKs a push.

## Steps

### 1. Survey the library
List its NKIs (full instruments vs single-articulation patches), sample
container (`.nkx` / `.ncw`), the Native Access product name, and read the
manual's articulation / keyswitch / controller chapters.
**Done when** you have the instrument list, the articulation list per
instrument with its keyswitches/CCs, and the controllers the manual names.

### 2. Decode samples and instruments
Follow the `kontakt-extract` skill. Toolchain:
`/run/media/Development/FastTrackStudio-legacy/nkx-extract` (`cargo build
--release`, in the background). Layout, as the CS libraries do it, under
`/run/media/AudioHaven/Sampled/<category>/<library>/`:
- `<mic>/<instrument>/<articulation>/<dyn>_<note>_<rr>.wav` — the samples;
- `_ksp/<NKI name>/` — `zones.tsv` (with the `group` column), `groups.json`
  (envelopes, mods, start criteria), `buses.json`, `program.json`,
  `script_N.ksp`, `persistent_N.tsv`;
- `_patches/<instrument>/library.styx` (zones, via `signalpack zones sync-tsv
  --disable-unmatched`) and `section.styx` (the overlay, written by a
  generator script — never by hand).

Confirm the category folder with the user for a new library.
**Done when** every NKI is decoded and a few zones' loop points and frame
counts match the extracted WAVs.

### 3. Model the script
For each articulation write the model: how it is selected, which group and
zone it plays (`play_note` velocity = velocity index), RR, velocity layers
and level, legato/transition, releases, timers. Cite KSP line numbers and
the **shipped** value of every variable (`persistent_N.tsv`, never the `:=`
default). A large decode (a phrase engine, a legato path) is a good
background subagent task: hand it the `.ksp` and `.tsv` paths, the KSP
idioms already known, and what you measured. `LESSONS.md` here is the
catalogue of Kontakt behaviours already decoded — read it before modelling.
**Done when** every articulation the probes will play has a written model.

**Run the script before you trust a reading of it.** KONTRA
(`/run/media/Development/KONTRA`, Apache-2.0) compiles and runs the
library's own KSP on the real NKI and traces every callback and script
action at the sample clock. Its trace is the ground truth for what the script
commands; our model must issue the same events. Use the extended build
`/run/media/Development/KONTRA-inspect-vars/target/release/kontra-inspect`
(local branch `local/inspect-vars`: group start criteria in
`get_engine_par`, the variable watch):

```sh
python3 -I features/rigs/orchestra/tests/analysis/mid2kontra.py probe.mid > scn.json
KONTRA_INSPECT_VARS='1:$x,1:%arr[3]' \
  kontra-inspect play <instrument.nki> --scenario scn.json --json out.json
```

In `out.json` → `result`: `stage_events` (callbacks with slot / name /
resume; `script_action` with `action` and `args`: PlayNote, ChangeVol,
FadeOut, SetEventParArr, Random…), `var_changes`, `script_faults`,
`unhandled_actions`. Check the trace reaches `PlayNote`: an unanswered
engine par reads 0 and can send the script down the wrong branch. Its
`random()` is not Kontakt's, so the draws still come from fitting. Where
KONTRA falls short, extend it on a local branch of your own (from
`local/inspect-vars`), never pushed upstream.

### 4. Spec, overlays, registration
- Library spec: `features/rigs/orchestra/specs/<library>.styx` (shared
  structure: articulations, keyswitch `cc58_map`, legato engine, defaults).
- Register it in `signal_orchestra::LIBRARIES`
  (`features/rigs/orchestra/src/lib.rs`: name, install root, spec).
- A generator script per library (templates: `tests/css_section_spec.py`,
  `tests/csw_instrument_spec.py`, `tests/csb_instrument_spec.py`, run by
  `tests/css_section.sh`) turns each instrument's decode into its
  `section.styx` overlay. Map each new KSP knob in the generator and
  regenerate; diff old vs new overlay before writing it.
**Done when** `render_css_live --library <name> --section "<instrument>"`
renders a probe and its `--trace` shows the voices you modelled.

### 5. Kontakt references
Probe MIDI from `tests/gen_probe.py` (add a mode for the library's own
articulations); references rendered on voyager with `tests/kontakt_batch.py`
(one REAPER, a track per instrument, two runs r1/r2), chains as in
`orchestra-kontakt-match` §2. Check each WAV is complete (`null_batch.py`'s
`wav_complete`: header length = file length).
**Done when** every (instrument, mode) has two complete references.

### 6. The null loop
`tests/null_batch.py [--refit] [--modes "…"] <library|section>…` (python
with numpy; `NPY_PYTHON` the same). Per unit results:
`target/orchestra-ab/sec/<tag>/<mode>/null.json`. Pick the worst units,
diagnose (below), decode or measure the fix, rerun with `--refit` when the
change moves random draws, check the strings regression, commit.
Kontakt's random draws (RR, detune, variants, pitch dips, phrase hits) are
fitted, not guessed: `fit_draws.py`, `fit_joint.py`, `fit_pitch_env.py`;
the engine replays them from its forced-draw queue (`--rrs`, `--tunes`, …).
**Done when** each mode's median is recorded and every remaining outlier
has a named cause (fixed, or listed as open in the PR).

## Reading a unit

`strict_db` = residual vs Kontakt; `at_lag_db` after the best lag;
`gain_db` = the least-squares gain onto ours.
- **gain ≈ −3…−20 dB with strict ≈ 0** → decorrelated (wrong file, pitch or
  draw), not a level error: identify what Kontakt plays.
- **lag a whole number of ms** (48 frames) → a script `wait`/tick.
- **residual growing through a steady note** → pitch (fractions of a cent
  matter): measure the drift as sub-sample lag over time.
- **flat level ratio from sample 0** → gain or pan (check per channel).
- **ratio diverging after the note-off** → the release / envelope model.

Tools in `features/rigs/orchestra/tests/analysis/`: `findseg.py` (which file
and offset a render segment comes from), `slide.py` (per window, each file's
implied start), `resprof.py` (residual and levels through one unit),
`pitchtrack.py` (cents over time), `lsfit.py` (joint least squares of a
window by several files). `render_css_live … --trace` prints every voice
(file, rate, gain, start).

## Libraries in progress

- **MOJO 2 Horn Section** (Vir2): `/run/media/AudioHaven/Kontakt/MOJO 2 Horn
  Section 2-3.0` — 17 full instruments in `Instruments/Instruments/`, ~220
  single-articulation patches in `Instruments/Articulations/<instrument>/`,
  NKX samples (`Samples/*.nkx`, 67 GB), manual in `Documentation/`. Native
  Access products "Mojo 2" (and "Mojo"). Not started.
