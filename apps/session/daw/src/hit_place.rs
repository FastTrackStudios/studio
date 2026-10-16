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

use daw::service::Tracks as _;
use daw::standalone::item_ops::ItemOp;
use daw_proto::StretchMarker;
use daw_proto::id::ItemId;
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

/// A mic's two lanes around a placement, as they are: what it reads its
/// shifts from and places again.
struct Lanes {
    edit_lane: u32,
    edit: Vec<Slice>,
    comp: Vec<Slice>,
}

/// An item on a lane and the recording it plays.
pub(crate) struct Slice {
    pub(crate) item: daw_proto::Item,
    pub(crate) file: Option<String>,
    pub(crate) markers: Vec<StretchMarker>,
}

impl Slice {
    pub(crate) fn start(&self) -> f64 {
        self.item.position.as_seconds()
    }

    pub(crate) fn end(&self) -> f64 {
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
    song: &crate::song::Song,
    moves: &[(f64, f64)],
    joins: Joins,
) -> Result<usize, String> {
    let mut stages = architect_telemetry::timing::Stages::start();
    // Every moment the call names — moved or held where it is — by time,
    // and those it moves.
    let mut named: Vec<(f64, f64)> = moves.to_vec();
    named.sort_by(|a, b| a.0.total_cmp(&b.0));
    let moved: Vec<(f64, f64)> = named.iter().copied().filter(|m| m.1.abs() > 1e-6).collect();
    // The move of the hit at `t`: its moment's — the named moment nearest
    // it within a moment's width, so every hit struck together goes
    // together (a hit a few ms off its moment's time kept its old place,
    // and a seam between the two left one attack in both pieces: a flam).
    let move_of = |t: f64| {
        let i = named.partition_point(|m| m.0 < t);
        [i.checked_sub(1), Some(i)]
            .into_iter()
            .flatten()
            .filter_map(|i| named.get(i))
            .filter(|m| (m.0 - t).abs() <= editing::drums::KIT_MERGE)
            .min_by(|a, b| (a.0 - t).abs().total_cmp(&(b.0 - t).abs()))
            .map(|m| m.1)
    };
    if moved.is_empty() {
        return Ok(0);
    }
    let mics = mics(song);
    stages.mark("place.mics_ms");
    if mics.is_empty() {
        return Err("no mic of the kit has COMP and EDIT lanes".to_owned());
    }
    let mut hits: Vec<f64> = crate::hit_index::playback(song)
        .iter()
        .map(|p| p.time)
        .collect();
    hits.sort_by(f64::total_cmp);
    hits.dedup_by(|a, b| (*a - *b).abs() < SAME_HIT);
    stages.mark("place.hits_ms");

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
    let (Some(&near), Some(&far)) = (around.first(), around.last()) else {
        return Ok(0);
    };

    // Every mic's lanes around it, read at once: a slice reached from as
    // far as a move goes, either side.
    let reach = moved.iter().map(|m| m.1.abs()).fold(0.0, f64::max) + READ_MARGIN;
    let kit = read_kit(song, &mics, (near - reach, far + reach));
    if kit.is_empty() {
        return Err("the kit could not be read".to_owned());
    }
    stages.mark("place.read_ms");

    // Each hit as recorded and shifted, read on the reference mic. A hit
    // beside the moved ones with no EDIT slice under it (where the lane
    // has nothing: a gap, the song's edge) has nothing there to keep, and
    // is left out; a moved one must be on a slice.
    let mut placed: Vec<(Hit, bool)> = Vec::with_capacity(around.len());
    for &t in around {
        let by = move_of(t);
        // Every mic moves together: the first with a slice there says it
        // (a trigger's lane holds only its hits).
        let Some(shift) = kit
            .iter()
            .find_map(|lanes| shift_at(&lanes.edit, &lanes.comp, t))
        else {
            if by.is_some_and(|by| by.abs() > 1e-6) {
                return Err(format!("the hit at {t:.3} s is not on an EDIT slice"));
            }
            continue;
        };
        placed.push((
            Hit {
                at: t - shift,
                shift: shift + by.unwrap_or(0.0),
            },
            by.is_some_and(|by| by.abs() > 1e-6),
        ));
    }
    // The span: from the start of the first hit's slice to the end of the
    // last's, so the pieces at its ends meet what is either side unchanged
    // — but no further than the hits it places: a slice that plays on past
    // the next hit (one long item over a section never edited) is cut just
    // before it, the rest of it left as it plays. Cut where one recording
    // runs on at one alignment, the join is seamless.
    let holding = |t: f64| {
        kit.iter().find_map(|lanes| {
            lanes
                .edit
                .iter()
                .rev()
                .find(|s| s.start() <= t + 1e-9 && t < s.end())
        })
    };
    let before = from.checked_sub(1).and_then(|i| hits.get(i)).copied();
    let after = hits.get(to + 1).copied();
    let start = holding(near).map_or(near, Slice::start);
    let start = match before {
        Some(prev) if start <= prev => (near - joins.lead - joins.crossfade).max(prev + 1e-3),
        _ => start,
    };
    let end = holding(far).map_or(far, Slice::end);
    let end = match after {
        Some(next) if end > next - joins.lead => (next - joins.lead).max(far + 1e-3),
        _ => end,
    };
    let span = (start, end);
    // The later side of each seam reaches back by the crossfade too, so
    // the hit before is guarded by that much more.
    let cut = placement::Cut {
        lead: joins.lead,
        guard: placement::Cut::default().guard + joins.crossfade,
    };
    // Read in the order they play; placed in the order they were played.
    // One recorded hit heard twice (a flam an earlier edit left: two
    // slices that both play its attack) is placed once — the one a move
    // names, else the first — and the stretch played again heals it.
    placed.sort_by(|a, b| a.0.at.total_cmp(&b.0.at).then(b.1.cmp(&a.1)));
    placed.dedup_by(|later, earlier| (later.0.at - earlier.0.at).abs() < 1e-6);
    let placed: Vec<Hit> = placed.into_iter().map(|(hit, _)| hit).collect();
    if !placed.windows(2).all(|w| w[0].at < w[1].at - 1e-9) {
        return Err(format!(
            "the hits as recorded are out of order: {:?}; moves {moved:?}; hits there {around:?}",
            placed.iter().map(|h| (h.at, h.shift)).collect::<Vec<_>>()
        ));
    }
    // The attacks just outside the hits placed again, as recorded: the
    // pieces at the span's ends may not play them either.
    let recorded = |i: Option<usize>| {
        let t = *hits.get(i?)?;
        kit.iter()
            .find_map(|lanes| shift_at(&lanes.edit, &lanes.comp, t))
            .map(|shift| t - shift)
    };
    let outside = (recorded(from.checked_sub(1)), recorded(Some(to + 1)));
    let pieces = placement::place_within(&placed, span, cut, outside);
    if let Some(bad) = pieces.iter().find(|p| p.end <= p.start) {
        return Err(format!(
            "a piece would end before it starts ({bad:?}) placing {:?} over {:.3}..{:.3}",
            placed.iter().map(|h| (h.at, h.shift)).collect::<Vec<_>>(),
            span.0,
            span.1
        ));
    }
    let ops: Vec<ItemOp> = kit
        .iter()
        .flat_map(|lanes| render_ops(lanes, &pieces, span, joins))
        .collect();
    stages.mark("place.plan_ms");

    song.undo_step("Place a drum hit", |song| song.apply(&ops))
        .map_err(|e| e.to_string())?;
    stages.mark("place.apply_ms");
    Ok(ops.len())
}

/// How far past the hits around a move its lanes are read, seconds: a
/// slice holding one of them reaches no further than this before or after.
const READ_MARGIN: f64 = 4.0;

/// Whether the kit can be placed from its recording: some mic has both a
/// COMP and an EDIT lane. A kit edited on the items it plays, without the
/// lanes, has no recording kept to place from.
#[must_use]
pub fn ready(song: &crate::song::Song) -> bool {
    !mics(song).is_empty()
}

/// Every track with both a COMP and an EDIT lane: the kit's mics.
fn mics(song: &crate::song::Song) -> Vec<Mic> {
    song.daw()
        .all(song.context())
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

/// Every mic's EDIT and COMP lanes over `from..to`, each slice with the
/// recording it plays — read in one look at the engine, nothing copied but
/// the slices there.
fn read_kit(song: &crate::song::Song, mics: &[Mic], (from, to): (f64, f64)) -> Vec<Lanes> {
    song.read(|p| {
        mics.iter()
            .map(|mic| Lanes {
                edit_lane: mic.edit,
                edit: lane_of(p, &mic.track, mic.edit, (from, to)),
                comp: lane_of(p, &mic.track, mic.comp, (from, to)),
            })
            .collect()
    })
    .unwrap_or_default()
}

/// `track`'s items on `lane` over `from..to`, by start, each with the
/// recording it plays — read off the state, nothing asked of the engine.
pub(crate) fn lane_of(
    p: &daw::standalone::sync::ProjectState,
    track: &str,
    lane: u32,
    (from, to): (f64, f64),
) -> Vec<Slice> {
    let mut out: Vec<Slice> = p
        .items_by_track
        .get(track)
        .into_iter()
        .flatten()
        .filter_map(|guid| p.items.get(guid))
        .map(|entry| &entry.item)
        .filter(|item| item.fixed_lane.unwrap_or(0) == lane)
        .filter(|item| {
            let at = item.position.as_seconds();
            at < to && at + item.length.as_seconds() > from
        })
        .map(|item| {
            let take = p
                .takes
                .get(&item.guid)
                .and_then(|list| list.takes.get(usize::try_from(list.active_idx).ok()?));
            let mut item = item.clone();
            // The engine keeps where a take starts in its source on the
            // take; a slice is read with it, as `get_item` gives it.
            if let Some(take) = take {
                item.start_offset = take.start_offset;
            }
            Slice {
                file: take.and_then(|t| t.source_file_path.clone()),
                markers: take
                    .and_then(|t| p.stretch_markers.get(&t.guid))
                    .cloned()
                    .unwrap_or_default(),
                item,
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
    // Where the COMP lane plays that moment of the recording — the nearest
    // of them: a comp can play one recording in more than one place (a
    // phrase used twice), and the far one is a shift of a bar.
    let recorded = comp
        .iter()
        .filter_map(|c| {
            let from = c.item.start_offset.as_seconds();
            let inside = source >= from - 1e-9 && source < from + c.item.length.as_seconds();
            (c.file == slice.file && inside).then(|| c.start() + (source - from))
        })
        .min_by(|a, b| (t - a).abs().total_cmp(&(t - b).abs()))?;
    Some(t - recorded)
}

/// What makes one mic's EDIT lane over `start..end` the `pieces` of its
/// COMP lane: what starts there cleared (one slice over the whole span cut
/// either side of it), each piece copied from the COMP items under it,
/// shifted, crossfaded into the one before; the span's ends keep the fades
/// they had, so its seams with what is outside stay as they were.
fn render_ops(
    lanes: &Lanes,
    pieces: &[Piece],
    (start, end): (f64, f64),
    joins: Joins,
) -> Vec<ItemOp> {
    const EDGE: f64 = 1e-6;
    let edge_fade_in = lanes
        .edit
        .iter()
        .find(|s| (s.start() - start).abs() < EDGE)
        .map_or(0.0, |s| s.item.fade_in_length.as_seconds());
    let edge_fade_out = lanes
        .edit
        .iter()
        .find(|s| (s.end() - end).abs() < EDGE)
        .map_or(0.0, |s| s.item.fade_out_length.as_seconds());
    let mut ops = Vec::new();
    // A slice from before that only crossfades into the span, or one after
    // that crossfades in from its end, is left alone.
    for slice in lanes.edit.iter().filter(|s| {
        let starts_in = s.start() >= start - EDGE && s.start() < end - joins.crossfade - EDGE;
        let spans = s.start() < start - EDGE && s.end() > start + joins.crossfade + EDGE;
        starts_in || spans
    }) {
        clear_ops(&slice.item, (start, end), &mut ops);
    }
    for (k, piece) in pieces.iter().enumerate() {
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
        let (from, to) = (
            piece.start - reach_back - piece.shift,
            piece.end - piece.shift,
        );
        let under: Vec<&Slice> = lanes
            .comp
            .iter()
            .filter(|c| c.end() > from && c.start() < to)
            .collect();
        for (n, source) in under.iter().enumerate() {
            let (a, b) = (source.start().max(from), source.end().min(to));
            let copy = ItemId::new();
            let offset = source.item.start_offset.as_seconds() + (a - source.start());
            // The piece's own ends crossfade; a join between two COMP items
            // inside it keeps the COMP lane's own fades.
            let (first, last) = (n == 0, n + 1 == under.len());
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
            let mut length = b - a;
            let mut markers = Vec::new();
            // The last resort: the decay drawn out to meet the next piece.
            if last && let Some(reach) = piece.drawn_to {
                let mut shape = source.item.clone();
                shape.position = daw_proto::PositionInSeconds::from_seconds(a + piece.shift);
                shape.length = daw_proto::Duration::from_seconds(length);
                shape.start_offset = daw_proto::Duration::from_seconds(offset);
                if let Some(drawn) = crate::arrange_edit::draw_out_markers(&mut shape, reach) {
                    length = shape.length.as_seconds();
                    markers = drawn
                        .into_iter()
                        .map(|(t, s)| StretchMarker::new(t, s))
                        .collect();
                }
            }
            ops.push(ItemOp::Copy {
                item: ItemId::from(source.item.guid.as_str()),
                into: copy.clone(),
            });
            ops.push(ItemOp::Lane {
                item: copy.clone(),
                lane: lanes.edit_lane,
            });
            ops.push(ItemOp::Place {
                item: copy.clone(),
                at: a + piece.shift,
                length,
                offset,
            });
            ops.push(ItemOp::Fades {
                item: copy.clone(),
                fade_in: fin,
                fade_out: fout,
            });
            if !markers.is_empty() {
                ops.push(ItemOp::Stretch {
                    item: copy,
                    markers,
                });
            }
        }
    }
    ops
}

/// What takes `start..end` out of `item`: gone if it lies inside, else
/// trimmed to what is either side — a copy keeping the part after.
fn clear_ops(item: &daw_proto::Item, (start, end): (f64, f64), ops: &mut Vec<ItemOp>) {
    const GRAIN: f64 = 1e-6;
    let at = item.position.as_seconds();
    let till = at + item.length.as_seconds();
    let offset = item.start_offset.as_seconds();
    let id = ItemId::from(item.guid.as_str());
    let (head, tail) = (at < start - GRAIN, till > end + GRAIN);
    if tail {
        let rest = if head {
            let copy = ItemId::new();
            ops.push(ItemOp::Copy {
                item: id.clone(),
                into: copy.clone(),
            });
            copy
        } else {
            id.clone()
        };
        ops.push(ItemOp::Place {
            item: rest.clone(),
            at: end,
            length: till - end,
            offset: offset + (end - at),
        });
        ops.push(ItemOp::Fades {
            item: rest,
            fade_in: 0.0,
            fade_out: item.fade_out_length.as_seconds(),
        });
    }
    if head {
        ops.push(ItemOp::Place {
            item: id.clone(),
            at,
            length: start - at,
            offset,
        });
        ops.push(ItemOp::Fades {
            item: id,
            fade_in: item.fade_in_length.as_seconds(),
            fade_out: 0.0,
        });
    } else if !tail {
        ops.push(ItemOp::Delete { item: id });
    }
}
