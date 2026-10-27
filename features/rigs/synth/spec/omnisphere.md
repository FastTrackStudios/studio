# Synth — Omnisphere

Behaviour spec for the Omnisphere routing experiment and `.prt_omn` / `.mlt_omn`
patch import. Requirements land here as the engine mapping is formalized; the
implementation lives in `features/rigs/synth/src/`.

## Goal

Play real Omnisphere patches through the native signal-sampler engine — no
Spectrasonics plugin in the chain. A `.prt_omn` (Part) or `.mlt_omn` (Multi)
parses into the [`omni`] composition tree, its Sample-mode Soundsources realize
against the local extraction, and the tree compiles to a `RenderNode` that
sounds under MIDI.

Two proven entry points today:

- **Headless render** — `omni_import::load_patch_file(path, &index)` → `Container`
  → `RenderNode::compile` → `render()`. Exercised by the `#[ignore]`d audible
  tests in `omni_import` / `omni` (machine-local, gated on the extraction).
- **Live play** — `cargo run --release -p signal-sampler --features pipewire
  --example keys_tui -- --omni <patch.prt_omn>` hosts the imported patch on a
  `KeysRig` and plays it from a hardware/computer keyboard.

## Status (2026-10-26, later)

Each found by isolating one control on a custom test part (a pure sine or
the init saw, `tools/omni_probe/*probe.py`, `vsweep.py`, `fxab.py`) and
sweeping it through both engines:

- **Older patches** (~12 %, no MULTISAMPLE) name their soundsource on the
  layer's `OSC` (`kind` 4): they had played a synth saw.
- **Mod envelopes**: `MODENV c` counts the live points (one spare `p`
  follows); curves bend `1 + 31.5·|c − 0.5|`; synced times are beats.
- **LFOs** run from load, as Omnisphere's: `render_patch` pre-rolls 1 s as
  `omni_render` does, so free-running LFOs meet the note in phase.
- **FM**: β ≈ 0.60·moddepth / ratio; `fmscl` ×1 / ×4 / ×8; `fmwf` 0.06 is a
  noise modulator (white noise low-passed ~10 Hz, swing ∝ depth^¼); the
  modulator starts a quarter cycle in.
- **Ring mod**: `amwf` sine / triangle / saw / noise / falling saw, `amscl`
  ×0.25 / ×0.5 / ×1; the modulator starts an eighth cycle in.
- **Unison**: `uoct` is a selector (0.04 = plain), not an amount.
- **Waveshaper**: type 0 is a sine folder (`w = 122·depth`, Gain drives it,
  a sampled layer folds by its own level); `pos` past 0 puts it after the
  filter; type 0.24 is a clean gain.
- **Filters**: the ladder holds its loop under the run-away edge; Juicy 24,
  Power 24 and Beefy LPF 4-pole resonance refit on the level.
- **FX**: Flame Distortion (22 types, each a gain and a ceiling), Unstable
  Drifter's tone, Half Speeder (an octave down), PRO-Verb's held level.
- **Scoring**: references cached by content; the held spectrum floored 40 dB
  under its peak and scored to 16 kHz; the level floored 60 dB under the
  reference peak (a tail's noise floor is not a level difference).

Batch (71 gap-free patches): median held-spectrum gap 6.5 dB with FX,
5.5 dB voice only. Open: the arpeggiator's timing, resonance in the less
used ladders (their `r^0.3` laws over-resonate near 0), Waveshaper types
other than 0 and 0.24 (not memoryless), the noise-free half of `uoct`
(octave down), multi-element soundsources (`OSC Layer0..3Act`).

Measured 2026-10-27, sample layers: **Timbre Shift** (`tbmd` 0.02, ~92 % of
active sample layers) is an exact semitone remap — the zone of the note minus
`sampler::instrument::timbre_shift_semis(t)` (−28 at 0 … 0 at 0.5 … +11 at
0.82, steeper below centre), repitched to the note; static and routed it
matches the plugin to its own noise on a chromatic source. Past 0.82 the
remap continues (+12 at 0.83 … +24 at 1) under a grain layer the plugin adds
(sidebands ~18 Hz about each harmonic; on the cello also far quieter from
0.98) — the remap is modelled, the grains and that drop are not. `tbmd` 0
is another process (open). **Mogrify** starts a fresh note
part way into its sample (25 ms at 0.05 … 257 ms at 1, measured on a
piano), static and routed. A sample soundsource takes `tune` routes.
**Filter Spread** (`sprd`, 0.5 none) splits the section's cutoff across the
channels: left at setting `s·(1.5 − sprd)`, right at `s·(0.5 + sprd)` —
static and routed within 1 dB per channel. **LFO rate** routes
(`LFOn rate`) add depth·source to the rate knob, through the same
48·knob³ law as the static rate — exact for a velocity row, and an LFO
sweeping another's rate tracks the plugin's pitch trace (r 0.98).
**Mod Env depth** rows (`Mod Env n Depth`) are a cut, as the LFO swing's:
`clamp(1 + depth·source, 0, 1)` — a plain row does nothing, an inverted
one scales the envelope down (exact on a held envelope). **Amp envelope
timing**: Speed (`AENVPARAMS spd`) is a time factor on every segment
(`sampler::native::breakpoints::env_speed_time`: ×2.49 … ×1 at 0.5 …
×0.1 at 1); Key / Velocity rows onto `AmpEnv Speed` add to the knob, a
Velocity row onto `AmpEnvAtk` takes `1 − depth·velocity²` of the attack —
played per note, within a few % of the plugin. **Pan rows** (`X pan`,
6 203 rows) pan the whole part, after the layer mix (layer A's row moved
layer B too); **Alt** is bipolar, −1 on the first note (the toggle on
later notes is assumed — the harness holds its earlier note into the
next); **Random Unipolar** draws 0..1. Open: a `B pan` row on the init
part (layer B switched on by its amp envelope only) does nothing in the
plugin, while on a factory patch every layer's row pans the part.

## Status (2026-10-25)

Measured over the whole factory library by `examples/patch_coverage`
(37,017 patches, all parse):

- **99.9 % sound** (at least one layer realizes); **60.6 % have no known
  gap**. Soundsource resolution was the blocker — 4,316 patches named a
  soundsource the old extraction had no styx for. A `.db` presents its
  soundsources through root-level `.zmap`s, not its file name or its
  `Default Layer.xml` (`Blue PWM Strings a/b/c` are three keymaps over
  `Virus Blue PWM Strings.db`; `Cello Tender Mutes` plays the instrument
  `Studio Cello Tender Mutes -12` from `Stereo Mics.xml`).
  `omni_import::steam` reads the `.zmap`s and `examples/omni_zonemap`
  writes one styx per soundsource beside the existing extraction: 5,587
  of 5,590 factory soundsources map.
- What is left is DSP, by patches hit: pass-through FX units **35 %**,
  Granular 5 %, sample-mode FM 2 % / Ring Mod 1 %, unextracted
  soundsources 0.3 % (Moog Tribute).

Fidelity, A/B against the plugin (`tools/omni_probe/abbatch.py`, 71 gap-free
patches, note C3): median held-spectrum gap 8.5 → 7.2 dB, within 3 dB 7 → 13,
within 6 dB 16 → 25 (2026-10-26). Calibrated so far, each against the plugin
one feature at a time: soundsource tuning (A440 divided out) and Octave/Semi
re-keying, filter key tracking and the cutoff knob's bottom fifth per model,
the envelope segment law (a power law), the Waveshaper's level and Bit Crush,
Harmonia (mix / level laws, voices on the layer table, off in older patches),
the AM section's gain, Valve Radio, Swiss Army Delay, Radio Delay, Studio
EQ's low cut. Open, by measured impact: the Waveshaper's fold curves (per
type, a shaping table), resonant filters, LFO / mod-env routes to the 29 %
of targets not yet translated (granular,
envelope times, FX parameters), the reverbs' build-up, and the 31
pass-through FX units (`tools/omni_probe/fxgap.py` ranks them).

Comparison runs on voyager (macOS): the real Omnisphere VST3
(`omni_render`), the warm build and the full extraction on the dev drive
(`.env` sets `FTS_SAMPLED_ROOT` / `FTS_PACK_LIBRARY`).
`tools/omni_probe/abbatch.py` A/Bs a spread of gap-free patches.

## Status (2026-07-13)

Import is solid: **37,006 factory patches parse, 0 failures**. Sample-mode
patches, pure synth-mode patches, and Multis all render **audibly**.

### Live native DSP (sounds correctly on import)

- Sample-mode Soundsource → real extracted audio (`BlockImpl::Sample`)
- Synth-mode → native Wavetable oscillator
- Dual Filters — 70-type name classification → native filter (cutoff calibrated
  against the real engine: `15 Hz × 2^(9.55·v)`)
- Amp + Amp/Filter ADSR envelopes (from AENV/FENV breakpoints)
- Waveshaper, Dual Freq Shifter, Harmonia (→ native modal)
- Mod Matrix routes, 8 LFOs, Arpeggiator (mod engine ticks live)
- **FX racks** (Layer / Common / Aux / Master) — each slot's Omnisphere unit
  name maps to native DSP via `model::classify_effect` (verbs → Reverb,
  echoes/BPM delays → Delay, choruses/ensembles → Chorus, EQs → Eq,
  compressors/limiters → Compressor, tremolo/phaser/flanger/vibrato, drive/
  distortion/saturation → Drive, filters → Filter, gates → Gate). Bypassed
  slots (`EFFMODULE Active≈0`) stay silent. Realizes with native defaults —
  see the fidelity gap below.

### Placeholder (pass-through — the remaining gap)

- Oscillator stack: **Unison**, **FM**, **Ring Mod**, **Granular** import their
  params but render as pass-through (no native DSP yet). Unison is the highest
  timbral payoff for synth patches.
- FX **parameter fidelity**: `classify_effect` picks the block *type*; each unit
  realizes with native defaults, not the patch's stored FX params. The
  `EFFMODULE` param blob is not yet decoded/mapped, so a reverb sounds like
  "a reverb" rather than *that* reverb.
- Exotic FX with no native equivalent (Imager, Retroplex, amp/console sims,
  backward/pitch FX) stay placeholders.
- Quadzone **Fader** scan mode needs the mod matrix at control rate.
- Multi level: 8 Parts + mixer + shared Aux racks — `multi.rs` imports the
  structure; per-part mix fidelity is partial.

## Next

1. **Unison** — real detuned voice stack (params already parsed).
2. **FX parameter mapping** — decode the `EFFMODULE` param blob and drive the
   native units, upgrading dry-defaults to per-patch settings.
3. FM / Ring Mod / Granular native DSP.
