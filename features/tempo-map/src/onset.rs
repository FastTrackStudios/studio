//! Onsets of one stem: the onset-strength envelope and its hits.
//!
//! A port of what the scripts took from librosa 1.0.1, step for step, so a
//! map made here agrees with the maps the owner accepted:
//! `onset.onset_strength(lag=1, max_size=1)` (a mel power spectrogram in
//! dB, its frame-to-frame rise averaged over the bands) and
//! `util.peak_pick` (its greedy form).

use rayon::prelude::*;
use realfft::RealFftPlanner;

/// The FFT size of the onset spectrogram.
pub const N_FFT: usize = 2048;
/// Its mel bands.
pub const N_MELS: usize = 128;

/// A hit: a peak of a stem's onset envelope.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hit {
    /// Seconds.
    pub time: f64,
    /// The envelope's height there (1.0 is the stem's 99.5th percentile).
    pub strength: f32,
}

fn hz_to_mel(f: f64) -> f64 {
    const F_SP: f64 = 200.0 / 3.0;
    const MIN_LOG_HZ: f64 = 1000.0;
    let min_log_mel = MIN_LOG_HZ / F_SP;
    let logstep = 6.4_f64.ln() / 27.0;
    if f >= MIN_LOG_HZ {
        min_log_mel + (f / MIN_LOG_HZ).ln() / logstep
    } else {
        f / F_SP
    }
}

fn mel_to_hz(m: f64) -> f64 {
    const F_SP: f64 = 200.0 / 3.0;
    const MIN_LOG_HZ: f64 = 1000.0;
    let min_log_mel = MIN_LOG_HZ / F_SP;
    let logstep = 6.4_f64.ln() / 27.0;
    if m >= min_log_mel {
        MIN_LOG_HZ * (logstep * (m - min_log_mel)).exp()
    } else {
        F_SP * m
    }
}

/// One mel band: its first FFT bin and its weights from there.
struct Band {
    first: usize,
    weights: Vec<f32>,
}

/// librosa's `filters.mel(sr, n_fft, n_mels=128, fmin=0, fmax=sr/2)`:
/// Slaney's scale, each triangle normalised to unit area.
fn mel_bank(rate: u32) -> Vec<Band> {
    let bins = N_FFT / 2 + 1;
    let nyquist = f64::from(rate) / 2.0;
    let (lo, hi) = (hz_to_mel(0.0), hz_to_mel(nyquist));
    let edges: Vec<f64> = (0..N_MELS + 2)
        .map(|i| mel_to_hz(lo + (hi - lo) * i as f64 / (N_MELS + 1) as f64))
        .collect();
    (0..N_MELS)
        .map(|m| {
            let (left, mid, right) = (edges[m], edges[m + 1], edges[m + 2]);
            let norm = 2.0 / (right - left);
            let mut first = bins;
            let mut weights = Vec::new();
            for bin in 0..bins {
                let f = nyquist * bin as f64 / (bins - 1) as f64;
                let lower = (f - left) / (mid - left);
                let upper = (right - f) / (right - mid);
                let w = lower.min(upper).max(0.0) * norm;
                if w > 0.0 {
                    if first == bins {
                        first = bin;
                    }
                    weights.push(w as f32);
                } else if first != bins {
                    break;
                }
            }
            Band {
                first: first.min(bins),
                weights,
            }
        })
        .collect()
}

/// The mel power spectrogram of `y` in dB, frames `hop` apart, each frame
/// centred on its time (`n_fft / 2` of silence either side): row-major,
/// `N_MELS` per frame, clamped 80 dB under its loudest value.
#[must_use]
pub fn mel_db(y: &[f32], rate: u32, hop: usize) -> Vec<f32> {
    let hop = hop.max(1);
    let frames = 1 + y.len() / hop;
    let bank = mel_bank(rate);
    let window: Vec<f32> = (0..N_FFT)
        .map(|n| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / N_FFT as f64).cos()) as f32)
        .collect();
    let mut out = vec![0.0_f32; frames * N_MELS];
    let half = N_FFT / 2;
    out.par_chunks_mut(N_MELS * 512)
        .enumerate()
        .for_each(|(chunk, rows)| {
            let mut planner = RealFftPlanner::<f32>::new();
            let fft = planner.plan_fft_forward(N_FFT);
            let mut input = fft.make_input_vec();
            let mut spectrum = fft.make_output_vec();
            let mut scratch = fft.make_scratch_vec();
            let mut power = vec![0.0_f32; N_FFT / 2 + 1];
            for (r, row) in rows.chunks_mut(N_MELS).enumerate() {
                let frame = chunk * 512 + r;
                // Sample index of the frame's first sample, before padding.
                let start = (frame * hop) as isize - half as isize;
                for (n, slot) in input.iter_mut().enumerate() {
                    let idx = start + n as isize;
                    *slot = if idx >= 0 && (idx as usize) < y.len() {
                        y[idx as usize] * window[n]
                    } else {
                        0.0
                    };
                }
                if fft
                    .process_with_scratch(&mut input, &mut spectrum, &mut scratch)
                    .is_err()
                {
                    continue;
                }
                for (p, c) in power.iter_mut().zip(&spectrum) {
                    *p = c.re * c.re + c.im * c.im;
                }
                for (slot, band) in row.iter_mut().zip(&bank) {
                    let mut acc = 0.0_f32;
                    for (w, p) in band.weights.iter().zip(&power[band.first..]) {
                        acc += w * p;
                    }
                    *slot = 10.0 * acc.max(1e-10).log10();
                }
            }
        });
    let top = out.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let floor = top - 80.0;
    for v in &mut out {
        if *v < floor {
            *v = floor;
        }
    }
    out
}

/// The onset-strength envelope of a spectrogram from [`mel_db`] at `hop`:
/// one value per `stride` frames (`stride` 2 on a hop-64 spectrogram is the
/// hop-128 envelope), the mean rise over the bands between a frame and the
/// one before, shifted late by `1 + n_fft / (2 * hop * stride)` frames as
/// librosa shifts it.
#[must_use]
pub fn strength(mel: &[f32], hop: usize, stride: usize) -> Vec<f32> {
    let stride = stride.max(1);
    let frames = mel.len() / N_MELS;
    // The frames this envelope sees: every `stride`th.
    let seen = if frames == 0 {
        0
    } else {
        1 + (frames - 1) / stride
    };
    let shift = N_FFT / (2 * hop.max(1) * stride);
    let mut env = vec![0.0_f32; seen];
    env.par_iter_mut().enumerate().for_each(|(i, slot)| {
        // env[i] is the rise into frame t = i - shift (lag 1, then the
        // shift): the padding puts flux index t-1 at t + shift.
        let Some(t) = i.checked_sub(shift) else {
            return;
        };
        if t == 0 {
            return;
        }
        let now = &mel[t * stride * N_MELS..(t * stride + 1) * N_MELS];
        let before = &mel[(t - 1) * stride * N_MELS..((t - 1) * stride + 1) * N_MELS];
        let mut acc = 0.0_f64;
        for (a, b) in now.iter().zip(before) {
            acc += f64::from((a - b).max(0.0));
        }
        *slot = (acc / N_MELS as f64) as f32;
    });
    env
}

/// The `q`th percentile of `values` (0 to 100), numpy's linear form.
#[must_use]
pub fn percentile(values: &[f32], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f32> = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    let at = (sorted.len() - 1) as f64 * q / 100.0;
    let lo = at.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    let frac = at - lo as f64;
    f64::from(sorted[lo]) * (1.0 - frac) + f64::from(sorted[hi]) * frac
}

/// `env` over its 99.5th percentile: 1.0 is a strong hit of this stem,
/// whatever the stem's level.
pub fn normalise(env: &mut [f32]) {
    let scale = percentile(env, 99.5) + 1e-9;
    for v in env.iter_mut() {
        *v = (f64::from(*v) / scale) as f32;
    }
}

/// librosa's `util.peak_pick` (greedy): index `n` is a peak when it is the
/// largest of `x[n - pre_max .. n + post_max]` and at least `delta` over
/// the mean of `x[n - pre_avg .. n + post_avg]`; the next `wait` frames
/// after a peak hold none.
#[must_use]
pub fn peak_pick(
    x: &[f64],
    max: (usize, usize),
    avg: (usize, usize),
    delta: f64,
    wait: usize,
) -> Vec<usize> {
    let len = x.len();
    let mut peaks = Vec::new();
    if len == 0 {
        return peaks;
    }
    let largest = |a: usize, b: usize| x[a..b].iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mean = |a: usize, b: usize| x[a..b].iter().sum::<f64>() / (b - a).max(1) as f64;
    let mut n = if x[0] >= largest(0, max.1.min(len)) && x[0] >= mean(0, avg.1.min(len)) + delta {
        peaks.push(0);
        wait + 1
    } else {
        1
    };
    while n < len {
        let top = largest(n.saturating_sub(max.0), (n + max.1).min(len));
        if x[n] != top {
            n += 1;
            continue;
        }
        let level = mean(n.saturating_sub(avg.0), (n + avg.1).min(len));
        if x[n] < level + delta {
            n += 1;
            continue;
        }
        peaks.push(n);
        n += wait + 1;
    }
    peaks
}

/// The hits of a normalised envelope at `hop`: the peak picking the snap
/// step used (`pre_max = post_max = 8`, `pre_avg = post_avg = 40`,
/// `delta = 0.15`, `wait = 20` on a hop of 64).
#[must_use]
pub fn hits(env: &[f32], rate: u32, hop: usize) -> Vec<Hit> {
    let x: Vec<f64> = env.iter().map(|v| f64::from(*v)).collect();
    peak_pick(&x, (8, 8), (40, 40), 0.15, 20)
        .into_iter()
        .map(|p| Hit {
            time: (p * hop) as f64 / f64::from(rate),
            strength: env[p],
        })
        .collect()
}
