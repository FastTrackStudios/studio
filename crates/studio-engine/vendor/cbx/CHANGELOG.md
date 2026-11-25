# Changelog

All notable changes to this project will be documented in this file.

## Vendored (FastTrackStudio Studio, 2026-11-25)

Copied from upstream 65093a0 into `crates/studio-engine/vendor/cbx` and
ported from ort 2.0.0-rc.10 to 2.0.0-rc.15 (ndarray 0.16 to 0.17):

- Sessions are made in an explicit ONNX Runtime `Environment` (rc.15 wants
  one per process, handed to `Session::builder`): `SessionConfig::environment`
  takes one a program already has; otherwise `chatterbox::environment()` makes
  one on first use (from `ORT_DYLIB_PATH` with the new `load-dynamic` feature).
  The CLI holds it for its run instead of `ort::init().commit()`.
- `ort::session::Input` is `ort::value::Outlet`: `name()`, `dtype()`, and
  `Session::inputs()` / `outputs()` are methods.
- `ort::tensor::TensorElementType` is `ort::value::TensorElementType`.
- Execution providers moved to `ort::ep` (`CPU`, `CoreML`, `CUDA`, `DirectML`;
  CoreML's `ComputeUnits`, `ModelFormat`, `SpecializationStrategy`), and
  `is_available()` is an associated function.
- A kept value fed to a session again is copied (a tensor's `to` its own
  device): rc.15's `ValueRef::try_upgrade` refuses while the original lives.
- Session-builder errors carry the builder (not `Send`), so they are turned
  into `ort::Error` before `?`.
- Dependencies come from the Studio workspace where it has them; no lockfile,
  scripts or packaging.

## [0.1.0] - 2024-12-18

### Initial Release

First public release of cbx, a single-binary CLI for local text-to-speech using Chatterbox.

#### Features

- **Text-to-speech generation** - Generate speech from text using Resemble AI's Chatterbox ONNX models
- **Built-in model management** - Download, list, and clean up model files with simple commands
- **Voice profile caching** - Encode a reference voice once and reuse it across runs
- **Multiple model variants** - Support for fp16, fp32, quantized, q4, and q4f16 variants
- **Cross-platform support** - macOS, Linux, and Windows binaries
- **Platform acceleration** - Optional CoreML (macOS), DirectML (Windows), and CUDA (Linux) support
- **Default voice** - Optional bundled default voice for quick testing

#### Commands

- `cbx speak` - Generate speech from text
- `cbx download` - Pre-download model files
- `cbx sizes` - Show download sizes for each model variant
- `cbx models` - List cached models
- `cbx voice add` - Create a cached voice profile
- `cbx voice list` - List cached voice profiles
- `cbx voice remove` - Delete a voice profile
- `cbx clean` - Remove cached models
