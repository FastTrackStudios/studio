//! The transient shaper: `dsp::transient::TransientDesigner`.
//!
//! Attack and sustain, each ± dB, on a level-independent detector: a
//! room's tail lifted and its attack pulled back, a snare's crack brought
//! forward. Keyed by another lane (`key`, a gate's way), it shapes only for
//! `key_window` after each of that lane's hits, so the bleed's own
//! transients are left alone; with no hit for ten seconds it shapes
//! everything again, unless the host placed every hit (a gap is then the
//! song's).

use dsp::transient::{Settings, TransientDesigner};

use crate::param::{ParamSpec, Unit};
use crate::{BlockProcessor, ModSourceSpec};

pub const PARAMS: &[ParamSpec] = &[
    ParamSpec::new("attack", "Attack", Unit::Db, -24.0, 24.0, 0.0).symmetrical_skewed(1.0, 0.0),
    ParamSpec::new("sustain", "Sustain", Unit::Db, -24.0, 24.0, 0.0).symmetrical_skewed(1.0, 0.0),
    ParamSpec::new("attack_time", "Attack Time", Unit::Ms, 1.0, 50.0, 10.0).skewed(0.5),
    ParamSpec::new(
        "sustain_time",
        "Sustain Time",
        Unit::Ms,
        20.0,
        1000.0,
        150.0,
    )
    .skewed(0.5),
    ParamSpec::new("key_window", "Key Window", Unit::Ms, 10.0, 2000.0, 250.0).skewed(0.5),
    ParamSpec::new("output", "Output", Unit::Db, -24.0, 24.0, 0.0),
];

pub const SOURCES: &[ModSourceSpec] = &[ModSourceSpec {
    id: "applied_gain",
    name: "Gain",
    unit: Unit::Db,
}];

const MAX_HITS: usize = 16;

pub struct Transient {
    shaper: TransientDesigner,
    settings: Settings,
    output: f64,
    hits: [u32; MAX_HITS],
    hit_count: usize,
    /// Another lane keys it.
    wired: bool,
    /// Every hit placed: never fall back to shaping everything.
    hits_known: bool,
    since_hit: usize,
    fail_open_after: usize,
}

impl Transient {
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sr = f64::from(sample_rate);
        Self {
            shaper: TransientDesigner::new(sr),
            settings: Settings::default(),
            output: 1.0,
            hits: [0; MAX_HITS],
            hit_count: 0,
            wired: false,
            hits_known: false,
            since_hit: 0,
            fail_open_after: dsp_core::num::f64_to_index(sr * 10.0),
        }
    }
}

impl BlockProcessor for Transient {
    fn params(&self) -> &'static [ParamSpec] {
        PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let v = f64::from(value);
        let s = &mut self.settings;
        match index {
            0 => s.attack_db = v,
            1 => s.sustain_db = v,
            2 => s.attack_ms = v,
            3 => s.sustain_ms = v,
            4 => s.key_window_ms = v,
            5 => {
                self.output = f64::from(crate::db_to_gain(value));
                return;
            }
            _ => return,
        }
        if self.shaper.settings() != self.settings {
            self.shaper.set_settings(self.settings);
        }
    }

    fn reset(&mut self) {
        self.shaper.reset();
        self.shaper.set_keyed(self.wired);
        self.hit_count = 0;
        self.since_hit = 0;
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        let count = core::mem::take(&mut self.hit_count);
        if count > 0 {
            self.since_hit = 0;
        } else {
            self.since_hit = self.since_hit.saturating_add(n);
        }
        let keyed = self.wired && (self.hits_known || self.since_hit < self.fail_open_after);
        self.shaper.set_keyed(keyed);
        let hits = self.hits.get(..count).unwrap_or(&[]);
        let g = self.output;
        for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            if hits
                .iter()
                .any(|&h| usize::try_from(h).unwrap_or(usize::MAX) == i)
            {
                self.shaper.trigger();
            }
            let (a, b) = self.shaper.tick(f64::from(*l), f64::from(*r));
            *l = dsp_core::num::narrow(a * g);
            *r = dsp_core::num::narrow(b * g);
        }
    }

    fn keyed(&mut self) {
        self.wired = true;
        self.since_hit = 0;
        self.shaper.set_keyed(true);
    }

    fn hits_known(&mut self, known: bool) {
        self.hits_known = known;
    }

    fn hit(&mut self, offset: u32, _velocity: f32) {
        if let Some(slot) = self.hits.get_mut(self.hit_count) {
            *slot = offset;
            self.hit_count = self.hit_count.saturating_add(1);
        }
    }

    fn mod_sources(&self) -> &'static [ModSourceSpec] {
        SOURCES
    }

    fn mod_source(&self, index: usize) -> f32 {
        if index == 0 {
            dsp_core::num::narrow(self.shaper.gain_db())
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_param_parses_what_it_prints() {
        for spec in PARAMS {
            for step in 0..=20u8 {
                let v = spec.denormalize(f32::from(step) / 20.0);
                let text = crate::ValueText::of(spec, v);
                let again = spec
                    .parse(text.as_str())
                    .map(|b| crate::ValueText::of(spec, b));
                assert_eq!(
                    again.as_ref().map(crate::ValueText::as_str),
                    Some(text.as_str()),
                    "{}",
                    spec.id
                );
            }
        }
    }

    /// A room: attack down, sustain up; at rest (0 dB both) a null.
    #[test]
    fn it_shapes_a_hit_and_is_a_null_at_zero() {
        let hit = |i: usize| -> f32 {
            let t = crate::util_f32(i % 12_000);
            libm::expf(-t / 2_880.0)
                * libm::sinf(t * 180.0 * core::f32::consts::TAU / 48_000.0)
                * 0.5
        };
        let input: alloc::vec::Vec<f32> = (0..48_000).map(hit).collect();
        let run = |t: &mut Transient| -> alloc::vec::Vec<f32> {
            let mut out = alloc::vec::Vec::new();
            for piece in input.chunks(512) {
                let (mut l, mut r) = (piece.to_vec(), piece.to_vec());
                t.process(&mut l, &mut r);
                out.extend(l);
            }
            out
        };
        let mut flat = Transient::new(48_000.0);
        assert_eq!(run(&mut flat), input);
        let mut room = Transient::new(48_000.0);
        room.set_param(0, -6.0);
        room.set_param(1, 6.0);
        let out = run(&mut room);
        let peak = |x: &[f32]| x.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        let front = peak(&out[12_000..12_240]) / peak(&input[12_000..12_240]);
        let tail = peak(&out[12_000 + 7_200..12_000 + 9_600])
            / peak(&input[12_000 + 7_200..12_000 + 9_600]);
        assert!(front < 0.8, "attack down: {front}");
        assert!(tail > 1.3, "sustain up: {tail}");
        assert!(room.mod_source(0).is_finite());
    }
}
