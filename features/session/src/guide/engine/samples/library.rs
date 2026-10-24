//! A guide library that is not a folder on this machine: the FTS-GUIDE
//! layout (`Click/`, `Counts/`, `Guide/`) as bytes fetched from wherever it
//! is kept — how a browser plays the click, the count and the cues.
//!
//! MIDI-mode playback needs a fixed set of files: one click kit's four,
//! one voice's eight counts, and one cue per section type the guide's MIDI
//! layout can name. [`files`] lists them, so a player fetches exactly those
//! (a fraction of the library) and [`SampleBank::load_bytes`] loads them
//! under the keys the folder loaders use.
//!
//! The library streams as Ogg Vorbis — the same layout, each `.wav` a
//! `.ogg` beside where it would be ([`files`] with `"ogg"`), a tenth of the
//! size. The names inside are the WAV library's; only the extension moves.

use std::collections::HashMap;
use std::path::Path;

use tracing::warn;

use super::bank::filename_to_key;
use super::{AudioSample, ClickSamplePaths, ClickSound, SampleBank};
use crate::guide::engine::GuideError;

/// The notes the guide's MIDI layout names sections by.
const SECTION_NOTES: std::ops::RangeInclusive<u8> = 84..=102;

/// A path relative to the library root, `/`-separated.
fn relative(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// The four click files of `click`, relative to the library root.
fn click_files(click: ClickSound) -> [String; 4] {
    let paths = ClickSamplePaths::for_sound(Path::new("Click"), click);
    [
        relative(&paths.beat_path),
        relative(&paths.eighth_path),
        relative(&paths.sixteenth_path),
        relative(&paths.accent_path),
    ]
}

/// Every file MIDI-mode playback can ask for with this click kit and
/// voice, relative to the library root, with extension `ext` (`"wav"` for
/// the library as recorded, `"ogg"` for the one a browser streams).
#[must_use]
pub fn files(click: ClickSound, voice: &str, ext: &str) -> Vec<String> {
    let mut out: Vec<String> = click_files(click).to_vec();
    out.extend((1..=8).map(|n| format!("Counts/{voice} - {n}.wav")));
    // A cue file is named by the same spelling its trigger's key is built
    // from (`get_guide_key`), so the two cannot disagree.
    for note in SECTION_NOTES {
        if let Some(section) = crate::guide::engine::midi::section_for_midi_note(note) {
            let name = super::bank::sample_name(&section.full_name());
            out.push(format!("Guide/{voice} - {name}.wav"));
        }
    }
    for path in &mut out {
        *path = with_ext(path, ext);
    }
    out.sort();
    out.dedup();
    out
}

/// `path` with its extension replaced by `ext`.
fn with_ext(path: &str, ext: &str) -> String {
    let stem = path.rsplit_once('.').map_or(path, |(stem, _)| stem);
    format!("{stem}.{ext}")
}

/// Decode one sample from its bytes (`ext` hints the format), at
/// `sample_rate`.
///
/// # Errors
///
/// The bytes are not audio symphonium decodes.
pub fn sample_from_bytes(
    bytes: &[u8],
    ext: &str,
    sample_rate: u32,
) -> Result<AudioSample, GuideError> {
    let loaded = fts_sample::decode_bytes_at(
        bytes,
        Some(ext),
        Some(sample_rate),
        fts_sample::ResampleQuality::Low,
    )
    .map_err(|e| GuideError::SampleLoad(e.to_string()))?;
    Ok(AudioSample {
        data: loaded.channels,
        sample_rate: loaded.sample_rate,
    })
}

impl SampleBank {
    /// Load the click kit, the counts and the section cues from `files`
    /// (library-relative path → bytes, as [`files`] names them, in any one
    /// format `ext`), the way `load_click` / `load_counts` /
    /// `load_guide_dir` load them from a folder. Anything missing stays
    /// empty for `synthesize_defaults`.
    pub fn load_bytes(
        &mut self,
        files: &HashMap<String, Vec<u8>>,
        ext: &str,
        click: ClickSound,
        voice: &str,
        sample_rate: u32,
    ) {
        let load = |path: &str| -> Option<AudioSample> {
            let path = with_ext(path, ext);
            let bytes = files.get(&path)?;
            sample_from_bytes(bytes, ext, sample_rate)
                .inspect_err(|e| warn!(path, error = %e, "guide sample did not decode"))
                .ok()
        };
        let [beat, eighth, sixteenth, accent] = click_files(click);
        self.beat = load(&beat);
        self.eighth = load(&eighth);
        self.sixteenth = load(&sixteenth);
        self.triplet = self.eighth.clone();
        self.measure_accent = load(&accent);
        for (i, slot) in self.counts.iter_mut().enumerate() {
            *slot = load(&format!("Counts/{voice} - {}.wav", i + 1));
        }
        for (path, _) in files.iter().filter(|(p, _)| p.starts_with("Guide/")) {
            // Keyed by the recorded (`.wav`) name, as the folder loader does.
            let name = with_ext(path.trim_start_matches("Guide/"), "wav");
            if let Some(sample) = load(path) {
                self.guides.insert(filename_to_key(&name), sample);
            }
        }
    }
}
