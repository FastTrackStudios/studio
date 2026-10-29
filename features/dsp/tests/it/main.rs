//! `dsp`'s integration binary: output pinned against reference vectors
//! (`tests/golden/`, the `dsp_core::golden` harness) and against exact
//! references, one module per effect.

mod mic;
mod reverb;
