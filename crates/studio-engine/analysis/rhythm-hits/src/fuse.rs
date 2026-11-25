//! A trigger and a calibrated mic made one list of hits.
//!
//! Each has what the other lacks. A trigger hears only its drum, so it
//! is the best word on *whether* the drum was hit — but its spike is a
//! contact mic's, not the sound that is cut, and crosses its threshold
//! part way up. The close mic is the sound itself, so it is the word on
//! *when* the attack starts and *how hard* — but it hears the kit.
//!
//! So: a stroke both report is certain, and placed on the mic's attack.
//! One only the trigger reports is still placed on the mic (the trigger
//! says where to look) but flagged, being a choke or a neighbour's
//! crosstalk as often as a soft hit. One only the mic reports is a ghost
//! the trigger was too quiet for, or bleed — kept, flagged, and left
//! for someone to look at.

use crate::attack::{attack_peak, attack_start};
use crate::calibrate::TOLERANCE;

/// One stroke on a piece.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Where the attack starts on the close mic, in samples.
    pub at: usize,
    /// The attack's peak on the close mic, linear.
    pub peak: f32,
    /// How sure: 0 to 1.
    pub confidence: f64,
    /// The trigger reported it.
    pub from_trigger: bool,
    /// The close mic reported it.
    pub from_mic: bool,
    /// Much quieter than the piece usually is ([`GHOST_DB`] under its
    /// median).
    pub ghost: bool,
    /// The other stroke of a flam: its index in the same list.
    pub flam_with: Option<usize>,
}

/// A stroke both report.
pub const BOTH: f64 = 0.95;
/// A stroke only the trigger reports.
pub const TRIGGER_ONLY: f64 = 0.6;
/// A stroke only the calibrated mic reports.
pub const MIC_ONLY: f64 = 0.4;
/// How far under the piece's median peak a ghost note is, dB.
pub const GHOST_DB: f64 = 15.0;
/// Two strokes nearer than this are a flam, seconds.
pub const FLAM: f64 = 0.035;
/// Two attacks this near are one stroke found twice, seconds.
const SAME: f64 = 0.002;

/// One hit at `at`, measured on `mic`.
fn hit(mic: &[f32], rate: f64, at: usize, confidence: f64, trigger: bool, own: bool) -> Hit {
    let at = attack_start(mic, rate, at);
    Hit {
        at,
        peak: attack_peak(mic, rate, at),
        confidence,
        from_trigger: trigger,
        from_mic: own,
        ghost: false,
        flam_with: None,
    }
}

/// The piece's hits from its trigger's (`trigger`) and its calibrated
/// mic's (`mic_hits`), both sorted sample positions on the same clock,
/// placed and measured on `mic`.
#[must_use]
pub fn fuse(mic: &[f32], rate: f64, trigger: &[usize], mic_hits: &[usize]) -> Vec<Hit> {
    let tolerance = crate::samples(TOLERANCE, rate);
    let mut out = Vec::with_capacity(trigger.len().max(mic_hits.len()));
    let (mut i, mut j) = (0_usize, 0_usize);
    loop {
        match (mic_hits.get(i), trigger.get(j)) {
            (Some(&m), Some(&t)) if m.abs_diff(t) <= tolerance => {
                out.push(hit(mic, rate, m, BOTH, true, true));
                i = i.saturating_add(1);
                j = j.saturating_add(1);
            }
            (Some(&m), Some(&t)) if m < t => {
                out.push(hit(mic, rate, m, MIC_ONLY, false, true));
                i = i.saturating_add(1);
            }
            (_, Some(&t)) => {
                out.push(hit(mic, rate, t, TRIGGER_ONLY, true, false));
                j = j.saturating_add(1);
            }
            (Some(&m), None) => {
                out.push(hit(mic, rate, m, MIC_ONLY, false, true));
                i = i.saturating_add(1);
            }
            (None, None) => break,
        }
    }
    settle(out, rate)
}

/// How far a trigger must rise over what it held just before for a
/// mic-only hit to count as heard by it too, dB.
pub const RISE_DB: f64 = 12.0;
/// What such a hit is trusted to.
pub const CORROBORATED: f64 = 0.85;

/// The loudest sample of `samples` from `from` to `to` (seconds around
/// `at`), linear.
fn peak_around(samples: &[f32], rate: f64, at: usize, (from, to): (f64, f64)) -> f64 {
    let lo = if from < 0.0 {
        at.saturating_sub(crate::samples(-from, rate))
    } else {
        at.saturating_add(crate::samples(from, rate))
    };
    let hi = at
        .saturating_add(crate::samples(to, rate))
        .min(samples.len());
    samples.get(lo.min(hi)..hi).map_or(0.0, |s| {
        f64::from(s.iter().fold(0.0_f32, |m, x| m.max(x.abs())))
    })
}

/// A mic-only hit the trigger rose at too — under its spike threshold, a
/// soft stroke, but plainly there — counted as heard by both.
///
/// A trigger spike is a hit only within `under_db` of the trigger's
/// loudest, so a soft stroke in a fill reaches the mic and not the
/// spike list: on Unbreakable a snare stroke whose trigger jumped 19 dB,
/// to 2 dB under the cut, was left as "mic only, unsure". What tells it
/// from bleed is the trigger itself: bleed barely moves a contact mic.
pub fn corroborate(hits: &mut [Hit], trigger: &[f32], rate: f64) {
    let loudest = f64::from(trigger.iter().fold(0.0_f32, |m, x| m.max(x.abs())));
    let audible = loudest * crate::gain(-45.0);
    for h in hits.iter_mut().filter(|h| h.from_mic && !h.from_trigger) {
        let now = peak_around(trigger, rate, h.at, (-0.001, 0.008));
        let before = peak_around(trigger, rate, h.at, (-0.030, -0.005));
        if now >= audible && now >= before * crate::gain(RISE_DB) {
            h.from_trigger = true;
            h.confidence = h.confidence.max(CORROBORATED);
        }
    }
}

/// Sort, merge strokes found twice, mark the ghosts and the flams.
#[must_use]
pub fn settle(mut hits: Vec<Hit>, rate: f64) -> Vec<Hit> {
    hits.sort_by_key(|h| h.at);
    let same = crate::samples(SAME, rate);
    let mut merged: Vec<Hit> = Vec::with_capacity(hits.len());
    for h in hits {
        match merged.last_mut() {
            Some(last) if h.at.abs_diff(last.at) <= same => {
                last.from_trigger |= h.from_trigger;
                last.from_mic |= h.from_mic;
                last.confidence = if last.from_trigger && last.from_mic {
                    BOTH
                } else {
                    last.confidence.max(h.confidence)
                };
                last.peak = last.peak.max(h.peak);
            }
            _ => merged.push(h),
        }
    }
    let mut peaks: Vec<f32> = merged.iter().map(|h| h.peak).collect();
    peaks.sort_by(f32::total_cmp);
    if let Some(median) = peaks.get(peaks.len() / 2) {
        let floor = f64::from(*median) * crate::gain(-GHOST_DB);
        for h in &mut merged {
            h.ghost = f64::from(h.peak) < floor;
        }
    }
    pair_flams(&mut merged, rate);
    merged
}

/// Mark each two neighbouring strokes nearer than [`FLAM`] as a flam's
/// two halves.
pub fn pair_flams(hits: &mut [Hit], rate: f64) {
    let window = crate::samples(FLAM, rate);
    let pairs: Vec<(usize, usize)> = hits
        .windows(2)
        .enumerate()
        .filter_map(|(i, w)| match w {
            [a, b] if b.at.saturating_sub(a.at) < window => Some((i, i.saturating_add(1))),
            _ => None,
        })
        .collect();
    for (a, b) in pairs {
        if let Some(h) = hits.get_mut(a) {
            h.flam_with = Some(b);
        }
        if let Some(h) = hits.get_mut(b) {
            h.flam_with = Some(a);
        }
    }
}
