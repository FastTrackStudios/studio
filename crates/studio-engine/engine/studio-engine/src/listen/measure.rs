//! What the listening tools say about a signal: plain sample slices in,
//! figures out. Loudness, true peak, the bands and the stereo figures are
//! MIX_MEASURE's own (`mix_measure::analysis`); this adds what a listen
//! needs besides — a dynamics FX's gain reduction over time, and hits:
//! each one cut out, averaged, its attack, decay and spectrum, and how
//! the mics of one piece line up and add.

use realfft::RealFftPlanner;
use serde_json::{Value, json};

use crate::mix_measure::analysis::{self, Bands, FLOOR};

/// `20·log10(x)`, floored.
#[must_use]
pub fn amp_db(x: f64) -> f64 {
    if x <= 1e-15 {
        FLOOR
    } else {
        (20.0 * x.log10()).max(FLOOR)
    }
}

/// One decimal.
fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Two decimals.
fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Split interleaved stereo.
#[must_use]
pub fn channels(s: &[f32]) -> (Vec<f32>, Vec<f32>) {
    s.chunks_exact(2).map(|f| (f[0], f[1])).unzip()
}

/// The mid (L+R)/2 of interleaved stereo.
#[must_use]
pub fn mid(s: &[f32]) -> Vec<f32> {
    s.chunks_exact(2).map(|f| 0.5 * (f[0] + f[1])).collect()
}

/// RMS of a signal.
#[must_use]
pub fn rms(x: &[f32]) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

/// Sample peak of a signal.
#[must_use]
pub fn peak(x: &[f32]) -> f64 {
    x.iter().fold(0.0f64, |m, s| m.max(f64::from(s.abs())))
}

/// LISTEN's figures for one stereo signal at `rate`: loudness (integrated,
/// and short-term every second), true peak, peak, RMS, crest, the
/// third-octave bands, and the stereo figures.
#[must_use]
pub fn figures(signal: &[f32], rate: u32) -> Value {
    let (l, r) = channels(signal);
    let s = analysis::measure(&l, &r, rate, Bands::Third);
    json!({
        "lufs": s.lufs,
        "short_term": analysis::short_term(&l, &r, rate, 1.0),
        "short_max": s.short_max,
        "true_peak": s.true_peak,
        "peak": s.peak,
        "rms": s.rms,
        "crest": s.crest,
        "bands": s.bands,
        "correlation": s.correlation,
        "width": s.width,
        "balance": s.balance,
    })
}

/// A dynamics FX's gain over time, from what went in and what came out
/// (both interleaved stereo, lined up): every `hop` seconds, the level out
/// over the level in across that window, dB, for the windows whose input
/// is above `gate_db`. Its resting gain (its make-up, the 95th percentile
/// of those) is `makeup_db`; `gr` is each window's gain against it — 0
/// where the FX does nothing, negative where it pulls down.
#[must_use]
pub fn gain_reduction(input: &[f32], output: &[f32], rate: u32, hop: f64) -> Value {
    const GATE_DB: f64 = -60.0;
    let hop = ((hop * f64::from(rate)).round() as usize).max(1) * 2;
    let change: Vec<Option<f64>> = input
        .chunks(hop)
        .zip(output.chunks(hop))
        .map(|(i, o)| {
            let (li, lo) = (amp_db(rms(i)), amp_db(rms(o)));
            (li > GATE_DB).then_some(lo - li)
        })
        .collect();
    let mut heard: Vec<f64> = change.iter().flatten().copied().collect();
    if heard.is_empty() {
        return json!({ "heard": 0 });
    }
    heard.sort_by(f64::total_cmp);
    let at = |q: f64| heard[((heard.len() - 1) as f64 * q).round() as usize];
    let makeup = at(0.95);
    let gr: Vec<Value> = change
        .iter()
        .map(|c| c.map_or(Value::Null, |c| json!(r1((c - makeup).min(0.0)))))
        .collect();
    let deepest = heard.first().copied().unwrap_or(0.0) - makeup;
    let mean = heard.iter().map(|c| (c - makeup).min(0.0)).sum::<f64>() / heard.len() as f64;
    json!({
        "makeup_db": r2(makeup),
        "max_gr_db": r2(deepest.min(0.0)),
        "mean_gr_db": r2(mean),
        "hop_s": hop as f64 / 2.0 / f64::from(rate),
        "gr": gr,
    })
}

/// Each hit's window of `x` (mono): `pre` frames before its frame and
/// `post` after, zero where it runs off either end.
#[must_use]
pub fn windows(x: &[f32], hits: &[usize], (pre, post): (usize, usize)) -> Vec<Vec<f32>> {
    hits.iter()
        .map(|&h| {
            (0..pre + post)
                .map(|i| {
                    (h + i)
                        .checked_sub(pre)
                        .and_then(|j| x.get(j))
                        .copied()
                        .unwrap_or(0.0)
                })
                .collect()
        })
        .collect()
}

/// The sample-by-sample mean of equal-length windows.
#[must_use]
pub fn average(windows: &[Vec<f32>]) -> Vec<f32> {
    let Some(len) = windows.first().map(Vec::len) else {
        return Vec::new();
    };
    let n = windows.len() as f32;
    (0..len)
        .map(|i| windows.iter().map(|w| w[i]).sum::<f32>() / n)
        .collect()
}

/// A peak envelope: instant attack, `release_ms` release.
#[must_use]
pub fn envelope(x: &[f32], rate: u32, release_ms: f64) -> Vec<f32> {
    let k = (-1.0 / (release_ms * 1e-3 * f64::from(rate))).exp() as f32;
    let mut e = 0.0f32;
    x.iter()
        .map(|s| {
            let a = s.abs();
            e = if a > e { a } else { a + k * (e - a) };
            e
        })
        .collect()
}

/// A hit's shape, from its envelope `env` (`onset` the hit's frame in it):
/// how long it takes from 10 % to 90 % of its peak (attack), and from its
/// peak until it has fallen 20 dB (decay), ms — `None` when it never
/// falls that far inside the window.
#[must_use]
pub fn shape(env: &[f32], rate: u32) -> (f64, Option<f64>) {
    let (top_at, top) = env
        .iter()
        .enumerate()
        .fold((0, 0.0f32), |m, (i, v)| if *v > m.1 { (i, *v) } else { m });
    let ms = |frames: usize| frames as f64 * 1e3 / f64::from(rate);
    if top <= 0.0 {
        return (0.0, None);
    }
    let first = |level: f32| env[..=top_at].iter().position(|v| *v >= level);
    let attack = match (first(0.1 * top), first(0.9 * top)) {
        (Some(a), Some(b)) => ms(b - a),
        _ => 0.0,
    };
    let decay = env[top_at..].iter().position(|v| *v < 0.1 * top).map(ms);
    (attack, decay)
}

/// The spectral centroid of `x` at `rate`, Hz (0 for silence).
#[must_use]
pub fn centroid(x: &[f32], rate: u32) -> f64 {
    let size = x.len().next_power_of_two().max(256);
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(size);
    let mut input = fft.make_input_vec();
    for (i, v) in input.iter_mut().enumerate() {
        let w = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / x.len().max(1) as f64).cos();
        *v = x.get(i).map_or(0.0, |s| f64::from(*s) * w);
    }
    let mut output = fft.make_output_vec();
    if fft.process(&mut input, &mut output).is_err() {
        return 0.0;
    }
    let bin = f64::from(rate) / size as f64;
    let (num, den) = output
        .iter()
        .enumerate()
        .fold((0.0, 0.0), |(n, d), (k, c)| {
            let m = c.norm();
            (n + m * k as f64 * bin, d + m)
        });
    if den <= 1e-15 { 0.0 } else { num / den }
}

/// The third-octave bands of a mono signal, dB.
#[must_use]
pub fn thirds(x: &[f32], rate: u32) -> Vec<f64> {
    analysis::spectrum(x, x, f64::from(rate), Bands::Third)
        .into_iter()
        .map(r1)
        .collect()
}

/// The lag of `b` against `a` within `most` samples that lines them up
/// best, and their correlation there (−1 … 1: negative is `b` upside
/// down against `a`). A positive lag: `b` is late.
#[must_use]
pub fn lag(a: &[f32], b: &[f32], most: usize) -> (i64, f64) {
    let n = a.len().min(b.len());
    let energy = |x: &[f32]| x.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>();
    let norm = (energy(&a[..n]) * energy(&b[..n])).sqrt();
    if norm <= 1e-15 {
        return (0, 0.0);
    }
    let at = |l: i64| -> f64 {
        (0..n)
            .filter_map(|i| {
                let j = i as i64 + l;
                (j >= 0 && (j as usize) < n).then(|| f64::from(a[i]) * f64::from(b[j as usize]))
            })
            .sum::<f64>()
    };
    let most = most as i64;
    let (best, c) = (-most..=most)
        .map(|l| (l, at(l)))
        .fold(
            (0, 0.0f64),
            |m, (l, c)| if c.abs() > m.1.abs() { (l, c) } else { m },
        );
    (best, r2(c / norm))
}

/// The figures of one averaged hit `avg` (mono, its onset `pre` frames
/// in) and the windows it was averaged from: level, shape, the first
/// 50 ms against the tail, and how much the hits differ.
#[must_use]
pub fn hit_figures(avg: &[f32], hits: &[Vec<f32>], rate: u32, pre: usize) -> Value {
    let head_end = (pre + (0.05 * f64::from(rate)) as usize).min(avg.len());
    let (head, tail) = (&avg[pre.min(head_end)..head_end], &avg[head_end..]);
    let env = envelope(avg, rate, 5.0);
    let (attack, decay) = shape(&env, rate);
    let each: Vec<(f64, f64, f64)> = hits
        .iter()
        .map(|w| (amp_db(peak(w)), amp_db(rms(w)), centroid(w, rate)))
        .collect();
    let spread = |f: &dyn Fn(&(f64, f64, f64)) -> f64| {
        let v: Vec<f64> = each.iter().map(f).collect();
        let mean = v.iter().sum::<f64>() / v.len().max(1) as f64;
        r2((v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len().max(1) as f64).sqrt())
    };
    json!({
        "peak_db": r2(amp_db(peak(avg))),
        "rms_db": r2(amp_db(rms(avg))),
        "attack_ms": r2(attack),
        "decay_20db_ms": decay.map(r1),
        "head": { "centroid_hz": centroid(head, rate).round(), "bands": thirds(head, rate) },
        "tail": { "centroid_hz": centroid(tail, rate).round(), "bands": thirds(tail, rate) },
        "spread": {
            "peak_db": spread(&|e| e.0),
            "rms_db": spread(&|e| e.1),
            "centroid_hz": spread(&|e| e.2).round(),
        },
    })
}

/// The first difference of `x`: its transients, its low end flattened —
/// what lines two mics' attacks up, where a kick's 60 Hz would line up
/// anywhere within its period.
#[must_use]
pub fn difference(x: &[f32]) -> Vec<f32> {
    x.windows(2).map(|w| w[1] - w[0]).collect()
}

/// How a mic lines up against the reference mic over a hit's attack
/// (`attack` the averaged windows' frames that hold it): its offset in
/// samples (positive: late) found on the transients within `most`, whether
/// that is a clear peak (not at the edge of the search), and its polarity
/// (the correlation of the plain signals at that offset, −1 … 1).
#[must_use]
pub fn alignment(
    reference: &[f32],
    mic: &[f32],
    attack: std::ops::Range<usize>,
    most: usize,
) -> (i64, bool, f64) {
    let cut = |x: &[f32]| {
        x.get(attack.clone())
            .map(<[f32]>::to_vec)
            .unwrap_or_default()
    };
    let (r, m) = (cut(reference), cut(mic));
    let (offset, _) = lag(&difference(&r), &difference(&m), most);
    let shifted: Vec<f32> = (0..m.len())
        .map(|i| {
            usize::try_from(i as i64 + offset)
                .ok()
                .and_then(|j| m.get(j))
                .copied()
                .unwrap_or(0.0)
        })
        .collect();
    let (_, polarity) = lag(&r, &shifted, 0);
    (offset, offset.unsigned_abs() < most as u64, polarity)
}

/// The correlation of a mic with the rest of its sum (the sum without it)
/// across every hit's window, at no offset: negative, it pulls against
/// the other mics.
#[must_use]
pub fn against_rest(mic: &[Vec<f32>], sum: &[Vec<f32>]) -> f64 {
    let (mut mm, mut rr, mut mr) = (0.0f64, 0.0f64, 0.0f64);
    for (m, s) in mic.iter().zip(sum) {
        for (a, b) in m.iter().zip(s) {
            let (a, rest) = (f64::from(*a), f64::from(*b) - f64::from(*a));
            mm += a * a;
            rr += rest * rest;
            mr += a * rest;
        }
    }
    if mm * rr <= 1e-24 {
        return 0.0;
    }
    r2(mr / (mm * rr).sqrt())
}

/// The median gap from one hit to the next, seconds (`None` for fewer
/// than two hits).
#[must_use]
pub fn median_gap(times: &[f64]) -> Option<f64> {
    let mut gaps: Vec<f64> = times
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|g| *g > 0.0)
        .collect();
    if gaps.is_empty() {
        return None;
    }
    gaps.sort_by(f64::total_cmp);
    Some(gaps[gaps.len() / 2])
}

/// How one mic sits in the sum it feeds, from the hits' windows of both
/// (each mic as it reaches the sum, the sum as it is): `without_db`, how
/// much louder (positive) or quieter the sum is with the mic than without
/// it — negative is a mic that cancels; `share`, its in-phase part of the
/// sum's power (1 all of it, negative pulling against it).
#[must_use]
pub fn contribution(mic: &[Vec<f32>], sum: &[Vec<f32>]) -> (f64, f64) {
    let (mut ss, mut without, mut dot) = (0.0f64, 0.0f64, 0.0f64);
    for (m, s) in mic.iter().zip(sum) {
        for (a, b) in m.iter().zip(s) {
            let (a, b) = (f64::from(*a), f64::from(*b));
            ss += b * b;
            without += (b - a).powi(2);
            dot += a * b;
        }
    }
    if ss <= 1e-15 {
        return (0.0, 0.0);
    }
    let db = if without <= 1e-15 {
        FLOOR.abs()
    } else {
        10.0 * (ss / without).log10()
    };
    (r2(db), r2(dot / ss))
}

/// How the mics add, from each one's windows and the sum's: the sum's
/// level, what it would be if the mics did not correlate (their powers
/// added), and the difference — positive, they reinforce; negative, they
/// cancel.
#[must_use]
pub fn coherence(mics: &[&[Vec<f32>]], sum: &[Vec<f32>]) -> Value {
    let power = |w: &[Vec<f32>]| {
        let n: usize = w.iter().map(Vec::len).sum();
        w.iter()
            .flatten()
            .map(|s| f64::from(*s).powi(2))
            .sum::<f64>()
            / n.max(1) as f64
    };
    let sum_p = power(sum);
    let mics_p: f64 = mics.iter().map(|m| power(m)).sum();
    let db = |p: f64| if p <= 1e-15 { FLOOR } else { 10.0 * p.log10() };
    json!({
        "sum_db": r2(db(sum_p)),
        "power_sum_db": r2(db(mics_p)),
        "coherence_db": r2(db(sum_p) - db(mics_p)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// A decaying 60 Hz thump starting at `at`, `len` frames long.
    fn thump(len: usize, at: usize, gain: f32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                if i < at {
                    return 0.0;
                }
                let t = (i - at) as f32 / RATE as f32;
                gain * (std::f32::consts::TAU * 60.0 * t).sin() * (-t * 20.0).exp()
            })
            .collect()
    }

    #[test]
    fn a_mic_upside_down_and_late_is_found_and_cancels() {
        let a = thump(9_600, 480, 0.5);
        // The same hit, 12 samples late and inverted, a little quieter.
        let b: Vec<f32> = thump(9_600, 492, 0.4).iter().map(|s| -s).collect();
        let (l, c) = lag(&a, &b, 240);
        assert_eq!(l, 12, "{l} {c}");
        assert!(c < -0.9, "{c}");
        let sum: Vec<f32> = a.iter().zip(&b).map(|(x, y)| x + y).collect();
        let (without, share) = contribution(&[b.clone()], &[sum.clone()]);
        assert!(without < -3.0, "b cancels: {without}");
        assert!(share < 0.0, "{share}");
        let c = coherence(&[&[a][..], &[b][..]], &[sum]);
        assert!(c["coherence_db"].as_f64().unwrap() < -3.0, "{c}");
    }

    #[test]
    fn a_kick_upside_down_is_called_inverted_not_late() {
        let a = thump(9_600, 480, 0.5);
        let b: Vec<f32> = thump(9_600, 486, 0.5).iter().map(|s| -s).collect();
        let (offset, clear, polarity) = alignment(&a, &b, 384..2_400, 144);
        assert_eq!(offset, 6, "{offset}");
        assert!(clear);
        assert!(polarity < -0.9, "{polarity}");
        let sum: Vec<f32> = a.iter().zip(&b).map(|(x, y)| x + y).collect();
        assert!(against_rest(&[b], &[sum]) < -0.9);
        assert_eq!(median_gap(&[0.0, 0.2, 0.4, 0.5]), Some(0.2));
    }

    #[test]
    fn a_thump_has_a_fast_attack_and_a_measurable_decay() {
        let x = thump(19_200, 480, 0.8);
        let env = envelope(&x, RATE, 5.0);
        let (attack, decay) = shape(&env, RATE);
        assert!(attack < 5.0, "{attack}");
        // exp(-20 t) falls 20 dB in ln(10)/20 s ≈ 115 ms (from its peak).
        let decay = decay.expect("it decays inside the window");
        assert!((90.0..130.0).contains(&decay), "{decay}");
        let c = centroid(&x, RATE);
        assert!(c > 30.0 && c < 300.0, "{c}");
    }

    #[test]
    fn windows_average_and_run_off_the_ends_as_silence() {
        let x: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let w = windows(&x, &[2, 50], (4, 4));
        assert_eq!(w[0][..2], [0.0, 0.0]);
        assert_eq!(w[0][4], 2.0);
        assert_eq!(w[1][4], 50.0);
        assert_eq!(average(&w)[4], 26.0);
    }

    #[test]
    fn a_compressor_pulling_down_reads_as_gain_reduction() {
        // In: 1 s loud, 1 s quiet; out: the loud second 6 dB down.
        let input: Vec<f32> = (0..RATE as usize * 2)
            .flat_map(|i| {
                let g = if i < RATE as usize { 0.5 } else { 0.05 };
                let s = g * (i as f32 * 0.05).sin();
                [s, s]
            })
            .collect();
        let output: Vec<f32> = input
            .chunks(2)
            .enumerate()
            .flat_map(|(i, f)| {
                let g = if i < RATE as usize { 0.5 } else { 1.0 };
                [f[0] * g, f[1] * g]
            })
            .collect();
        let v = gain_reduction(&input, &output, RATE, 0.1);
        assert!((v["max_gr_db"].as_f64().unwrap() + 6.02).abs() < 0.2, "{v}");
        assert!(v["makeup_db"].as_f64().unwrap().abs() < 0.1, "{v}");
    }
}
