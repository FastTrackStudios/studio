//! The manual slip: drag one hit, and the audio between it and the next
//! hit slides with the mouse — on every member of the group.
//!
//! This is the hand-edit half of drum editing (`quick-edit` in the
//! REAPER workflow). It is deliberately *not* its own write path: a slip
//! is a three-piece split plan — the audio before the hit (does not
//! move), the audio from the hit to the next hit (moves by the drag),
//! and the audio after (does not move) — handed to the same
//! [`apply_split`](crate::audio::apply_quantize::apply_split) the quantizer
//! uses. One cut law, one pad law, one crossfade law; a quantize cut and
//! a hand cut cannot come apart.
//!
//! Spec: `drum-mode.md` r[drums.manual.slip], r[drums.manual.daw-split].

use daw::service::item::Items;
use daw::service::{ItemRef, ProjectContext, Takes};

use crate::audio::apply_quantize::{Applied, GroupError, apply_split};
use crate::audio::quantize::{Piece, SplitConfig};

/// The three-piece plan for one slipped hit.
///
/// Times are seconds from the item's start (take playback time), the
/// same axis a [`Piece`] uses. `hit` is the dragged transient, `next` the
/// transient after it (or the item end when the hit is the last), and
/// `delta` how far the drag moved it — positive is later.
///
/// The cut goes `pad` *before* each transient and the shift is measured
/// at the transient — the same law as the quantize planner, and for the
/// same reason: cutting on the attack clips it, and shifting the cut
/// instead of the hit flams the piece against the kit.
///
/// The drag is clamped so the moved piece cannot land before the item
/// start or swallow the following hit: `delta` is limited to the span
/// between the two cuts on either side.
// r[impl drums.manual.slip]
pub fn slip_pieces(
    hit: f64,
    next: f64,
    take_secs: f64,
    delta: f64,
    cfg: SplitConfig,
) -> Vec<Piece> {
    let pad = cfg.leading_pad_secs.max(0.0);
    let cut1 = (hit - pad).max(0.0);
    let cut2 = (next - pad).clamp(cut1, take_secs);
    let end = take_secs.max(cut2);

    // The moved piece may not slide past its neighbours' cuts.
    let delta = delta.clamp(-cut1, (end - cut2).max(0.0));

    let mut pieces = Vec::with_capacity(3);
    if cut1 > 0.0 {
        pieces.push(Piece {
            cut: 0.0,
            end: cut1,
            shift: 0.0,
            transient: None,
        });
    }
    if cut2 > cut1 {
        pieces.push(Piece {
            cut: cut1,
            end: cut2,
            shift: delta,
            transient: Some(hit),
        });
    }
    if end > cut2 {
        pieces.push(Piece {
            cut: cut2,
            end,
            shift: 0.0,
            transient: Some(next),
        });
    }
    pieces
}

/// Cut the take in two at `at`, moving nothing.
///
/// The cut lands a `leading_pad_secs` *before* the click, for the same
/// reason every other cut in this crate does: a cut exactly on a
/// transient clips the attack off it, and the click will be near a hit
/// because that is where anyone aims. The pad is the difference between
/// a split you can hear and one you cannot.
///
/// Empty when the cut would fall outside the take, or would leave a
/// piece too short to be one. Splitting past either end produces a
/// single piece, which is not a split and would rebuild every item to
/// achieve nothing; splitting a hair inside an end produces a sliver
/// that cannot even hold its own crossfade, which is worse — it is a
/// item in the timeline that the user has to find and delete.
// r[impl drums.manual.split]
pub fn split_pieces(at: f64, take_secs: f64, cfg: SplitConfig) -> Vec<Piece> {
    // A click at or beyond either end is not a split at all, before any
    // question of where the pad puts the cut.
    if !(at > 0.0 && at < take_secs) {
        return Vec::new();
    }
    let pad = cfg.leading_pad_secs.max(0.0);
    let cut = (at - pad).clamp(0.0, take_secs);
    // Both halves need room for a fade in *and* out plus something
    // between them, and 20ms regardless — shorter than any drum's decay
    // and longer than a click, so the floor never refuses a split
    // anybody meant while still refusing the slivers nobody does.
    let min_piece = (cfg.crossfade_secs * 2.0).max(0.020);
    if cut < min_piece || take_secs - cut < min_piece {
        return Vec::new();
    }
    vec![
        Piece {
            cut: 0.0,
            end: cut,
            shift: 0.0,
            transient: None,
        },
        Piece {
            cut,
            end: take_secs,
            shift: 0.0,
            transient: Some(at),
        },
    ]
}

/// Split a whole group at one time.
///
/// Every member is cut at the same place, which is the group rule: mics
/// cut at different times are no longer phase-coherent, and a kit that
/// has lost phase coherence cannot be un-lost by hand.
// r[impl drums.manual.split]
pub fn split_group<D>(
    daw: &D,
    project: ProjectContext,
    items: &[ItemRef],
    at: f64,
    take_secs: f64,
    cfg: SplitConfig,
) -> Result<Applied, GroupError>
where
    D: Items + Takes + Clone,
{
    let pieces = split_pieces(at, take_secs, cfg);
    if pieces.is_empty() {
        return Ok(Applied::default());
    }
    apply_split(daw, project, items, &pieces, cfg)
}

/// Slip one hit on a whole group.
///
/// `items` are the group's members — one item per mic, sharing a start
/// (the group rule; refused as [`GroupError::Ragged`] otherwise). The
/// cut times and the slide are identical on every member by
/// construction, which is what keeps the mics phase-coherent.
// r[impl drums.manual.slip]
pub fn slip_hit<D>(
    daw: &D,
    project: ProjectContext,
    items: &[ItemRef],
    hit: f64,
    next: f64,
    take_secs: f64,
    delta: f64,
    cfg: SplitConfig,
) -> Result<Applied, GroupError>
where
    D: Items + Takes,
{
    let pieces = slip_pieces(hit, next, take_secs, delta, cfg);
    apply_split(daw, project, items, &pieces, cfg)
}
