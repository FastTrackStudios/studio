//! A stretch of the drum edit put back as recorded, on the engine.
//!
//! Each mic of the kit plays its EDIT lane, made from its COMP lane (the
//! comp as recorded; `hit_place`). Over a stretch, putting the edit back
//! is making the EDIT lane what the COMP lane is there: the EDIT slices
//! over it cleared (one reaching past either end trimmed to what is
//! outside, crossfaded into what comes in), and the COMP items under it
//! copied onto the EDIT lane. Every mic together, as one batch of item
//! edits under one lock and one undo step — no window to queue it for and
//! wait on (what `EDIT_REVERT` did, 30 s at worst).
//!
//! A kit without the lanes has no recording kept to put back from: the
//! older window-queued restore is what puts it back then.

use daw::service::Tracks as _;
use daw::standalone::item_ops::ItemOp;
use daw_proto::id::ItemId;

use crate::drum_lanes::{COMP, EDIT, lane_named};
use crate::hit_place::Slice;

/// What putting a stretch back did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Restored {
    /// Mics put back (tracks with both lanes).
    pub tracks: usize,
    /// EDIT slices over the stretch: taken out or trimmed to its ends.
    pub cleared: usize,
    /// COMP items copied onto the EDIT lane there.
    pub copied: usize,
    /// Seams crossfaded (one each end, on every mic, where audio meets).
    pub crossfades: usize,
    /// Item edits made.
    pub items_written: usize,
}

/// Whether a revert lays a no-quantize zone over what it put back: only
/// when asked (`hold: true`). A zone nobody asked for hid a whole verse
/// from every quantize after, and nothing said why.
#[must_use]
pub const fn holds(asked: Option<bool>) -> bool {
    matches!(asked, Some(true))
}

/// Grain under which two times are one, seconds.
const GRAIN: f64 = 1e-6;

/// Put the kit back as recorded over `spans` (song seconds, as it plays
/// now), every mic with COMP and EDIT lanes, as one undo step, seams
/// crossfaded over `crossfade` seconds. Traced as `revert.read`,
/// `revert.plan` and `revert.apply`.
///
/// # Errors
/// No mic of the kit has both lanes, or the engine refused an edit.
pub fn restore(
    song: &crate::song::Song,
    spans: &[(f64, f64)],
    crossfade: f64,
) -> Result<Restored, String> {
    let stage = crate::edit_trace::stage_span!("revert.read").entered();
    let mics: Vec<(String, u32, u32)> = song
        .daw()
        .all(song.context())
        .into_iter()
        .filter(|t| !crate::hits_midi::is_view(t))
        .filter_map(|t| Some((t.guid.clone(), lane_named(&t, COMP)?, lane_named(&t, EDIT)?)))
        .collect();
    if mics.is_empty() {
        return Err("no mic of the kit has COMP and EDIT lanes".to_owned());
    }
    crate::edit_trace::count_here("edit.tracks", mics.len());
    let reach = crossfade.max(0.0) + 1.0;
    let lanes: Vec<(u32, Vec<Vec<Slice>>, Vec<Vec<Slice>>)> = song
        .read(|p| {
            mics.iter()
                .map(|(track, comp, edit)| {
                    let (mut edits, mut comps) = (Vec::new(), Vec::new());
                    for &(from, to) in spans {
                        let at = (from - reach, to + reach);
                        edits.push(crate::hit_place::lane_of(p, track, *edit, at));
                        comps.push(crate::hit_place::lane_of(p, track, *comp, at));
                    }
                    (*edit, edits, comps)
                })
                .collect()
        })
        .ok_or("the song closed")?;
    drop(stage);
    let stage = crate::edit_trace::stage_span!("revert.plan").entered();
    let mut done = Restored {
        tracks: mics.len(),
        ..Restored::default()
    };
    let mut ops: Vec<ItemOp> = Vec::new();
    for (edit_lane, edits, comps) in &lanes {
        for ((span, edit), comp) in spans.iter().zip(edits).zip(comps) {
            let made = restore_ops(edit, comp, *edit_lane, *span, crossfade);
            done.cleared = done.cleared.saturating_add(made.cleared);
            done.copied = done.copied.saturating_add(made.copied);
            done.crossfades = done.crossfades.saturating_add(made.crossfades);
            ops.extend(made.ops);
        }
    }
    done.items_written = ops.len();
    crate::edit_trace::count_here("edit.items_written", ops.len());
    crate::edit_trace::count_here("edit.crossfades", done.crossfades);
    drop(stage);
    if ops.is_empty() {
        return Ok(done);
    }
    let stage = crate::edit_trace::stage_span!("revert.apply").entered();
    crate::edit_trace::count_here("edit.items_written", ops.len());
    song.undo_step("Put the drums back as played", |song| song.apply(&ops))
        .map_err(|e| e.to_string())?;
    drop(stage);
    Ok(done)
}

/// One mic's edits for one stretch, and what they come to.
#[derive(Debug, Default)]
pub struct Ops {
    pub ops: Vec<ItemOp>,
    pub cleared: usize,
    pub copied: usize,
    pub crossfades: usize,
}

/// What makes one mic's EDIT lane (`edit`, on lane `edit_lane`) over
/// `from..to` what its COMP lane (`comp`) is there: each EDIT slice over
/// it taken out, or trimmed to what lies outside — reaching `crossfade`
/// into the stretch, faded out (or in) over it — and each COMP item's part
/// in it copied onto the EDIT lane, faded in and out over the crossfade
/// where it meets what is outside, else the comp's own short join.
#[must_use]
pub fn restore_ops(
    edit: &[Slice],
    comp: &[Slice],
    edit_lane: u32,
    (from, to): (f64, f64),
    crossfade: f64,
) -> Ops {
    let xfade = crossfade.max(0.0);
    let mut out = Ops::default();
    // Whether the stretch's ends meet audio outside it on the EDIT lane.
    let (mut met_before, mut met_after) = (false, false);
    for slice in edit {
        let (at, till) = (slice.start(), slice.end());
        if till <= from + GRAIN || at >= to - GRAIN {
            continue;
        }
        out.cleared = out.cleared.saturating_add(1);
        let item = &slice.item;
        let id = ItemId::from(item.guid.as_str());
        let offset = item.start_offset.as_seconds();
        let (head, tail) = (at < from - GRAIN, till > to + GRAIN);
        if tail {
            met_after = true;
            // What plays after the stretch, reaching back into it by the
            // crossfade, faded in over it.
            let rest = if head {
                let copy = ItemId::new();
                out.ops.push(ItemOp::Copy {
                    item: id.clone(),
                    into: copy.clone(),
                });
                copy
            } else {
                id.clone()
            };
            let start = (to - xfade).max(at);
            out.ops.push(ItemOp::Place {
                item: rest.clone(),
                at: start,
                length: till - start,
                offset: offset + (start - at),
            });
            out.ops.push(ItemOp::Fades {
                item: rest,
                fade_in: to - start,
                fade_out: item.fade_out_length.as_seconds(),
            });
        }
        if head {
            met_before = true;
            // What plays before it, reaching into it by the crossfade,
            // faded out over it.
            let end = (from + xfade).min(till);
            out.ops.push(ItemOp::Place {
                item: id.clone(),
                at,
                length: end - at,
                offset,
            });
            out.ops.push(ItemOp::Fades {
                item: id,
                fade_in: item.fade_in_length.as_seconds(),
                fade_out: end - from,
            });
        } else if !tail {
            out.ops.push(ItemOp::Delete { item: id });
        }
    }
    for slice in comp {
        let item = &slice.item;
        let start = slice.start();
        let (a, b) = (from.max(start), to.min(slice.end()));
        if b - a <= GRAIN {
            continue;
        }
        out.copied = out.copied.saturating_add(1);
        let copy = ItemId::new();
        // At the stretch's ends, the crossfade with what is outside; inside
        // it, the comp's own joins.
        let fade_in = if (a - from).abs() < GRAIN && met_before {
            xfade
        } else if (a - start).abs() < GRAIN {
            item.fade_in_length.as_seconds()
        } else {
            crate::comp::JOIN
        };
        let fade_out = if (b - to).abs() < GRAIN && met_after {
            xfade
        } else if (b - slice.end()).abs() < GRAIN {
            item.fade_out_length.as_seconds()
        } else {
            crate::comp::JOIN
        };
        out.ops.extend([
            ItemOp::Copy {
                item: ItemId::from(item.guid.as_str()),
                into: copy.clone(),
            },
            ItemOp::Lane {
                item: copy.clone(),
                lane: edit_lane,
            },
            ItemOp::Place {
                item: copy.clone(),
                at: a,
                length: b - a,
                offset: item.start_offset.as_seconds() + (a - start),
            },
            ItemOp::Fades {
                item: copy,
                fade_in,
                fade_out,
            },
        ]);
    }
    out.crossfades = usize::from(met_before && xfade > 0.0) + usize::from(met_after && xfade > 0.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_proto::primitives::{Duration, PositionInSeconds};

    fn slice(guid: &str, at: f64, length: f64, offset: f64) -> Slice {
        Slice {
            item: daw_proto::Item {
                guid: guid.to_owned(),
                position: PositionInSeconds::from_seconds(at),
                length: Duration::from_seconds(length),
                start_offset: Duration::from_seconds(offset),
                ..daw_proto::Item::default()
            },
            file: Some("kick.wav".to_owned()),
            markers: Vec::new(),
        }
    }

    fn places(ops: &[ItemOp]) -> Vec<(f64, f64, f64)> {
        ops.iter()
            .filter_map(|op| match op {
                ItemOp::Place {
                    at, length, offset, ..
                } => Some((*at, *length, *offset)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn one_edit_slice_over_the_stretch_is_trimmed_either_side_and_the_comp_copied_in() {
        // The EDIT lane: one slice 0..10 slipped 20 ms (its offset says so).
        let edit = [slice("e", 0.0, 10.0, 0.02)];
        let comp = [slice("c", 0.0, 10.0, 0.0)];
        let made = restore_ops(&edit, &comp, 3, (4.0, 6.0), 0.01);
        assert_eq!(made.cleared, 1);
        assert_eq!(made.copied, 1);
        assert_eq!(made.crossfades, 2);
        let p = places(&made.ops);
        // The rest after, from 6 − 10 ms; the head before, to 4 + 10 ms;
        // the comp's copy over 4..6, as recorded.
        assert!(
            (p[0].0 - 5.99).abs() < 1e-9 && (p[0].2 - (0.02 + 5.99)).abs() < 1e-9,
            "{p:?}"
        );
        assert!(
            (p[1].0 - 0.0).abs() < 1e-9 && (p[1].1 - 4.01).abs() < 1e-9,
            "{p:?}"
        );
        assert!(
            (p[2].0 - 4.0).abs() < 1e-9
                && (p[2].1 - 2.0).abs() < 1e-9
                && (p[2].2 - 4.0).abs() < 1e-9
        );
        assert!(
            made.ops
                .iter()
                .any(|op| matches!(op, ItemOp::Lane { lane: 3, .. }))
        );
    }

    #[test]
    fn slices_inside_the_stretch_are_taken_out_and_those_outside_left() {
        let edit = [
            slice("before", 0.0, 3.0, 0.0),
            slice("in1", 4.2, 0.5, 4.2),
            slice("in2", 4.7, 1.0, 4.71),
            slice("after", 7.0, 2.0, 7.0),
        ];
        let comp = [slice("c", 0.0, 10.0, 0.0)];
        let made = restore_ops(&edit, &comp, 1, (4.0, 6.0), 0.01);
        let deleted = made
            .ops
            .iter()
            .filter(|op| matches!(op, ItemOp::Delete { .. }))
            .count();
        assert_eq!(deleted, 2);
        assert_eq!(made.cleared, 2);
        // Nothing outside met: the copy keeps the comp's joins, no crossfade.
        assert_eq!(made.crossfades, 0);
        assert!(made.ops.iter().all(|op| op.item().as_str() != "before"));
    }

    #[test]
    fn a_revert_lays_no_zone_unless_asked() {
        assert!(!holds(None), "no zone by default");
        assert!(!holds(Some(false)));
        assert!(holds(Some(true)));
    }

    #[test]
    fn nothing_on_the_comp_there_copies_nothing() {
        let made = restore_ops(&[], &[slice("c", 20.0, 1.0, 0.0)], 1, (4.0, 6.0), 0.01);
        assert!(made.ops.is_empty());
    }
}
