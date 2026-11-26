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
pub(crate) struct Mic {
    pub(crate) track: String,
    pub(crate) comp: u32,
    pub(crate) edit: u32,
}

/// A mic's two lanes around a placement, as they are: what it reads its
/// shifts from and places again.
struct Lanes {
    edit_lane: u32,
    edit: Vec<Slice>,
    comp: Vec<Slice>,
}

/// An item on a lane and the recording it plays.
pub struct Slice {
    pub item: daw_proto::Item,
    pub file: Option<String>,
    pub markers: Vec<StretchMarker>,
}

impl Slice {
    pub fn start(&self) -> f64 {
        self.item.position.as_seconds()
    }

    pub fn end(&self) -> f64 {
        self.start() + self.item.length.as_seconds()
    }

    /// Where in its recording it plays at `t` (timeline seconds).
    pub fn source_at(&self, t: f64) -> f64 {
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
#[tracing::instrument(
    name = "hits.place",
    skip_all,
    fields(
        hits.moves = moves.len(),
        edit.moved = tracing::field::Empty,
        edit.hits = tracing::field::Empty,
        edit.tracks = tracing::field::Empty,
        edit.slices = tracing::field::Empty,
        edit.crossfades = tracing::field::Empty,
        edit.items_written = tracing::field::Empty,
    )
)]
pub fn place(
    song: &crate::song::Song,
    moves: &[(f64, f64)],
    joins: Joins,
) -> Result<usize, String> {
    let ops = plan(song, moves, joins, None)?.ops;
    if ops.is_empty() {
        return Ok(0);
    }
    let step =
        tracing::info_span!("hits.place.apply", edit.lock_ms = tracing::field::Empty).entered();
    let lock_at = crate::edit_trace::lock_us();
    song.undo_step("Place a drum hit", |song| song.apply(&ops))
        .map_err(|e| e.to_string())?;
    step.record(
        "edit.lock_ms",
        num_traits::ToPrimitive::to_f64(&crate::edit_trace::lock_us().saturating_sub(lock_at))
            .unwrap_or(0.0)
            / 1e3,
    );
    drop(step);
    Ok(ops.len())
}

/// A placement planned: its item edits, and what the algorithm did at
/// each gap (`daw.edit` 0.3's fills: played on, reached back,
/// crossfaded, the last span stretched and its rate, what is left).
#[derive(Debug, Default)]
pub struct Plan {
    pub ops: Vec<ItemOp>,
    pub fills: Vec<placement::Filled>,
}

/// The item edits [`place`] makes, not yet applied: for a caller that
/// writes them with edits of its own as one undo step (`DRUMS_APPLY`).
/// `cut_on`, when given, are the hits (where they play now) a piece
/// starts at — the kit's markers otherwise: a hit left out rides in the
/// piece before it. Every hit lands exactly where it is sent: a gap a move
/// opens is the algorithm's to fill (`daw.edit` 0.3), from the takes the
/// COMP lane is made of, never by moving a hit.
///
/// # Errors
/// As [`place`].
pub fn plan(
    song: &crate::song::Song,
    moves: &[(f64, f64)],
    joins: Joins,
    cut_on: Option<&[f64]>,
) -> Result<Plan, String> {
    // Each step its own span under `hits.place`: the trace says where the
    // time went.
    let step = tracing::info_span!("hits.place.mics").entered();
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
        return Ok(Plan::default());
    }
    let call = tracing::Span::current();
    call.record("edit.moved", moved.len());
    let mics = mics(song);
    call.record("edit.tracks", mics.len());
    drop(step);
    let step = tracing::info_span!("hits.place.hits").entered();
    if mics.is_empty() {
        return Err("no mic of the kit has COMP and EDIT lanes".to_owned());
    }
    let mut hits: Vec<f64> = cut_on.map_or_else(
        || {
            crate::hit_index::playback(song)
                .iter()
                .map(|p| p.time)
                .collect()
        },
        <[f64]>::to_vec,
    );
    hits.sort_by(f64::total_cmp);
    hits.dedup_by(|a, b| (*a - *b).abs() < SAME_HIT);
    drop(step);
    let step = tracing::info_span!("hits.place.read").entered();

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
        return Ok(Plan::default());
    };

    // Every mic's lanes around it, read at once: a slice reached from as
    // far as a move goes, either side.
    let reach = moved.iter().map(|m| m.1.abs()).fold(0.0, f64::max) + READ_MARGIN;
    let kit = read_kit(song, &mics, (near - reach, far + reach));
    if kit.is_empty() {
        return Err("the kit could not be read".to_owned());
    }
    drop(step);
    let step = tracing::info_span!("hits.place.plan").entered();

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
    // The chosen `daw.edit` algorithm places them (an extension: rhythm's,
    // or another's); the engine has no placement of its own.
    call.record("edit.hits", placed.len());
    let provider = tracing::info_span!(
        "hits.place.provider",
        edit.slices = tracing::field::Empty,
        edit.takes = tracing::field::Empty,
        fill.gaps = tracing::field::Empty,
        fill.reached_back = tracing::field::Empty,
        fill.replayed_decay = tracing::field::Empty,
        fill.crossfaded = tracing::field::Empty,
        fill.stretched = tracing::field::Empty,
        fill.slowest = tracing::field::Empty,
        fill.slowest_leftover = tracing::field::Empty,
        fill.forced = tracing::field::Empty,
        fill.left = tracing::field::Empty,
        fill.left_ms = tracing::field::Empty,
    )
    .entered();
    // The takes the COMP lane is made of, on the reference mic: a fill
    // reaches no further than its own take.
    let takes = kit
        .first()
        .map_or_else(Vec::new, |lanes| takes_of(&lanes.comp));
    provider.record("edit.takes", takes.len());
    let answer = crate::mix_calls::place(&daw_edit::PlaceRequest {
        hits: placed.clone(),
        span,
        cut,
        bounds: outside,
        takes,
        fill: placement::Fill {
            crossfade: joins.crossfade,
            ..placement::Fill::default()
        },
    })?;
    let pieces = answer.pieces;
    let fills = answer.fills;
    provider.record("edit.slices", pieces.len());
    record_fills(&provider, &fills);
    drop(provider);
    let render = tracing::info_span!("hits.place.render").entered();
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
    drop(render);
    drop(step);
    // Every seam between two pieces is crossfaded, on every mic.
    call.record("edit.slices", pieces.len().saturating_mul(kit.len()));
    call.record(
        "edit.crossfades",
        pieces.len().saturating_sub(1).saturating_mul(kit.len()),
    );
    call.record("edit.items_written", ops.len());
    Ok(Plan { ops, fills })
}

/// The fills' counts on `span`: how many gaps, how each was filled, the
/// slowest stretch (and the slowest below 0.8, what the replay left) and
/// any silence left (none since `daw.edit` 0.3.2), and how many were forced.
fn record_fills(span: &tracing::Span, fills: &[placement::Filled]) {
    let count = |f: &dyn Fn(&placement::Filled) -> bool| fills.iter().filter(|x| f(x)).count();
    span.record("fill.gaps", fills.len());
    span.record("fill.reached_back", count(&|f| f.reached_back > 0.0));
    span.record("fill.replayed_decay", count(&|f| f.replayed_decay > 0.0));
    span.record("fill.forced", count(&|f| f.forced_rate));
    span.record("fill.crossfaded", count(&|f| f.crossfade > 0.0));
    span.record("fill.stretched", count(&|f| f.stretched.is_some()));
    span.record("fill.left", count(&|f| f.left > 0.0));
    span.record(
        "fill.left_ms",
        fills.iter().map(|f| f.left).sum::<f64>() * 1_000.0,
    );
    if let Some(slowest) = fills
        .iter()
        .filter_map(|f| f.stretched.map(|s| s.rate))
        .reduce(f64::min)
    {
        span.record("fill.slowest", slowest);
    }
    if let Some(slowest) = fills
        .iter()
        .filter_map(|f| f.stretched.map(|s| s.rate))
        .filter(|&rate| rate < crate::drum_apply::MIN_RATE - 1e-9)
        .reduce(f64::min)
    {
        span.record("fill.slowest_leftover", slowest);
    }
}

/// The takes a COMP lane is made of, recording (COMP) time, in order: its
/// items, those of one file played on at one alignment joined into one.
fn takes_of(comp: &[Slice]) -> Vec<placement::Take> {
    let mut out: Vec<(placement::Take, Option<&str>, f64)> = Vec::new();
    for c in comp {
        // Where its file's time 0 is on the timeline: one alignment.
        let origin = c.start() - c.item.start_offset.as_seconds();
        let file = c.file.as_deref();
        match out.last_mut() {
            Some((take, f, o))
                if *f == file
                    && file.is_some()
                    && (*o - origin).abs() < 1e-6
                    && c.start() <= take.to + 1e-3 =>
            {
                take.to = take.to.max(c.end());
            }
            _ => out.push((
                placement::Take {
                    from: c.start(),
                    to: c.end(),
                },
                file,
                origin,
            )),
        }
    }
    out.into_iter().map(|(take, _, _)| take).collect()
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
pub(crate) fn mics(song: &crate::song::Song) -> Vec<Mic> {
    song.daw()
        .all(song.context())
        .into_iter()
        // The hits' MIDI views are written from the hits, not cut with the mics.
        .filter(|t| !crate::hits_midi::is_view(t))
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
pub fn lane_of(
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
pub(crate) fn shift_at(edit: &[Slice], comp: &[Slice], t: f64) -> Option<f64> {
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
    // A seam the algorithm crossfades (`daw.edit` 0.3) is that long, the
    // piece after starting where it says; one it leaves to the host (0.2)
    // reaches back by the crossfade. A seam with a gap left (no overlap)
    // fades each side within itself.
    let seam_fade = |next: &Piece| match next.crossfade {
        Some(x) if x > 1e-4 => x,
        _ => joins.crossfade,
    };
    for (k, piece) in pieces.iter().enumerate() {
        let fade_in = if k == 0 {
            edge_fade_in
        } else {
            seam_fade(piece)
        };
        let fade_out = match pieces.get(k + 1) {
            None => edge_fade_out,
            Some(next) => seam_fade(next),
        };
        let reach_back = if k == 0 || piece.crossfade.is_some() {
            0.0
        } else {
            fade_in
        };
        let (from, to) = (
            piece.start - reach_back - piece.shift,
            piece.end - piece.shift,
        );
        let warp = Warp::of(piece);
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
            let mut at = a + piece.shift;
            // The last resort: only the decay's last span drawn out to meet
            // the next piece, as the algorithm says — on every COMP item
            // the span covers (a take switch inside it: each item placed
            // and stretched by the one warp, never the last alone).
            if let Some(warp) = warp {
                let end = b + piece.shift;
                if end > warp.from + 1e-9 {
                    let split = (warp.from - at).max(0.0);
                    let (starts, ends) = (warp.at(at), warp.at(end));
                    markers = vec![StretchMarker::new(0.0, offset)];
                    if split > 1e-6 {
                        markers.push(StretchMarker::new(split, offset + split));
                    }
                    markers.push(StretchMarker::new(ends - starts, offset + length));
                    at = starts;
                    length = ends - starts;
                }
            } else if last && let Some(reach) = piece.drawn_to {
                let mut shape = source.item.clone();
                shape.position = daw_proto::PositionInSeconds::from_seconds(a + piece.shift);
                shape.length = daw_proto::Duration::from_seconds(length);
                shape.start_offset = daw_proto::Duration::from_seconds(offset);
                if let Some(drawn) = crate::stretch::draw_out_markers(&mut shape, reach) {
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
                at,
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

/// A piece's stretch (`daw.edit` 0.3): as recorded up to `from`
/// (timeline), then `from..end` drawn over `from..drawn_to` at `rate`.
#[derive(Clone, Copy, Debug)]
struct Warp {
    from: f64,
    rate: f64,
}

impl Warp {
    /// The stretch `piece` asks for, if it is drawn past its end from
    /// inside it.
    fn of(piece: &Piece) -> Option<Self> {
        let (reach, from) = (piece.drawn_to?, piece.drawn_from?);
        (reach > piece.end + 1e-9 && piece.end > from + 1e-9).then(|| Self {
            from,
            rate: (piece.end - from) / (reach - from),
        })
    }

    /// Where the moment the recording has at `t` (timeline, unstretched)
    /// plays once drawn.
    fn at(self, t: f64) -> f64 {
        if t <= self.from {
            t
        } else {
            self.from + (t - self.from) / self.rate
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn slice(guid: &str, file: &str, (at, length, offset): (f64, f64, f64)) -> Slice {
        Slice {
            item: daw_proto::Item {
                guid: guid.to_owned(),
                position: daw_proto::PositionInSeconds::from_seconds(at),
                length: daw_proto::Duration::from_seconds(length),
                start_offset: daw_proto::Duration::from_seconds(offset),
                ..daw_proto::Item::default()
            },
            file: Some(file.to_owned()),
            markers: Vec::new(),
        }
    }

    #[test]
    fn a_comps_takes_are_its_items_one_file_at_one_alignment_joined() {
        let comp = [
            slice("a", "t1.wav", (0.0, 2.0, 10.0)),
            // The same take split, played on at the same alignment.
            slice("b", "t1.wav", (2.0, 1.0, 12.0)),
            // Another take.
            slice("c", "t2.wav", (2.99, 2.0, 3.0)),
            // The first take again, elsewhere in it.
            slice("d", "t1.wav", (4.99, 1.0, 40.0)),
        ];
        let takes = takes_of(&comp);
        let spans: Vec<(f64, f64)> = takes.iter().map(|t| (t.from, t.to)).collect();
        assert_eq!(spans, vec![(0.0, 3.0), (2.99, 4.99), (4.99, 5.99)]);
    }

    #[test]
    fn a_fill_is_rendered_as_the_algorithm_says_on_every_mic() {
        // The kick's piece stretched over its last 216 ms only; the
        // snare's crossfading over it for 4 ms, starting where it says.
        let lanes = Lanes {
            edit_lane: 1,
            edit: Vec::new(),
            comp: vec![slice("comp", "t1.wav", (0.0, 3.0, 0.0))],
        };
        let pieces = [
            Piece {
                start: 0.995,
                end: 1.295,
                shift: 0.0,
                drawn_to: Some(1.349),
                drawn_from: Some(1.079),
                crossfade: None,
            },
            Piece {
                start: 1.345,
                end: 2.0,
                shift: 0.33,
                drawn_to: None,
                drawn_from: None,
                crossfade: Some(0.004),
            },
        ];
        let joins = Joins {
            lead: 0.005,
            crossfade: 0.004,
        };
        let ops = render_ops(&lanes, &pieces, (0.995, 2.0), joins);
        let places: Vec<(f64, f64, f64)> = ops
            .iter()
            .filter_map(|op| match op {
                ItemOp::Place {
                    at, length, offset, ..
                } => Some((*at, *length, *offset)),
                _ => None,
            })
            .collect();
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        // The kick: placed from its start, as long as it now sounds.
        assert!(near(places[0].0, 0.995) && near(places[0].1, 1.349 - 0.995));
        // The snare: from where it says, not a crossfade earlier.
        assert!(near(places[1].0, 1.345) && near(places[1].2, 1.345 - 0.33));
        let markers: Vec<&Vec<StretchMarker>> = ops
            .iter()
            .filter_map(|op| match op {
                ItemOp::Stretch { markers, .. } => Some(markers),
                _ => None,
            })
            .collect();
        let [kick] = markers.as_slice() else {
            panic!("{markers:?}")
        };
        // As recorded up to the stretch, then its last span drawn at 0.8.
        assert_eq!(kick.len(), 3);
        assert!(near(kick[1].position, 1.079 - 0.995));
        assert!(near(kick[1].source_position, 0.995 + (1.079 - 0.995)));
        let rate = (kick[2].source_position - kick[1].source_position)
            / (kick[2].position - kick[1].position);
        assert!(near(rate, (1.295 - 1.079) / (1.349 - 1.079)));
        let fades: Vec<(f64, f64)> = ops
            .iter()
            .filter_map(|op| match op {
                ItemOp::Fades {
                    fade_in, fade_out, ..
                } => Some((*fade_in, *fade_out)),
                _ => None,
            })
            .collect();
        assert!(near(fades[0].1, 0.004) && near(fades[1].0, 0.004));
    }

    #[test]
    fn a_stretch_across_a_take_switch_is_drawn_at_its_one_rate() {
        // Unbreakable, 253.59 (DRUMS_APPLY with daw.edit 0.3.2): the kick
        // before the gap is from take 420134, the snare after from 078d8b;
        // the comp switches at 253.500. The piece before is drawn from
        // 253.510 (timeline) at 0.615 to 253.5619 — across the switch, at
        // 253.5415 as it plays. Only the last COMP item, 0.4 ms of it, was
        // stretched: drawn over 20.4 ms, it played at 0.019. Each item
        // under the drawn span now plays at the piece's rate, one after
        // the other, the last reaching where the piece is drawn to.
        let lanes = Lanes {
            edit_lane: 1,
            edit: Vec::new(),
            comp: vec![
                slice("t420134", "420134.wav", (250.0, 3.5, 10.0)),
                slice("t078d8b", "078d8b.wav", (253.5, 6.0, 20.0)),
            ],
        };
        let drawn = Piece {
            start: 253.4763,
            end: 253.5419,
            shift: 0.0415,
            drawn_to: Some(253.5619),
            drawn_from: Some(253.51),
            crossfade: Some(0.01),
        };
        let pieces = [
            drawn,
            Piece {
                start: 253.5569,
                end: 253.6825,
                shift: 0.0884,
                drawn_to: None,
                drawn_from: None,
                crossfade: Some(0.005),
            },
        ];
        let joins = Joins {
            lead: 0.005,
            crossfade: 0.005,
        };
        let ops = render_ops(&lanes, &pieces, (253.4763, 253.6825), joins);
        let planned = (drawn.end - 253.51) / (253.5619 - 253.51);
        let rendered = rendered(&ops);
        // The drawn piece is two items, one each side of the switch (and
        // the snare's, reaching back through the comp's switch, two).
        assert_eq!(rendered.len(), 4, "{rendered:?}");
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        let (first, second) = (&rendered[0], &rendered[1]);
        // As recorded up to the stretch, then at the piece's rate, on both.
        assert!(near(first.slowest, planned), "{first:?}");
        assert!(near(second.slowest, planned), "{second:?}");
        assert!(second.fastest <= planned + 1e-9, "{second:?}");
        // One after the other, the second ending at the piece's reach.
        assert!(near(first.at + first.length, second.at), "{rendered:?}");
        assert!(near(second.at + second.length, 253.5619), "{rendered:?}");
        // From the take it was recorded in: the first from 420134 up to the
        // switch, the second from 078d8b.
        assert!(near(first.offset + first.natural, 10.0 + 3.5), "{first:?}");
        assert!(near(second.offset, 20.0), "{second:?}");
    }

    /// One item a render places: where, how long, from where in its file,
    /// how much of the recording it plays, and the slowest and fastest of
    /// its stretch (1 unstretched).
    #[derive(Debug)]
    struct Rendered {
        at: f64,
        length: f64,
        offset: f64,
        natural: f64,
        slowest: f64,
        fastest: f64,
    }

    fn rendered(ops: &[ItemOp]) -> Vec<Rendered> {
        let mut out: Vec<(ItemId, Rendered)> = Vec::new();
        for op in ops {
            match op {
                ItemOp::Place {
                    item,
                    at,
                    length,
                    offset,
                } => out.push((
                    item.clone(),
                    Rendered {
                        at: *at,
                        length: *length,
                        offset: *offset,
                        natural: *length,
                        slowest: 1.0,
                        fastest: 1.0,
                    },
                )),
                ItemOp::Stretch { item, markers } => {
                    if let Some((_, r)) = out.iter_mut().find(|(id, _)| id == item) {
                        let rates: Vec<f64> = markers
                            .windows(2)
                            .map(|w| {
                                (w[1].source_position - w[0].source_position)
                                    / (w[1].position - w[0].position)
                            })
                            .collect();
                        r.slowest = rates.iter().copied().fold(1.0, f64::min);
                        r.fastest = rates.iter().copied().fold(0.0, f64::max);
                        r.natural = markers.last().map_or(r.length, |m| m.source_position)
                            - markers.first().map_or(0.0, |m| m.source_position);
                    }
                }
                _ => {}
            }
        }
        out.into_iter().map(|(_, r)| r).collect()
    }

    #[test]
    fn every_piece_renders_at_the_rate_its_fill_planned_on_every_mic() {
        // Pieces drawn inside one COMP item, across a take switch, and
        // across two: each item's rendered rate is its piece's planned one,
        // on every mic (each mic its own COMP lane, the same takes).
        let joins = Joins {
            lead: 0.005,
            crossfade: 0.005,
        };
        for switches in [vec![], vec![1.03], vec![1.02, 1.04]] {
            for mic in 0..3_u8 {
                let mut cuts = vec![0.0];
                cuts.extend(&switches);
                cuts.push(3.0);
                let comp: Vec<Slice> = cuts
                    .windows(2)
                    .enumerate()
                    .map(|(k, w)| {
                        let file = format!("mic{mic}-take{k}.wav");
                        slice(&format!("{mic}-{k}"), &file, (w[0], w[1] - w[0], w[0]))
                    })
                    .collect();
                let lanes = Lanes {
                    edit_lane: 1,
                    edit: Vec::new(),
                    comp,
                };
                let drawn = Piece {
                    start: 0.995,
                    end: 1.055,
                    shift: 0.0,
                    drawn_to: Some(1.105),
                    drawn_from: Some(1.01),
                    crossfade: None,
                };
                let next = Piece {
                    start: 1.1,
                    end: 2.0,
                    shift: 0.05,
                    drawn_to: None,
                    drawn_from: None,
                    crossfade: Some(0.005),
                };
                let ops = render_ops(&lanes, &[drawn, next], (0.995, 2.0), joins);
                let planned = (1.055 - 1.01) / (1.105 - 1.01);
                let items = rendered(&ops);
                let drawn_items = &items[..=switches.len()];
                for r in drawn_items {
                    if r.at + r.length > 1.01 + 1e-9 {
                        assert!((r.slowest - planned).abs() < 1e-9, "{switches:?}: {r:?}");
                    }
                }
                let last = drawn_items.last().expect("an item");
                assert!((last.at + last.length - 1.105).abs() < 1e-9, "{items:?}");
                for w in drawn_items.windows(2) {
                    assert!((w[0].at + w[0].length - w[1].at).abs() < 1e-9, "{items:?}");
                }
            }
        }
    }
}
