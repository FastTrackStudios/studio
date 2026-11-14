---
name: orchestra-kontakt-match
description: Bring a Cinematic Studio instrument (CSS strings section, CSW woodwind, …) to match Kontakt in our sampler, and render scores through it. Covers decoding the NKI, syncing zones by Kontakt group, the per-instrument overlay, making factory Kontakt reference chains on voyager, the probe null suite, and rendering a MusicXML score through our engine and Kontakt for an A/B. Use when working on orchestral sample playback, a Kontakt null test, a new CS section/instrument, or "render this score with our strings/woodwinds".
---

# Orchestra ↔ Kontakt match

Goal: an instrument played through our sampler sounds like the factory
Kontakt patch, note for note. 1st Violins was matched by hand; everything
else follows this loop. Tools live in `features/rigs/orchestra/tests/`.

## Rules

- **Never print Native Access keys** (JDX/HU). The decode scripts read them
  from `/run/media/AudioHaven/Kontakt/NativeAccess.xml` inside their own process.
- **Don't disturb the user's REAPER on voyager.** Render with
  `REAPER -newinst` (a headless instance of its own); kill only PIDs you started.
  GUI work on voyager only when the user says voyager is free.
- **Copyrighted material stays out of git**: scores, renders, Kontakt chains
  (`target/orchestra-ab/chains/`), decoded KSP. Library data lives on
  AudioHaven (`_ksp/`, `_patches/`), edited in place with dated backups.
- **Never render on the user's `reaper.ini`.** A `REAPER -newinst -renderproject`
  instance rewrites its ini as a cut-down stub (this wiped voyager's config on
  2026-11-13). Render with `-cfgfile ~/orchestra-ab/cfg/render-<tag>.ini`, a copy
  of `cfg/user-reaper.ini` (the user's settings, a present audio device) with the
  plugin caches beside it; `null_suite.sh` does this. Never SIGTERM a render
  REAPER that runs on the user's config.
- Timeouts on every ssh/scp/render; builds in the background; commit through
  `nix develop -c git commit` (shellcheck/rustfmt hooks).

## The engine holds no library numbers

Everything an instrument does is data: the library spec
(`features/rigs/orchestra/specs/<library>.styx`) and the instrument's patch
folder (`<root>/_patches/<instrument>/library.styx` zones + `section.styx`
numbers). The engine has only mechanisms. When matching a library needs
something the spec cannot express, add the GENERIC mechanism to the engine
(a field with a neutral default) and put the library's values in its spec
— never a library-specific constant, label or branch in engine code. CC58
values are tokens like keyswitch notes (`"Nonvib+@legato-low"`,
`"Marcato+@overlay-on"`, `"@ignore"`). After any engine change, re-run the
strings null suite (`SKIP_KONTAKT=1`) and compare every unit with the
previous `null.json`s: a refactor must leave all 50 probes unchanged.

Libraries: `signal_orchestra::LIBRARIES` (strings, woodwinds, brass,
solo-strings); `render_css_live --library woodwinds --section "Solo Flute"`
(patch folder name). `tests/cs_ksp_var_map.tsv` maps each KSP var the strings
generator reads to its CSW / CSB equivalent (or "absent").

## 1. Decode and sync an instrument (offline)

`css_section.sh "<Section>"` (strings) or `css_section.sh "CSW Solo Flute"` /
`"CSB Solo Trumpet"` (full NKI name; `DECODE_ONLY=1` decodes only):
1. decodes `CSS <Section>.nki` (nkx-extract `nki --zones`) into
   `_ksp/CSS <Section>/`: zones.tsv **with the `group` column**, groups.json,
   KSP scripts, persistent values;
2. `signalpack zones sync-tsv --disable-unmatched` → `_patches/<Section>/library.styx`;
3. `css_section_spec.py` → `_patches/<Section>/section.styx`, the overlay of
   numbers that differ per section (velocity tables, RR, group curves, editor knobs).

Zones come from **Kontakt groups, not sample folders**: a group can play
another articulation's recordings (sync-tsv emits them as `borrowed`).
Never hand-edit section numbers into the shared spec: map the KSP knob in the
spec generator and regenerate.

## 2. Factory Kontakt chains (voyager, foreground GUI)

One chain per instrument: `target/orchestra-ab/chains/fxchain_<tag>.txt`
(strings `1v 2v va vc cb`; woodwinds `ww_<tag>`). To make more:
1. On voyager: copy `~/orchestra-ab/sec_2v.rpp` (Kontakt window floating) and
   open it with `REAPER -newinst -nosplash <rpp>`.
2. `cua-driver` (session `kt`, `delivery_mode foreground`; Kontakt ignores
   background clicks). Pick the library in Kontakt's browser, then per
   instrument `~/orchestra-ab/bin/ww_load.sh <desktop_y> <tag>`: window-target
   double-click on the row (desktop double_click is refused), "replace?" **Yes**
   at desktop 958,469, ⌘S, copy to `sec_saved_ww_<tag>.rpp`. Edit the pid /
   window ids at the top of the script for the new instance.
3. Screenshot after each batch; ⌘Q your instance; check the user's REAPER still runs.
4. Here: extract the `<FXCHAIN … >` block of each saved rpp (SHOW → 0) into the
   chains folder. Smoke-test one with `make_ab_rpp.py` + a headless render.

## 3. Null suite

**Run sections in parallel: `tests/null_all.sh <Section>…`** (JOBS=12,
MODES="…"): every (section, mode) pair is its own job. The strings
regression (5 × 10) takes ~6 min instead of ~100.

**Kontakt renders in bulk: `tests/kontakt_batch.py <mode> <tag>:<transpose> …`**
builds one project with a track per instrument and run (r1, r2), renders it
in ONE REAPER as stems (seconds, not minutes per render) and drops each stem
where `null_suite.sh` reads it; then `SKIP_KONTAKT=1 null_suite.sh` scores
without the host. Batch stems are bit-identical to single renders. Keep a
batch to ~40 Kontakt instances (voyager has 32 GB).

`null_suite.sh "<Section>" [modes]` (woodwinds by patch folder: `"Solo Flute"`,
`"Saf"`; transpositions and chain tags `ww_<tag>` are in the script) — probe MIDI (`gen_probe.py`, transposed
into range), two Kontakt renders, ours, `null_test.py`. Random draws (detune,
legato variants, attacks) are fitted with `fit_draws.py` and replayed.
Results: `target/orchestra-ab/sec/<tag>/<mode>/null.json` (read the JSON; the
log is buffered). `SKIP_KONTAKT=1` reuses renders, `BIN=` swaps the renderer.

## 4. Render a score (and A/B with Kontakt)

```bash
KONTAKT=1 features/rigs/orchestra/tests/score_render.sh score.mxl /tmp/out "Song - Quartet" \
  P1="1st Violins":-0.55 P2="2nd Violins":-0.2 P3=Violas:0.2 P4=Cellos:0.5
```

`score_to_css.py` turns each part into CSS MIDI: 2 s pre-roll (CC58=2 legato
sustain, CC1=90 = f-ish sustains and the Staccato short type, CC2=90 vibrato),
ties merged, plain notes detached, tenuto/slurred notes overlapping (legato
transitions), staccato → shorts group (CC58=21). Eighths straight unless
`SWING=0.6`. Mixes are Mix-mic only (the spec default, like the factory patch).
Both engines render ~10–12× realtime per part. Copy results to the user's
`~/Downloads` on voyager when asked to listen. The waveforms won't null on a
song (Kontakt's random draws aren't fitted there); levels should agree within ~0.1 dB.
