//! Engine actions run off the async thread that asked for them: the core's
//! [`engine_core::off_loop`], re-exported under its old path. Its test
//! serves the engine's view service (`studio_engine::view_host`), so it
//! stays with the assembly.

pub use engine_core::off_loop::{blocking, run, run_by};

#[cfg(test)]
mod tests;
