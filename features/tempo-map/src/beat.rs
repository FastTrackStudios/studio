//! One consistent pulse over an onset envelope.
//!
//! A port of librosa 1.0.1's `beat.beat_track(onset_envelope, start_bpm,
//! tightness, trim=False)`: its tempo estimate (the mean autocorrelation
//! tempogram under a log-normal prior around the hint), then Ellis's
//! dynamic program, which places beats where the envelope is strong and
//! pays for every interval that is not the estimated period.

use rayon::prelude::*;
use realfft::RealFftPlanner;

/// Python's `round`: halves go to the even neighbour.
fn round_half_even(x: f64) -> f64 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 && r % 2.0 != 0.0 {
        r - x.signum()
    } else {
        r
    }
}

/// librosa's `feature.tempo`: the tempo of `env` (frames `hop` samples
/// apart at `rate`), bpm, nearest `start_bpm` among the strong
/// periodicities.
#[must_use]
pub fn estimate_tempo(env: &[f64], rate: u32, hop: usize, start_bpm: f64) -> f64 {
    let frame_rate = f64::from(rate) / hop.max(1) as f64;
    // Eight seconds of envelope per autocorrelation.
    let win = (8.0 * frame_rate).floor() as usize;
    if env.len() < 4 || win < 4 {
        return start_bpm;
    }
    let pad = win / 2;
    let n = env.len();
    // The envelope with a linear ramp down to zero either side.
    let mut padded = vec![0.0_f64; n + 2 * pad];
    let (first, last) = (env[0], env[n - 1]);
    for k in 0..pad {
        padded[k] = first * k as f64 / pad as f64;
        padded[n + pad + k] = last * (pad - 1 - k) as f64 / pad as f64;
    }
    padded[pad..pad + n].copy_from_slice(env);
    let window: Vec<f64> = (0..win)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / win as f64).cos())
        .collect();
    let size = (2 * win).next_power_of_two();
    // Each frame's autocorrelation over its own energy, averaged: by
    // linearity, the inverse transform of the averaged normalised power
    // spectra.
    let bins = size / 2 + 1;
    let total = (0..n)
        .into_par_iter()
        .fold(
            || {
                let mut planner = RealFftPlanner::<f64>::new();
                let fft = planner.plan_fft_forward(size);
                let input = fft.make_input_vec();
                let spectrum = fft.make_output_vec();
                let scratch = fft.make_scratch_vec();
                (fft, input, spectrum, scratch, vec![0.0_f64; bins])
            },
            |(fft, mut input, mut spectrum, mut scratch, mut acc), frame| {
                let mut energy = 0.0_f64;
                for (i, slot) in input.iter_mut().enumerate() {
                    *slot = if i < win {
                        let v = padded[frame + i] * window[i];
                        energy += v * v;
                        v
                    } else {
                        0.0
                    };
                }
                if energy > f64::MIN_POSITIVE
                    && fft
                        .process_with_scratch(&mut input, &mut spectrum, &mut scratch)
                        .is_ok()
                {
                    for (a, c) in acc.iter_mut().zip(&spectrum) {
                        *a += (c.re * c.re + c.im * c.im) / energy;
                    }
                }
                (fft, input, spectrum, scratch, acc)
            },
        )
        .map(|(_, _, _, _, acc)| acc)
        .reduce(
            || vec![0.0_f64; bins],
            |mut a, b| {
                for (x, y) in a.iter_mut().zip(&b) {
                    *x += y;
                }
                a
            },
        );
    let mut planner = RealFftPlanner::<f64>::new();
    let inverse = planner.plan_fft_inverse(size);
    let mut spectrum = inverse.make_input_vec();
    for (slot, power) in spectrum.iter_mut().zip(&total) {
        slot.re = *power;
        slot.im = 0.0;
    }
    let mut ac = inverse.make_output_vec();
    if inverse.process(&mut spectrum, &mut ac).is_err() {
        return start_bpm;
    }
    // realfft's inverse is unnormalised.
    let scale = 1.0 / (size as f64 * n as f64);
    let mut best = (f64::NEG_INFINITY, 0_usize);
    for lag in 1..win {
        let bpm = 60.0 * frame_rate / lag as f64;
        if bpm >= 320.0 {
            continue;
        }
        let prior = -0.5 * (bpm.log2() - start_bpm.log2()).powi(2);
        let score = (1e6 * (ac[lag] * scale).max(0.0)).ln_1p() + prior;
        if score > best.0 {
            best = (score, lag);
        }
    }
    if best.1 == 0 {
        start_bpm
    } else {
        60.0 * frame_rate / best.1 as f64
    }
}

/// What the dynamic program found.
#[derive(Debug, Clone)]
pub struct Pulse {
    /// The beats, as frames of the envelope.
    pub frames: Vec<usize>,
    /// The tempo it was held to, bpm.
    pub bpm: f64,
    /// That tempo as frames per beat.
    pub period: f64,
}

/// The beats of `env`: the tempo nearest `start_bpm`, then the dynamic
/// program at `tightness` (how much an interval off the period costs; the
/// scripts ran 400).
#[must_use]
pub fn track(env: &[f64], rate: u32, hop: usize, start_bpm: f64, tightness: f64) -> Pulse {
    let frame_rate = f64::from(rate) / hop.max(1) as f64;
    let bpm = estimate_tempo(env, rate, hop, start_bpm);
    let period = round_half_even(frame_rate * 60.0 / bpm.max(1e-9)).max(1.0);
    let frames = track_at(env, period, tightness);
    Pulse {
        frames,
        bpm,
        period,
    }
}

/// The dynamic program alone, at `period` frames per beat.
#[must_use]
pub fn track_at(env: &[f64], period: f64, tightness: f64) -> Vec<usize> {
    let n = env.len();
    if n < 2 {
        return Vec::new();
    }
    // The envelope over its standard deviation.
    let mean = env.iter().sum::<f64>() / n as f64;
    let var = env.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
    let norm = var.sqrt() + f64::MIN_POSITIVE;
    let p = period as usize;
    // A Gaussian a beat wide either side, 1/32 of a beat in deviation.
    let window: Vec<f64> = (0..=2 * p)
        .map(|k| (-0.5 * ((k as f64 - p as f64) * 32.0 / period).powi(2)).exp())
        .collect();
    let width = window.len();
    let half = width / 2;
    let local: Vec<f64> = (0..n)
        .into_par_iter()
        .map(|i| {
            // As librosa's loop: k from max(0, i + K/2 - N + 1) up to (not
            // including) min(i + K/2, K).
            let lo = (i + half + 1).saturating_sub(n);
            let hi = (i + half).min(width);
            let mut acc = 0.0_f64;
            for k in lo..hi {
                acc += window[k] * env[i + half - k] / norm;
            }
            acc
        })
        .collect();

    let threshold = 0.01 * local.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut backlink = vec![-1_i64; n];
    let mut cumscore = vec![0.0_f64; n];
    let near = round_half_even(period / 2.0) as usize;
    let far = (2.0 * period) as usize;
    let log_period = period.ln();
    let mut first_beat = true;
    for i in 0..n {
        let mut best = f64::NEG_INFINITY;
        let mut at = -1_i64;
        // Predecessors from half a beat back to two beats back.
        let mut back = near;
        while back <= far && back <= i {
            let loc = i - back;
            let score = cumscore[loc] - tightness * ((back as f64).ln() - log_period).powi(2);
            if score > best {
                best = score;
                at = loc as i64;
            }
            back += 1;
        }
        cumscore[i] = if at >= 0 { local[i] + best } else { local[i] };
        if first_beat && local[i] < threshold {
            backlink[i] = -1;
        } else {
            backlink[i] = at;
            first_beat = false;
        }
    }

    // The last beat: the last local maximum of the running score at least
    // half the median of those maxima.
    let is_max = |i: usize| {
        i > 0 && cumscore[i] > cumscore[i - 1] && (i + 1 == n || cumscore[i] >= cumscore[i + 1])
    };
    let mut maxima: Vec<f64> = (0..n).filter(|i| is_max(*i)).map(|i| cumscore[i]).collect();
    maxima.sort_by(f64::total_cmp);
    let median = if maxima.is_empty() {
        0.0
    } else if maxima.len() % 2 == 1 {
        maxima[maxima.len() / 2]
    } else {
        (maxima[maxima.len() / 2 - 1] + maxima[maxima.len() / 2]) / 2.0
    };
    let tail = (0..n)
        .rev()
        .find(|i| is_max(*i) && cumscore[*i] >= 0.5 * median)
        .unwrap_or(n - 1);

    let mut frames = Vec::new();
    let mut at = tail as i64;
    while at >= 0 {
        frames.push(at as usize);
        at = backlink[at as usize];
    }
    frames.reverse();
    // `trim=False` still drops beats in the silent frames at either end.
    let first_loud = local.iter().position(|v| *v > 0.0).unwrap_or(n);
    let last_loud = local.iter().rposition(|v| *v > 0.0).unwrap_or(0);
    frames.retain(|f| *f >= first_loud && *f <= last_loud);
    frames
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A click every `period` frames, with one `between` as tall between.
    fn clicks_with(frames: usize, period: usize, between: f64) -> Vec<f64> {
        (0..frames)
            .map(|i| match i % period {
                0 => 1.0,
                r if r == period / 2 => between,
                _ => 0.0,
            })
            .collect()
    }

    fn clicks(frames: usize, period: usize) -> Vec<f64> {
        clicks_with(frames, period, 0.3)
    }

    #[test]
    fn the_tempo_of_a_click_is_found() {
        // 344.53 frames a second at hop 128: a beat every 172 frames is
        // 120.2 bpm.
        let env = clicks(172 * 64, 172);
        let bpm = estimate_tempo(&env, 44_100, 128, 110.0);
        assert!((bpm - 120.2).abs() < 0.5, "{bpm}");
    }

    #[test]
    fn the_hint_decides_the_octave() {
        let env = clicks_with(172 * 64, 172, 0.8);
        let double = estimate_tempo(&env, 44_100, 128, 240.0);
        assert!((double - 240.4).abs() < 1.5, "{double}");
    }

    #[test]
    fn beats_land_on_the_clicks() {
        let env = clicks(172 * 64, 172);
        let pulse = track(&env, 44_100, 128, 120.0, 400.0);
        assert!(pulse.frames.len() >= 60, "{}", pulse.frames.len());
        // The last one is where the running score last peaks, at the
        // envelope's end: the pipeline drops beats after the last hit.
        for f in pulse.frames.iter().rev().skip(1) {
            assert_eq!(f % 172, 0, "beat at frame {f}");
        }
    }

    #[test]
    fn halves_round_to_even() {
        assert_eq!(round_half_even(96.5), 96.0);
        assert_eq!(round_half_even(97.5), 98.0);
        assert_eq!(round_half_even(96.4), 96.0);
    }
}
