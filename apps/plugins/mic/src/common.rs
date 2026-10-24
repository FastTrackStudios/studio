//! Where the models live (the mic list and parameter helpers are
//! `dsp::mic::ui::params`).
//!
//! Installed models (`$FTS_MIC_MODELS`, else `~/.local/share/fts/mic-models`,
//! a checkout of the private `mic-models` repository) are read first, so they
//! can be updated without rebuilding; the default mic is also built in.

use dsp::mic::MicModel;
use std::path::PathBuf;

pub use dsp::mic::ui::params::{MICS, index};

pub fn models_dir() -> PathBuf {
    std::env::var_os("FTS_MIC_MODELS").map_or_else(
        || {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
                .unwrap_or_default()
                .join("fts/mic-models")
        },
        PathBuf::from,
    )
}

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded.rs"));
}

/// The mic built into the plugin (Mic 1 Type's default).
pub const DEFAULT_MIC: usize = 0;

/// `variant` is "" for the forward models, "-180" for the 180's backward ones.
/// An installed file wins; the default mic falls back to its built-in copy.
pub fn load_model_variant(
    mic: usize,
    source: usize,
    sample_rate: u32,
    variant: &str,
) -> Option<MicModel> {
    let name = MICS.get(mic)?.replace(' ', "_");
    let dir = if source == 1 {
        format!("{sample_rate}-LX{variant}")
    } else {
        format!("{sample_rate}{variant}")
    };
    let installed = std::fs::read(models_dir().join(&dir).join(format!("{name}.micm")))
        .ok()
        .and_then(|b| MicModel::from_bytes(&b).ok());
    installed.or_else(|| {
        (mic == DEFAULT_MIC)
            .then(|| embedded::EMBEDDED.iter().find(|(set, _)| *set == dir))
            .flatten()
            .and_then(|(_, bytes)| MicModel::from_bytes(bytes).ok())
    })
}

/// A mic's model, or the default mic's when that one is not installed.
pub fn load_model_or_default(
    mic: usize,
    source: usize,
    sample_rate: u32,
    variant: &str,
) -> Option<MicModel> {
    load_model_variant(mic, source, sample_rate, variant)
        .or_else(|| load_model_variant(DEFAULT_MIC, source, sample_rate, variant))
}
