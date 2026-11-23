//! Shared ONNX Runtime session loader (`--features onnx`, `ort 2.0.0-rc.15`).
//!
//! Uses `load-dynamic` so builds require zero system OpenSSL/C++ toolchains.
//! At runtime, set `ORT_DYLIB_PATH` to `libonnxruntime.so` (or let `ort::init()`
//! search standard library paths).

use std::path::Path;

use ort::environment::Environment;
use ort::session::{Session, builder::GraphOptimizationLevel};

/// Initialize an ONNX Runtime [`Environment`], honoring `ORT_DYLIB_PATH` if set.
pub fn init_environment() -> Result<Environment, String> {
    if let Ok(dylib) = std::env::var("ORT_DYLIB_PATH")
        && !dylib.trim().is_empty()
    {
        return ort::init_from(&dylib)
            .map_err(|e| format!("ort load dylib '{dylib}': {e}"))?
            .with_name("fts-rhythm")
            .build()
            .map_err(|e| format!("ort init environment failed: {e}"));
    }
    ort::init()
        .with_name("fts-rhythm")
        .build()
        .map_err(|e| format!("ort init environment failed (set ORT_DYLIB_PATH if needed): {e}"))
}

/// Load an ONNX model from disk with Level 3 graph optimization.
pub fn load_session(model_path: impl AsRef<Path>) -> Result<Session, String> {
    let path = model_path.as_ref();
    let env = init_environment()?;
    Session::builder(&env)
        .map_err(|e| format!("ort session builder failed: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| format!("ort optimization level failed: {e}"))?
        .commit_from_file(path)
        .map_err(|e| format!("ort load {}: {e}", path.display()))
}
