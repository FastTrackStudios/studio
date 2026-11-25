//! The process's one ONNX Runtime [`Environment`].
//!
//! ort 2.0.0-rc.15 allows a single environment per process, and a session
//! can only be made in one. Every model in Studio (rhythm-hits' detectors,
//! chatterbox TTS) asks for it here instead of building its own, so they can
//! run side by side in the engine.
//!
//! The runtime is loaded dynamically: from `ORT_DYLIB_PATH` when it is set
//! (the flake sets it), otherwise from the standard library paths.

use std::sync::Mutex;

pub use ort::environment::Environment;

/// The environment's name in ONNX Runtime's logs.
const NAME: &str = "studio";

/// Why the environment could not be made: the runtime library did not load,
/// or ONNX Runtime refused to start.
#[derive(Debug)]
pub struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// The process's environment, made on first use and shared after.
pub fn environment() -> Result<Environment, Error> {
    static ENVIRONMENT: Mutex<Option<Environment>> = Mutex::new(None);
    let mut held = ENVIRONMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(environment) = &*held {
        return Ok(environment.clone());
    }
    let environment = build(std::env::var("ORT_DYLIB_PATH").ok())?;
    *held = Some(environment.clone());
    Ok(environment)
}

fn build(dylib: Option<String>) -> Result<Environment, Error> {
    let builder = match dylib.filter(|path| !path.trim().is_empty()) {
        Some(path) => ort::init_from(&path).map_err(|e| Error(format!("load ONNX Runtime from {path}: {e}")))?,
        None => ort::init(),
    };
    builder.with_name(NAME).build().map_err(|e| Error(format!("start ONNX Runtime: {e}")))
}
