//! Breakpoint envelopes — Omnisphere's envelope model, measured against the
//! real plugin (see `features/rigs/synth/tools/omni_probe`).
//!
//! An envelope is a run of points (`level` at `time`). The segment leaving a
//! point takes that point's shape: a curve `k` (`0` linear, `> 0`
//! fast-then-slow, `< 0` slow-then-fast; a power law on amplitude,
//! `g(x) = 1 − (1 − x)^e` or `x^e` with `e = 1 + 3|k|/7` — see [`shape`])
//! or a step (hold, then jump at the next point). An amp or
//! filter envelope has a **sustain** point (the penultimate) where it holds
//! while the key is down; on release it runs the final segment from wherever
//! it is, over that segment's whole duration. A mod envelope has none and
//! runs free, optionally looping.

/// The tempo synced envelopes run at (BPM as f32 bits). The render graph
/// publishes the band's tempo here every time it moves (see
/// `RenderNode::process`), so a voice deep in a source needs no plumbing.
static TEMPO_BPM: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x42F0_0000); // 120.0

/// Set the tempo synced envelopes follow.
pub fn set_tempo_bpm(bpm: f32) {
    TEMPO_BPM.store(
        bpm.clamp(1.0, 999.0).to_bits(),
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// The tempo synced envelopes follow.
#[must_use]
pub fn tempo_bpm() -> f32 {
    f32::from_bits(TEMPO_BPM.load(std::sync::atomic::Ordering::Relaxed))
}

/// One breakpoint: `level` at `time` (seconds, or beats when synced). Its
/// `curve` and `step` shape the segment that LEAVES it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegPoint {
    pub time: f32,
    pub level: f32,
    /// Shape of the segment from this point: `0` linear, `> 0`
    /// fast-then-slow, `< 0` slow-then-fast.
    pub curve: f32,
    /// Hold this level until the next point, then jump (a step).
    pub step: bool,
}

/// The shape `g(x)` (0..1 → 0..1) of a segment with curve `k`: a power
/// law, `1 − (1 − x)^e` for `k > 0` (fast, then slow) and `x^e` for `k < 0`,
/// `e = 1 + 3|k|/7`. Measured on Omnisphere's amp envelope (a 1.5 s decay
/// at curve values `c` 0.05 … 0.95, `k = 42·(0.5 − c)`): one exponent fits
/// each whole segment, and `e = 1 + 18·|c − 0.5|` gives it within ~0.3 —
/// the exponential `(1 − e^(−kx)) / (1 − e^(−k))` this replaced fitted no
/// segment's tail.
#[inline]
#[must_use]
pub fn shape(k: f32, x: f32) -> f32 {
    if k.abs() < 1e-3 {
        return x;
    }
    let e = 1.0 + k.abs() * (3.0 / 7.0);
    let x = x.clamp(0.0, 1.0);
    if k > 0.0 {
        1.0 - (1.0 - x).powf(e)
    } else {
        x.powf(e)
    }
}

/// [`shape`], at the call sites that once cached its exponential's
/// denominator in `memo` (the power law needs no cache; the slot stays so
/// the players keep one shape entry point).
#[inline]
fn shape_memo(_memo: &mut (f32, f32), k: f32, x: f32) -> f32 {
    shape(k, x)
}

/// An envelope's points and structure.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Breakpoints {
    pub points: Vec<SegPoint>,
    /// Hold here while the key is down (an amp/filter envelope's penultimate
    /// point); `None` runs free.
    pub sustain: Option<usize>,
    /// Free-running: start over at the end.
    pub looping: bool,
    /// Times are beats, not seconds: the run follows [`tempo_bpm`].
    pub synced: bool,
}

impl Breakpoints {
    /// Points sorted by time; `sustain` is an index into them.
    #[must_use]
    pub fn new(mut points: Vec<SegPoint>, sustain: Option<usize>, looping: bool) -> Self {
        points.sort_by(|a, b| a.time.total_cmp(&b.time));
        let sustain = sustain.filter(|&s| s + 1 < points.len());
        Self {
            points,
            sustain,
            looping,
            synced: false,
        }
    }

    /// The same envelope with its times in beats (following the tempo).
    #[must_use]
    pub fn with_synced(mut self, synced: bool) -> Self {
        self.synced = synced;
        self
    }

    /// Seconds per time unit: 1, or a beat at the current tempo.
    #[must_use]
    pub fn unit_s(&self) -> f32 {
        if self.synced { 60.0 / tempo_bpm() } else { 1.0 }
    }

    /// An amp/filter envelope: sustain at the penultimate point.
    #[must_use]
    pub fn with_penultimate_sustain(points: Vec<SegPoint>) -> Self {
        let n = points.len();
        Self::new(points, n.checked_sub(2), false)
    }

    #[must_use]
    pub fn end(&self) -> f32 {
        self.points.last().map_or(0.0, |p| p.time)
    }

    /// The level at `t` along the (unreleased) run.
    #[must_use]
    pub fn level_at(&self, t: f32) -> f32 {
        self.level_at_memo(t, &mut (f32::NAN, 0.0))
    }

    /// [`level_at`](Self::level_at) with the curve's denominator remembered
    /// across calls (see [`shape_memo`]).
    fn level_at_memo(&self, t: f32, memo: &mut (f32, f32)) -> f32 {
        let Some(first) = self.points.first() else {
            return 0.0;
        };
        if t < first.time {
            return first.level;
        }
        for w in self.points.windows(2) {
            let (a, b) = (w[0], w[1]);
            if t < b.time {
                if a.step {
                    return a.level;
                }
                let x = (t - a.time) / (b.time - a.time).max(1e-9);
                return a.level + (b.level - a.level) * shape_memo(memo, a.curve, x);
            }
        }
        self.points.last().map_or(0.0, |p| p.level)
    }
}

/// One playing instance of a [`Breakpoints`] envelope.
#[derive(Clone, Copy, Debug, Default)]
pub struct EnvPlayer {
    /// Position in the run, `None` before the first note.
    t: Option<f32>,
    /// Released: `(time since release, level at release)`.
    released: Option<(f32, f32)>,
    level: f32,
    /// The last curve and its denominator (see [`shape_memo`]).
    memo: (f32, f32),
}

impl EnvPlayer {
    pub fn note_on(&mut self) {
        self.t = Some(0.0);
        self.released = None;
    }

    /// Start the release from the current level (only an envelope with a
    /// sustain releases; a free-running one ignores the key).
    pub fn note_off(&mut self, bp: &Breakpoints) {
        if bp.sustain.is_some() && self.t.is_some() && self.released.is_none() {
            self.released = Some((0.0, self.level));
        }
    }

    /// Finished: released past its end (or never started).
    #[must_use]
    pub fn is_idle(&self, bp: &Breakpoints) -> bool {
        match (self.t, self.released, bp.sustain) {
            (None, _, _) => true,
            (Some(_), Some((rt, _)), Some(s)) => {
                let last = bp.points.len() - 1;
                rt >= bp.points[last].time - bp.points[s].time
            }
            _ => false,
        }
    }

    /// The level now, then advance by `dt`.
    pub fn tick(&mut self, bp: &Breakpoints, dt: f32) -> f32 {
        let Some(t) = self.t else {
            return 0.0;
        };
        // A synced run advances in beats.
        let dt = dt / bp.unit_s();
        let v = match (self.released, bp.sustain) {
            (Some((rt, from)), Some(s)) => {
                let last = bp.points.len() - 1;
                let (a, b) = (bp.points[s], bp.points[last]);
                let dur = (b.time - a.time).max(1e-6);
                let x = (rt / dur).min(1.0);
                self.released = Some((rt + dt, from));
                if a.step && x < 1.0 {
                    from
                } else {
                    from + (b.level - from) * shape_memo(&mut self.memo, a.curve, x)
                }
            }
            _ => {
                // Holding at the sustain point: its own level (several
                // points can share its time — an instant attack).
                let v = match bp.sustain {
                    Some(s) if t >= bp.points[s].time => bp.points[s].level,
                    _ => bp.level_at_memo(t, &mut self.memo),
                };
                let mut next = t + dt;
                if let Some(s) = bp.sustain {
                    next = next.min(bp.points[s].time);
                } else if bp.looping && bp.end() > 0.0 && next >= bp.end() {
                    next %= bp.end();
                }
                self.t = Some(next);
                v
            }
        };
        self.level = v;
        v
    }
}
