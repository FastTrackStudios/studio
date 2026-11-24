//! Detection for a folded group: sum the mics, detect once, merge lanes.
//!
//! Transients for a kit lane are found on the lane's **summed** signal —
//! the mean of its member mics — not per mic. Per-mic detection is the
//! thing the group rule forbids downstream (two mics would get different
//! cut points), and the sum is also simply a better detector input: the
//! mics are phase-aligned takes of one source, so summing raises the
//! drum against the bleed.
//!
//! The kick and snare lanes each detect on their own sum, and the two
//! hit lists are merged: union, with near-duplicates inside the
//! retrigger window collapsing to the louder. That merged list is the
//! kit's edit points.
//!
//! Spec: `drum-mode.md` r[drums.group.detection-source].

use crate::audio::detect::{DetectConfig, Transient, transients};

/// Mean of the members' mono samples, out to the longest member.
///
/// Mean rather than sum so the level (and so the absolute threshold in
/// the detector) does not depend on how many mics a lane has. A shorter
/// member contributes silence past its end — an item shorter than the
/// group is ordinary.
// r[impl drums.group.detection-source]
pub fn summed_signal(members: &[&[f64]]) -> Vec<f64> {
    let n = members.iter().map(|m| m.len()).max().unwrap_or(0);
    if n == 0 || members.is_empty() {
        return Vec::new();
    }
    let scale = 1.0 / members.len() as f64;
    let mut out = vec![0.0f64; n];
    for m in members {
        for (o, v) in out.iter_mut().zip(m.iter()) {
            *o += v * scale;
        }
    }
    out
}

/// Detect on a lane's summed signal.
// r[impl drums.group.detection-source]
pub fn detect_summed(members: &[&[f64]], sample_rate: f64, cfg: DetectConfig) -> Vec<Transient> {
    transients(&summed_signal(members), sample_rate, cfg)
}

/// Merge hit lists from several lanes into the kit's one edit list.
///
/// Union, sorted by time; two hits within `window_secs` of each other
/// are one hit, and the louder one is kept — the same contest the
/// spacing rule in detection runs, for the same reason: the ghost note
/// must not suppress the hit.
// r[impl drums.group.detection-source]
pub fn merge_hits(lists: &[&[Transient]], window_secs: f64) -> Vec<Transient> {
    let mut all: Vec<Transient> = lists.iter().flat_map(|l| l.iter().cloned()).collect();
    all.sort_by(|a, b| a.at.total_cmp(&b.at));
    let mut out: Vec<Transient> = Vec::with_capacity(all.len());
    for t in all {
        match out.last_mut() {
            Some(last) if t.at - last.at < window_secs => {
                if t.loudness > last.loudness {
                    *last = t;
                }
            }
            _ => out.push(t),
        }
    }
    out
}

/// Refine a hand-placed hit to the audio.
///
/// A click lands where the eye put it; the hit the user *meant* is the
/// attack nearby. This finds the strongest rise of the rectified signal
/// within `window_secs` either side of `at` — the biggest positive jump
/// between neighbouring 1 ms frames, which is where an attack starts —
/// and falls back to the click itself in silence.
// r[impl drums.manual.add-remove]
pub fn refine_onset(samples: &[f64], sample_rate: f64, at: f64, window_secs: f64) -> f64 {
    if samples.is_empty() || sample_rate <= 0.0 {
        return at;
    }
    let hop = (sample_rate / 1000.0).max(1.0) as usize; // 1 ms frames
    let center = (at * sample_rate) as isize;
    let half = (window_secs.max(0.0) * sample_rate) as isize;
    let lo = (center - half).max(0) as usize;
    let hi = ((center + half).max(0) as usize).min(samples.len());
    if hi <= lo + hop {
        return at;
    }
    let peak_of = |start: usize| -> f64 {
        samples[start..(start + hop).min(hi)]
            .iter()
            .fold(0.0f64, |m, v| m.max(v.abs()))
    };
    let mut best = (0.0f64, at);
    let mut prev = peak_of(lo);
    let mut i = lo + hop;
    while i + hop <= hi {
        let cur = peak_of(i);
        let rise = cur - prev;
        if rise > best.0 {
            best = (rise, i as f64 / sample_rate);
        }
        prev = cur;
        i += hop;
    }
    if best.0 > 1e-6 { best.1 } else { at }
}
