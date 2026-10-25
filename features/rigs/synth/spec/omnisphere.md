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
