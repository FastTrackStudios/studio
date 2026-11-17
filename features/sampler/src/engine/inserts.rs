//! Kontakt group insert filters and EQs, run per voice (spec `group_fx`).
//!
//! A dynamic layer's [`InsertPlan`] is built once per patch from its
//! `group_fx` entry: the stages (filter or EQ) and the controller routes that
//! move their knobs, each route's table sampled at every controller value
//! (controllers are integers, so the 128-entry table is exact). A voice of
//! that layer carries the plan (an `Arc`, no allocation at spawn) and its own
//! [`InsertState`]: the filter memories, the routes' smoothed values and the
//! coefficients, recomputed every [`COEF_EVERY`] frames.
//!
//! The laws are Kontakt's, as measured by KONTRA (Apache-2.0,
//! `crates/format/kontakt/src/laws.rs`, `docs/re/kontakt-bindings/filter-laws.tsv`):
//!
//! - state-variable filters (types 50..): cutoff knob `u` is the MIDI note
//!   `115u + 20` (at most 160); resonance `R = 1 − (1 − r)³` damps a stage
//!   by `2(1 − R)` (the second of four poles by `2(1 − 0.666R)`), and the
//!   output falls with `R` (one-pole: unaffected; two poles `1 − ½R⁵`; four
//!   `1 − 0.77R`).
//! - legacy filters (types 2..): `21759.5 · (min((e^(6.15u) − 1) · 0.0025728, 1) + 0.002)` Hz,
//!   Butterworth sections, resonance raising the sharpest one's Q by
//!   `e^(6.4r)` with a `5.5 r` dB passband loss for two poles.
//! - EQ bands: the cookbook bell with twice the stored dB at the centre
//!   (`A = 2^(dB/6)`) and `α = sin ω · sinh(octaves · ln 2)`; a route of
//!   intensity 1 moves the stored dB by 12, clamped to ±18.
//! - a route's lag is the time to settle within 1 %: a one-pole with
//!   `τ = lag / ln 100`.

use std::sync::Arc;

use super::voice::FlexEnv;

use crate::spec::{FxRouteSpec, FxStageSpec, GroupFxSpec, kontakt_table_value};

/// Frames between coefficient updates (as the voice filter envelope).
pub const COEF_EVERY: u16 = 16;
/// Most stages a layer's chain keeps (Kontakt has 8 slots; CS uses ≤ 3 live).
pub const MAX_STAGES: usize = 4;
/// Most routes a layer's chain keeps.
pub const MAX_ROUTES: usize = 8;
/// Sections one stage expands into (a 3-band EQ, a four-pole filter: ≤ 3).
const MAX_SECTIONS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Law {
    Sv,
    Legacy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StageKind {
    /// One-pole lowpass (SV LP1, legacy LP1).
    OnePole(Law),
    /// State-variable lowpass with `n` two-pole sections (SV LP2 / LP4,
    /// legacy LP2).
    Svf(Law, u8),
    /// 1..=3 bell bands.
    Eq(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Param {
    Cutoff,
    Resonance,
    EqGain(u8),
}

/// One stage of a plan: its kind and saved knobs.
#[derive(Debug, Clone)]
struct PlanStage {
    kind: StageKind,
    cutoff: f32,
    resonance: f32,
    /// (Hz, octaves, stored dB) per band.
    bands: [[f32; 3]; MAX_SECTIONS],
}

/// One route of a plan: the knob it moves and its value at every controller
/// value (table, invert and intensity applied).
#[derive(Debug, Clone)]
struct PlanRoute {
    stage: u8,
    param: Param,
    cc: u8,
    /// Kontakt units the knob moves: 0..1 knob for cutoff/resonance, stored
    /// dB for an EQ gain.
    lut: [f32; 128],
    /// Smoothing coefficient per update (1 = no lag).
    coef: f32,
    /// An envelope source (`FxRouteSpec::env`): its segments, a level < 0
    /// standing for `cc` ÷ 127; empty = a controller route.
    env: Vec<(f32, f32, f32)>,
    /// `intensity × span` the envelope's level moves the knob by.
    env_depth: f32,
}

/// A dynamic layer's insert chain, shared by its voices.
#[derive(Debug, Clone)]
pub struct InsertPlan {
    stages: Vec<PlanStage>,
    routes: Vec<PlanRoute>,
}

impl InsertPlan {
    /// Build the plan of one `group_fx` entry; `None` when nothing in it is
    /// a stage this engine runs.
    #[must_use]
    pub fn new(spec: &GroupFxSpec, sample_rate: f32) -> Option<Arc<Self>> {
        let mut map = Vec::new(); // spec stage index → plan stage index
        let mut stages = Vec::new();
        for (i, s) in spec.stages.iter().enumerate() {
            match plan_stage(s) {
                Some(p) if stages.len() < MAX_STAGES => {
                    map.push((i, stages.len()));
                    stages.push(p);
                }
                _ => {}
            }
        }
        if stages.is_empty() {
            return None;
        }
        let routes = spec
            .routes
            .iter()
            .filter_map(|r| {
                let stage = map.iter().find(|(i, _)| *i == usize::from(r.stage))?.1;
                plan_route(r, stage as u8, &stages[stage], sample_rate)
            })
            .take(MAX_ROUTES)
            .collect();
        Some(Arc::new(Self { stages, routes }))
    }

    /// The controllers the routes read.
    pub fn ccs(&self) -> impl Iterator<Item = u8> + '_ {
        self.routes.iter().map(|r| r.cc)
    }
}

fn plan_stage(s: &FxStageSpec) -> Option<PlanStage> {
    let kind = match s.kind.as_str() {
        "sv_lp1" => StageKind::OnePole(Law::Sv),
        "legacy_lp1" => StageKind::OnePole(Law::Legacy),
        "sv_lp2" => StageKind::Svf(Law::Sv, 1),
        "sv_lp4" => StageKind::Svf(Law::Sv, 2),
        "legacy_lp2" => StageKind::Svf(Law::Legacy, 1),
        "eq" => {
            let n = (s.bands.len() / 3).min(MAX_SECTIONS);
            if n == 0 {
                return None;
            }
            StageKind::Eq(n as u8)
        }
        _ => return None,
    };
    let mut bands = [[0.0; 3]; MAX_SECTIONS];
    for (b, c) in bands.iter_mut().zip(s.bands.chunks_exact(3)) {
        *b = [c[0], c[1], c[2]];
    }
    Some(PlanStage {
        kind,
        cutoff: s.cutoff,
        resonance: s.resonance,
        bands,
    })
}

fn plan_route(r: &FxRouteSpec, stage: u8, s: &PlanStage, sample_rate: f32) -> Option<PlanRoute> {
    let param = match r.param.as_str() {
        "cutoff" => Param::Cutoff,
        "resonance" => Param::Resonance,
        p => {
            let n: u8 = p.strip_prefix("eq_gain")?.parse().ok()?;
            match s.kind {
                StageKind::Eq(bands) if (1..=bands).contains(&n) => Param::EqGain(n - 1),
                _ => return None,
            }
        }
    };
    if matches!(param, Param::Cutoff | Param::Resonance) && matches!(s.kind, StageKind::Eq(_)) {
        return None;
    }
    // an EQ gain route of depth 1 moves the stored dB by 12
    let span = if matches!(param, Param::EqGain(_)) {
        12.0
    } else {
        1.0
    };
    let mut lut = [0.0; 128];
    for (v, out) in lut.iter_mut().enumerate() {
        let x = v as f32 / 127.0;
        let shaped = if r.points.is_empty() {
            x
        } else {
            kontakt_table_value(&r.points, x)
        };
        *out = r.intensity * span * shaped;
    }
    let dt_ms = f32::from(COEF_EVERY) * 1000.0 / sample_rate.max(1.0);
    let coef = if r.lag_ms > 0.0 {
        let tau = r.lag_ms / 100f32.ln();
        1.0 - (-dt_ms / tau).exp()
    } else {
        1.0
    };
    Some(PlanRoute {
        stage,
        param,
        cc: r.cc,
        lut,
        coef,
        env: r
            .env
            .iter()
            .map(|e| (e.time_ms, e.level, e.curve))
            .collect(),
        env_depth: r.intensity * span,
    })
}

/// One filter section's coefficients.
#[derive(Debug, Clone, Copy)]
enum Section {
    Off,
    /// Trapezoidal one-pole lowpass, `G = g / (1 + g)`.
    OnePole(f32),
    /// Trapezoidal state-variable lowpass (`a1 a2 a3` of `g`, `k`).
    Svf(f32, f32, f32),
    /// Direct form II transposed biquad, normalised.
    Biquad([f32; 5]),
}

/// A voice's run of its layer's plan.
#[derive(Debug, Clone)]
pub struct InsertState {
    plan: Arc<InsertPlan>,
    sample_rate: f32,
    /// Smoothed and target value per route.
    cur: [f32; MAX_ROUTES],
    tgt: [f32; MAX_ROUTES],
    sections: [[Section; MAX_SECTIONS]; MAX_STAGES],
    /// The envelope of each envelope route, run per voice.
    envs: [Option<FlexEnv>; MAX_ROUTES],
    /// Output gain per stage (resonance loss).
    gain: [f32; MAX_STAGES],
    /// Two memories per section per channel.
    mem: [[[[f32; 2]; MAX_SECTIONS]; MAX_STAGES]; 2],
    countdown: u16,
}

impl InsertState {
    /// Start a voice's chain at the controllers' current values (no lag on
    /// the way in: the routes start where the controllers are).
    pub fn new(plan: Arc<InsertPlan>, sample_rate: f32, cc: impl Fn(u8) -> u8) -> Self {
        let mut s = Self {
            plan,
            sample_rate,
            cur: [0.0; MAX_ROUTES],
            tgt: [0.0; MAX_ROUTES],
            sections: [[Section::Off; MAX_SECTIONS]; MAX_STAGES],
            envs: Default::default(),
            gain: [1.0; MAX_STAGES],
            mem: [[[[0.0; 2]; MAX_SECTIONS]; MAX_STAGES]; 2],
            countdown: 0,
        };
        for (slot, r) in s.envs.iter_mut().zip(&s.plan.routes) {
            if r.env.is_empty() {
                continue;
            }
            let v = f32::from(cc(r.cc).min(127)) / 127.0;
            let segs: Vec<(f32, f32, f32)> = r
                .env
                .iter()
                .map(|&(t, l, c)| (t, if l < 0.0 { v } else { l }, c))
                .collect();
            *slot = FlexEnv::from_segments(&segs, 0.0, sample_rate as u32, true);
        }
        s.set_ccs(cc);
        s.advance_envs(0);
        s.cur = s.tgt;
        s
    }

    /// Step the envelope routes `frames` frames and take their levels as
    /// the routes' targets (no lag: the envelope is the motion).
    fn advance_envs(&mut self, frames: u16) {
        for (i, e) in self.envs.iter_mut().enumerate() {
            let Some(e) = e else { continue };
            let mut lv = 0.0;
            for _ in 0..frames.max(1) {
                lv = e.next(false);
            }
            let v = lv * self.plan.routes[i].env_depth;
            self.tgt[i] = v;
            self.cur[i] = v;
        }
    }

    /// The controllers' current values (read again every block).
    pub fn set_ccs(&mut self, cc: impl Fn(u8) -> u8) {
        for (t, r) in self.tgt.iter_mut().zip(&self.plan.routes) {
            if r.env.is_empty() {
                *t = r.lut[usize::from(cc(r.cc).min(127))];
            }
        }
    }

    fn update(&mut self) {
        let Self {
            plan,
            sample_rate,
            cur,
            tgt,
            sections,
            gain,
            ..
        } = self;
        for ((c, t), r) in cur.iter_mut().zip(tgt.iter()).zip(&plan.routes) {
            *c += (*t - *c) * r.coef;
        }
        for (i, st) in plan.stages.iter().enumerate() {
            let mut cutoff = st.cutoff;
            let mut res = st.resonance;
            let mut db = [st.bands[0][2], st.bands[1][2], st.bands[2][2]];
            for (r, v) in plan.routes.iter().zip(cur.iter()) {
                if usize::from(r.stage) != i {
                    continue;
                }
                match r.param {
                    Param::Cutoff => cutoff += v,
                    Param::Resonance => res += v,
                    Param::EqGain(b) => db[usize::from(b)] += v,
                }
            }
            (sections[i], gain[i]) = coefficients(st, cutoff, res, db, *sample_rate);
        }
    }

    /// Filter one stereo frame.
    #[inline]
    pub fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        if self.countdown == 0 {
            self.countdown = COEF_EVERY;
            self.advance_envs(COEF_EVERY);
            self.update();
        }
        self.countdown -= 1;
        let mut out = [l, r];
        let n = self.plan.stages.len();
        for (ch, x) in out.iter_mut().enumerate() {
            for i in 0..n {
                for (sec, m) in self.sections[i].iter().zip(self.mem[ch][i].iter_mut()) {
                    *x = run(*sec, m, *x);
                }
                *x *= self.gain[i];
            }
        }
        (out[0], out[1])
    }
}

#[inline]
fn run(sec: Section, m: &mut [f32; 2], x: f32) -> f32 {
    match sec {
        Section::Off => x,
        Section::OnePole(g) => {
            let v = (x - m[0]) * g;
            let y = v + m[0];
            m[0] = y + v;
            y
        }
        Section::Svf(a1, a2, a3) => {
            let (ic1, ic2) = (m[0], m[1]);
            let v3 = x - ic2;
            let v1 = a1 * ic1 + a2 * v3;
            let v2 = ic2 + a2 * ic1 + a3 * v3;
            m[0] = 2.0 * v1 - ic1;
            m[1] = 2.0 * v2 - ic2;
            v2
        }
        Section::Biquad([b0, b1, b2, a1, a2]) => {
            let y = b0 * x + m[0];
            m[0] = b1 * x - a1 * y + m[1];
            m[1] = b2 * x - a2 * y;
            y
        }
    }
}

/// Cutoff knob → Hz by the stage's law.
fn cutoff_hz(law: Law, u: f32) -> f32 {
    match law {
        Law::Sv => {
            let note = (115.0 * u.max(0.0) + 20.0).min(160.0);
            440.0 * ((note - 69.0) / 12.0).exp2()
        }
        Law::Legacy => {
            let corner = ((6.15 * u.clamp(0.0, 1.0)).exp() - 1.0) * 0.002_572_831_7;
            21_759.5 * (corner.min(1.0) + 0.002)
        }
    }
}

/// Prewarped trapezoidal gain `tan(π f / fs)`, the cutoff kept below Nyquist.
fn warp(hz: f32, fs: f32) -> f32 {
    (std::f32::consts::PI * hz.clamp(1.0, 0.49 * fs) / fs).tan()
}

fn svf(g: f32, k: f32) -> Section {
    let a1 = 1.0 / (1.0 + g * (g + k));
    let a2 = g * a1;
    Section::Svf(a1, a2, g * a2)
}

fn coefficients(
    st: &PlanStage,
    cutoff: f32,
    res: f32,
    db: [f32; 3],
    fs: f32,
) -> ([Section; MAX_SECTIONS], f32) {
    let mut out = [Section::Off; MAX_SECTIONS];
    let r = res.clamp(0.0, 1.0);
    match st.kind {
        StageKind::OnePole(law) => {
            let g = warp(cutoff_hz(law, cutoff), fs);
            out[0] = Section::OnePole(g / (1.0 + g));
            (out, 1.0)
        }
        StageKind::Svf(Law::Sv, n) => {
            let g = warp(cutoff_hz(Law::Sv, cutoff), fs);
            let big = 1.0 - (1.0 - r).powi(3);
            let k = (2.0 * (1.0 - big)).max(0.01);
            out[0] = svf(g, k);
            let gain = if n >= 2 {
                out[1] = svf(g, (2.0 * (1.0 - 0.666 * big)).max(0.01));
                1.0 - 0.77 * big
            } else {
                1.0 - 0.5 * big.powi(5)
            };
            (out, gain)
        }
        StageKind::Svf(Law::Legacy, _) => {
            let g = warp(cutoff_hz(Law::Legacy, cutoff), fs);
            // one Butterworth section; resonance sharpens it
            let k = (std::f32::consts::SQRT_2 / (6.4 * r).exp()).max(0.01);
            out[0] = svf(g, k);
            (out, 10f32.powf(-5.5 * r / 20.0))
        }
        StageKind::Eq(n) => {
            for ((b, stored), o) in st
                .bands
                .iter()
                .zip(db)
                .zip(out.iter_mut())
                .take(usize::from(n))
            {
                *o = bell(b[0], b[1], stored.clamp(-18.0, 18.0), fs);
            }
            (out, 1.0)
        }
    }
}

/// Kontakt's EQ band: the cookbook bell at twice the stored dB, bandwidth
/// `α = sin ω · sinh(octaves · ln 2)`.
fn bell(hz: f32, octaves: f32, stored_db: f32, fs: f32) -> Section {
    if !(hz > 0.0 && octaves > 0.0) {
        return Section::Off;
    }
    let w = 2.0 * std::f32::consts::PI * hz.min(20_000.0).min(0.49 * fs) / fs;
    let (sw, cw) = w.sin_cos();
    let alpha = sw * (octaves * std::f32::consts::LN_2).sinh();
    // the cookbook's A (10^(peak/40)) is 2^(dB/6): the centre gain A² is
    // twice the stored dB
    let a = (stored_db / 6.0).exp2();
    let a0 = 1.0 + alpha / a;
    Section::Biquad([
        (1.0 + alpha * a) / a0,
        -2.0 * cw / a0,
        (1.0 - alpha * a) / a0,
        -2.0 * cw / a0,
        (1.0 - alpha / a) / a0,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: &str, cutoff: f32, bands: Vec<f32>, routes: Vec<FxRouteSpec>) -> GroupFxSpec {
        GroupFxSpec {
            dynamic: "ff".into(),
            stages: vec![FxStageSpec {
                kind: kind.into(),
                cutoff,
                resonance: 0.0,
                bands,
            }],
            routes,
        }
    }

    /// Steady-state gain of a sine through the chain.
    fn gain_at(plan: Arc<InsertPlan>, hz: f32, cc: u8) -> f32 {
        let fs = 48_000.0;
        let mut st = InsertState::new(plan, fs, |_| cc);
        let n = 48_000;
        let mut peak = 0.0f32;
        for i in 0..n {
            let x = (2.0 * std::f32::consts::PI * hz * i as f32 / fs).sin();
            let (y, _) = st.tick(x, x);
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        20.0 * peak.log10()
    }

    #[test]
    fn sv_cutoff_law_matches_kontakt_display() {
        // the Kontakt GUI: 0 → 26.0 Hz, 0.2305 → 120.0 Hz
        assert!((cutoff_hz(Law::Sv, 0.0) - 25.96).abs() < 0.05);
        assert!((cutoff_hz(Law::Sv, 0.230_490_12) - 120.0).abs() < 0.2);
        // legacy LP1 at 0.25098 shows 249.9 Hz
        assert!((cutoff_hz(Law::Legacy, 0.250_976_7) - 249.9).abs() < 2.5);
    }

    #[test]
    fn one_pole_is_3db_down_at_its_cutoff() {
        let plan = InsertPlan::new(&spec("sv_lp1", 0.5, vec![], vec![]), 48_000.0).unwrap();
        let fc = cutoff_hz(Law::Sv, 0.5);
        let g = gain_at(plan, fc, 0);
        assert!((g + 3.01).abs() < 0.2, "{g}");
    }

    #[test]
    fn cc_route_opens_the_filter() {
        // CSB Solo Trumpet Mix vibsus ff: closed below CC1 ≈ 76, open at 127
        let route = FxRouteSpec {
            stage: 0,
            param: "cutoff".into(),
            cc: 1,
            intensity: 1.0,
            lag_ms: 120.0,
            points: vec![0.0, 0.0, 0.600_393_7, 0.0, -0.168_918_88, 1.0, 1.0, 0.0],
            env: vec![],
        };
        let plan = InsertPlan::new(&spec("sv_lp1", 0.0, vec![], vec![route]), 48_000.0).unwrap();
        assert!(gain_at(Arc::clone(&plan), 1000.0, 127) > -0.1);
        assert!(gain_at(plan, 1000.0, 40) < -30.0);
    }

    #[test]
    fn eq_band_peaks_at_twice_the_stored_db() {
        let plan =
            InsertPlan::new(&spec("eq", 0.0, vec![1000.0, 1.0, 6.0], vec![]), 48_000.0).unwrap();
        let g = gain_at(plan, 1000.0, 0);
        assert!((g - 12.04).abs() < 0.1, "{g}");
    }
}
