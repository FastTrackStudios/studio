//! Beats from the stems: one pulse over the weighted drum envelope, then
//! every beat moved onto a hit.
//!
//! The two steps the scripts ran (`dpbeats.py`, then `dpsnap.py`), ported
//! as they were: the maps the owner accepted came out of them.

use crate::audio::RATE;
use crate::beat;
use crate::features::{Features, HOP, KitHit, WEIGHTS};
use crate::fit;
use crate::onset;

/// How the beats are tracked.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TrackParams {
    /// The song's tempo, roughly: it decides the octave (half or double).
    pub bpm: f64,
    /// How much an interval off the tempo costs the tracker.
    pub tightness: f64,
    /// Beats before this are dropped (noise before the music), seconds.
    pub from: f64,
    /// How far a beat moves to a kit piece's hit, seconds.
    pub reach: f64,
    /// How far it moves to a cymbal's, seconds.
    pub loose_reach: f64,
    /// What becomes of the beats before the first beat on a hit and after
    /// the last: the scripts stacked them on that beat (numpy's `interp`),
    /// and the next pass spread them into the bar after it as extra
    /// beats, which is what `--from` and most hand fixes of a song's
    /// first bar were for. `false` keeps them at their own spacing.
    #[serde(default)]
    pub stack_edges: bool,
}

impl Default for TrackParams {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            tightness: 400.0,
            from: 0.0,
            reach: 0.12,
            loose_reach: 0.06,
            stack_edges: false,
        }
    }
}

/// The pulse, before it is snapped to the kit's hits.
#[derive(Debug, Clone)]
pub struct Pulse {
    /// The beats, lightly smoothed, seconds.
    pub beats: Vec<f64>,
    /// Each beat as placed: on a peak of the summed envelope, or where the
    /// tracker put it.
    pub raw: Vec<f64>,
    /// Whether each beat is on a peak of the envelope.
    pub on_hit: Vec<bool>,
    /// The stems the envelope was summed from.
    pub sources: Vec<String>,
    /// The tempo the tracker held, bpm.
    pub bpm: f64,
}

/// The summed onset envelope: every audible stem of [`WEIGHTS`] at its
/// weight, over the mix's length. With the stems it came from.
#[must_use]
pub fn envelope(features: &Features) -> (Vec<f64>, Vec<String>) {
    let len = (features.duration * f64::from(RATE) / HOP as f64) as usize + 2;
    let mut env = vec![0.0_f64; len];
    let mut used = Vec::new();
    for (name, weight) in WEIGHTS {
        let Some(stem) = features.stem(name) else {
            continue;
        };
        let Some(own) = &stem.envelope else {
            continue;
        };
        for (slot, v) in env.iter_mut().zip(own) {
            *slot += weight * f64::from(*v);
        }
        used.push(name.to_owned());
    }
    (env, used)
}

/// The pulse of the song (`dpbeats.py`): the beat tracker over the summed
/// envelope, each beat moved to the strongest peak of that envelope within
/// 30 ms, the beats after the last such peak dropped, and a light
/// smoothing (a line through each beat and its two neighbours).
#[must_use]
pub fn pulse(features: &Features, params: &TrackParams) -> Pulse {
    let (env, sources) = envelope(features);
    let found = beat::track(&env, RATE, HOP, params.bpm, params.tightness);
    let seconds = |frame: usize| (frame * HOP) as f64 / f64::from(RATE);
    let peaks = onset::peak_pick(&env, (4, 4), (30, 30), 0.15, 10);
    let mut raw = Vec::with_capacity(found.frames.len());
    let mut on_hit = Vec::with_capacity(found.frames.len());
    for frame in &found.frames {
        let t = seconds(*frame);
        let mut best: Option<(f64, f64)> = None;
        for p in &peaks {
            let at = seconds(*p);
            let off = (at - t).abs();
            if off >= 0.03 {
                continue;
            }
            let score = env[*p] - 3.0 * off / 0.03;
            if best.is_none_or(|(s, _)| score > s) {
                best = Some((score, at));
            }
        }
        match best {
            Some((_, at)) => {
                raw.push(at);
                on_hit.push(true);
            }
            None => {
                raw.push(t);
                on_hit.push(false);
            }
        }
    }
    if let Some(last) = on_hit.iter().rposition(|h| *h) {
        raw.truncate(last + 1);
        on_hit.truncate(last + 1);
    }
    let n = raw.len();
    let mut beats = raw.clone();
    for i in 0..n {
        let lo = i.saturating_sub(1);
        let hi = (i + 2).min(n);
        if hi - lo > 2 {
            let xs: Vec<f64> = (lo..hi).map(|k| k as f64).collect();
            let ws: Vec<f64> = on_hit[lo..hi]
                .iter()
                .map(|h| if *h { 1.0 } else { 0.3 })
                .collect();
            beats[i] = fit::line_at(&xs, &raw[lo..hi], &ws, i as f64);
        }
    }
    Pulse {
        beats,
        raw,
        on_hit,
        sources,
        bpm: found.bpm,
    }
}

/// What a beat is on: a kit piece's hit, or `between` two beats that are.
pub const BETWEEN: &str = "between";

/// Whether a beat with this source sits on a hit.
#[must_use]
pub fn on_a_hit(source: &str) -> bool {
    // A beat placed by hand is where the owner heard it: full weight.
    !matches!(source, "between" | "dp" | "interp" | "manual")
}

/// The beats snapped to the kit's hits (`dpsnap.py`): four passes of
/// moving each beat to the strongest hit near it (a kit piece within
/// `reach`, else a cymbal within `loose_reach`), refusing a move that
/// leaves the intervals either side more than a quarter apart, and spacing
/// the beats that did not move evenly between those that did. The last
/// beat goes to the strongest kit hit within 0.2 s. With each beat's
/// source.
#[must_use]
pub fn snap(beats: &[f64], hits: &[KitHit], params: &TrackParams) -> (Vec<f64>, Vec<String>) {
    let mut b: Vec<f64> = beats
        .iter()
        .copied()
        .filter(|t| *t >= params.from - 0.03)
        .collect();
    let n = b.len();
    let mut who = vec![BETWEEN.to_owned(); n];
    if n == 0 || hits.is_empty() {
        return (b, who);
    }
    for _ in 0..4 {
        let mut snapped: Vec<Option<f64>> = vec![None; n];
        who = vec![BETWEEN.to_owned(); n];
        for (i, t) in b.iter().enumerate() {
            if let Some(hit) = nearest(hits, *t, params) {
                snapped[i] = Some(hit.time);
                who[i].clone_from(&hit.piece);
            }
        }
        for _ in 0..2 {
            for i in 1..n.saturating_sub(1) {
                let Some(at) = snapped[i] else {
                    continue;
                };
                let left = snapped[i - 1].unwrap_or(b[i - 1]);
                let right = snapped[i + 1].unwrap_or(b[i + 1]);
                let ratio = (at - left) / (right - at).max(1e-3);
                if !(ratio > 0.8 && ratio < 1.25) {
                    snapped[i] = None;
                    BETWEEN.clone_into(&mut who[i]);
                }
            }
        }
        b = fill_between(&snapped, &b, params.stack_edges);
    }
    // The final hit of the song.
    if let Some(last) = b.last().copied() {
        let mut best: Option<&KitHit> = None;
        for hit in hits
            .iter()
            .filter(|h| !h.loose && (h.time - last).abs() < 0.2)
        {
            if best.is_none_or(|b| hit.strength > b.strength) {
                best = Some(hit);
            }
        }
        if let Some(hit) = best {
            if let Some(slot) = b.last_mut() {
                *slot = hit.time;
            }
            if let Some(slot) = who.last_mut() {
                *slot = format!("{} (final hit)", hit.piece);
            }
        }
    }
    (b, who)
}

/// The hit a beat at `t` moves to: the strongest kit-piece hit within
/// `reach` (strength less how far it is), else the strongest cymbal within
/// `loose_reach`.
fn nearest<'a>(hits: &'a [KitHit], t: f64, params: &TrackParams) -> Option<&'a KitHit> {
    let lo = hits.partition_point(|h| h.time <= t - params.reach.max(params.loose_reach));
    let mut best: Option<(f64, &KitHit)> = None;
    let near = hits[lo..]
        .iter()
        .take_while(|h| h.time < t + params.reach.max(params.loose_reach));
    let mut any_tight = false;
    let candidates: Vec<&KitHit> = near.collect();
    for hit in &candidates {
        if !hit.loose && (hit.time - t).abs() < params.reach {
            any_tight = true;
        }
    }
    for hit in candidates {
        let off = (hit.time - t).abs();
        let (ok, reach) = if any_tight {
            (!hit.loose && off < params.reach, params.reach)
        } else {
            (hit.loose && off < params.loose_reach, params.loose_reach)
        };
        if !ok {
            continue;
        }
        let score = f64::from(hit.strength) - 1.5 * off / reach;
        if best.is_none_or(|(s, _)| score > s) {
            best = Some((score, hit));
        }
    }
    best.map(|(_, hit)| hit)
}

/// The beats with the unsnapped ones spaced evenly between their snapped
/// neighbours. Before the first and after the last snapped beat they keep
/// their own spacing from it, or with `stack` take that beat's time
/// (numpy's `interp`). With nothing snapped, `fallback`.
fn fill_between(snapped: &[Option<f64>], fallback: &[f64], stack: bool) -> Vec<f64> {
    let known: Vec<(usize, f64)> = snapped
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.map(|t| (i, t)))
        .collect();
    let (Some(first), Some(last)) = (known.first(), known.last()) else {
        return fallback.to_vec();
    };
    let edge = |i: usize, anchor: &(usize, f64)| {
        if stack {
            anchor.1
        } else {
            anchor.1 + (fallback[i] - fallback[anchor.0])
        }
    };
    let mut out = Vec::with_capacity(snapped.len());
    let mut k = 0;
    for i in 0..snapped.len() {
        if i <= first.0 {
            out.push(edge(i, first));
            continue;
        }
        if i >= last.0 {
            out.push(edge(i, last));
            continue;
        }
        while k + 1 < known.len() && known[k + 1].0 <= i {
            k += 1;
        }
        let (i0, t0) = known[k];
        let (i1, t1) = known[(k + 1).min(known.len() - 1)];
        if i1 == i0 {
            out.push(t0);
        } else {
            out.push(t0 + (t1 - t0) * (i - i0) as f64 / (i1 - i0) as f64);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(time: f64, piece: &str) -> KitHit {
        KitHit {
            time,
            strength: 1.0,
            piece: piece.to_owned(),
            loose: matches!(piece, "Cymbals" | "Drums Other"),
        }
    }

    #[test]
    fn beats_move_onto_hits_and_the_rest_are_spaced() {
        // Hits every half second, the one at 1.5 missing.
        let hits: Vec<KitHit> = [0.5, 1.0, 2.0, 2.5, 3.0]
            .iter()
            .map(|t| hit(*t, "Kick"))
            .collect();
        let beats = [0.52, 0.98, 1.47, 2.03, 2.5, 3.01];
        let (out, who) = snap(&beats, &hits, &TrackParams::default());
        for (got, want) in out.iter().zip([0.5, 1.0, 1.5, 2.0, 2.5, 3.0]) {
            assert!((got - want).abs() < 1e-9, "{got} for {want}");
        }
        assert_eq!(who[2], BETWEEN);
        assert_eq!(who[0], "Kick");
    }

    #[test]
    fn a_cymbal_is_taken_only_with_no_kit_piece_near() {
        let hits = vec![hit(1.0, "Kick"), hit(1.04, "Cymbals"), hit(2.03, "Cymbals")];
        let params = TrackParams::default();
        assert_eq!(
            nearest(&hits, 1.03, &params).map(|h| h.piece.as_str()),
            Some("Kick")
        );
        assert_eq!(
            nearest(&hits, 2.0, &params).map(|h| h.piece.as_str()),
            Some("Cymbals")
        );
        assert!(nearest(&hits, 2.1, &params).is_none());
    }

    #[test]
    fn beats_before_the_first_hit_keep_their_spacing() {
        // The drums come in at 2.0: the two beats before have no hit.
        let hits: Vec<KitHit> = [2.0, 2.5, 3.0, 3.5, 4.0]
            .iter()
            .map(|t| hit(*t, "Kick"))
            .collect();
        let beats = [1.01, 1.51, 2.01, 2.51, 3.01, 3.51, 4.01];
        let (kept, _) = snap(&beats, &hits, &TrackParams::default());
        for (got, want) in kept.iter().zip([1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0]) {
            assert!((got - want).abs() < 1e-9, "{got} for {want}");
        }
        // As the scripts did it: stacked on the first hit, then spread
        // into the bar after it.
        let stacked = TrackParams {
            stack_edges: true,
            ..TrackParams::default()
        };
        let (old, _) = snap(&beats, &hits, &stacked);
        assert!(old[0] > 1.9, "{old:?}");
    }
}
