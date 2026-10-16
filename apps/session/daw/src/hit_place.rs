//! A hand's hit moves, on the engine: the stretch around them played
//! again from the recording ([`editing::placement`]), not the slices
//! already there cut and patched again.
//!
//! Each mic of the kit plays its EDIT lane, made from its COMP lane (the
//! comp as recorded). A hit's *shift* is where its EDIT slice plays it
//! minus where the COMP lane has it — read through the recording both
//! play. The moves add to those shifts; the stretch from two hits before
//! the first moved to two after the last is cleared on every mic's EDIT
//! lane and placed again from its COMP lane, piece by piece. A hit moved
//! back is the recording again, uncut.

use daw::service::{ItemRef, Items as _, ProjectContext, Takes as _, TrackRef, Tracks as _};
use daw_proto::primitives::{Duration, PositionInSeconds};
use daw_proto::{StretchMarker, StretchMarkers as _, StretchTakeRef, TakeRef};
use editing::placement::{self, Hit, Piece};

use crate::drum_lanes::{COMP, EDIT, lane_named};

/// How many hits either side of the moved ones are placed again with
/// them: the seams around a move are worked out against their hits.
const CONTEXT: usize = 2;

/// Two times this close are one hit, seconds.
const SAME_HIT: f64 = 0.002;

/// How a placement cuts and joins, seconds.
#[derive(Clone, Copy, Debug)]
pub struct Joins {
    /// How far before a hit its piece starts.
    pub lead: f64,
    /// The crossfade across a seam.
    pub crossfade: f64,
}

/// A mic of the kit: its track and its two lanes.
struct Mic {
    track: String,
    comp: u32,
    edit: u32,
}

/// An item on a lane and the recording it plays.
struct Slice {
    item: daw_proto::Item,
    file: Option<String>,
    markers: Vec<StretchMarker>,
}

impl Slice {
    fn start(&self) -> f64 {
        self.item.position.as_seconds()
    }

    fn end(&self) -> f64 {
        self.start() + self.item.length.as_seconds()
    }

    /// Where in its recording it plays at `t` (timeline seconds).
    fn source_at(&self, t: f64) -> f64 {
        let take = t - self.start();
        let segment = self.markers.windows(2).find(|w| take <= w[1].position);
        match segment.or_else(|| self.markers.windows(2).last()) {
            Some([a, b]) if b.position > a.position => {
                a.source_position
                    + (take - a.position) * (b.source_position - a.source_position)
                        / (b.position - a.position)
            }
            _ => self.item.start_offset.as_seconds() + take,
        }
    }
}

/// Move the kit's hits — `moves` as `(hit, by)`, the hit where it plays
/// now, seconds — and play the stretch around them again from the
/// recording, every mic together, as one undo step. How many items the
/// EDIT lanes now hold there.
///
/// # Errors
/// When the kit has no mic with both lanes, a hit is not on an EDIT
/// slice, or the engine refuses an edit.
pub fn place(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    moves: &[(f64, f64)],
    joins: Joins,
) -> Result<usize, String> {
    let moved: Vec<(f64, f64)> = moves.iter().copied().filter(|m| m.1.abs() > 1e-6).collect();
    if moved.is_empty() {
        return Ok(0);
    }
    let mics = mics(daw, project);
    let reference = mics
        .first()
        .ok_or("no mic of the kit has COMP and EDIT lanes")?;
    let mut hits: Vec<f64> = crate::hits::playback(daw, project)
        .iter()
        .map(|p| p.time)
        .collect();
    hits.sort_by(f64::total_cmp);
    hits.dedup_by(|a, b| (*a - *b).abs() < SAME_HIT);

    // The hits placed again: from where the moved ones were to where they
    // land, and a couple either side.
    let index_of = |t: f64| hits.partition_point(|h| *h < t - SAME_HIT);
    let lowest = moved
        .iter()
        .map(|m| m.0.min(m.0 + m.1))
        .fold(f64::INFINITY, f64::min);
    let highest = moved
        .iter()
        .map(|m| m.0.max(m.0 + m.1))
        .fold(f64::NEG_INFINITY, f64::max);
    let first = index_of(lowest);
    let last = index_of(highest).min(hits.len().saturating_sub(1));
    let from = first.saturating_sub(CONTEXT);
    let to = (last + CONTEXT).min(hits.len().saturating_sub(1));
    let around = hits.get(from..=to).ok_or("no hit where a move was asked")?;

    // Each hit as recorded and shifted, read on the reference mic.
    let edit = slices(daw, project, reference, reference.edit);
    let comp = slices(daw, project, reference, reference.comp);
    let mut placed: Vec<Hit> = Vec::with_capacity(around.len());
    for &t in around {
        let shift = shift_at(&edit, &comp, t)
            .ok_or_else(|| format!("the hit at {t:.3} s is not on an EDIT slice"))?;
        let by = moved
            .iter()
            .find(|m| (m.0 - t).abs() < SAME_HIT)
            .map_or(0.0, |m| m.1);
        placed.push(Hit {
            at: t - shift,
            shift: shift + by,
        });
    }
    // The span: from the start of the first hit's slice to the end of the
    // last's, so the pieces at its ends meet what is either side unchanged.
    let holding = |t: f64| {
        edit.iter()
            .rev()
            .find(|s| s.start() <= t + 1e-9 && t < s.end())
    };
    let (start, end) = match (around.first(), around.last()) {
        (Some(&a), Some(&b)) => (
            holding(a).map_or(a, Slice::start),
            holding(b).map_or(b, Slice::end),
        ),
        _ => return Ok(0),
    };
    // The later side of each seam reaches back by the crossfade too, so
    // the hit before is guarded by that much more.
    let cut = placement::Cut {
        lead: joins.lead,
        guard: placement::Cut::default().guard + joins.crossfade,
    };
    // Read in the order they play; placed in the order they were played.
    // Two at one recorded moment: a shift read off the wrong slice.
    placed.sort_by(|a, b| a.at.total_cmp(&b.at));
    if !placed.windows(2).all(|w| w[0].at < w[1].at - 1e-9) {
        return Err(format!(
            "the hits as recorded are out of order: {:?}; moves {moved:?}; hits there {around:?}",
            placed.iter().map(|h| (h.at, h.shift)).collect::<Vec<_>>()
        ));
    }
    let pieces = placement::place(&placed, (start, end), cut);
    if let Some(bad) = pieces.iter().find(|p| p.end <= p.start) {
        return Err(format!(
            "a piece would end before it starts ({bad:?}) placing {:?} over {start:.3}..{end:.3}",
            placed.iter().map(|h| (h.at, h.shift)).collect::<Vec<_>>()
        ));
    }

    daw::service::Projects::begin_undo_block(daw, project.clone(), "Place a drum hit");
    let rendered = mics
        .iter()
        .map(|mic| render(daw, project, mic, &pieces, (start, end), joins))
        .sum::<Result<usize, daw_proto::DawError>>();
    daw::service::Projects::end_undo_block(daw, project.clone(), "Place a drum hit", None);
    rendered.map_err(|e| e.to_string())
}

/// Whether the kit can be placed from its recording: some mic has both a
/// COMP and an EDIT lane. A kit edited on the items it plays, without the
/// lanes, has no recording kept to place from.
#[must_use]
pub fn ready(daw: &daw::standalone::Standalone, project: &ProjectContext) -> bool {
    !mics(daw, project).is_empty()
}

/// Every track with both a COMP and an EDIT lane: the kit's mics.
fn mics(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<Mic> {
    daw.all(project.clone())
        .into_iter()
        .filter_map(|t| {
            Some(Mic {
                comp: lane_named(&t, COMP)?,
                edit: lane_named(&t, EDIT)?,
                track: t.guid,
            })
        })
        .collect()
}

/// `mic`'s items on `lane`, by start, each with its recording.
fn slices(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    mic: &Mic,
    lane: u32,
) -> Vec<Slice> {
    let mut out: Vec<Slice> = daw
        .get_items(project.clone(), TrackRef::Guid(mic.track.clone()))
        .into_iter()
        .filter(|i| i.fixed_lane.unwrap_or(0) == lane)
        .map(|item| {
            let at = ItemRef::Guid(item.guid.clone());
            let file = daw
                .get_active_take(project.clone(), at.clone())
                .and_then(|t| t.source_file_path);
            let markers = daw.get_stretch_markers(project.clone(), at, TakeRef::Active);
            Slice {
                item,
                file,
                markers,
            }
        })
        .collect();
    out.sort_by(|a, b| a.start().total_cmp(&b.start()));
    out
}

/// Where the EDIT lane plays the moment at `t` minus where the COMP lane
/// has it: the slice there, read through the recording it plays, found on
/// the COMP lane.
fn shift_at(edit: &[Slice], comp: &[Slice], t: f64) -> Option<f64> {
    // The latest slice to start by `t`: at a crossfade, the one coming in.
    let slice = edit
        .iter()
        .rev()
        .find(|s| s.start() <= t + 1e-9 && t < s.end())?;
    let source = slice.source_at(t);
    let recorded = comp.iter().find_map(|c| {
        let from = c.item.start_offset.as_seconds();
        let inside = source >= from - 1e-9 && source < from + c.item.length.as_seconds();
        (c.file == slice.file && inside).then(|| c.start() + (source - from))
    })?;
    Some(t - recorded)
}

/// `mic`'s EDIT lane over `start..end` made `pieces` of its COMP lane: what
/// was there cleared, each piece copied from the COMP items under it,
/// shifted, crossfaded into the one before. How many items it made.
fn render(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    mic: &Mic,
    pieces: &[Piece],
    (start, end): (f64, f64),
    joins: Joins,
) -> Result<usize, daw_proto::DawError> {
    const EDGE: f64 = 1e-6;
    let edit = slices(daw, project, mic, mic.edit);
    // The span's own ends keep the fades they had: the seams either side
    // of it, with what is outside, stay as they were.
    let edge_fade_in = edit
        .iter()
        .find(|s| (s.start() - start).abs() < EDGE)
        .map_or(0.0, |s| s.item.fade_in_length.as_seconds());
    let edge_fade_out = edit
        .iter()
        .find(|s| (s.end() - end).abs() < EDGE)
        .map_or(0.0, |s| s.item.fade_out_length.as_seconds());
    // What starts in the span is made again; a slice from before that
    // only crossfades into it, or one after that crossfades in from its
    // end, is left alone. One unedited item over the whole span is cut
    // either side of it.
    for slice in edit.iter().filter(|s| {
        let starts_in = s.start() >= start - EDGE && s.start() < end - joins.crossfade - EDGE;
        let spans = s.start() < start - EDGE && s.end() > start + joins.crossfade + EDGE;
        starts_in || spans
    }) {
        crate::comp::clear(daw, project, &slice.item, (start, end))?;
    }
    let comp = slices(daw, project, mic, mic.comp);
    let mut made = 0usize;
    for (k, piece) in pieces.iter().enumerate() {
        // Into the piece before by the crossfade; the span's first and
        // last as the slices there were.
        let fade_in = if k == 0 {
            edge_fade_in
        } else {
            joins.crossfade
        };
        let fade_out = if k + 1 == pieces.len() {
            edge_fade_out
        } else {
            joins.crossfade
        };
        let reach_back = if k == 0 { 0.0 } else { fade_in };
        let (from, to) = (piece.start - reach_back, piece.end);
        let under: Vec<&Slice> = comp
            .iter()
            .filter(|c| c.end() > from - piece.shift && c.start() < to - piece.shift)
            .collect();
        for (n, source) in under.iter().enumerate() {
            let a = source.start().max(from - piece.shift);
            let b = source.end().min(to - piece.shift);
            let copy = daw
                .duplicate_item(project.clone(), ItemRef::Guid(source.item.guid.clone()))
                .ok_or_else(|| daw_proto::DawError::not_found("Item", &source.item.guid))?;
            let at = ItemRef::Guid(copy);
            daw.set_fixed_lane(project.clone(), at.clone(), mic.edit)?;
            daw.set_position(
                project.clone(),
                at.clone(),
                PositionInSeconds::from_seconds(a + piece.shift),
            )?;
            daw.set_length(project.clone(), at.clone(), Duration::from_seconds(b - a))?;
            daw.set_start_offset(
                project.clone(),
                at.clone(),
                TakeRef::Active,
                Duration::from_seconds(
                    source.item.start_offset.as_seconds() + (a - source.start()),
                ),
            )?;
            // The piece's own ends crossfade; a join between two COMP
            // items inside it keeps the COMP lane's own fades.
            let first = n == 0;
            let last = n + 1 == under.len();
            let fin = if first {
                fade_in
            } else {
                source.item.fade_in_length.as_seconds()
            };
            let fout = if last {
                fade_out
            } else {
                source.item.fade_out_length.as_seconds()
            };
            daw.set_fade_in(
                project.clone(),
                at.clone(),
                Duration::from_seconds(fin),
                source.item.fade_in_shape,
            )?;
            daw.set_fade_out(
                project.clone(),
                at.clone(),
                Duration::from_seconds(fout),
                source.item.fade_out_shape,
            )?;
            // The last resort: the decay drawn out to meet the next piece.
            if last && let Some(reach) = piece.drawn_to {
                draw_out(daw, project, &at, reach)?;
            }
            made += 1;
        }
    }
    Ok(made)
}

/// The item at `at` drawn out to end at `reach`: its attack kept, the rest
/// stretched over the longer span.
fn draw_out(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    at: &ItemRef,
    reach: f64,
) -> Result<(), daw_proto::DawError> {
    let Some(mut item) = daw.get_item(project.clone(), at.clone()) else {
        return Ok(());
    };
    let Some(markers) = crate::arrange_edit::draw_out_markers(&mut item, reach) else {
        return Ok(());
    };
    daw.set_length(project.clone(), at.clone(), item.length)?;
    daw.set_stretch_markers(
        StretchTakeRef::new(project.clone(), at.clone(), TakeRef::Active),
        markers
            .into_iter()
            .map(|(take, source)| StretchMarker::new(take, source))
            .collect(),
    )
}
