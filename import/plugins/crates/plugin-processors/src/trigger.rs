//! Drum hit detection: `dsp::trigger`'s detector on a trigger mic (a kick
//! or snare trigger, or the close mic itself). What a kit's `Trig` lane
//! runs: its hits key the piece's gate and play its reinforcement sample.

use dsp::trigger::TriggerChain;
use dsp_core::audio::{AudioConfig, Processor as _};

/// A hit: detected `offset` frames into the piece, `velocity` 0–1. Its
/// onset was `lag` frames before (at most [`HitDetector::lag`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub offset: u32,
    pub velocity: f32,
    pub lag: u32,
}

/// Quiet this long (below the floor) is before a hit, not inside it: a
/// kick's zero crossings are shorter.
const QUIET: usize = 16;

/// The most hits taken from one piece (a 1024-frame piece holds at most a
/// few real ones; the detector's retrigger guard is milliseconds).
pub const MAX_HITS: usize = 16;

pub struct HitDetector {
    chain: TriggerChain,
    /// How far back from a detection its onset is looked for: the
    /// confirmation window (the detector fires that long after it crosses
    /// the threshold) and as long again before the crossing, where the
    /// transient began below the threshold.
    window: usize,
    /// The sidechain's last `window` frames, rectified.
    history: alloc::vec::Vec<f32>,
    at: usize,
    /// Below this a frame is quiet: 12 dB under the threshold.
    floor: f32,
    hits: [Hit; MAX_HITS],
    count: usize,
}

impl HitDetector {
    /// A detector at `sample_rate`, firing above `threshold_db` (dBFS).
    #[must_use]
    pub fn new(sample_rate: f32, max_block: usize, threshold_db: f32) -> Self {
        let mut chain = TriggerChain::new();
        chain.threshold_db = f64::from(threshold_db);
        chain.update(AudioConfig {
            sample_rate: f64::from(sample_rate),
            max_buffer_size: max_block,
        });
        // `detect_time_ms` (1 ms by default) at this rate, as `TriggerChain`
        // counts it.
        let confirm = (chain.detect_time_ms * 0.001 * f64::from(sample_rate)) as usize;
        let window = confirm.saturating_mul(2).max(QUIET * 2);
        Self {
            chain,
            window,
            history: alloc::vec![0.0; window + 1],
            at: 0,
            floor: libm::powf(10.0, (threshold_db - 12.0) / 20.0),
            hits: [Hit {
                offset: 0,
                velocity: 0.0,
                lag: 0,
            }; MAX_HITS],
            count: 0,
        }
    }

    /// Detect the hits in one piece. No allocation.
    pub fn detect(&mut self, left: &[f32], right: &[f32]) -> &[Hit] {
        self.count = 0;
        for (i, (l, r)) in left.iter().zip(right).enumerate() {
            let n = self.history.len();
            if let Some(h) = self.history.get_mut(self.at) {
                *h = (0.5 * (l + r)).abs();
            }
            let fired = self.chain.detect_tick(f64::from(*l), f64::from(*r));
            let lag = if fired.is_some() { self.onset_lag() } else { 0 };
            self.at = (self.at + 1) % n;
            if let Some(v) = fired {
                if let Some(slot) = self.hits.get_mut(self.count) {
                    // The velocity as FTS Trigger sends it (its MIDI value,
                    // 0–1).
                    let midi = dsp::trigger::velocity::VelocityMapper::to_midi(v);
                    *slot = Hit {
                        offset: u32::try_from(i).unwrap_or(u32::MAX),
                        velocity: (f32::from(midi) / 127.0).clamp(0.0, 1.0),
                        lag: u32::try_from(lag).unwrap_or(0),
                    };
                    self.count += 1;
                }
            }
        }
        self.hits.get(..self.count).unwrap_or(&[])
    }

    /// How many frames back, from the one just detected, the hit began:
    /// back through the history to the last quiet stretch, then forward
    /// to the first frame above the floor.
    fn onset_lag(&self) -> usize {
        let n = self.history.len();
        let back = |k: usize| {
            self.history
                .get((self.at + n - k % n) % n)
                .copied()
                .unwrap_or(0.0)
        };
        let mut quiet = 0;
        let mut onset = 0;
        for k in 0..n.min(self.window + 1) {
            if back(k) < self.floor {
                quiet += 1;
                if quiet >= QUIET {
                    break;
                }
            } else {
                quiet = 0;
                onset = k;
            }
        }
        onset
    }

    /// The most frames a hit's onset is before its detection. A caller
    /// that looks ahead this far places every hit on its onset (Signal's
    /// graph delays its audio by it, and places each hit `Hit::lag` back);
    /// one that can't places it back, clamped to the piece.
    #[must_use]
    pub fn lag(&self) -> u32 {
        u32::try_from(self.window).unwrap_or(u32::MAX)
    }

    pub fn reset(&mut self) {
        self.chain.reset();
        self.history.fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hit_is_found_at_its_frame() {
        let mut d = HitDetector::new(48_000.0, 512, -30.0);
        let mut l = alloc::vec![0.0_f32; 512];
        // A decaying burst at frame 200.
        for (i, s) in l.iter_mut().enumerate().skip(200).take(200) {
            *s = 0.9 * libm::expf(-((i - 200) as f32) / 40.0) * if i % 2 == 0 { 1.0 } else { -1.0 };
        }
        let r = l.clone();
        let hits = d.detect(&l, &r).to_vec();
        assert_eq!(hits.len(), 1, "one burst, one hit: {hits:?}");
        let onset = hits[0].offset - hits[0].lag;
        assert_eq!(d.lag(), 96, "2 ms at 48 kHz");
        assert_eq!(onset, 200, "{hits:?}");
        assert!(hits[0].velocity > 0.0);
        // Silence after: none.
        let z = alloc::vec![0.0_f32; 512];
        assert!(d.detect(&z, &z).is_empty());
    }
}
