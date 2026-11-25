//! Shared ONNX Runtime session loader (`--features onnx`, `ort 2.0.0-rc.15`).
//!
//! Uses `load-dynamic` so builds require zero system OpenSSL/C++ toolchains.
//! At runtime, set `ORT_DYLIB_PATH` to `libonnxruntime.so` (or let `ort::init()`
//! search standard library paths).

use std::path::Path;
use std::sync::Mutex;

use ort::environment::Environment;
use ort::session::{Session, builder::GraphOptimizationLevel};

/// Initialize an ONNX Runtime [`Environment`], honoring `ORT_DYLIB_PATH` if set.
pub fn init_environment() -> Result<Environment, String> {
    static ENVIRONMENT: Mutex<Option<Environment>> = Mutex::new(None);
    let mut environment = ENVIRONMENT.lock().map_err(|e| e.to_string())?;
    if let Some(existing) = &*environment {
        return Ok(existing.clone());
    }
    let initialized = build_environment()?;
    *environment = Some(initialized.clone());
    Ok(initialized)
}

fn build_environment() -> Result<Environment, String> {
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

pub(crate) fn resample_mono(signal: &[f32], rate: f64, target_rate: f64) -> Vec<f32> {
    let step = rate / target_rate;
    (0..((signal.len() as f64 / step).round() as usize).max(2048))
        .map(|i| {
            let pos = i as f64 * step;
            let left = pos as usize;
            let a = signal.get(left).copied().unwrap_or(0.0);
            let b = signal.get(left + 1).copied().unwrap_or(0.0);
            a + (b - a) * pos.fract() as f32
        })
        .collect()
}
