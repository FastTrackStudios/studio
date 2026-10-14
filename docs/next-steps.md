# Next steps (handoff, 2026-10-13)

Where the studio reorganisation and the one-engine work stand, and what
comes next, in order. The design docs this points at are in `docs/design/`.

## Done

- **Step 1 — the feature layout** (`11e12c7`): `features/signal` (Signal's
  domain) vs `features/rigs` (the instruments, `rigs/host`, `rigs/proto`);
  `adapters/<host>` (`reaper`, `protools`, `ableton`, `logic`,
  `interchange`, `web`); the daw's own file is `crates/daw/session-format`;
  the REAPER overlay forks are merged (`reaper-dioxus`, `reaper-embed`);
  `expression-editor` is `editor` (folders `model/ pitch/ drums/ harmony/
  interact/`, `chords` folded into `harmony/chords`).
- One copy of the app shell, loudness and tempo; the sampler's DrumMixer is
  gone; legacy/ folded in (`6f0a34c`).

## Known red

- `just ci` wasm step (`signal-desktop` for wasm32): architect pulls tokio
  `net` → mio, which does not build for the browser. Pre-existing.
- `reaper-rs` and `reaper-lib` are still git deps on Codeberg: GitHub's
  `reaper-rs` branch lacks the locked commit `3a9e7063`, and `reaper-lib`
  has no GitHub repo yet.

## Step 2 — one editing engine: `timeline`

*In progress:* migration steps 1–15 of 17 are done (rename, camera
characterisation tests, input, time, grid, snap, the shared camera in
both surfaces, rows, zoom, cursor/selection, the mouse map, hit testing and clicks; the one `Interaction<S>`
engine is not built, see §4; undo is the song's Loro doc; one clipboard). Next is 16, scroll
and touch. `docs/design/timeline.md` §4 records where the code differs
from the plan.

The arrangement and the editor must share the SAME mouse handling, zoom,
camera, grid, cursor, razor, tools, selection, clipboard and undo logic.
`features/editing` becomes `timeline`, renderer-free and generic over rows
and items. Design and a 17-step migration (each step compiles, each names
the tests that pin it): **`docs/design/timeline.md`**.

Bugs it found, fixed along the way: the arrangement snaps to quarter notes
at one constant tempo (`arrange_edit.rs`, three copies) and its lane grid
is 4/4 at one tempo; the editor's `Grid::refit` only agrees with
`Adaptive::fit` in 4/4; the editor roll never produces clicks (Shift-click
bindings are dead); two zoom drags zoom opposite ways; razor/gesture ops
push empty undo steps; `interact/zoom.rs` SmartZoom duplicates MeMagic.

**Decided 2026-10-13** (§5 of the doc records the answers): empty-ground
Ctrl razor; per-surface move snap; Shift+click adds and Shift+drag is
one-axis; down zooms in; the embedded Editor undoes through Loro;
`timeline::wheel` with an optional table override. The questions were:
1. Ctrl-drag on an item/note body: razor (arrangement) or velocity (editor)?
2. Moving an item: snap its start, or snap the move distance?
3. Shift on a body: one-axis, add to selection, or ignore snap?
4. Zoom-drag vertical: down zooms in (REAPER) or up?
5. The editor inside Session: its undo steps in Session's Loro undo?
6. Standalone editor wheel: keep the keymap scroll table or retire it?

## Step 3 — split `session-daw` into features

`apps/session/daw` is ~139k lines in 156 modules; 115 of them are one
strongly connected component, so it cannot be cut as-is. The measured
graph and a checked cut (118 symbol moves + 40 references turned into
hooks/traits → an acyclic crate graph, adding `session-daw-core`, `timeline`
and `skin` at the bottom): **`docs/design/session-daw-split/`**
(`proposal.txt` first). Do step 2 first — most cross-group references go
through the editing machinery. Notable: ~2.6k lines of `actions.rs` are the
drum quantize engine (belongs in drum-editing); actions should register per
feature crate.

## Step 4 — rigs

1. **One rig service**: every rig repeats RigCore's start/stop/MIDI
   ports/recent MIDI (and keys/synth their preset lists). Grow RigCore so
   nothing is lost (typed `MidiEvent`, optional kind/scope/user/path on a
   preset), mount it on the in-process and browser routers, drop the
   copies, move the UIs onto `RigCoreClient` (scoped by `Rig::slug()`).
   Guitar keeps `patches` (richer) and its `presets` (the capture pool).
2. **Untangle `KeysRig`**: it is the generic composition-tree host the synth
   rig also runs on; pull it out so `synth` + `synth-rig` fold into one crate
   (today that fold is a cycle: keys → synth, synth-rig → keys).
3. **Session hosts rigs as tracks** (decided; no backward compatibility —
   update the few examples by hand): bass, guitar and drums mixed through
   Signal rigs in Session's own project (`docs/design/one-engine.md`, step 5).
   - `Engine::on` needs the device sample rate (rigs fail with "engine has no
     device" on Session's engine today); per-song engines, not the global
     `install_shared` (a setlist switches songs).
   - Guitar/bass: the rig's FX slots on the instrument's `Amp` track, fed by
     its `DI`; a control slot carries the Patch parameter so per-section tone
     automation still works.
   - Drums, maximal: `Kick/Sum/{In, Out, Sample/{In, Out}}` — the acoustic
     mics and the Signal kit's sampled mics summed per piece; without a
     recorded kit the Sample folder goes and the kit's tracks are the piece's
     mics. The hit-marker notes drive the rig.
   - Delete signal-fx's `FTS Bass Rig`, `FTS Kit Mic`, `FTS Signal Rig` and
     their global registry; rewrite session-daw's `bass_rig`,
     `drum_samples`, `rig_actions` and their tests on the rigs; the golden
     template gets the Sample folders.

## Duplicates still to merge (read both, keep the better)

`dsp/hits` vs `dsp/trigger` (two hit detectors); `ui/audio-controls` vs
`ui/fts-audio-ui` (two widget kits); `preset-browser` vs `signal/browser`;
`song` into `session`. The editor's `harmony/chords/panel.rs`
`include_str!`s the Session app's Tailwind sheet — a library reaching into
an app; give it its own sheet.

## Working notes

- `cargo nextest run --workspace --features
  session-desktop/cli,signal-desktop/cli,web-audio-worklet/guitar,web-audio-worklet/keys`
  is the full run (~5 min after the build). Most of the wall clock is
  compiling ~790 test binaries; iterate with `-p <crate>`.
- ~50 tests fail on a fresh checkout without the private assets
  (`just assets`).
