//! `dyn_eq.bands@1`: a keyed dynamic EQ, four bands on `dsp`'s dynamic band
//! (`dsp::eq::dynamics::DynBand`, the Simper SVF the FTS EQ's dynamic bands
//! run on).
//!
//! Each band is a bell or a shelf with a static `gain` and a dynamic
//! `range`: it sits at `gain` at rest and moves to `gain + range` while its
//! source drives it. A band's `source` is one of
//!
//! - **Self**: its own input, band-limited around the band (classic dynamic
//!   EQ: `threshold`, `attack`, `release`);
//! - **Key**: the key lane's audio (a sidechain), heard the same way. Only a
//!   lane of the same Signal instance has audio to give; across tracks use
//!   Hits;
//! - **Hits**: the key lane's hits (a trigger's, or the session's placed
//!   ones, `key.from` as a gate takes it). Each hit opens the band
//!   sample-accurately at its offset: `attack` to full, held `hold`, then
//!   `release` back, its depth scaled by the hit's velocity by `velocity`.
//!
//! A band at 0 dB `gain` is an exact null at rest (the SVF at 0 dB is the
//! identity and the driven gain lands on its base), so a dynamic EQ that
//! only cuts or boosts on hits is gain neutral between them.
//!
//! Why a family of its own rather than more `eq` targets: the canonical EQ
//! already has 24 bands × 7 targets, and keying is per block, not per band
//! of a 24-band face. Four keyed bands on their own block read in a chain
//! as what they do ("cut 450 on the snare"), and leave the static EQ's
//! schema (and every preset on it) untouched.

use alloc::boxed::Box;
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use dsp::eq::dynamics::{DynBand, DynShape, SideMode};
use dsp_core::num::narrow;

use crate::{BlockProcessor, ModSourceSpec, ParamSpec, Unit};

/// Bands in the engine.
pub const BANDS: usize = 4;

/// The band shapes, by index.
pub const SHAPES: &[&str] = &["Bell", "Low Shelf", "High Shelf"];

/// What drives a band, by index.
pub const SOURCES_OF_DRIVE: &[&str] = &["Self", "Key", "Hits"];

/// Per band, in target order.
const FIELDS: usize = 11;

/// Each band's default frequency: the low end, the box, the body, the top.
const DEFAULT_FREQS: [f32; BANDS] = [80.0, 250.0, 450.0, 4000.0];

/// Hits a band remembers between two pieces before it drops one.
const MAX_HITS: usize = 16;

/// The engine's targets: four bands, then `velocity` and `output`.
#[must_use]
pub fn targets() -> &'static [ParamSpec] {
    static TARGETS: std::sync::OnceLock<&'static [ParamSpec]> = std::sync::OnceLock::new();
    TARGETS.get_or_init(|| {
        let leak = |s: alloc::string::String| -> &'static str { Box::leak(s.into_boxed_str()) };
        let mut t: Vec<ParamSpec> = Vec::with_capacity(BANDS * FIELDS + 2);
        for (i, freq) in DEFAULT_FREQS.iter().enumerate() {
            let b = i.saturating_add(1);
            let id = |f: &str| leak(format!("band.{b}.{f}"));
            let name = |f: &str| leak(format!("Band {b} {f}"));
            t.push(ParamSpec::new(
                id("used"),
                name("Used"),
                Unit::Toggle,
                0.0,
                1.0,
                0.0,
            ));
            t.push(ParamSpec::choice(id("shape"), name("Shape"), SHAPES));
            t.push(
                ParamSpec::new(id("freq"), name("Freq"), Unit::Hz, 20.0, 20_000.0, *freq)
                    .skewed(0.25),
            );
            t.push(ParamSpec::new(id("q"), name("Q"), Unit::Plain(2), 0.1, 10.0, 1.0).skewed(0.35));
            t.push(
                ParamSpec::new(id("gain"), name("Gain"), Unit::Db, -24.0, 24.0, 0.0)
                    .symmetrical_skewed(1.0, 0.0),
            );
            t.push(
                ParamSpec::new(id("range"), name("Range"), Unit::Db, -24.0, 24.0, 0.0)
                    .symmetrical_skewed(1.0, 0.0),
            );
            t.push(ParamSpec::choice(
                id("source"),
                name("Source"),
                SOURCES_OF_DRIVE,
            ));
            t.push(ParamSpec::new(
                id("threshold"),
                name("Threshold"),
                Unit::Db,
                -80.0,
                0.0,
                -30.0,
            ));
            t.push(
                ParamSpec::new(id("attack"), name("Attack"), Unit::Ms, 0.0, 200.0, 1.0).skewed(0.4),
            );
            t.push(
                ParamSpec::new(id("hold"), name("Hold"), Unit::Ms, 0.0, 1000.0, 60.0).skewed(0.5),
            );
            t.push(
                ParamSpec::new(id("release"), name("Release"), Unit::Ms, 1.0, 2000.0, 150.0)
                    .skewed(0.4),
            );
        }
        t.push(ParamSpec::new(
            "velocity",
            "Velocity",
            Unit::Percent,
            0.0,
            1.0,
            0.0,
        ));
        t.push(ParamSpec::new(
            "output",
            "Output",
            Unit::Db,
            -24.0,
            24.0,
            0.0,
        ));
        Box::leak(t.into_boxed_slice())
    })
}

/// Each band's applied gain (dB) after the last piece: what a meter reads.
pub const SOURCES: &[ModSourceSpec] = &[
    ModSourceSpec {
        id: "band1_gain",
        name: "Band 1 Gain",
        unit: Unit::Db,
    },
    ModSourceSpec {
        id: "band2_gain",
        name: "Band 2 Gain",
        unit: Unit::Db,
    },
    ModSourceSpec {
        id: "band3_gain",
        name: "Band 3 Gain",
        unit: Unit::Db,
    },
    ModSourceSpec {
        id: "band4_gain",
        name: "Band 4 Gain",
        unit: Unit::Db,
    },
];

/// What drives a band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Own,
    Key,
    Hits,
}

/// One band's settings, as the targets set them.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Settings {
    used: bool,
    shape: DynShape,
    freq: f32,
    q: f32,
    gain: f32,
    range: f32,
    source: Source,
    threshold: f32,
    attack_ms: f32,
    hold_ms: f32,
    release_ms: f32,
}

impl Settings {
    const fn new(freq: f32) -> Self {
        Self {
            used: false,
            shape: DynShape::Bell,
            freq,
            q: 1.0,
            gain: 0.0,
            range: 0.0,
            source: Source::Own,
            threshold: -30.0,
            attack_ms: 1.0,
            hold_ms: 60.0,
            release_ms: 150.0,
        }
    }
}

/// A band: its filter and detector, and the hit envelope that drives it in
/// Hits mode.
struct Band {
    dsp: DynBand,
    settings: Settings,
    /// The hit envelope, 0..1, and what it is heading for.
    env: f64,
    peak: f64,
    hold_left: usize,
    hold_frames: usize,
    attack: f64,
    release: f64,
}

impl Band {
    fn new(sample_rate: f64, freq: f32) -> Self {
        let mut b = Self {
            dsp: DynBand::new(sample_rate),
            settings: Settings::new(freq),
            env: 0.0,
            peak: 0.0,
            hold_left: 0,
            hold_frames: 0,
            attack: 1.0,
            release: 1.0,
        };
        b.apply(sample_rate);
        b.dsp.reset();
        b
    }

    /// Hand the settings to the dsp band (coefficients, no allocation).
    fn apply(&mut self, sr: f64) {
        let s = self.settings;
        let p = &mut self.dsp.params;
        p.shape = s.shape;
        p.freq_hz = f64::from(s.freq);
        p.q = f64::from(s.q);
        p.base_gain_db = f64::from(s.gain);
        p.range_db = f64::from(s.range);
        p.side_mode = SideMode::BandLinked;
        p.keyed = s.source == Source::Key;
        p.enabled = true;
        let d = &mut self.dsp.detector.params;
        d.threshold_db = f64::from(s.threshold);
        d.attack_ms = f64::from(s.attack_ms);
        d.release_ms = f64::from(s.release_ms);
        d.auto = false;
        d.adaptive = false;
        d.relative = false;
        self.dsp.update(sr);
        let coef = |ms: f32| -> f64 {
            let frames = f64::from(ms) * 0.001 * sr;
            if frames < 1.0 {
                1.0
            } else {
                1.0 - libm::exp(-1.0 / frames)
            }
        };
        self.attack = coef(s.attack_ms);
        self.release = coef(s.release_ms);
        self.hold_frames = dsp_core::num::f64_to_index(f64::from(s.hold_ms) * 0.001 * sr);
    }

    /// The hit envelope's next value: `hit` is a hit's depth at this frame.
    #[inline]
    fn envelope(&mut self, hit: Option<f64>) -> f64 {
        if let Some(depth) = hit {
            // A hit while the band is still open lifts it, never drops it.
            self.peak = if self.hold_left > 0 {
                self.peak.max(depth)
            } else {
                depth
            };
            // Attack, then hold: the hold counts from where the attack
            // reaches the peak, so a slow attack still holds full.
            self.hold_left = self.hold_frames.saturating_add(1);
        }
        let target = if self.hold_left > 0 {
            self.hold_left = self.hold_left.saturating_sub(1);
            self.peak
        } else {
            0.0
        };
        let coef = if target > self.env {
            self.attack
        } else {
            self.release
        };
        self.env += (target - self.env) * coef;
        // Home: a ten-thousandth of the range is inaudible, and from here
        // the band's gain lands exactly on its base (a null at rest).
        if self.env < 1.0e-4 && target == 0.0 {
            self.env = 0.0;
        }
        self.env
    }

    const fn reset(&mut self) {
        self.dsp.reset();
        self.env = 0.0;
        self.peak = 0.0;
        self.hold_left = 0;
    }
}

pub struct DynEqEngine {
    bands: Box<[Band; BANDS]>,
    sample_rate: f64,
    velocity: f32,
    output: f64,
    l: Vec<f64>,
    r: Vec<f64>,
    /// The key's mono sum for the next piece, and how much of it came.
    key: Vec<f64>,
    keyed: usize,
    /// Hits for the next piece: `(offset, velocity)`.
    hits: [(u32, f32); MAX_HITS],
    hit_count: usize,
}

impl DynEqEngine {
    #[must_use]
    pub fn new(sample_rate: f32, max_block: usize) -> Self {
        let sr = f64::from(sample_rate);
        let bands = Box::new(DEFAULT_FREQS.map(|f| Band::new(sr, f)));
        Self {
            bands,
            sample_rate: sr,
            velocity: 0.0,
            output: 1.0,
            l: vec![0.0; max_block],
            r: vec![0.0; max_block],
            key: vec![0.0; max_block],
            keyed: 0,
            hits: [(0, 0.0); MAX_HITS],
            hit_count: 0,
        }
    }
}

/// A hit's depth: full, or its velocity's share of full by `amount` (the
/// block's `velocity`).
fn depth(amount: f32, velocity: f32) -> f64 {
    let v = velocity.clamp(0.0, 1.0);
    f64::from(1.0 - amount * (1.0 - v))
}

impl BlockProcessor for DynEqEngine {
    fn params(&self) -> &'static [ParamSpec] {
        targets()
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let globals = BANDS * FIELDS;
        if index >= globals {
            match index.saturating_sub(globals) {
                0 => self.velocity = value,
                1 => self.output = f64::from(crate::db_to_gain(value)),
                _ => {}
            }
            return;
        }
        let (band, field) = (index / FIELDS, index % FIELDS);
        let sr = self.sample_rate;
        let Some(b) = self.bands.get_mut(band) else {
            return;
        };
        let before = b.settings;
        let s = &mut b.settings;
        match field {
            0 => s.used = value >= 0.5,
            1 => {
                s.shape = match dsp_core::num::f32_to_index(value) {
                    1 => DynShape::LowShelf,
                    2 => DynShape::HighShelf,
                    _ => DynShape::Bell,
                };
            }
            2 => s.freq = value,
            3 => s.q = value,
            4 => s.gain = value,
            5 => s.range = value,
            6 => {
                s.source = match dsp_core::num::f32_to_index(value) {
                    1 => Source::Key,
                    2 => Source::Hits,
                    _ => Source::Own,
                };
            }
            7 => s.threshold = value,
            8 => s.attack_ms = value,
            9 => s.hold_ms = value,
            _ => s.release_ms = value,
        }
        if b.settings != before {
            let switched_on = b.settings.used && !before.used;
            b.apply(sr);
            if switched_on {
                b.reset();
            }
        }
    }

    fn reset(&mut self) {
        for b in &mut *self.bands {
            b.reset();
        }
        self.key.fill(0.0);
        self.keyed = 0;
        self.hit_count = 0;
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len()).min(self.l.len());
        let keyed = core::mem::take(&mut self.keyed).min(n);
        let hit_count = core::mem::take(&mut self.hit_count);
        let amount = self.velocity;
        let (Some(l), Some(r)) = (self.l.get_mut(..n), self.r.get_mut(..n)) else {
            return;
        };
        for ((d, s), (e, t)) in l
            .iter_mut()
            .zip(left.iter())
            .zip(r.iter_mut().zip(right.iter()))
        {
            *d = f64::from(*s);
            *e = f64::from(*t);
        }
        // The hits in frame order, as depths: at most MAX_HITS, no
        // allocation.
        let mut hits = [(0_usize, 0.0_f64); MAX_HITS];
        let mut hn = 0;
        for &(at, v) in self.hits.iter().take(hit_count) {
            if let Some(slot) = hits.get_mut(hn) {
                *slot = (
                    usize::try_from(at)
                        .unwrap_or(usize::MAX)
                        .min(n.saturating_sub(1)),
                    depth(amount, v),
                );
                hn = hn.saturating_add(1);
            }
        }
        if let Some(h) = hits.get_mut(..hn) {
            h.sort_unstable_by_key(|(at, _)| *at);
        }
        let hits = hits.get(..hn).unwrap_or(&[]);
        let key = self.key.get(..keyed).unwrap_or(&[]);
        for band in self.bands.iter_mut().filter(|b| b.settings.used) {
            match band.settings.source {
                Source::Hits => {
                    let mut next = 0;
                    for (i, (x, y)) in l.iter_mut().zip(r.iter_mut()).enumerate() {
                        let mut hit = None;
                        while let Some(&(at, depth)) = hits.get(next) {
                            if at != i {
                                break;
                            }
                            hit = Some(hit.map_or(depth, |h: f64| h.max(depth)));
                            next = next.saturating_add(1);
                        }
                        let d = band.envelope(hit);
                        band.dsp.tick_driven(x, y, d);
                    }
                }
                Source::Key => {
                    // No key this piece (none routed, or a lane of another
                    // instance): the band hears silence, so it rests.
                    for (i, (x, y)) in l.iter_mut().zip(r.iter_mut()).enumerate() {
                        let side = key.get(i).copied().unwrap_or(0.0);
                        band.dsp.tick(x, y, side);
                    }
                }
                Source::Own => {
                    for (x, y) in l.iter_mut().zip(r.iter_mut()) {
                        let side = f64::midpoint(*x, *y);
                        band.dsp.tick(x, y, side);
                    }
                }
            }
        }
        let g = self.output;
        for ((d, s), (e, t)) in left
            .iter_mut()
            .zip(l.iter())
            .zip(right.iter_mut().zip(r.iter()))
        {
            *d = narrow(*s * g);
            *e = narrow(*t * g);
        }
    }

    fn set_key(&mut self, left: &[f32], right: &[f32]) {
        let n = left.len().min(right.len()).min(self.key.len());
        for ((k, l), r) in self.key.iter_mut().zip(left).zip(right).take(n) {
            *k = f64::midpoint(f64::from(*l), f64::from(*r));
        }
        self.keyed = n;
    }

    fn hit(&mut self, offset: u32, velocity: f32) {
        if let Some(slot) = self.hits.get_mut(self.hit_count) {
            *slot = (offset, velocity);
            self.hit_count = self.hit_count.saturating_add(1);
        }
    }

    fn mod_sources(&self) -> &'static [ModSourceSpec] {
        SOURCES
    }

    fn mod_source(&self, index: usize) -> f32 {
        self.bands
            .get(index)
            .filter(|b| b.settings.used)
            .map_or(0.0, |b| narrow(b.dsp.live_gain_db()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn index(id: &str) -> usize {
        targets()
            .iter()
            .position(|t| t.id == id)
            .unwrap_or_else(|| panic!("{id}"))
    }

    fn set(e: &mut DynEqEngine, id: &str, v: f32) {
        e.set_param(index(id), v);
    }

    fn sine(n: usize, hz: f32, at: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = crate::util_f32(at.saturating_add(i)) / SR;
                libm::sinf(core::f32::consts::TAU * hz * t) * 0.25
            })
            .collect()
    }

    fn peak(s: &[f32]) -> f32 {
        s.iter().map(|x| x.abs()).fold(0.0, f32::max)
    }

    fn db(x: f32) -> f32 {
        20.0 * libm::log10f(x.max(1.0e-12))
    }

    #[test]
    fn every_target_parses_what_it_prints() {
        for spec in targets() {
            for step in 0..=20u8 {
                let v = spec.denormalize(f32::from(step) / 20.0);
                let text = crate::ValueText::of(spec, v);
                let back = spec.parse(text.as_str());
                let again = back.map(|b| crate::ValueText::of(spec, b));
                assert_eq!(
                    again.as_ref().map(crate::ValueText::as_str),
                    Some(text.as_str()),
                    "{}: {v} printed {:?}",
                    spec.id,
                    text.as_str()
                );
            }
        }
    }

    /// With bands in use but nothing driving them, the block is a null.
    #[test]
    fn at_rest_it_is_gain_neutral() {
        let mut e = DynEqEngine::new(SR, 512);
        for b in 1..=BANDS {
            set(&mut e, &format!("band.{b}.used"), 1.0);
            set(&mut e, &format!("band.{b}.source"), 2.0);
            set(&mut e, &format!("band.{b}.range"), -6.0);
        }
        for piece in 0..20 {
            let input = sine(512, 450.0, piece * 512);
            let (mut l, mut r) = (input.clone(), input.clone());
            e.process(&mut l, &mut r);
            assert_eq!(l, input, "piece {piece}");
            assert_eq!(r, input, "piece {piece}");
        }
    }

    /// Colt's overhead trick, keyed: −4 dB at 450 Hz and +3 dB at the
    /// snare's fundamental only while a snare hit sounds; the overheads
    /// untouched between hits.
    #[test]
    fn hits_cut_and_boost_only_while_the_hit_sounds() {
        let mut e = DynEqEngine::new(SR, 512);
        set(&mut e, "band.1.used", 1.0);
        set(&mut e, "band.1.freq", 450.0);
        set(&mut e, "band.1.range", -4.0);
        set(&mut e, "band.1.source", 2.0);
        set(&mut e, "band.1.hold", 80.0);
        set(&mut e, "band.1.release", 60.0);
        set(&mut e, "band.2.used", 1.0);
        set(&mut e, "band.2.freq", 251.5);
        set(&mut e, "band.2.range", 3.0);
        set(&mut e, "band.2.source", 2.0);
        set(&mut e, "band.2.hold", 80.0);
        set(&mut e, "band.2.release", 60.0);
        let run = |e: &mut DynEqEngine, hz: f32| -> Vec<f32> {
            e.reset();
            let mut out = Vec::new();
            for piece in 0..120 {
                let input = sine(512, hz, piece * 512);
                let (mut l, mut r) = (input.clone(), input);
                if piece == 20 {
                    e.hit(100, 1.0);
                }
                e.process(&mut l, &mut r);
                out.extend(l);
            }
            out
        };
        let hit = 20 * 512 + 100;
        // 20..60 ms after the hit: held open.
        let open = hit + 960..hit + 2880;
        let before = hit - 4000..hit - 100;
        // Well after hold + release.
        let after = hit + 40_000..hit + 48_000;
        // Each band alone: together, the boost's skirt lifts 450 Hz by ~1.7.
        set(&mut e, "band.2.used", 0.0);
        let at450 = run(&mut e, 450.0);
        set(&mut e, "band.2.used", 1.0);
        set(&mut e, "band.1.used", 0.0);
        let cut = db(peak(&at450[open.clone()]) / peak(&at450[before.clone()]));
        assert!((cut + 4.0).abs() < 0.5, "450 Hz cut on the hit: {cut} dB");
        let dry = sine(at450.len(), 450.0, 0);
        assert_eq!(&at450[after.clone()], &dry[after], "a null again after");
        assert_eq!(
            &at450[before.clone()],
            &dry[before.clone()],
            "a null before"
        );
        let at250 = run(&mut e, 251.5);
        let boost = db(peak(&at250[open]) / peak(&at250[before.clone()]));
        assert!(
            (boost - 3.0).abs() < 0.6,
            "251.5 Hz boost on the hit: {boost} dB"
        );
        let rest = sine(at250.len(), 251.5, 0);
        assert_eq!(
            &at250[before.clone()],
            &rest[before],
            "a null before the hit"
        );
    }

    /// A kick-keyed low-end dip from the kick's audio (a sidechain).
    #[test]
    fn an_audio_key_ducks_the_band() {
        let mut e = DynEqEngine::new(SR, 512);
        set(&mut e, "band.1.used", 1.0);
        set(&mut e, "band.1.shape", 1.0);
        set(&mut e, "band.1.freq", 100.0);
        set(&mut e, "band.1.range", -6.0);
        set(&mut e, "band.1.source", 1.0);
        set(&mut e, "band.1.threshold", -40.0);
        set(&mut e, "band.1.attack", 1.0);
        let mut quiet = Vec::new();
        let mut ducked = Vec::new();
        for piece in 0..40 {
            let input = sine(512, 60.0, piece * 512);
            let (mut l, mut r) = (input.clone(), input.clone());
            let key = if piece >= 20 {
                sine(512, 60.0, piece * 512)
            } else {
                vec![0.0; 512]
            };
            e.set_key(&key, &key);
            e.process(&mut l, &mut r);
            if (10..20).contains(&piece) {
                quiet.extend(l);
            } else if piece >= 30 {
                ducked.extend(l);
            }
        }
        let dip = db(peak(&ducked) / peak(&quiet));
        assert!(dip < -3.0, "the key ducks the low end: {dip} dB");
        assert!(dip > -6.5, "by at most its range: {dip} dB");
    }

    /// Classic dynamic EQ: the band reacts to its own level.
    #[test]
    fn its_own_level_drives_a_self_band() {
        let mut e = DynEqEngine::new(SR, 512);
        set(&mut e, "band.1.used", 1.0);
        set(&mut e, "band.1.freq", 1000.0);
        set(&mut e, "band.1.range", -6.0);
        set(&mut e, "band.1.threshold", -60.0);
        let mut out = Vec::new();
        for piece in 0..40 {
            let input = sine(512, 1000.0, piece * 512);
            let (mut l, mut r) = (input.clone(), input);
            e.process(&mut l, &mut r);
            out.extend(l);
        }
        let g = db(peak(&out[out.len() - 4800..]) / 0.25);
        assert!(g < -4.0 && g > -6.5, "loud, it cuts: {g} dB");
    }
}
