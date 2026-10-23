//! A take played through a stretch map, its pitch kept: what stretch
//! markers ask for.
//!
//! A stretch map says where in the source each instant of a take plays
//! ([`Anchor`]s, piecewise linear between them). Between two anchors the
//! take plays at one rate; where that rate is 1 the source is copied as it
//! is, and anywhere else it is time-stretched.
//!
//! Stretch markers sit on hits — an edit moves a hit and stretches the
//! decay before the next one — so each stretched segment is played as a
//! drummer would want it:
//!
//! - **The attack as recorded.** The segment's first moments (up to
//!   [`MapConfig::attack`]) are copied from the source at speed: a hit's
//!   attack is never stretched, smeared or re-phased, and lands
//!   sample-exact where the map puts it.
//! - **The decay stretched by the phase vocoder** ([`Stretcher`]), started
//!   fresh where the attack copy ends: its first frames take the input's
//!   phases, so the copy crossfades into it without a seam, and its pitch
//!   is the source's whatever the rate.
//! - **Nothing of the next hit.** The vocoder reads a segment's source
//!   only up to a few milliseconds before where the next segment starts:
//!   the next attack is never smeared back into this decay (pre-echo).
//!   Those last milliseconds, and the joint past the end, are copied at
//!   speed, so the run-up to the next hit — and the first samples of its
//!   attack, when its marker sits where a detector fired, a little way in
//!   — play as recorded.
//! - **Mics stay in phase.** Every decision here is the map's — where the
//!   copies end, where the vocoder restarts — never the signal's: the
//!   mics of one kit, rendered one take at a time through the same map,
//!   are cut at the same instants and restarted from their own input
//!   phases there, so they stay as aligned with each other as they were
//!   recorded. (A splice search — WSOLA's — picks different splices on
//!   each mic, and the kit's tails comb against each other.)
//!
//! Segments are joined across a short linear crossfade ([`MapConfig::joint`]):
//! both sides are the same signal there, so linear keeps the level.
//!
//! Offline, and allocating: a render is done once per take and kept.

use crate::{Config, Source, Stretcher};

/// One point of a stretch map: output frame `out` plays source frame
/// `src`. Frames may be fractional.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub out: f64,
    pub src: f64,
}

/// How a map is rendered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapConfig {
    /// The vocoder's frame and overlap.
    pub vocoder: Config,
    /// How much of a stretched segment's start is copied as recorded,
    /// frames: the attack.
    pub attack: usize,
    /// The crossfade between segments, and between a segment's attack
    /// copy and its stretched decay, frames.
    pub joint: usize,
}

impl MapConfig {
    /// The defaults for a sample rate: an ~85 ms frame, the first 20 ms of
    /// each hit as recorded, 2 ms joints.
    #[must_use]
    pub fn for_sample_rate(sample_rate: f64) -> Self {
        let ms = |ms: f64| (sample_rate * ms / 1_000.0).round().max(1.0) as usize;
        Self {
            vocoder: Config::for_sample_rate(sample_rate),
            attack: ms(20.0),
            joint: ms(2.0),
        }
    }
}

/// A rate this close to 1 is played as it is.
const AT_SPEED: f64 = 1e-4;

/// Render `out_len` frames of `channels` (each a whole source) through
/// the map `anchors` (sorted by `out`; at least two). Before the first
/// anchor and after the last the map runs on at speed.
#[must_use]
pub fn render(
    channels: &[&[f32]],
    anchors: &[Anchor],
    out_len: usize,
    config: &MapConfig,
) -> Vec<Vec<f32>> {
    let mut out: Vec<Vec<f32>> = channels.iter().map(|_| vec![0.0; out_len]).collect();
    if channels.is_empty() || out_len == 0 {
        return out;
    }
    let cuts = cuts(anchors, out_len);
    let mut stretcher = Stretcher::new(config.vocoder);
    // A pair of channels at a time: the vocoder's phases are shared by a
    // pair (its mid), so a stereo source keeps its image.
    for (pair_at, pair) in channels.chunks(2).enumerate() {
        let (left, right) = match pair {
            [l, r] => (*l, *r),
            [mono] => (*mono, *mono),
            _ => continue,
        };
        let rendered = render_pair([left, right], &cuts, out_len, config, &mut stretcher);
        for (k, ch) in rendered.into_iter().enumerate().take(pair.len()) {
            if let Some(slot) = out.get_mut(pair_at * 2 + k) {
                *slot = ch;
            }
        }
    }
    out
}

/// The map as segments `(out from, out to, src from, src to)` covering
/// `0..out_len`, in whole frames.
fn cuts(anchors: &[Anchor], out_len: usize) -> Vec<(usize, usize, f64, f64)> {
    let end = out_len as f64;
    let src_at = |t: f64| -> f64 {
        let after = anchors.partition_point(|a| a.out <= t);
        match (
            after.checked_sub(1).and_then(|i| anchors.get(i)),
            anchors.get(after),
        ) {
            (Some(a), Some(b)) if b.out > a.out => {
                a.src + (t - a.out) * (b.src - a.src) / (b.out - a.out)
            }
            (Some(a), _) => a.src + (t - a.out),
            (None, Some(b)) => b.src - (b.out - t),
            (None, None) => t,
        }
    };
    let mut points: Vec<f64> = vec![0.0];
    points.extend(
        anchors
            .iter()
            .map(|a| a.out.round())
            .filter(|&t| t > 0.0 && t < end),
    );
    points.push(end);
    points.dedup();
    points
        .windows(2)
        .filter_map(|w| {
            let (a, b) = (w[0], w[1]);
            (b > a).then(|| (a as usize, b as usize, src_at(a), src_at(b)))
        })
        .collect()
}

/// One pair of channels through the segments.
fn render_pair(
    [left, right]: [&[f32]; 2],
    cuts: &[(usize, usize, f64, f64)],
    out_len: usize,
    config: &MapConfig,
    stretcher: &mut Stretcher,
) -> [Vec<f32>; 2] {
    let joint = config.joint.max(1);
    let mut out = [vec![0.0f32; out_len + joint], vec![0.0f32; out_len + joint]];
    let last = cuts.len().saturating_sub(1);
    for (k, &(o0, o1, s0, s1)) in cuts.iter().enumerate() {
        let n = o1 - o0;
        // Each but the last runs on a joint past its end, to fade out
        // under the next one fading in.
        let more = if k < last { joint } else { 0 };
        let rate = (s1 - s0) / n as f64;
        let mut piece = [vec![0.0f32; n + more], vec![0.0f32; n + more]];
        if (rate - 1.0).abs() <= AT_SPEED || s1 <= s0 {
            copy([left, right], s0, &mut piece, 0, n + more);
        } else {
            stretched(
                [left, right],
                (n, more),
                (s0, s1),
                config,
                stretcher,
                &mut piece,
            );
        }
        for (dest, src) in out.iter_mut().zip(&piece) {
            for (i, &s) in src.iter().enumerate() {
                let gain = if k > 0 && i < joint {
                    (i as f32 + 0.5) / joint as f32
                } else if i >= n {
                    1.0 - ((i - n) as f32 + 0.5) / joint as f32
                } else {
                    1.0
                };
                if let Some(slot) = dest.get_mut(o0 + i) {
                    *slot += s * gain;
                }
            }
        }
    }
    for ch in &mut out {
        ch.truncate(out_len);
    }
    out
}

/// One stretched segment: `n` frames (and `more` past them) playing
/// source `s0..s1` — its attack copied, its decay through the vocoder.
fn stretched(
    [left, right]: [&[f32]; 2],
    (n, more): (usize, usize),
    (s0, s1): (f64, f64),
    config: &MapConfig,
    stretcher: &mut Stretcher,
    piece: &mut [Vec<f32>; 2],
) {
    let span = s1 - s0;
    // The attack: at most a third of the segment either way, so a short
    // one still has room to stretch in.
    let keep = (config.attack as f64)
        .min(n as f64 / 3.0)
        .min(span / 3.0)
        .max(0.0)
        .floor() as usize;
    let joint = config.joint.max(1).min(keep.max(1));
    let rest_out = n - keep;
    let ratio = (span - keep as f64) / rest_out.max(1) as f64;
    // The run-up to the next segment: the last `lead` frames play the
    // source at speed, so a hit whose marker sits a few samples into its
    // attack (where a detector fires) keeps those samples as recorded.
    let lead = (2 * joint).min(rest_out / 3);
    // The decay, from where the attack copy ends: its source read only up
    // to where the run-up takes over. Reading on to the segment's end let
    // the vocoder frame the next attack's first samples, cut off hard, and
    // smear them up to a frame early — a pre-echo a detector fires on.
    let mut source = Bounded {
        channels: [left, right],
        until: (s1 - lead as f64).ceil() as i64,
    };
    let from = s0 + keep as f64;
    stretcher.seek(from, ratio, 1.0, &mut source);
    let len = rest_out + more;
    let (mut l, mut r) = (vec![0.0f32; len], vec![0.0f32; len]);
    stretcher.render(&mut l, &mut r, ratio, 1.0, &mut source);
    // The attack as recorded, on a joint into the decay.
    copy([left, right], s0, piece, 0, keep + joint);
    blend_from(piece, [&l, &r], keep, joint);
    // The run-up and the overrun past the end, at speed, on a joint out of
    // the decay. The overrun is what the next segment starts with, so the
    // two fade across each other as one signal; the vocoder's own overrun
    // decays into silence and dipped the next attack for a joint.
    let at = n - lead;
    let mut tail = [vec![0.0f32; lead + more], vec![0.0f32; lead + more]];
    copy([left, right], s1 - lead as f64, &mut tail, 0, lead + more);
    let [tl, tr] = &tail;
    blend_from(piece, [tl, tr], at, joint.min(lead));
}

/// Write `src` into `piece` from `at`, crossfading over its first `joint`
/// frames from what `piece` already holds there.
fn blend_from(piece: &mut [Vec<f32>; 2], src: [&[f32]; 2], at: usize, joint: usize) {
    for (dest, src) in piece.iter_mut().zip(src) {
        for (i, &s) in src.iter().enumerate() {
            let Some(slot) = dest.get_mut(at + i) else {
                break;
            };
            if i < joint {
                let t = (i as f32 + 0.5) / joint as f32;
                *slot = *slot * (1.0 - t) + s * t;
            } else {
                *slot = s;
            }
        }
    }
}

/// Copy source frames from `src` into `piece` at `at`, `len` of them.
fn copy([left, right]: [&[f32]; 2], src: f64, piece: &mut [Vec<f32>; 2], at: usize, len: usize) {
    let start = src.round() as i64;
    for (dest, ch) in piece.iter_mut().zip([left, right]) {
        for i in 0..len {
            let s = usize::try_from(start + i as i64)
                .ok()
                .and_then(|j| ch.get(j))
                .copied()
                .unwrap_or(0.0);
            if let Some(slot) = dest.get_mut(at + i) {
                *slot = s;
            }
        }
    }
}

/// A source read only before `until`: zeros from there on.
struct Bounded<'a> {
    channels: [&'a [f32]; 2],
    until: i64,
}

impl Source for Bounded<'_> {
    fn read(&mut self, start: i64, left: &mut [f32], right: &mut [f32]) {
        let [l, r] = self.channels;
        for (i, (a, b)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            let t = start + i as i64;
            let at = usize::try_from(t).ok().filter(|_| t < self.until);
            *a = at.and_then(|j| l.get(j)).copied().unwrap_or(0.0);
            *b = at.and_then(|j| r.get(j)).copied().unwrap_or(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    fn tone(freq: f64, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| (0.5 * (core::f64::consts::TAU * freq * i as f64 / SR).sin()) as f32)
            .collect()
    }

    fn crossings_hz(x: &[f32]) -> f64 {
        let n = x
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count();
        n as f64 / 2.0 / (x.len() as f64 / SR)
    }

    /// A drawn-out stretch keeps its pitch; where it plays at speed it is
    /// the source, sample for sample.
    #[test]
    fn a_drawn_out_tail_keeps_its_pitch() {
        let src = tone(440.0, 48_000);
        // 30 ms as played, then 70 ms of source over 280 ms.
        let anchors = [
            Anchor {
                out: 0.0,
                src: 9_600.0,
            },
            Anchor {
                out: 1_440.0,
                src: 11_040.0,
            },
            Anchor {
                out: 14_880.0,
                src: 14_400.0,
            },
        ];
        let config = MapConfig::for_sample_rate(SR);
        let out = render(&[&src], &anchors, 14_880, &config);
        let hz = crossings_hz(&out[0][4_800..13_000]);
        assert!((hz - 440.0).abs() < 440.0 * 0.01, "{hz:.1} Hz");
        for i in [10, 500, 1_000] {
            assert!((out[0][i] - src[9_600 + i]).abs() < 1e-6);
        }
    }

    /// A stretched segment starts with its attack exactly as recorded.
    #[test]
    fn the_attack_is_the_recording() {
        let src: Vec<f32> = (0..48_000)
            .map(|i| ((i as f32 * 0.37).sin() * (-(i as f32 % 6_000.0) / 900.0).exp()) as f32)
            .collect();
        let anchors = [
            Anchor {
                out: 0.0,
                src: 6_000.0,
            },
            Anchor {
                out: 9_000.0,
                src: 12_000.0,
            },
            Anchor {
                out: 15_000.0,
                src: 18_000.0,
            },
        ];
        let config = MapConfig::for_sample_rate(SR);
        let out = render(&[&src], &anchors, 15_000, &config);
        // The second segment's first attack frames (after the joint).
        for i in config.joint..config.attack - config.joint {
            assert!(
                (out[0][9_000 + i] - src[12_000 + i]).abs() < 1e-6,
                "frame {i}"
            );
        }
    }

    /// Two mics of one sound — one a little later — stretched one at a
    /// time through one map stay aligned with each other.
    #[test]
    fn two_mics_stay_in_phase() {
        let decay = |i: usize| (-(i as f64 % 12_000.0) / 4_000.0).exp();
        let a: Vec<f32> = (0..48_000)
            .map(|i| {
                let t = i as f64 / SR;
                (0.4 * decay(i)
                    * ((core::f64::consts::TAU * 110.0 * t).sin()
                        + 0.5 * (core::f64::consts::TAU * 347.0 * t).sin())) as f32
            })
            .collect();
        let delay = 37;
        let b: Vec<f32> = (0..48_000_usize)
            .map(|i| i.checked_sub(delay).map_or(0.0, |j| a[j]))
            .collect();
        let anchors = [
            Anchor { out: 0.0, src: 0.0 },
            Anchor {
                out: 12_000.0,
                src: 12_000.0,
            },
            Anchor {
                out: 30_000.0,
                src: 24_000.0,
            },
        ];
        let config = MapConfig::for_sample_rate(SR);
        let oa = render(&[&a], &anchors, 30_000, &config);
        let ob = render(&[&b], &anchors, 30_000, &config);
        // In the stretched decay, b is a still `delay` behind a.
        let (x, y) = (&oa[0][14_000..28_000], &ob[0][14_000..28_000]);
        let corr = |lag: usize| -> f64 {
            let dot: f64 = x
                .iter()
                .zip(&y[lag..])
                .map(|(p, q)| f64::from(*p) * f64::from(*q))
                .sum();
            let e = |s: &[f32]| s.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>().sqrt();
            dot / (e(x) * e(&y[lag..])).max(1e-12)
        };
        let at_delay = corr(delay);
        assert!(at_delay > 0.98, "aligned: {at_delay:.4}");
    }
}
