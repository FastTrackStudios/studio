//! Recorded DMX into Steps scenes: what another controller sent, frame by
//! frame, played back by the desk.
//!
//! A recording is a list of frames, each a universe's bytes, taken `fps`
//! times a second (Ignition's headless cook, `ignition-viz`'s `cook_dmx`
//! example). It becomes a scene in three moves:
//!
//! 1. [`find_cycle`]: where it starts repeating, after a warm-up (a look
//!    fading in), and how long one repeat is. One frame is a static look.
//! 2. [`steps`]: the cycle's frames as steps, consecutive repeats folded
//!    into one longer step, capped at a step count the desk handles.
//! 3. [`scene_from_frames`]: each step sliced per fixture by its patch
//!    address and footprint.
//!
//! Step times are in the desk's ticks, [`TICKS_PER_SECOND`] to the second.

use crate::patch::{Fixture, Patch};
use crate::profile::{Kind, channels};
use crate::scene::{Scene, Step};

/// The desk's step-time unit: `WAITTIME` / `FADETIME` count 1/25 s.
///
/// Read off real scenes rather than documented: a fog timer programmed on a
/// desk stores 175, 1500 and 22 500 (7 s, 1 min, 15 min) and two mover
/// sweeps fade 125 and 75 (5 s, 3 s); the desk's default step is 25.
pub const TICKS_PER_SECOND: u32 = 25;

/// Steps a cooked scene keeps at most; a longer cycle is resampled.
pub const MAX_STEPS: usize = 80;

/// Pairs a cycle needs compared before it counts as found.
const MIN_EVIDENCE: usize = 8;

/// Where a recording repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cycle {
    /// The first frame of the cycle kept.
    pub start: usize,
    /// Frames in one repeat; 1 is a static look.
    pub len: usize,
    /// Every frame matched the one a cycle later. `false`: the period falls
    /// between frames (or there is none), and the best fit was taken.
    pub exact: bool,
}

/// How a scene moves from one step to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// Hold each step, jump to the next: chases, strobes, stabs.
    Snap,
    /// Fade into each next step over one frame's time: rolls, sweeps.
    Fade,
}

/// One step of a cooked cycle: a frame and its times, in ticks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timed {
    pub frame: Vec<u8>,
    pub wait: u32,
    pub fade: u32,
}

fn max_diff(a: &[u8], b: &[u8]) -> u8 {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

fn total_diff(a: &[u8], b: &[u8]) -> u64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .fold(0, u64::saturating_add)
}

/// The recording's cycle after `warmup` frames.
///
/// A frame within `tol` of every other frame is a static look (len 1).
/// Otherwise the shortest period at which every frame matches the one a
/// period later, within `tol`, with at least a few pairs to go on. A
/// period that falls between frames matches nowhere exactly; then the lag
/// that fits best once the frames have first drifted apart (the dip of the
/// difference curve) is taken, and failing that the whole tail.
#[must_use]
pub fn find_cycle(frames: &[Vec<u8>], warmup: usize, tol: u8) -> Cycle {
    let start = warmup.min(frames.len().saturating_sub(1));
    let tail = frames.get(start..).unwrap_or_default();
    let Some(first) = tail.first() else {
        return Cycle {
            start,
            len: 0,
            exact: true,
        };
    };
    if tail.iter().all(|f| max_diff(f, first) <= tol) {
        return Cycle {
            start,
            len: 1,
            exact: true,
        };
    }
    let longest = tail.len().saturating_sub(MIN_EVIDENCE);
    let mut errors: Vec<(usize, u64)> = Vec::new();
    for p in 2..=longest {
        let pairs = tail.iter().zip(tail.iter().skip(p));
        let (mut worst, mut total, mut count) = (0_u8, 0_u64, 0_u64);
        for (a, b) in pairs {
            worst = worst.max(max_diff(a, b));
            total = total.saturating_add(total_diff(a, b));
            count = count.saturating_add(1);
        }
        if worst <= tol {
            return Cycle {
                start,
                len: p,
                exact: true,
            };
        }
        errors.push((p, total.checked_div(count).unwrap_or(u64::MAX)));
    }
    Cycle {
        start,
        len: best_fit(&errors).unwrap_or(tail.len()),
        exact: false,
    }
}

/// The lag of least mean difference once the curve has climbed to half its
/// peak, if it then falls back below half again: a repeat, not drift.
fn best_fit(errors: &[(usize, u64)]) -> Option<usize> {
    let peak = errors.iter().map(|e| e.1).max()?;
    let half = peak / 2;
    let climbed = errors.iter().position(|e| e.1 >= half)?;
    errors
        .get(climbed..)?
        .iter()
        .filter(|e| e.1 < half)
        .min_by_key(|e| e.1)
        .map(|e| e.0)
}

/// Where a recording that ends held comes to rest.
///
/// The first frame from which every frame is within `tol` of the last, so
/// `frames[..=k]` is a one-shot (a hit, a fly-in) and where it lands.
/// `None` when the whole recording is that one held frame: nothing
/// happens.
#[must_use]
pub fn settles_at(frames: &[Vec<u8>], tol: u8) -> Option<usize> {
    let last = frames.last()?;
    let moving = frames.iter().rposition(|f| max_diff(f, last) > tol)?;
    Some(moving.saturating_add(1))
}

/// The tick at which frame `k` starts, rounded, so a cycle keeps its exact
/// length however the frames divide into ticks.
fn tick_at(k: usize, fps: u16) -> u32 {
    let k = u64::try_from(k).unwrap_or(u64::MAX);
    let num = k
        .saturating_mul(u64::from(TICKS_PER_SECOND))
        .saturating_mul(2)
        .saturating_add(u64::from(fps));
    let ticks = num
        .checked_div(u64::from(fps).saturating_mul(2))
        .unwrap_or(0);
    u32::try_from(ticks).unwrap_or(u32::MAX)
}

/// A cycle's frames as steps: each run of identical frames one step,
/// `cap` steps at most (a longer cycle keeps every n-th frame), timed from
/// `fps`.
#[must_use]
pub fn steps(cycle: &[Vec<u8>], fps: u16, motion: Motion, cap: usize) -> Vec<Timed> {
    // (first frame of the run, frames it spans)
    let runs = |stride: usize| -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = Vec::new();
        let mut at = 0;
        while at < cycle.len() {
            let span = stride.min(cycle.len().saturating_sub(at));
            let same = out
                .last()
                .and_then(|(i, _)| cycle.get(*i))
                .zip(cycle.get(at))
                .is_some_and(|(a, b)| a == b);
            match out.last_mut() {
                Some((_, n)) if same => *n = n.saturating_add(span),
                _ => out.push((at, span)),
            }
            at = at.saturating_add(span);
        }
        out
    };
    let mut stride = 1;
    let mut list = runs(stride);
    while list.len() > cap.max(1) {
        stride = stride.saturating_add(1);
        list = runs(stride);
    }
    // In Fade mode a run fades over its last sample's time: that sample
    // stood for `stride` frames (fewer at the end).
    let mut out = Vec::new();
    for (from, span) in list {
        let to = from.saturating_add(span);
        let duration = tick_at(to, fps).saturating_sub(tick_at(from, fps)).max(1);
        // Samples start on multiples of the stride; the run's last one
        // starts at the last multiple before `to`.
        let last_start = to
            .saturating_sub(1)
            .checked_div(stride)
            .unwrap_or(0)
            .saturating_mul(stride)
            .max(from);
        let fade = tick_at(to, fps)
            .saturating_sub(tick_at(last_start, fps))
            .min(duration);
        let (wait, fade) = match motion {
            Motion::Snap => (duration, 0),
            Motion::Fade => (duration.saturating_sub(fade), fade),
        };
        if let Some(frame) = cycle.get(from) {
            out.push(Timed {
                frame: frame.clone(),
                wait,
                fade,
            });
        }
    }
    out
}

/// Whether a cycle reads as smooth motion (fade between steps) or as
/// jumps: smooth when most step-to-step changes are small.
#[must_use]
pub fn motion_of(cycle: &[Vec<u8>]) -> Motion {
    let mut jumps: Vec<u8> = cycle
        .iter()
        .zip(cycle.iter().cycle().skip(1))
        .map(|(a, b)| max_diff(a, b))
        .filter(|d| *d > 0)
        .collect();
    jumps.sort_unstable();
    let median = jumps.get(jumps.len() / 2).copied().unwrap_or(0);
    if median <= 64 {
        Motion::Fade
    } else {
        Motion::Snap
    }
}

/// The frames with every fine (16-bit low byte) pan and tilt channel of
/// `universe` zeroed: a sweep's fine byte wraps from 255 to 0 every few
/// frames, and [`motion_of`] would take that for a jump.
#[must_use]
pub fn coarse(frames: &[Vec<u8>], patch: &Patch, universe: u16) -> Vec<Vec<u8>> {
    let fine: Vec<usize> = patch
        .fixtures
        .iter()
        .filter(|f| f.universe == universe)
        .flat_map(|f| {
            let start = usize::from(f.address).saturating_sub(1);
            patch
                .profile(&f.profile)
                .map(channels)
                .unwrap_or_default()
                .into_iter()
                .enumerate()
                .filter(|(_, c)| matches!(c.kind, Kind::PanFine | Kind::TiltFine))
                .map(move |(i, _)| start.saturating_add(i))
        })
        .collect();
    frames
        .iter()
        .map(|frame| {
            let mut out = frame.clone();
            for i in &fine {
                if let Some(b) = out.get_mut(*i) {
                    *b = 0;
                }
            }
            out
        })
        .collect()
}

/// One fixture's channels out of a frame, by its patch address.
#[must_use]
pub fn fixture_values(frame: &[u8], patch: &Patch, fixture: &Fixture) -> Option<Vec<Option<u8>>> {
    let from = usize::from(fixture.address).checked_sub(1)?;
    let to = from.checked_add(patch.footprint(fixture))?;
    Some(frame.get(from..to)?.iter().copied().map(Some).collect())
}

/// Whether a fixture's values give light: its dimmer (if it has one) is
/// up and one of its colour or white channels (if it has any) is too. A
/// parked mover's pan and tilt are not light.
fn gives_light(kinds: &[Kind], values: &[Option<u8>]) -> bool {
    let up = |want: &dyn Fn(Kind) -> bool| -> Option<bool> {
        let mut seen = false;
        let mut on = false;
        for (k, v) in kinds.iter().zip(values) {
            if want(*k) {
                seen = true;
                on |= v.is_some_and(|v| v > 0);
            }
        }
        seen.then_some(on)
    };
    let dimmer = up(&|k| k == Kind::Dimmer).unwrap_or(true);
    let colour = up(&|k| {
        matches!(
            k,
            Kind::Red | Kind::Green | Kind::Blue | Kind::White | Kind::WarmWhite
        )
    })
    .unwrap_or(true);
    dimmer && colour
}

/// The fixtures a recording lights in any frame ([`gives_light`]) and, of
/// those, the ones whose values change.
#[must_use]
pub fn lit_and_moving<'p>(
    frames: &[Vec<u8>],
    patch: &'p Patch,
    universe: u16,
) -> (Vec<&'p Fixture>, Vec<&'p Fixture>) {
    let mut lit = Vec::new();
    let mut moving = Vec::new();
    for f in patch.fixtures.iter().filter(|f| f.universe == universe) {
        let kinds: Vec<Kind> = patch
            .profile(&f.profile)
            .map(|p| channels(p).iter().map(|c| c.kind).collect())
            .unwrap_or_default();
        let values: Vec<Vec<Option<u8>>> = frames
            .iter()
            .filter_map(|fr| fixture_values(fr, patch, f))
            .collect();
        if values.iter().any(|v| gives_light(&kinds, v)) {
            lit.push(f);
            if values.windows(2).any(|w| w.first() != w.get(1)) {
                moving.push(f);
            }
        }
    }
    (lit, moving)
}

/// A scene from a cycle's frames, writing only `fixtures`: one step held
/// for a static cycle, else a Steps scene ([`steps`]).
#[must_use]
pub fn scene_from_frames(
    name: &str,
    color: &str,
    cycle: &[Vec<u8>],
    fps: u16,
    motion: Motion,
    patch: &Patch,
    fixtures: &[&Fixture],
) -> Scene {
    let slice = |frame: &[u8]| -> Vec<(String, Vec<Option<u8>>)> {
        fixtures
            .iter()
            .filter_map(|f| Some((f.uid.clone(), fixture_values(frame, patch, f)?)))
            .collect()
    };
    let mut scene = Scene::new(name, color);
    let timed = steps(cycle, fps, motion, MAX_STEPS);
    match timed.as_slice() {
        [] => {}
        [one] => scene.fixtures = slice(&one.frame),
        many => {
            scene.steps = many
                .iter()
                .map(|t| Step {
                    wait: t.wait,
                    fade: t.fade,
                    fixtures: slice(&t.frame),
                })
                .collect();
        }
    }
    scene
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(values: &[u8]) -> Vec<Vec<u8>> {
        values.iter().map(|v| vec![*v, 0]).collect()
    }

    #[test]
    fn a_held_look_is_one_frame() {
        let f = frames(&[0, 40, 80, 120, 120, 120, 120, 120, 120, 120, 120, 120, 120, 120]);
        assert_eq!(
            find_cycle(&f, 4, 1),
            Cycle {
                start: 4,
                len: 1,
                exact: true
            }
        );
    }

    #[test]
    fn a_hit_settles_and_a_held_frame_does_nothing() {
        assert_eq!(settles_at(&frames(&[0, 255, 128, 40, 40, 40]), 1), Some(3));
        assert_eq!(settles_at(&frames(&[40, 41, 40, 40]), 1), None);
    }

    #[test]
    fn a_chase_repeats_every_four_frames() {
        let f = frames(&[9, 9, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0]);
        let c = find_cycle(&f, 2, 1);
        assert_eq!((c.len, c.exact), (4, true));
    }

    #[test]
    fn a_period_between_frames_is_fitted() {
        // A sine with a period of 13.3 frames never repeats on a frame.
        let f: Vec<Vec<u8>> = (0..80)
            .map(|i| {
                let x = (f64::from(i) * std::f64::consts::TAU / 13.3).sin();
                vec![format!("{:.0}", 127.0_f64.mul_add(x, 127.5)).parse().unwrap_or(0)]
            })
            .collect();
        let c = find_cycle(&f, 0, 1);
        assert!(!c.exact);
        // Three periods (39.9 frames) land closest to a whole frame.
        assert_eq!(c.len, 40);
    }

    #[test]
    fn repeats_fold_into_one_longer_step() {
        let f = frames(&[0, 0, 0, 255, 255, 0]);
        let s = steps(&f, 10, Motion::Snap, 80);
        let shape: Vec<(u8, u32, u32)> = s
            .iter()
            .map(|t| (t.frame.first().copied().unwrap_or(9), t.wait, t.fade))
            .collect();
        // 0.3 s, 0.2 s and 0.1 s at 25 ticks a second, rounded so the
        // cycle is still 0.6 s (15 ticks) long.
        assert_eq!(shape, [(0, 8, 0), (255, 5, 0), (0, 2, 0)]);
        assert_eq!(s.iter().map(|t| t.wait).sum::<u32>(), 15);
    }

    #[test]
    fn a_fade_spends_the_last_frame_fading() {
        let f = frames(&[0, 0, 100, 200]);
        let s = steps(&f, 10, Motion::Fade, 80);
        let shape: Vec<(u32, u32)> = s.iter().map(|t| (t.wait, t.fade)).collect();
        assert_eq!(shape, [(3, 2), (0, 3), (0, 2)]);
    }

    #[test]
    fn a_long_cycle_is_resampled_under_the_cap() {
        let f: Vec<Vec<u8>> = (0..200_u8).map(|i| vec![i]).collect();
        let s = steps(&f, 10, Motion::Fade, 80);
        assert!(s.len() <= 80);
        // Still 20 s long.
        assert_eq!(s.iter().map(|t| t.wait + t.fade).sum::<u32>(), 500);
    }

    #[test]
    fn coarse_zeroes_fine_pan_and_tilt() {
        let mover = crate::library::mini_gobo_moving_head_11ch().unwrap();
        let uid = mover.get("SSLFIXUID").unwrap().to_owned();
        let mut patch = Patch::default();
        patch.add_profile(mover);
        patch.fixtures.push(Fixture::named("m", &uid, 3));
        let frame: Vec<u8> = (1..=16).collect();
        let out = coarse(&[frame], &patch, 1);
        // X, µX, Y, µY from address 3: the fine halves zeroed, the rest kept.
        assert_eq!(out[0][..8], [1, 2, 3, 0, 5, 0, 7, 8]);
    }

    #[test]
    fn a_parked_mover_gives_no_light() {
        let kinds = [Kind::Pan, Kind::Tilt, Kind::Dimmer];
        assert!(!gives_light(&kinds, &[Some(128), Some(128), Some(0)]));
        assert!(gives_light(&kinds, &[Some(128), Some(128), Some(9)]));
        let bar = [Kind::Red, Kind::Green, Kind::Blue];
        assert!(!gives_light(&bar, &[Some(0), Some(0), Some(0)]));
        let tower = [Kind::Dimmer, Kind::Red];
        assert!(!gives_light(&tower, &[Some(255), Some(0)]));
    }

    #[test]
    fn rolls_fade_and_chases_snap() {
        let roll: Vec<Vec<u8>> = (0..20_u8).map(|i| vec![i.saturating_mul(10)]).collect();
        assert_eq!(motion_of(&roll), Motion::Fade);
        assert_eq!(motion_of(&frames(&[0, 255, 0, 255])), Motion::Snap);
    }
}
