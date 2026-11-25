//! A transient designer: attack and sustain, each ± dB, independent of the
//! input's level.
//!
//! Two differential envelope pairs on the stereo peak (the SPL idea, from
//! the published principle: compare two followers that differ in one time
//! constant, so the gain depends on the shape of the envelope, not its
//! level):
//!
//! - **attack**: a fast follower against one whose attack is `attack_ms`;
//!   while a hit rises the fast one leads, and their normalised difference
//!   (0..1) is how much of `attack_db` applies;
//! - **sustain**: a follower whose release is `sustain_ms` against one that
//!   releases fast; as the hit decays the slow one holds, and their
//!   normalised difference is how much of `sustain_db` applies (a room's
//!   tail lifted or pulled back).
//!
//! At 0 dB both, it is an exact null. **Keyed** ([`TransientDesigner::set_keyed`]),
//! it shapes only within `key_window_ms` after each hit it is handed
//! ([`TransientDesigner::trigger`]): a snare's attack lifted, not the hi-hat
//! bleeding into its mic. `no_std`, no allocation.

/// The shaper's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Gain on the attack, dB (−24..24).
    pub attack_db: f64,
    /// Gain on the sustain, dB (−24..24).
    pub sustain_db: f64,
    /// How long a hit's attack counts, ms: the slow attack follower's attack.
    pub attack_ms: f64,
    /// How long a hit's sustain counts, ms: the slow sustain follower's release.
    pub sustain_ms: f64,
    /// Keyed: how long after each hit it shapes, ms.
    pub key_window_ms: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            attack_db: 0.0,
            sustain_db: 0.0,
            attack_ms: 10.0,
            sustain_ms: 150.0,
            key_window_ms: 250.0,
        }
    }
}

/// The fast followers' time constants, ms.
const FAST_ATTACK_MS: f64 = 0.1;
/// The peak envelope both pairs read: fast up, and released slowly enough
/// that a low drum's own cycles (a kick's 50 Hz) are not seen as attacks.
const ENVELOPE_RELEASE_MS: f64 = 50.0;
const ATTACK_PAIR_RELEASE_MS: f64 = 60.0;
/// The sustain pair's fast release, as a share of `sustain_ms` (at least
/// [`MIN_FAST_RELEASE_MS`]).
const FAST_RELEASE_SHARE: f64 = 0.1;
const MIN_FAST_RELEASE_MS: f64 = 5.0;
/// Under this level (−100 dBFS) there is nothing to shape.
const FLOOR: f64 = 1.0e-5;
/// The applied gain's own smoothing, ms: declicks the detector's edges.
const GAIN_SMOOTH_MS: f64 = 0.5;

/// A one-pole coefficient for `ms` at `sr`.
fn coef(ms: f64, sr: f64) -> f64 {
    let frames = ms * 0.001 * sr;
    if frames <= 1.0 {
        1.0
    } else {
        1.0 - libm::exp(-1.0 / frames)
    }
}

/// A follower with separate attack and release.
#[derive(Clone, Copy, Debug, Default)]
struct Follower {
    value: f64,
    attack: f64,
    release: f64,
}

impl Follower {
    #[inline]
    fn tick(&mut self, x: f64) -> f64 {
        let c = if x > self.value {
            self.attack
        } else {
            self.release
        };
        self.value += (x - self.value) * c;
        self.value
    }
}

/// The transient designer.
#[derive(Clone, Debug)]
pub struct TransientDesigner {
    settings: Settings,
    sample_rate: f64,
    envelope: Follower,
    attack_fast: Follower,
    attack_slow: Follower,
    sustain_slow: Follower,
    sustain_fast: Follower,
    gain_db: f64,
    gain_coef: f64,
    keyed: bool,
    window: usize,
    window_left: usize,
    triggered: bool,
}

impl TransientDesigner {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let mut t = Self {
            settings: Settings::default(),
            sample_rate,
            envelope: Follower::default(),
            attack_fast: Follower::default(),
            attack_slow: Follower::default(),
            sustain_slow: Follower::default(),
            sustain_fast: Follower::default(),
            gain_db: 0.0,
            gain_coef: 1.0,
            keyed: false,
            window: 0,
            window_left: 0,
            triggered: false,
        };
        t.set_settings(Settings::default());
        t
    }

    /// New settings (coefficients only; the followers keep their state).
    pub fn set_settings(&mut self, s: Settings) {
        self.settings = s;
        let sr = self.sample_rate;
        self.envelope.attack = coef(FAST_ATTACK_MS, sr);
        self.envelope.release = coef(ENVELOPE_RELEASE_MS, sr);
        self.attack_fast.attack = coef(FAST_ATTACK_MS, sr);
        self.attack_fast.release = coef(ATTACK_PAIR_RELEASE_MS, sr);
        self.attack_slow.attack = coef(s.attack_ms.max(FAST_ATTACK_MS), sr);
        self.attack_slow.release = coef(ATTACK_PAIR_RELEASE_MS, sr);
        self.sustain_slow.attack = coef(FAST_ATTACK_MS, sr);
        self.sustain_slow.release = coef(s.sustain_ms, sr);
        self.sustain_fast.attack = coef(FAST_ATTACK_MS, sr);
        self.sustain_fast.release = coef(
            (s.sustain_ms * FAST_RELEASE_SHARE).max(MIN_FAST_RELEASE_MS),
            sr,
        );
        self.gain_coef = coef(GAIN_SMOOTH_MS, sr);
        self.window = dsp_core::num::f64_to_index(s.key_window_ms * 0.001 * sr);
    }

    #[must_use]
    pub const fn settings(&self) -> Settings {
        self.settings
    }

    /// Shape only after the hits it is handed, from now on.
    pub const fn set_keyed(&mut self, keyed: bool) {
        self.keyed = keyed;
    }

    /// A hit at the next [`tick`](Self::tick): keyed, it shapes for
    /// `key_window_ms` from there.
    pub const fn trigger(&mut self) {
        self.triggered = true;
    }

    /// The gain it applied last, dB: a meter.
    #[must_use]
    pub const fn gain_db(&self) -> f64 {
        self.gain_db
    }

    /// One stereo frame.
    #[inline]
    pub fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        if core::mem::take(&mut self.triggered) {
            self.window_left = self.window;
        }
        let x = self.envelope.tick(left.abs().max(right.abs()));
        let af = self.attack_fast.tick(x);
        let asl = self.attack_slow.tick(x);
        let ss = self.sustain_slow.tick(x);
        let sf = self.sustain_fast.tick(x);
        let open = !self.keyed || self.window_left > 0;
        self.window_left = self.window_left.saturating_sub(1);
        let s = self.settings;
        let target = if open && (s.attack_db != 0.0 || s.sustain_db != 0.0) {
            let attack = if af > FLOOR {
                ((af - asl) / af).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let sustain = if ss > FLOOR {
                ((ss - sf) / ss).clamp(0.0, 1.0)
            } else {
                0.0
            };
            s.attack_db * attack + s.sustain_db * sustain
        } else {
            0.0
        };
        self.gain_db += (target - self.gain_db) * self.gain_coef;
        if libm::fabs(self.gain_db - target) < 1.0e-9 {
            self.gain_db = target;
        }
        if self.gain_db == 0.0 {
            return (left, right);
        }
        let g = libm::exp(self.gain_db * (core::f64::consts::LN_10 / 20.0));
        (left * g, right * g)
    }

    /// As built: followers, gain and key window cleared.
    pub const fn reset(&mut self) {
        self.envelope.value = 0.0;
        self.attack_fast.value = 0.0;
        self.attack_slow.value = 0.0;
        self.sustain_slow.value = 0.0;
        self.sustain_fast.value = 0.0;
        self.gain_db = 0.0;
        self.window_left = 0;
        self.triggered = false;
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use super::*;
    use alloc::vec::Vec;

    const SR: f64 = 48_000.0;

    /// A drum hit every `every` frames: an instant rise, a 60 ms decay, of
    /// a 180 Hz tone.
    fn hits(n: usize, every: usize, level: f64) -> Vec<f64> {
        (0..n)
            .map(|i| {
                let t = dsp_core::num::count_to_f64(i % every);
                level
                    * libm::exp(-t / (0.06 * SR))
                    * libm::sin(t * 180.0 * core::f64::consts::TAU / SR)
            })
            .collect()
    }

    fn run(t: &mut TransientDesigner, x: &[f64]) -> Vec<f64> {
        x.iter().map(|&s| t.tick(s, s).0).collect()
    }

    fn peak(x: &[f64]) -> f64 {
        x.iter().fold(0.0, |m, s| m.max(s.abs()))
    }

    fn db(x: f64) -> f64 {
        20.0 * libm::log10(x.max(1.0e-12))
    }

    #[test]
    fn at_zero_it_is_a_null() {
        let x = hits(48_000, 12_000, 0.5);
        let mut t = TransientDesigner::new(SR);
        assert_eq!(run(&mut t, &x), x);
    }

    /// Attack lifts the first milliseconds of each hit and leaves its tail;
    /// sustain lifts the tail and leaves its front; the same at any level.
    #[test]
    fn attack_and_sustain_shape_their_own_part_at_any_level() {
        for level in [0.5, 0.05] {
            let x = hits(48_000, 12_000, level);
            let front = 12_000..12_000 + 240; // the first 5 ms
            let tail = 12_000 + 4_800..12_000 + 9_600; // 100–200 ms in
            let mut a = TransientDesigner::new(SR);
            a.set_settings(Settings {
                attack_db: 6.0,
                ..Settings::default()
            });
            let y = run(&mut a, &x);
            let up = db(peak(&y[front.clone()]) / peak(&x[front.clone()]));
            let tail_a = db(peak(&y[tail.clone()]) / peak(&x[tail.clone()]));
            assert!(up > 3.0, "attack lifts the front at {level}: {up}");
            assert!(
                tail_a.abs() < 0.5,
                "and leaves the tail at {level}: {tail_a}"
            );
            let mut s = TransientDesigner::new(SR);
            s.set_settings(Settings {
                sustain_db: 6.0,
                ..Settings::default()
            });
            let y = run(&mut s, &x);
            let front_s = db(peak(&y[front.clone()]) / peak(&x[front]));
            let lifted = db(peak(&y[tail.clone()]) / peak(&x[tail]));
            assert!(
                front_s.abs() < 1.0,
                "sustain leaves the front at {level}: {front_s}"
            );
            assert!(lifted > 2.0, "and lifts the tail at {level}: {lifted}");
        }
    }

    /// Keyed, a hit it isn't handed (bleed) is left alone.
    #[test]
    fn keyed_it_shapes_only_after_its_hits() {
        let x = hits(48_000, 12_000, 0.5);
        let mut t = TransientDesigner::new(SR);
        t.set_settings(Settings {
            attack_db: 6.0,
            ..Settings::default()
        });
        t.set_keyed(true);
        let mut y = Vec::new();
        for (i, &s) in x.iter().enumerate() {
            if i == 24_000 {
                t.trigger();
            }
            y.push(t.tick(s, s).0);
        }
        assert_eq!(&y[..24_000], &x[..24_000], "the unkeyed hits untouched");
        let up = db(peak(&y[24_000..24_240]) / peak(&x[24_000..24_240]));
        assert!(up > 3.0, "its hit shaped: {up}");
        assert_eq!(
            &y[36_000 + 2_000..],
            &x[36_000 + 2_000..],
            "the next, unkeyed, not"
        );
    }
}
