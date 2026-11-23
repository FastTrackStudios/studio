# stem-seperation

Music stem separation through a Rust library and CLI. The initial backends run
locally through [audio-separator](https://github.com/nomadkaraoke/python-audio-separator)
and [Demucs](https://github.com/facebookresearch/demucs).

## Run it

Install Rust, uv, Python 3.12 and FFmpeg, or enter the supplied environment with
`nix develop`. Then install the backend you want to use.

```sh
uv tool install --python 3.12 'audio-separator[cpu]==0.48.0' --with audioread
cargo run --locked -- models audio-separator
cargo run --locked -- audio-separator mix.wav --model model_bs_roformer_ep_317_sdr_12.9755.ckpt --output vocals-run
```

Use `audio-separator[gpu]` in place of the CPU extra for a compatible NVIDIA
setup. Choose a model filename from the installed catalog. The catalog covers
RoFormer, MDX and other model families, and first use can download weights.
Set `--model-dir` to reuse a cache or point at locally supplied checkpoints.
Use `--single-stem Vocals` to request one output from a compatible model.

```sh
uv tool install --python 3.12 'demucs==4.0.1' --with 'torch==2.8.0' --with 'torchaudio==2.8.0' --with 'torchcodec==0.6.0'
cargo run --locked -- models demucs
cargo run --locked -- demucs mix.wav --model htdemucs --device cpu --output demucs-run
```

Demucs supports alternate models such as `htdemucs_ft` and `htdemucs_6s`
and a stem-plus-complement mode such as `--two-stems vocals`.
Use `--executable` to select an executable outside PATH. Each job requires a
fresh output directory and prints its WAV paths on success.

## Embed it

Use the library from the CLI package. Run this blocking, file-based API on a
worker thread in applications with a UI or audio callback.

```rust
use std::{ffi::OsStr, path::Path};
use stem_seperation::{Backend, Job};

fn separate_mix() -> std::io::Result<Vec<std::path::PathBuf>> {
    let backend = Backend::Demucs {
        device: Some("cpu".into()),
        two_stems: None,
    };
    backend.separate(OsStr::new(backend.executable()), &Job {
        input: Path::new("mix.wav"),
        output_dir: Path::new("demucs-run"),
        model: "htdemucs",
    })
}
```

Run `cargo test --locked` to verify the adapter behavior. A quality comparison
needs the same source mix and aligned reference stems for each candidate, with
per-stem scores and runtime recorded alongside the backend and model versions.

## License

GPL-3.0-or-later. The selected backend and model weights have their own terms.
