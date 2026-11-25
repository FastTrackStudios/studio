//! Record and compare reference vectors.
//!
//! # The contract
//!
//! A reference holds on every platform the workspace builds for: x86_64 Linux
//! (CI) and aarch64 macOS (the dev machines) render the same fixture a few ULP
//! apart, because their libm `sin`/`tanh`/`exp`/`pow` differ in the last bit
//! and a feedback loop or an IIR filter carries that bit forward. A bit-exact
//! hash recorded on one of them is red on the other forever, and a reference
//! that is always red protects nothing. So the comparison is:
//!
//! 1. **Bit-exact first.** The whole-buffer hash matching is a pass, and on the
//!    platform that recorded the reference that is the normal case.
//! 2. **Otherwise within tolerance.** Every pinned statistic must lie within
//!    `tolerance × peak` of the reference, `peak` being the reference's largest
//!    `|sample|` (or `1e-6` for a reference that is numerically silent).
//!    The default tolerance is [`Sample::TOLERANCE`]: `1e-6` (-120 dB) for
//!    `f64` and `2.5e-4` (-72 dB) for `f32`. Measured between the two
//!    platforms, the delay's `f64` engines drifted about `1e-8` of peak and the
//!    saturator's `f32` stages about `2e-5`; the worst, `4e-5` on an `f64` EQ,
//!    was the excitation itself differing (its sweep phase was computed in
//!    `f32`), which [`super::signal`] no longer does. A wrong index, a lost
//!    parameter or a moved repeat is -50 dB re peak or louder.
//!
//! Set `GOLDEN_EXACT=1` to switch step 2 off and demand bit-exactness, which is
//! what a pure refactor (iterator for index, checked conversion for `as`) owes
//! on the machine that recorded the references: there, a reordered float
//! operation that stays within tolerance is still a change worth seeing.
//!
//! # What a reference file pins
//!
//! Reference files are text and live in `<crate>/tests/golden/`. Besides the
//! hash they pin:
//!
//! - **windows** — the buffer cut into [`WINDOWS`] equal spans, each with its
//!   RMS, peak, mean and a fixed pseudo-random ±1 projection. A delay's repeats
//!   and a reverb's tail land in different windows, so a failure names the
//!   span that moved: *which repeat*, not only *that* the output changed.
//! - **probes** — [`PROBES`] individual samples spread over the buffer, which
//!   catch a one-sample shift no window statistic would.
//!
//! Every statistic is 1-Lipschitz in the per-sample difference (an RMS, a
//! peak, a mean or a normalised ±1 projection never moves further than the
//! largest sample moved), so platform noise below the tolerance can never fail
//! a check — the tolerance is a bound on the per-sample error, not a guess.
//!
//! # Regenerating
//!
//! `UPDATE_GOLDEN=1 cargo nextest run -p <crate> --test golden`, then read the
//! diff. Re-record only when the output changed on purpose (a fix, a new
//! sound — say which in the commit), or when a reference's `hash` line is the
//! only thing that moved, which is the recording platform changing and not
//! the audio. A reference whose windows or probes move in a commit that
//! claimed to be a pure refactor is the alarm, not a chore.
//!
//! Both float widths are pinned. Several DSP cores compute in `f64` and only
//! narrow at the output, and pinning those at `f32` would throw away exactly
//! the low bits a refactor is most likely to disturb.

use core::fmt::{self, Write as _};
use std::fs;
use std::path::{Path, PathBuf};

/// How many individual samples a reference file pins.
pub const PROBES: usize = 64;
/// How many equal spans a reference file summarises.
pub const WINDOWS: usize = 64;
/// The peak below which a reference counts as silence (-120 dBFS).
const SILENT: f64 = 1e-6;
/// Out-of-tolerance windows or probes listed in a failure before truncating.
const REPORTED: usize = 8;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The reference format this module writes and reads.
const HEADER: &str = "# dsp-golden v2";

/// A float this harness can pin.
///
/// Implemented for `f32` and `f64` only.
pub trait Sample: Copy + fmt::Display {
    /// Bytes in the bit pattern — what the hash consumes per sample.
    const BYTES: usize;
    /// Hex digits a probe is written with.
    const HEX_WIDTH: usize;
    /// Default comparison tolerance, as a fraction of the reference's peak.
    const TOLERANCE: f64;

    /// The IEEE-754 bit pattern, widened to `u64`.
    fn bits(self) -> u64;
    /// Rebuild from a pattern produced by [`Sample::bits`].
    fn from_bits(bits: u64) -> Self;
    /// Whether this is a real number rather than a NaN or an infinity.
    fn finite(self) -> bool;
    /// Widen for the statistics and the tolerance comparison.
    fn to_f64(self) -> f64;
}

impl Sample for f32 {
    const BYTES: usize = 4;
    const HEX_WIDTH: usize = 8;
    const TOLERANCE: f64 = 2.5e-4;

    fn bits(self) -> u64 {
        u64::from(Self::to_bits(self))
    }
    fn from_bits(bits: u64) -> Self {
        Self::from_bits(u32::try_from(bits & 0xFFFF_FFFF).unwrap_or(0))
    }
    fn finite(self) -> bool {
        Self::is_finite(self)
    }
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
}

impl Sample for f64 {
    const BYTES: usize = 8;
    const HEX_WIDTH: usize = 16;
    const TOLERANCE: f64 = 1e-6;

    fn bits(self) -> u64 {
        Self::to_bits(self)
    }
    fn from_bits(bits: u64) -> Self {
        Self::from_bits(bits)
    }
    fn finite(self) -> bool {
        Self::is_finite(self)
    }
    fn to_f64(self) -> Self {
        self
    }
}

/// FNV-1a over the raw bit patterns. Spelled out rather than pulled from a
/// crate so that no dependency bump can silently invalidate every reference
/// file in the tree.
#[must_use]
fn digest<S: Sample>(samples: &[S]) -> u64 {
    samples.iter().fold(FNV_OFFSET, |acc, sample| {
        sample
            .bits()
            .to_le_bytes()
            .iter()
            .take(S::BYTES)
            .fold(acc, |h, byte| {
                (h ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
            })
    })
}

/// Evenly spaced probe indices, always including the first sample.
fn probe_indices(len: usize) -> impl Iterator<Item = usize> {
    let stride = len.div_ceil(PROBES).max(1);
    (0..len).step_by(stride).take(PROBES)
}

/// The `[start, end)` spans the buffer is summarised in: [`WINDOWS`] of them,
/// or one per sample for a buffer shorter than that.
fn window_spans(len: usize) -> impl Iterator<Item = (usize, usize)> {
    let count = WINDOWS.min(len);
    (0..count).map(move |k| (k * len / count, (k + 1) * len / count))
}

/// The fixed ±1 sequence the projection statistic weighs samples by. A pure
/// function of the index, so recording and checking agree on every platform.
fn sign(index: usize) -> f64 {
    let mut x = u64::try_from(index).unwrap_or(u64::MAX) ^ 0x9e37_79b9_7f4a_7c15;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    if (x ^ (x >> 31)) & 1 == 0 { 1.0 } else { -1.0 }
}

/// What one window of the buffer looks like.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Window {
    start: usize,
    end: usize,
    rms: f64,
    peak: f64,
    mean: f64,
    projection: f64,
}

/// The statistic names, in the order [`Window::stats`] yields them.
const STAT_NAMES: [&str; 4] = ["rms", "peak", "mean", "projection"];

impl Window {
    fn measure<S: Sample>(samples: &[S], start: usize, end: usize) -> Self {
        let span = samples.get(start..end).unwrap_or(&[]);
        let n = f64::from(u32::try_from(span.len().max(1)).unwrap_or(u32::MAX));
        let (energy, peak, sum, projected) = span.iter().enumerate().fold(
            (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64),
            |(energy, peak, sum, projected), (offset, s)| {
                let x = s.to_f64();
                (
                    x.mul_add(x, energy),
                    peak.max(x.abs()),
                    sum + x,
                    x.mul_add(sign(start + offset), projected),
                )
            },
        );
        Self {
            start,
            end,
            rms: (energy / n).sqrt(),
            peak,
            mean: sum / n,
            projection: projected / n,
        }
    }

    const fn stats(&self) -> [f64; 4] {
        [self.rms, self.peak, self.mean, self.projection]
    }
}

/// The measured shape of one output buffer.
#[derive(Debug, Clone, PartialEq)]
struct Record {
    len: usize,
    hash: u64,
    peak: f64,
    windows: Vec<Window>,
    probes: Vec<(usize, u64)>,
}

/// An `f64` as its exact bit pattern, followed by a readable rendering.
fn exact(value: f64) -> String {
    format!("{:016x} {value:+.9e}", value.to_bits())
}

fn parse_exact(field: Option<&str>) -> Option<f64> {
    u64::from_str_radix(field?, 16).ok().map(f64::from_bits)
}

impl Record {
    fn measure<S: Sample>(samples: &[S]) -> Self {
        Self {
            len: samples.len(),
            hash: digest(samples),
            peak: samples.iter().fold(0.0, |m, s| m.max(s.to_f64().abs())),
            windows: window_spans(samples.len())
                .map(|(start, end)| Window::measure(samples, start, end))
                .collect(),
            probes: probe_indices(samples.len())
                .filter_map(|i| samples.get(i).map(|s| (i, s.bits())))
                .collect(),
        }
    }

    fn render<S: Sample>(&self, name: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{HEADER} — reference vector, compared bit-exact then within tolerance."
        );
        out.push_str("# Regenerate: UPDATE_GOLDEN=1 cargo nextest run -p <crate> --test golden\n");
        out.push_str(
            "# A window or probe changing here in a refactor commit means the audio changed. Investigate.\n",
        );
        out.push_str(
            "# window <start> <end>  then rms, peak, mean, projection: <f64 bits> <value>\n",
        );
        let _ = writeln!(out, "name {name}");
        let _ = writeln!(out, "len {}", self.len);
        let _ = writeln!(out, "hash {:016x}", self.hash);
        let _ = writeln!(out, "peak {}", exact(self.peak));
        for w in &self.windows {
            let _ = write!(out, "window {} {}", w.start, w.end);
            for stat in w.stats() {
                let _ = write!(out, "  {}", exact(stat));
            }
            out.push('\n');
        }
        for (index, bits) in &self.probes {
            let _ = writeln!(
                out,
                "probe {index} {bits:0width$x} {}",
                S::from_bits(*bits),
                width = S::HEX_WIDTH,
            );
        }
        out
    }

    fn parse(text: &str) -> Option<Self> {
        if !text.starts_with(HEADER) {
            return None;
        }
        let mut len = None;
        let mut hash = None;
        let mut peak = None;
        let mut windows = Vec::new();
        let mut probes = Vec::new();
        for line in text.lines() {
            let mut field = line.split_whitespace();
            match field.next() {
                Some("len") => len = field.next()?.parse().ok(),
                Some("hash") => hash = u64::from_str_radix(field.next()?, 16).ok(),
                Some("peak") => peak = parse_exact(field.next()),
                Some("window") => {
                    let start = field.next()?.parse().ok()?;
                    let end = field.next()?.parse().ok()?;
                    let mut stat = || {
                        let value = parse_exact(field.next());
                        let _readable = field.next();
                        value
                    };
                    windows.push(Window {
                        start,
                        end,
                        rms: stat()?,
                        peak: stat()?,
                        mean: stat()?,
                        projection: stat()?,
                    });
                }
                Some("probe") => {
                    let index = field.next()?.parse().ok()?;
                    probes.push((index, u64::from_str_radix(field.next()?, 16).ok()?));
                }
                _ => {}
            }
        }
        Some(Self {
            len: len?,
            hash: hash?,
            peak: peak?,
            windows,
            probes,
        })
    }
}

/// Why a comparison failed, phrased so the message alone is enough to act on.
#[derive(Debug)]
pub enum Mismatch {
    /// No reference file yet. Not a silent pass: an unrecorded vector proves
    /// nothing, so it fails until someone records it deliberately.
    Missing { name: String, path: PathBuf },
    /// The file exists but could not be read or written.
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
    /// The file is present but malformed (or in an older format) — treated as
    /// a hard failure rather than re-recorded, because overwriting a corrupt
    /// reference destroys the only evidence of what the output used to be.
    Corrupt { path: PathBuf },
    /// The output changed.
    Drift { name: String, report: String },
    /// The output is not a finite signal, which no reference should ever pin.
    NotFinite {
        name: String,
        index: usize,
        value: f64,
    },
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { name, path } => write!(
                f,
                "no golden reference for `{name}`.\n  \
                 expected at: {}\n  \
                 record it with: UPDATE_GOLDEN=1 cargo nextest run\n  \
                 then read the new file before committing it — you are declaring this output correct.",
                path.display()
            ),
            Self::Io { path, error } => write!(f, "golden file {}: {error}", path.display()),
            Self::Corrupt { path } => write!(
                f,
                "golden file {} is malformed or not `{HEADER}`. Restore it from git rather than \
                 re-recording — re-recording would destroy the reference you are trying to \
                 compare against.",
                path.display()
            ),
            Self::Drift { name, report } => write!(
                f,
                "`{name}` no longer produces the reference output.\n{report}"
            ),
            Self::NotFinite { name, index, value } => {
                write!(
                    f,
                    "`{name}` produced a non-finite sample at index {index}: {value}"
                )
            }
        }
    }
}

impl std::error::Error for Mismatch {}

fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| v != "0")
}

/// A crate's reference-vector directory.
#[derive(Debug, Clone)]
pub struct Golden {
    dir: PathBuf,
    update: bool,
    exact: bool,
    tolerance: Option<f64>,
}

impl Golden {
    /// Point at `<manifest_dir>/tests/golden`. Use the [`crate::golden!`] macro
    /// rather than passing the path by hand.
    #[must_use]
    pub fn new(manifest_dir: &str) -> Self {
        Self {
            dir: Path::new(manifest_dir).join("tests").join("golden"),
            update: env_flag("UPDATE_GOLDEN"),
            exact: env_flag("GOLDEN_EXACT"),
            tolerance: None,
        }
    }

    /// Override [`Sample::TOLERANCE`] — a fraction of the reference's peak —
    /// for a fixture that needs it. Say why at the call site.
    #[must_use]
    pub const fn with_tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = Some(tolerance);
        self
    }

    fn path_for(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.golden", name.replace('/', "__")))
    }

    /// Compare `samples` against the stored reference for `name`, or record it
    /// when `UPDATE_GOLDEN` is set.
    ///
    /// # Errors
    ///
    /// Returns [`Mismatch`] when the reference is absent, unreadable, malformed,
    /// or no longer matches the output.
    pub fn check<S: Sample>(&self, name: &str, samples: &[S]) -> Result<(), Mismatch> {
        if let Some((index, value)) = samples.iter().enumerate().find(|(_, s)| !s.finite()) {
            return Err(Mismatch::NotFinite {
                name: name.to_owned(),
                index,
                value: value.to_f64(),
            });
        }

        let measured = Record::measure(samples);
        let path = self.path_for(name);

        if self.update {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| Mismatch::Io {
                    path: path.clone(),
                    error,
                })?;
            }
            return fs::write(&path, measured.render::<S>(name))
                .map_err(|error| Mismatch::Io { path, error });
        }

        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(Mismatch::Missing {
                    name: name.to_owned(),
                    path,
                });
            }
            Err(error) => return Err(Mismatch::Io { path, error }),
        };
        let stored = Record::parse(&text).ok_or(Mismatch::Corrupt { path })?;

        if stored.len == measured.len && stored.hash == measured.hash {
            return Ok(());
        }
        let tolerance = if self.exact {
            0.0
        } else {
            self.tolerance.unwrap_or(S::TOLERANCE)
        };
        let report = match compare::<S>(&stored, &measured, tolerance) {
            None if !self.exact => return Ok(()),
            None => format!(
                "  hash {:016x} -> {:016x}, below the resolution of every pinned statistic — \
                 GOLDEN_EXACT is set, so that still fails.\n",
                stored.hash, measured.hash
            ),
            Some(report) => report,
        };
        Err(Mismatch::Drift {
            name: name.to_owned(),
            report,
        })
    }
}

/// One pinned value that moved further than the tolerance allows.
struct Excess {
    what: String,
    was: f64,
    now: f64,
}

/// `None` when every pinned statistic is within `tolerance × peak` of the
/// reference, otherwise the report saying which windows and probes moved.
fn compare<S: Sample>(stored: &Record, measured: &Record, tolerance: f64) -> Option<String> {
    let mut out = String::new();
    if stored.len != measured.len
        || stored.windows.len() != measured.windows.len()
        || stored.probes.len() != measured.probes.len()
    {
        let _ = writeln!(
            out,
            "  length changed: {} -> {} samples (the reference cannot be compared further)",
            stored.len, measured.len
        );
        return Some(out);
    }
    // A reference peaking below SILENT is numerically silent — rounding noise
    // around zero, like an all-pass's magnitude in dB — and is held to an
    // absolute bound instead: `tolerance × SILENT`.
    let scale = stored.peak.max(SILENT);
    let allowed = tolerance * scale;
    let beyond = |was: f64, now: f64| (now - was).abs() > allowed;

    let windows: Vec<(&Window, Vec<Excess>)> = stored
        .windows
        .iter()
        .zip(&measured.windows)
        .filter_map(|(was, now)| {
            let moved: Vec<Excess> = STAT_NAMES
                .iter()
                .zip(was.stats().into_iter().zip(now.stats()))
                .filter(|(_, (a, b))| beyond(*a, *b))
                .map(|(stat, (a, b))| Excess {
                    what: (*stat).to_owned(),
                    was: a,
                    now: b,
                })
                .collect();
            (!moved.is_empty()).then_some((was, moved))
        })
        .collect();
    let probes: Vec<Excess> = stored
        .probes
        .iter()
        .zip(&measured.probes)
        .filter_map(|((index, was), (_, now))| {
            let (was, now) = (S::from_bits(*was).to_f64(), S::from_bits(*now).to_f64());
            beyond(was, now).then(|| Excess {
                what: format!("[{index}]"),
                was,
                now,
            })
        })
        .collect();

    if windows.is_empty() && probes.is_empty() {
        return None;
    }

    let _ = writeln!(
        out,
        "  hash {:016x} -> {:016x}; tolerance {tolerance:.1e} of peak {:.6e} = {allowed:.3e}",
        stored.hash, measured.hash, stored.peak
    );
    let db = |delta: f64| 20.0 * (delta.abs() / scale).max(1e-30).log10();
    if !windows.is_empty() {
        let _ = writeln!(
            out,
            "  {} of {} windows moved (first is where the output first diverged):",
            windows.len(),
            stored.windows.len()
        );
        for (w, moved) in windows.iter().take(REPORTED) {
            let _ = write!(out, "    samples {}..{}:", w.start, w.end);
            for e in moved {
                let _ = write!(
                    out,
                    " {} {:+.6e} -> {:+.6e} ({:+.1} dB re peak);",
                    e.what,
                    e.was,
                    e.now,
                    db(e.now - e.was)
                );
            }
            out.push('\n');
        }
        if windows.len() > REPORTED {
            let _ = writeln!(
                out,
                "    ... and {} more",
                windows.len().saturating_sub(REPORTED)
            );
        }
    }
    if !probes.is_empty() {
        let _ = writeln!(
            out,
            "  {} of {} probes moved:",
            probes.len(),
            stored.probes.len()
        );
        for e in probes.iter().take(REPORTED) {
            let _ = writeln!(
                out,
                "    {} {:+.9e} -> {:+.9e}  (delta {:+.3e}, {:+.1} dB re peak)",
                e.what,
                e.was,
                e.now,
                e.now - e.was,
                db(e.now - e.was)
            );
        }
        if probes.len() > REPORTED {
            let _ = writeln!(
                out,
                "    ... and {} more",
                probes.len().saturating_sub(REPORTED)
            );
        }
    }
    out.push_str(if tolerance == 0.0 {
        "  GOLDEN_EXACT is set: any change at all fails.\n"
    } else {
        "  Cross-platform float noise stays far inside the tolerance; this is the audio changing.\n"
    });
    Some(out)
}

/// The reference-vector directory of the calling crate.
#[macro_export]
macro_rules! golden {
    () => {
        $crate::golden::Golden::new(env!("CARGO_MANIFEST_DIR"))
    };
}

/// Compare a buffer against its reference, panicking with the full diagnosis on
/// any mismatch. Intended for `#[test]` bodies.
#[macro_export]
macro_rules! assert_golden {
    ($golden:expr, $name:expr, $samples:expr) => {
        if let Err(mismatch) = $golden.check($name, $samples) {
            panic!("{mismatch}");
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked(stored: &[f32], now: &[f32], tolerance: f64) -> Option<String> {
        compare::<f32>(&Record::measure(stored), &Record::measure(now), tolerance)
    }

    #[test]
    fn a_record_round_trips_through_its_text_form() {
        let samples = crate::golden::signal::noise(1024, 3);
        let record = Record::measure(&samples);
        let parsed = Record::parse(&record.render::<f32>("round/trip"));
        assert_eq!(parsed, Some(record));
    }

    #[test]
    fn one_flipped_bit_changes_the_hash() {
        let mut samples = crate::golden::signal::sine(512, 440.0, 48_000.0);
        let before = digest(&samples);
        if let Some(last) = samples.last_mut() {
            *last = f32::from_bits(last.to_bits() ^ 1);
        }
        assert_ne!(before, digest(&samples));
    }

    #[test]
    fn probes_cover_the_buffer_without_overrunning_it() {
        for len in [1_usize, 7, 64, 1000, 48_000] {
            let indices: Vec<_> = probe_indices(len).collect();
            assert!(!indices.is_empty());
            assert!(indices.iter().all(|i| *i < len));
            assert_eq!(indices.first(), Some(&0));
        }
    }

    #[test]
    fn windows_tile_the_buffer() {
        for len in [1_usize, 7, 64, 1000, 24_000] {
            let spans: Vec<_> = window_spans(len).collect();
            assert_eq!(spans.first().map(|s| s.0), Some(0));
            assert_eq!(spans.last().map(|s| s.1), Some(len));
            assert!(spans.windows(2).all(|p| p[0].1 == p[1].0));
        }
    }

    #[test]
    fn noise_below_the_tolerance_passes() {
        // Every sample nudged by just under the tolerance, alternating sign:
        // the Lipschitz bound means no statistic can exceed it.
        let stored = crate::golden::signal::noise(4096, 5);
        let peak = stored.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        let nudge = peak * 2.0e-4;
        let now: Vec<f32> = stored
            .iter()
            .enumerate()
            .map(|(i, s)| if i % 3 == 0 { s + nudge } else { s - nudge })
            .collect();
        assert_eq!(checked(&stored, &now, f32::TOLERANCE), None);
    }

    #[test]
    fn a_change_in_one_window_is_reported_there() {
        let stored = crate::golden::signal::sine(6400, 440.0, 48_000.0);
        let mut now = stored.clone();
        // Silence the 40th window only: a "repeat" that went missing.
        for s in now.iter_mut().skip(3900).take(100) {
            *s = 0.0;
        }
        let report = checked(&stored, &now, f32::TOLERANCE).unwrap_or_default();
        assert!(report.contains("samples 3900..4000"), "{report}");
        assert!(!report.contains("samples 3800..3900"), "{report}");
    }

    #[test]
    fn a_one_sample_shift_fails() {
        let stored = crate::golden::signal::sine(4800, 220.0, 48_000.0);
        let now: Vec<f32> = core::iter::once(0.0)
            .chain(stored.iter().copied())
            .take(stored.len())
            .collect();
        assert!(checked(&stored, &now, f32::TOLERANCE).is_some());
    }

    #[test]
    fn exact_mode_fails_on_one_ulp() {
        let stored = crate::golden::signal::sine(512, 440.0, 48_000.0);
        let mut now = stored.clone();
        if let Some(s) = now.get_mut(100) {
            *s = f32::from_bits(s.to_bits() ^ 1);
        }
        assert!(checked(&stored, &now, 0.0).is_some());
        assert_eq!(checked(&stored, &now, f32::TOLERANCE), None);
    }
}
