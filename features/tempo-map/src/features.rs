//! What is measured from a song's stems, once: each stem's level, its
//! onset envelope and hits, and a coarse peak and energy envelope.
//!
//! Everything after this (beats, the phase check, bar 1, the map, the
//! evaluation) works from these and never opens the audio again.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::audio::{self, AudioError, RATE};
use crate::onset::{self, Hit};

/// The hop of the beat envelope, samples.
pub const HOP: usize = 128;
/// The hop of the hit envelope and of the peak and energy envelopes.
pub const FINE_HOP: usize = 64;

/// The kit pieces, in the order their hits are merged.
pub const KIT: [&str; 6] = ["Kick", "Snare", "Hi-Hat", "Toms", "Cymbals", "Drums Other"];

/// The stems that carry the beat and how much each counts: every kit
/// piece (so a beat carried by hats, or by toms, counts as one carried by
/// the kick), and the bass at half.
pub const WEIGHTS: [(&str, f64); 7] = [
    ("Kick", 1.0),
    ("Snare", 1.0),
    ("Hi-Hat", 1.0),
    ("Toms", 1.0),
    ("Cymbals", 0.6),
    ("Drums Other", 0.6),
    ("Bass", 0.5),
];

/// A stem quieter than this is left out, dB RMS.
pub const SILENT_DB: f64 = -60.0;

/// One stem, measured.
#[derive(Debug, Clone)]
pub struct Stem {
    pub name: String,
    pub path: PathBuf,
    /// Its level over the whole song, dB RMS.
    pub rms_db: f64,
    /// Its onset envelope at [`HOP`], normalised: the stems of [`WEIGHTS`]
    /// that are audible.
    pub envelope: Option<Vec<f32>>,
    /// Its hits, from its envelope at [`FINE_HOP`]: the audible kit pieces.
    pub hits: Vec<Hit>,
    /// The largest absolute sample of each [`FINE_HOP`] block.
    pub peak: Vec<f32>,
    /// The mean square of each [`FINE_HOP`] block.
    pub energy: Vec<f32>,
}

impl Stem {
    /// Whether it is loud enough to count.
    #[must_use]
    pub fn audible(&self) -> bool {
        self.rms_db >= SILENT_DB
    }

    /// The largest absolute sample from `from` to `to`, seconds.
    #[must_use]
    pub fn peak_between(&self, from: f64, to: f64) -> f32 {
        let (a, b) = blocks(from, to, self.peak.len());
        self.peak
            .get(a..b)
            .map_or(0.0, |s| s.iter().copied().fold(0.0_f32, f32::max))
    }

    /// The RMS level from `from` to `to`, seconds (linear).
    #[must_use]
    pub fn rms_between(&self, from: f64, to: f64) -> f64 {
        let (a, b) = blocks(from, to, self.energy.len());
        self.energy.get(a..b).map_or(0.0, |s| {
            if s.is_empty() {
                0.0
            } else {
                (s.iter().map(|v| f64::from(*v)).sum::<f64>() / s.len() as f64).sqrt()
            }
        })
    }

    /// The tallest point of its beat envelope from `from` to `to`, seconds.
    #[must_use]
    pub fn onset_between(&self, from: f64, to: f64) -> f32 {
        let Some(env) = &self.envelope else {
            return 0.0;
        };
        let scale = f64::from(RATE) / HOP as f64;
        let a = ((from.max(0.0) * scale) as usize).min(env.len());
        let b = ((to.max(0.0) * scale).ceil() as usize).clamp(a, env.len());
        env.get(a..b)
            .map_or(0.0, |s| s.iter().copied().fold(0.0_f32, f32::max))
    }
}

/// The block range of a span of seconds, clamped to `len` blocks.
fn blocks(from: f64, to: f64, len: usize) -> (usize, usize) {
    let scale = f64::from(RATE) / FINE_HOP as f64;
    let a = ((from.max(0.0) * scale) as usize).min(len);
    let b = ((to.max(0.0) * scale).ceil() as usize).clamp(a, len);
    (a, b)
}

/// A song's stems, measured.
#[derive(Debug, Clone)]
pub struct Features {
    /// The folder they came from.
    pub folder: PathBuf,
    /// The mix's length, seconds: how long the song is.
    pub duration: f64,
    pub stems: BTreeMap<String, Stem>,
}

impl Features {
    #[must_use]
    pub fn stem(&self, name: &str) -> Option<&Stem> {
        self.stems.get(name)
    }

    /// The hits of every audible kit piece, in time order: each with its
    /// piece, and whether it is a cymbal or the kit's residue (those snap
    /// only from close by).
    #[must_use]
    pub fn kit_hits(&self) -> Vec<KitHit> {
        let mut all: Vec<KitHit> = KIT
            .iter()
            .filter_map(|name| self.stems.get(*name))
            .filter(|stem| stem.audible())
            .flat_map(|stem| {
                let loose = matches!(stem.name.as_str(), "Cymbals" | "Drums Other");
                stem.hits.iter().map(move |hit| KitHit {
                    time: hit.time,
                    strength: hit.strength,
                    piece: stem.name.clone(),
                    loose,
                })
            })
            .collect();
        // As Python sorts the tuples: time, then strength, then name.
        all.sort_by(|a, b| {
            a.time
                .total_cmp(&b.time)
                .then(a.strength.total_cmp(&b.strength))
                .then_with(|| a.piece.cmp(&b.piece))
        });
        all
    }
}

/// A hit of the kit, with the piece that made it.
#[derive(Debug, Clone, PartialEq)]
pub struct KitHit {
    pub time: f64,
    pub strength: f32,
    pub piece: String,
    /// A cymbal or the kit's residue: less exact in time.
    pub loose: bool,
}

/// The stems in `folder`: every `<anything> (<Name>).wav`, by name.
///
/// # Errors
///
/// A folder that cannot be read.
pub fn find_stems(folder: &Path) -> Result<BTreeMap<String, PathBuf>, AudioError> {
    let entries =
        std::fs::read_dir(folder).map_err(|e| AudioError(format!("{}: {e}", folder.display())))?;
    let mut stems = BTreeMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(stem) = file
            .strip_suffix(".wav")
            .or_else(|| file.strip_suffix(".WAV"))
        else {
            continue;
        };
        let Some(stem) = stem.strip_suffix(')') else {
            continue;
        };
        let Some((_, name)) = stem.rsplit_once('(') else {
            continue;
        };
        stems.insert(name.to_owned(), path);
    }
    Ok(stems)
}

/// Measures one stem.
fn measure(name: &str, path: &Path) -> Result<Stem, AudioError> {
    let audio = audio::load(path)?;
    let rms_db = audio.rms_db();
    let peak: Vec<f32> = audio
        .samples
        .par_chunks(FINE_HOP)
        .map(|c| c.iter().fold(0.0_f32, |m, s| m.max(s.abs())))
        .collect();
    let energy: Vec<f32> = audio
        .samples
        .par_chunks(FINE_HOP)
        .map(|c| c.iter().map(|s| s * s).sum::<f32>() / c.len().max(1) as f32)
        .collect();
    let in_beat = WEIGHTS.iter().any(|(n, _)| *n == name);
    let in_kit = KIT.contains(&name);
    let (envelope, hits) = if rms_db < SILENT_DB || !in_beat {
        (None, Vec::new())
    } else if in_kit {
        let mel = onset::mel_db(&audio.samples, RATE, FINE_HOP);
        let mut coarse = onset::strength(&mel, FINE_HOP, 2);
        onset::normalise(&mut coarse);
        let mut fine = onset::strength(&mel, FINE_HOP, 1);
        onset::normalise(&mut fine);
        (Some(coarse), onset::hits(&fine, RATE, FINE_HOP))
    } else {
        let mel = onset::mel_db(&audio.samples, RATE, HOP);
        let mut coarse = onset::strength(&mel, HOP, 1);
        onset::normalise(&mut coarse);
        (Some(coarse), Vec::new())
    };
    Ok(Stem {
        name: name.to_owned(),
        path: path.to_owned(),
        rms_db,
        envelope,
        hits,
        peak,
        energy,
    })
}

/// Measures every stem in `folder`.
///
/// # Errors
///
/// A folder with no stems, no `(Mix)`, or a stem that cannot be read.
pub fn extract(folder: &Path) -> Result<Features, AudioError> {
    let paths = find_stems(folder)?;
    if paths.is_empty() {
        return Err(AudioError(format!(
            "{}: no stems (`<song> (<Name>).wav`)",
            folder.display()
        )));
    }
    let duration = match paths.get("Mix") {
        Some(mix) => audio::duration(mix)?,
        None => paths
            .values()
            .filter_map(|p| audio::duration(p).ok())
            .fold(0.0_f64, f64::max),
    };
    // One stem at a time: each spectrogram is some 100 MB, and the work
    // inside a stem already uses every core.
    let mut stems = BTreeMap::new();
    for (name, path) in &paths {
        stems.insert(name.clone(), measure(name, path)?);
    }
    Ok(Features {
        folder: folder.to_owned(),
        duration,
        stems,
    })
}
