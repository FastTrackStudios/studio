//! The manual stretch: drag one hit in WARP mode, and the audio between
//! it and its neighbours stretches — on every member of the group.
//!
//! The WARP twin of [`slip`](crate::audio::slip): where a slip cuts and slides
//! rigid pieces, a stretch writes a marker map — the dragged hit moves,
//! its neighbours stay pinned, and the material between plays at the
//! rate the new spans imply. Like the slip it is deliberately not its
//! own write path: the three anchors become an [`Alignment`] and go
//! through the same [`apply_warp`](crate::audio::apply_quantize::apply_warp)
//! the quantizer uses, so a hand stretch and a quantize warp cannot
//! disagree about how a marker map reaches the group.
//!
//! The stretch is clamped to the timing separators' ⅛×–4× — the same
//! limits, imported rather than restated, because "how far can audio
//! stretch before it is mush" is one answer, not two.
//!
//! Spec: `drum-mode.md` r[drums.manual.stretch].

use crate::timing::{MAX_FACTOR, MIN_FACTOR};
use daw::service::item::Items;
use daw::service::{ItemRef, ProjectContext, StretchMarkers, Takes};

use crate::audio::align::{Alignment, Anchor, AnchorKind, Offset};
use crate::audio::apply_quantize::{Applied, GroupError, apply_warp};

/// Clamp a drag so both spans stay within the stretch limits.
///
/// The left span `hit - prev` plays as `hit + delta - prev` and the
/// right span `next - hit` as `next - hit - delta`; each ratio must
/// stay in `MIN_FACTOR..=MAX_FACTOR`. Beyond that the gesture refuses
/// rather than degrading — the same law as the timing separators.
// r[impl drums.manual.stretch]
pub fn clamp_delta(hit: f64, prev: f64, next: f64, delta: f64) -> f64 {
    let left = (hit - prev).max(0.0);
    let right = (next - hit).max(0.0);
    let lo = (left * (MIN_FACTOR - 1.0)).max(right * (1.0 - MAX_FACTOR));
    let hi = (left * (MAX_FACTOR - 1.0)).min(right * (1.0 - MIN_FACTOR));
    delta.clamp(lo.min(0.0), hi.max(0.0))
}

/// The three-anchor map for one stretched hit, in take playback time.
///
/// `both = false` pins the neighbours (`prev`, `next`); `both = true`
/// pins the take's ends instead, so the whole take before and after the
/// hit stretches — the `BothStretch` law from the timing separators.
///
/// `None` when the clamped drag is zero: an untouched take must not
/// acquire a warp.
// r[impl drums.manual.stretch]
pub fn stretch_alignment(
    hit: f64,
    prev: f64,
    next: f64,
    take_secs: f64,
    delta: f64,
    both: bool,
    frame_rate: f64,
) -> Option<Alignment> {
    if frame_rate <= 0.0 {
        return None;
    }
    let (pin_left, pin_right) = if both {
        (0.0, take_secs.max(hit))
    } else {
        (prev.clamp(0.0, hit), next.clamp(hit, take_secs.max(hit)))
    };
    let delta = clamp_delta(hit, pin_left, pin_right, delta);
    if delta.abs() < 1e-9 {
        return None;
    }
    let f = |secs: f64| (secs * frame_rate).round();
    let anchor = |secs: f64, target: f64, kind: AnchorKind| Anchor {
        dub: f(secs).max(0.0) as usize,
        reference: f(target),
        confidence: 1.0,
        kind,
    };
    let anchors = vec![
        anchor(pin_left, pin_left, AnchorKind::Start),
        anchor(hit, hit + delta, AnchorKind::Onset),
        anchor(pin_right, pin_right, AnchorKind::End),
    ];
    Some(Alignment {
        // `alignment_markers` reads the anchors when they exist; the
        // map only has to be non-empty for the "corrects nothing"
        // fast path not to fire.
        map: vec![0.0],
        anchors,
        offset: Offset::NONE,
        frame_rate,
    })
}

/// Stretch one hit on a whole group: the same marker map, adjusted per
/// take for its own placement, on every mic — one gesture, one
/// [`apply_warp`] call.
// r[impl drums.manual.stretch]
#[allow(clippy::too_many_arguments)]
pub fn stretch_hit<D>(
    daw: &D,
    project: ProjectContext,
    items: &[ItemRef],
    hit: f64,
    prev: f64,
    next: f64,
    take_secs: f64,
    delta: f64,
    both: bool,
    frame_rate: f64,
) -> Result<Applied, GroupError>
where
    D: Items + Takes + StretchMarkers,
{
    let Some(alignment) = stretch_alignment(hit, prev, next, take_secs, delta, both, frame_rate)
    else {
        return Ok(Applied::default());
    };
    apply_warp(daw, project, items, &alignment)
}
