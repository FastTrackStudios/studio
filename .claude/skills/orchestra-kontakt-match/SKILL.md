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
- Timeouts on every ssh/scp/render; builds in the background; commit through
  `nix develop -c git commit` (shellcheck/rustfmt hooks).

## 1. Decode and sync an instrument (offline)

`css_section.sh "<Section>"`:
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

`null_suite.sh "<Section>" [modes]` — probe MIDI (`gen_probe.py`, transposed
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
