//! Opening a song: the core's (`engine_core::open`), under its old path.
//!
//! The ways in that stand the engine up install the engine around the core
//! first ([`crate::install`]: the body, the Action List, the core's
//! `assembly`), as they did when they lived here: whichever a binary calls
//! first, the engine it stands up has its domains.

use std::path::Path;

pub use engine_core::open::*;

/// [`engine_core::open::open_and_serve`], the engine installed first.
///
/// # Errors
///
/// As the core's.
pub fn open_and_serve(path: &Path) -> eyre::Result<Opened> {
    crate::install();
    engine_core::open::open_and_serve(path)
}

/// [`engine_core::open::open_first_with`], the engine installed first.
///
/// # Errors
///
/// As the core's.
pub fn open_first_with(path: &Path, audio: bool, media: Media) -> eyre::Result<Opened> {
    crate::install();
    engine_core::open::open_first_with(path, audio, media)
}

/// [`engine_core::open::open_silent`], the engine installed first.
///
/// # Errors
///
/// As the core's.
pub fn open_silent(path: &Path) -> eyre::Result<Opened> {
    crate::install();
    engine_core::open::open_silent(path)
}

/// [`engine_core::open::open_untitled`], the engine installed first.
///
/// # Errors
///
/// As the core's.
pub fn open_untitled(name: &str, audio: Option<bool>) -> eyre::Result<Opened> {
    crate::install();
    engine_core::open::open_untitled(name, audio)
}

/// [`engine_core::open::start_engine`], the engine installed first.
///
/// # Errors
///
/// As the core's.
pub fn start_engine() -> eyre::Result<daw::standalone::Standalone> {
    crate::install();
    engine_core::open::start_engine()
}

/// [`engine_core::open::serve_actions`], the engine installed first.
pub fn serve_actions() {
    crate::install();
    engine_core::open::serve_actions();
}
