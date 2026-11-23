# Local open-model pilot

The local fixtures and raw outputs live under `target/`. Reference audio,
trigger timelines, model weights, and separated stems stay outside git.

## Crescendum kit

Use the aligned active-comp exports from Rhythm. The [Rhythm benchmark
guide](https://github.com/FastTrackStudios/rhythm/blob/main/BENCHMARKING.md)
documents the REAPER fixed-lane rendering and trigger health checks. The
pilot uses a 12-second excerpt selected from healthy kick, snare and tom
trigger counts. The reference mix sums kick-in, snare-top, four tom mics,
hi-hat and ride, resampled from 48 kHz mono exports to dual-mono 44.1 kHz.
These are close-mic proxies that can contain bleed. The ride channel is the
only cymbal proxy. Overheads and room mics contain multiple kit pieces and
are outside this reference mix.

```sh
.venv/bin/python bench/prepare_crescendum.py \
  "$HOME/.cache/fts-rhythm/crescendum-stems/the plague" target/crescendum/reference
```

Run [LarsNet](https://github.com/polimi-ispl/larsnet) on a directory with the
fixture mix named `crescendum.wav`. Use its CPU default with Wiener filtering
disabled and write to `target/crescendum/larsnet`. The upstream weights
archive used in the pilot has SHA-256
`dd785d77f66413c5acbd3a4cedac02951578c838496d059930c43f159647c907`.

The six-stem DrumSep checkpoint is absent from the audio-separator 0.48.0
catalog. The explicit MDXC runner uses separately supplied checkpoint and
YAML files. This run used the checkpoint at
[mirror revision 3b391204](https://huggingface.co/lainlives/audio-separator-models/tree/3b39120409f3c2d50e9cc4c391f4169131e9d643).
Its SHA-256 is
`d2a4aa53eb584d21eead358a4e66d1882ad182911be018f052b5da73be9096d0`, which
matches the mirror's LFS object. The legacy fts-fx registry records a
different hash. Keep results identified by the exact checkpoint used.

```sh
python bench/separate_mdxc.py target/crescendum/reference/mix.wav \
  CHECKPOINT.ckpt CONFIG.yaml target/crescendum/drumsep
.venv/bin/python bench/pilot_manifests.py target/crescendum crescendum
cargo run --locked -- bench score target/crescendum/kit.json \
  --output target/crescendum/scores --python .venv/bin/python
python bench/score_trigger_onsets.py target/crescendum/kit.json target/crescendum/onsets.json
```

The inference environment used Python 3.12, CPU PyTorch and TorchAudio
2.5.1, audio-separator 0.48.0, and librosa 1.0.1 for onset detection. Enable
FFmpeg and ffprobe on PATH. The timing test applies one audio-only onset
detector to the separator outputs, unseparated mix, and source close mics.
Healthy triggers provide labels at ±20 ms and ±50 ms. A missing or unhealthy
trigger excludes its class from the timing comparison. Cymbal and hi-hat
waveforms are scored against the mic proxies only.

## Full song and finer stems

Build the 75–95 second excerpt of Patrick Talbot's “A Reason To Leave” from
the local Cambridge multitracks. The manifest records selected channels.
The piano class is electric piano. Rhodes and vibraphone remain in other.

```sh
.venv/bin/python bench/prepare_pilot.py CAMBRIDGE_ROOT \
  target/song-pilot/references --fixture patrick-song --start 75
```

Run `stem-seperation demucs` with `htdemucs` and `htdemucs_6s` on
`target/song-pilot/references/patrick-song/mix.wav`. Set the output
directories to `target/song-pilot/htdemucs` and `target/song-pilot/htdemucs_6s`.
Generate the `four` and `six` manifests with `bench/pilot_manifests.py`.
Score them separately. Four-stem scoring sums guitar and piano into other
for the six-stem model. Six-stem scoring assesses those classes directly.
Pass `--global-only` to `bench score` for a fast waveform score without BSS
Eval.

### Additional four-stem candidates

Run BS-RoFormer on the same input with `bench/separate_mdxc.py`. Use the
`model_bs_roformer_ep_17_sdr_9.6568.ckpt` checkpoint and
`config_bs_roformer_384_8_2_485100.yaml` from the [MSST v1.0.12
release](https://github.com/ZFTurbo/Music-Source-Separation-Training/releases/tag/v1.0.12).
Write to `target/song-pilot/bs-roformer`.

For [SCNet](https://github.com/starrytong/SCNet), use the author's small
checkpoint and `python -m scnet.inference` with its default configuration.
Point `--input_dir` to a directory containing `patrick-song/mixture.wav`
(the same input mix) and `--output_dir` to `target/song-pilot/scnet`. The CPU
pilot used Torch 2.5.1 and Accelerate 1.2.1. The manifest records the source
archive and checkpoint hashes.

Regenerate the four-stem manifest with `--bs-roformer --scnet` once both
exports are available. Score all candidates together.

## Kit cascade with accompaniment

Re-use the “the plague” drum references and add the Cambridge song's
non-drum stems at equal RMS. This measures controlled cross-song
interference. It does not simulate the original Crescendum full mix.

```sh
.venv/bin/python bench/prepare_cascade.py target/crescendum/reference \
  target/song-pilot/references/patrick-song target/cascade
```

Run HTDemucs on `target/cascade/mix.wav` into `target/cascade/extracted`.
Pass `target/cascade/extracted/htdemucs/mix/drums.wav` to both kit models
instead of the clean kit bus. Use `target/cascade/larsnet` and
`target/cascade/drumsep` as output directories and name the LarsNet input
`crescendum.wav`.

```sh
.venv/bin/python bench/compare_cascade.py target/crescendum/kit.json \
  target/cascade --gain 1
cargo run --locked -- bench score target/cascade/paired.json \
  --output target/cascade/scores --global-only --python .venv/bin/python
python bench/score_trigger_onsets.py target/cascade/paired.json target/cascade/onsets.json
```

Use the `mix_gain` from `target/cascade/mixture.json` in place of 1 when
peak protection reduced the mix. The paired manifest applies the equivalent
gain to the clean references and estimates. Trigger scoring includes the
unseparated contaminated mix and HTDemucs drum bus as additional baselines.

## Lead and backing vocals

Prepare seconds 95–115 of “A Reason To Leave” with
`bench/prepare_pilot.py CAMBRIDGE_ROOT target/vocal-pilot/references --fixture patrick-vocals --start 95`.
This reference vocal bus groups the lead double with the lead and the five
backing takes together. Run `UVR_MDXNET_KARA.onnx` and
`UVR_MDXNET_KARA_2.onnx` with the audio-separator CLI adapter, each into its
own directory under `target/vocal-pilot` named after the checkpoint without
`.onnx`. Generate the manifest with `bench/pilot_manifests.py target/vocal-pilot vocals`.
The manifest maps the models' Vocals and Instrumental labels to lead and
backing in this voice-only test. Use the CPU TorchVision build that matches
the installed CPU PyTorch build.

## Fixed ensemble and reconstruction

```sh
.venv/bin/python bench/mean_ensemble.py target/song-pilot/four.json scnet bs-roformer
cargo run --locked -- bench score target/song-pilot/ensemble.json \
  --output target/song-pilot/scores-ensemble --global-only --python .venv/bin/python
```

The ensemble averages corresponding waveforms with equal weights. Its
separate reconstruction report measures residual energy after summing the
stems, relative to the reference mix energy. Use the per-stem scores to
assess where the audio was assigned.

## Six-stem and kit checkpoint comparisons

For the matched six-stem test, run the [BS-RoFormer SW checkpoint and YAML
mirror](https://huggingface.co/koidn/BS-RoFormer-SW-6stem) through
`bench/separate_mdxc.py` on the same `patrick-song/mix.wav` used for
HTDemucs six-stem. Write to `target/song-pilot/bs-roformer-six` and generate
the manifest with `bench/pilot_manifests.py target/song-pilot six --bs-roformer-six`.
Score `six-matched.json`. The mirror records unknown weight terms. The
checkpoint is for local evaluation, with redistribution status unresolved.

For the third kit candidate, obtain `drumsep_5stems_mdx23c_jarredou.ckpt`
and `config_mdx23c_drumsep2025.yaml` from the [DrumSep
mirror](https://huggingface.co/xavriley/source_separation_mirror). Run the
same MDXC runner on each unchanged Crescendum reference mix, writing to
`drumsep-five` under each pilot directory. Generate
`kit-three-models.json` with `bench/pilot_manifests.py RUN_DIR crescendum --drumsep-five`
and use it for waveform and trigger scoring. This five-stem checkpoint
provides cymbals as a single output. The six-stem checkpoint uses
ride plus crash for that same comparison group. The reference is still
the ride mic, so this score is a waveform proxy for the cymbal group.

To compare multiple songs, pass their separate score reports to
`bench/paired_summary.py` with `--baseline drumsep` and `--output FILE.json`.
The summary computes per-track score differences before taking the median
and reports how many tracks were available for each pair. It rejects
duplicate rows, differing protocols, and changed system metadata. Run
global-only and full BSS Eval summaries on their respective reports.

The second six-stem fixture is seconds 40–60 of Angela Thomas Wade's “Milk
Cow Blues,” with acoustic piano, a grouped guitar stem, and fiddle as
other. Prepare it with
`bench/prepare_pilot.py CAMBRIDGE_ROOT target/acoustic-pilot/references --fixture angela-song --start 40`.
Use `target/acoustic-pilot/references/angela-song/mix.wav` for both models.
Keep the same output directory names under `target/acoustic-pilot` and
pass `--track angela-song` when generating the matched manifest.
