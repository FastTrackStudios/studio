//! Reading a stem: a WAV file as mono at the analysis rate.
//!
//! The method was measured at 44.1 kHz (librosa's `load(sr=44100,
//! mono=True)`), so every stem is brought there: channels averaged, then a
//! windowed-sinc polyphase resampler when the file is at another rate.

use std::path::Path;

/// The rate every analysis runs at, Hz.
pub const RATE: u32 = 44_100;

/// A mono signal.
#[derive(Debug, Clone)]
pub struct Audio {
    pub rate: u32,
    pub samples: Vec<f32>,
}

impl Audio {
    /// Its length, seconds.
    #[must_use]
    pub fn duration(&self) -> f64 {
        self.samples.len() as f64 / f64::from(self.rate.max(1))
    }

    /// Its level, dB RMS (−240 for digital silence).
    #[must_use]
    pub fn rms_db(&self) -> f64 {
        if self.samples.is_empty() {
            return -240.0;
        }
        let sum: f64 = self.samples.iter().map(|s| f64::from(*s).powi(2)).sum();
        20.0 * ((sum / self.samples.len() as f64).sqrt() + 1e-12).log10()
    }
}

/// Why a stem could not be read.
#[derive(Debug)]
pub struct AudioError(pub String);

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AudioError {}

/// How long the file at `path` is, seconds, from its header.
///
/// # Errors
///
/// A file that is not a WAV.
pub fn duration(path: &Path) -> Result<f64, AudioError> {
    let reader =
        hound::WavReader::open(path).map_err(|e| AudioError(format!("{}: {e}", path.display())))?;
    let spec = reader.spec();
    Ok(f64::from(reader.duration()) / f64::from(spec.sample_rate.max(1)))
}

/// The file at `path` as mono, at its own rate.
///
/// # Errors
///
/// A file that is not a WAV, or one that stops short.
pub fn read_mono(path: &Path) -> Result<Audio, AudioError> {
    let err = |e: hound::Error| AudioError(format!("{}: {e}", path.display()));
    let mut reader = hound::WavReader::open(path).map_err(err)?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels.max(1));
    let frames = reader.duration() as usize;
    let mut samples = Vec::with_capacity(frames);
    let scale = 1.0 / channels as f32;
    match spec.sample_format {
        hound::SampleFormat::Float => {
            let mut acc = 0.0_f32;
            for (i, s) in reader.samples::<f32>().enumerate() {
                acc += s.map_err(err)?;
                if i % channels == channels - 1 {
                    samples.push(acc * scale);
                    acc = 0.0;
                }
            }
        }
        hound::SampleFormat::Int => {
            let full = 2.0_f32.powi(i32::from(spec.bits_per_sample) - 1);
            let mut acc = 0.0_f32;
            for (i, s) in reader.samples::<i32>().enumerate() {
                acc += s.map_err(err)? as f32 / full;
                if i % channels == channels - 1 {
                    samples.push(acc * scale);
                    acc = 0.0;
                }
            }
        }
    }
    Ok(Audio {
        rate: spec.sample_rate,
        samples,
    })
}

/// The file at `path` as mono at [`RATE`].
///
/// # Errors
///
/// As [`read_mono`].
pub fn load(path: &Path) -> Result<Audio, AudioError> {
    let audio = read_mono(path)?;
    Ok(resample(&audio, RATE))
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Zeroth-order modified Bessel function, for the Kaiser window.
fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term, mut k) = (1.0_f64, 1.0_f64, 1.0_f64);
    while term > 1e-12 * sum {
        term *= (x / (2.0 * k)).powi(2);
        sum += term;
        k += 1.0;
    }
    sum
}

/// `audio` at rate `to`: a Kaiser-windowed sinc, one phase per output
/// position, cut off just below the lower Nyquist.
#[must_use]
pub fn resample(audio: &Audio, to: u32) -> Audio {
    if audio.rate == to || audio.samples.is_empty() || audio.rate == 0 || to == 0 {
        return Audio {
            rate: to.max(1),
            samples: audio.samples.clone(),
        };
    }
    let g = gcd(audio.rate, to);
    // Output sample n sits at input position n * down / up.
    let up = (to / g) as usize;
    let down = (audio.rate / g) as usize;
    // Zero crossings each side, and the cutoff as a fraction of the input
    // Nyquist.
    const HALF: usize = 32;
    const BETA: f64 = 9.0;
    let cutoff = (f64::from(to) / f64::from(audio.rate)).min(1.0) * 0.96;
    let taps = 2 * HALF;
    let i0_beta = bessel_i0(BETA);
    // One filter per phase (the fractional input position, in `up`ths).
    let mut bank = vec![0.0_f32; up * taps];
    for phase in 0..up {
        let frac = phase as f64 / up as f64;
        let mut norm = 0.0_f64;
        let row = &mut bank[phase * taps..(phase + 1) * taps];
        let mut exact = vec![0.0_f64; taps];
        for (k, slot) in exact.iter_mut().enumerate() {
            // Tap k reads input index floor(pos) - HALF + 1 + k.
            let x = (k as f64 - (HALF as f64 - 1.0)) - frac;
            let arg = std::f64::consts::PI * x * cutoff;
            let sinc = if arg.abs() < 1e-12 {
                1.0
            } else {
                arg.sin() / arg
            };
            let r = x / HALF as f64;
            let window = if r.abs() >= 1.0 {
                0.0
            } else {
                bessel_i0(BETA * (1.0 - r * r).sqrt()) / i0_beta
            };
            *slot = cutoff * sinc * window;
            norm += *slot;
        }
        for (slot, value) in row.iter_mut().zip(&exact) {
            *slot = (*value / norm) as f32;
        }
    }
    let input = &audio.samples;
    let n_out = (input.len() as u128 * up as u128 / down as u128) as usize;
    let mut out = vec![0.0_f32; n_out];
    use rayon::prelude::*;
    out.par_chunks_mut(1 << 16)
        .enumerate()
        .for_each(|(chunk, slice)| {
            let base = chunk << 16;
            for (i, o) in slice.iter_mut().enumerate() {
                let n = base + i;
                let pos = n as u128 * down as u128;
                let whole = (pos / up as u128) as usize;
                let phase = (pos % up as u128) as usize;
                let row = &bank[phase * taps..(phase + 1) * taps];
                let first = whole as isize - HALF as isize + 1;
                let mut acc = 0.0_f32;
                for (k, w) in row.iter().enumerate() {
                    let idx = first + k as isize;
                    if idx >= 0 && (idx as usize) < input.len() {
                        acc += w * input[idx as usize];
                    }
                }
                *o = acc;
            }
        });
    Audio {
        rate: to,
        samples: out,
    }
}
