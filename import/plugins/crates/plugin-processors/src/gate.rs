//! The drum gate: `dsp::gate::DrumGate`.
//!
//! Keyed from its own input, or from a key handed in before a piece
//! ([`BlockProcessor::set_key`]): in a kit, the piece's `Trig` channel, so
//! the gate opens on the drum and not on the bleed.

use dsp::gate::{DrumGate, Mode, Settings};

use crate::param::{ParamSpec, Unit};
use crate::{BlockProcessor, ModSourceSpec};

const MODES: &[&str] = &["Kick", "Snare Top", "Snare Bottom", "Toms"];

pub const PARAMS: &[ParamSpec] = &[
    ParamSpec::new("threshold", "Threshold", Unit::Db, -70.0, 0.0, -30.0),
    ParamSpec::new("reduction", "Reduction", Unit::Db, -80.0, 0.0, -80.0),
    ParamSpec::new("length", "Length", Unit::Ms, 50.0, 2000.0, 300.0),
    ParamSpec::choice("mode", "Mode", MODES),
];

pub const SOURCES: &[ModSourceSpec] = &[ModSourceSpec {
    id: "gate",
    name: "Gate",
    unit: Unit::Percent,
}];

pub struct Gate {
    gate: DrumGate,
    settings: Settings,
    sample_rate: f64,
    /// The key for the next piece (mono-summed), and how much of it there is.
    key: alloc::vec::Vec<f64>,
    keyed: usize,
    /// Hits for the next piece, as frame offsets: a key built from them
    /// when no audio key comes (a trigger on another track, whose hits
    /// arrive over the session and not its audio).
    hits: [u32; MAX_HITS],
    hit_count: usize,
    /// Frames of the last piece's pulse still to come.
    pulse_left: usize,
    /// Once a hit has come, the gate is hit-keyed: between hits its key
    /// is silence, not its own (bleeding) input.
    hit_keyed: bool,
    /// Another lane keys it (`keyed`): hit-keyed from the start, a reset
    /// too.
    wired: bool,
    pulse: usize,
    /// Frames since the last hit, and how long without one before the
    /// gate keys from its own input again: a dead trigger never silences
    /// the drum.
    since_hit: usize,
    fail_open_after: usize,
}

const MAX_HITS: usize = 16;

impl Gate {
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        Self::with_max_block(sample_rate, 1024)
    }

    /// A gate whose key can be up to `max_block` frames a piece.
    #[must_use]
    pub fn with_max_block(sample_rate: f32, max_block: usize) -> Self {
        let settings = Settings::default();
        let sr = f64::from(sample_rate);
        Self {
            gate: DrumGate::new(sr, 2, settings),
            settings,
            sample_rate: sr,
            key: alloc::vec![0.0; max_block],
            keyed: 0,
            hits: [0; MAX_HITS],
            hit_count: 0,
            pulse_left: 0,
            hit_keyed: false,
            wired: false,
            // A 2 ms full-level pulse per hit: past any threshold, and
            // the gate's own hold does the rest.
            pulse: (sr * 0.002) as usize,
            since_hit: 0,
            fail_open_after: (sr * 10.0) as usize,
        }
    }

    /// The key from the hits: a pulse at each.
    fn key_from_hits(&mut self, n: usize) {
        let Some(key) = self.key.get_mut(..n) else {
            return;
        };
        key.fill(0.0);
        let mut fill = |from: usize, len: usize, key: &mut [f64]| -> usize {
            let end = (from + len).min(n);
            if let Some(k) = key.get_mut(from.min(end)..end) {
                k.fill(1.0);
            }
            from + len - end
        };
        let mut left = fill(0, self.pulse_left, key);
        for h in self.hits.iter().take(core::mem::take(&mut self.hit_count)) {
            let at = usize::try_from(*h).unwrap_or(0).min(n);
            left = left.max(fill(at, self.pulse, key));
        }
        self.pulse_left = left;
        self.keyed = n;
    }
}

impl BlockProcessor for Gate {
    fn params(&self) -> &'static [ParamSpec] {
        PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let v = f64::from(value);
        match index {
            0 => self.settings.threshold_db = v,
            1 => self.settings.reduction_db = v,
            2 => self.settings.length_ms = v,
            3 => {
                self.settings.mode = match dsp_core::num::f32_to_index(value) {
                    1 => Mode::SnareTop,
                    2 => Mode::SnareBottom,
                    3 => Mode::Toms,
                    _ => Mode::Kick,
                };
            }
            _ => return,
        }
        self.gate.set_settings(self.settings);
    }

    /// As it was built: its envelope, its key and hits, and how long since
    /// a hit all cleared (a gate that had gone ten seconds without one
    /// waits for a hit again).
    fn reset(&mut self) {
        self.gate.reset();
        self.key.fill(0.0);
        self.keyed = 0;
        self.hit_count = 0;
        self.pulse_left = 0;
        self.hit_keyed = self.wired;
        self.since_hit = 0;
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len()).min(self.key.len());
        if self.hit_count > 0 {
            self.since_hit = 0;
        } else {
            self.since_hit = self.since_hit.saturating_add(n);
        }
        if self.keyed == 0 && self.hit_keyed && self.since_hit < self.fail_open_after {
            self.key_from_hits(n);
        }
        let keyed = core::mem::take(&mut self.keyed);
        // Hits that came with an audio key: the audio keyed this piece.
        self.hit_count = 0;
        for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            let mut frame = [f64::from(*l), f64::from(*r)];
            match self.key.get(i).filter(|_| i < keyed) {
                // The key on both channels: the detector takes the louder.
                Some(k) => self.gate.process_frame(&mut frame, Some(&[*k, *k])),
                None => self.gate.process_frame(&mut frame, None),
            }
            *l = dsp_core::num::narrow(frame[0]);
            *r = dsp_core::num::narrow(frame[1]);
        }
    }

    fn keyed(&mut self) {
        self.wired = true;
        self.hit_keyed = true;
        self.since_hit = 0;
    }

    fn hit(&mut self, offset: u32, _velocity: f32) {
        self.hit_keyed = true;
        if let Some(slot) = self.hits.get_mut(self.hit_count) {
            *slot = offset;
            self.hit_count += 1;
        }
    }

    fn set_key(&mut self, left: &[f32], right: &[f32]) {
        let n = left.len().min(right.len()).min(self.key.len());
        for ((k, l), r) in self.key.iter_mut().zip(left).zip(right).take(n) {
            *k = (f64::from(*l) + f64::from(*r)) * 0.5;
        }
        self.keyed = n;
    }

    fn mod_sources(&self) -> &'static [ModSourceSpec] {
        SOURCES
    }

    fn mod_source(&self, index: usize) -> f32 {
        if index == 0 {
            dsp_core::num::narrow(self.gate.gain())
        } else {
            0.0
        }
    }

    fn latency(&self) -> u32 {
        u32::try_from(DrumGate::latency_for(self.settings.mode, self.sample_rate)).unwrap_or(0)
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn a_keyed_gate_follows_its_key_not_its_input() {
        let mut g = Gate::with_max_block(48_000.0, 4096);
        // Loud input (bleed), a silent key: the gate stays shut.
        let quiet = alloc::vec![0.0_f32; 4096];
        let mut l = alloc::vec![0.5_f32; 4096];
        let mut r = l.clone();
        for _ in 0..4 {
            l.fill(0.5);
            r.fill(0.5);
            g.set_key(&quiet, &quiet);
            g.process(&mut l, &mut r);
        }
        let shut = l.iter().map(|s| s.abs()).fold(0.0, f32::max);
        // The same input, keyed by itself: it opens.
        let mut g = Gate::with_max_block(48_000.0, 4096);
        for _ in 0..4 {
            l.fill(0.5);
            r.fill(0.5);
            let key = l.clone();
            g.set_key(&key, &key);
            g.process(&mut l, &mut r);
        }
        let open = l.iter().map(|s| s.abs()).fold(0.0, f32::max);
        assert!(open > shut * 4.0, "open {open}, shut {shut}");
    }
}

#[cfg(test)]
mod hit_tests {
    use super::*;

    /// A gate keyed by hits alone (its trigger on another track): open at
    /// each hit, shut on the bleed between, whatever its own input.
    #[test]
    fn hits_key_a_gate_with_no_audio_key() {
        let mut g = Gate::with_max_block(48_000.0, 512);
        g.keyed();
        g.set_param(2, 100.0); // open 100 ms
        let mut out = alloc::vec::Vec::new();
        for piece in 0..200 {
            // Loud bleed throughout: alone it would hold the gate open.
            let (mut l, mut r) = (alloc::vec![0.3_f32; 512], alloc::vec![0.3_f32; 512]);
            if piece == 20 || piece == 120 {
                g.hit(100, 1.0);
            }
            g.process(&mut l, &mut r);
            out.extend(l);
        }
        let level =
            |from: usize, to: usize| out[from..to].iter().map(|s| s.abs()).fold(0.0, f32::max);
        let hit = 20 * 512 + 100;
        let shut = level(hit - 5_000, hit - 1_000);
        let open = level(hit + 2_000, hit + 3_000);
        let after = level(hit + 30_000, hit + 40_000);
        assert!(open > 0.2, "open after the hit: {open}");
        assert!(shut < open * 0.1, "shut before: {shut}");
        assert!(after < open * 0.1, "shut again, the bleed ignored: {after}");
    }
}

#[cfg(test)]
mod fail_open_tests {
    use super::*;

    /// A gate keyed by a trigger that never fires opens on its own input
    /// after ten seconds: the drum is never silenced for good.
    #[test]
    fn a_dead_trigger_never_silences_the_drum() {
        let mut g = Gate::with_max_block(48_000.0, 512);
        g.keyed();
        let mut level_at = alloc::vec::Vec::new();
        for piece in 0..(48_000 * 12 / 512) {
            let (mut l, mut r) = (alloc::vec![0.5_f32; 512], alloc::vec![0.5_f32; 512]);
            g.process(&mut l, &mut r);
            if piece % (48_000 / 512) == 0 {
                level_at.push(l.iter().map(|s| s.abs()).fold(0.0, f32::max));
            }
        }
        assert!(
            level_at[5] < 0.05,
            "shut while it waits for a hit: {level_at:?}"
        );
        assert!(
            level_at[11] > 0.25,
            "open on its own input after 10 s: {level_at:?}"
        );
    }

    /// Reset after it failed open, it sounds as a fresh one: shut, waiting
    /// for a hit.
    #[test]
    fn a_reset_gate_waits_for_a_hit_again() {
        let run = |g: &mut Gate, pieces: usize| -> alloc::vec::Vec<f32> {
            let mut out = alloc::vec::Vec::new();
            for _ in 0..pieces {
                let (mut l, mut r) = (alloc::vec![0.5_f32; 512], alloc::vec![0.5_f32; 512]);
                g.process(&mut l, &mut r);
                out.extend_from_slice(&l);
            }
            out
        };
        let mut warm = Gate::with_max_block(48_000.0, 512);
        warm.keyed();
        let _ = run(&mut warm, 48_000 * 11 / 512);
        warm.reset();
        let mut fresh = Gate::with_max_block(48_000.0, 512);
        fresh.keyed();
        fresh.reset();
        assert_eq!(run(&mut warm, 200), run(&mut fresh, 200));
    }
}
