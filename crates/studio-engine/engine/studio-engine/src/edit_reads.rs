//! The reads an agent edits drums from: compact, complete, and under the
//! MCP's answer cap (about 100 KB).
//!
//! - `HITS_LIST`: `playing` (only what plays now), `bars`, each hit's
//!   `take` (the comp's pass name, as `COMP_CANDIDATES` says it), and
//!   `compact` — one header and a row per hit.
//! - `DRUMS_BARS`: per bar, the playing hits, the drum MIDI as pieces, the
//!   zones, the takes that play it and its section — what five reads said.
//! - `SLICES_LIST`: the EDIT lane's slices, each with its take, offset,
//!   speed (a slice drawn out or squeezed is flagged) and the hits on it.
//! - `QUANTIZE`'s detail ([`SpanDetail`]): every move, the hits held and
//!   why, and the zones that held them.
//! - `ZONES_LIST`: the no-quantize zones apart from the fills.

use architect::action::{ActionCaller, ActionList, ActionMeta, Effect, RawJson};
use daw::service::ProjectContext;
use facet::Facet;
use serde_json::{Value, json};

use crate::hits::HitRow;

// ── HITS_LIST ─────────────────────────────────────────────────────────

/// Which hits, and how to say them.
#[derive(Facet, Default)]
pub struct ListArg {
    #[facet(default)]
    pub pieces: Vec<String>,
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub flag: Option<String>,
    pub origin: Option<String>,
    /// Only hits heard so: `both`, `trigger` or `mic`.
    pub heard: Option<String>,
    /// Only the hits that play now (a slice plays them).
    pub playing: Option<bool>,
    /// Only those in these bars, where they play now: `[first, last]`
    /// (one bar: `[n]`).
    #[facet(default)]
    pub bars: Vec<i32>,
    /// Only those of this take (its pass, as COMP_CANDIDATES says it).
    pub take: Option<String>,
    /// One header and a row per hit, not an object each.
    pub compact: Option<bool>,
    pub offset: Option<u32>,
    pub limit: Option<u32>,
}

/// A page of hits, as objects.
#[derive(Facet)]
struct ListPage {
    total: u32,
    offset: u32,
    hits: Vec<HitRow>,
}

/// The columns of a compact hit list.
pub const HIT_COLS: [&str; 10] = [
    "id", "piece", "take", "time", "now", "bar", "beat", "peak_db", "heard", "flags",
];

/// The most hits a page holds: objects, rows.
const MOST_OBJECTS: u32 = 500;
const MOST_ROWS: u32 = 2_000;

/// `x` to `places` decimals, for an answer that stays small.
fn round(x: f64, places: i32) -> f64 {
    let k = 10_f64.powi(places);
    (x * k).round() / k
}

/// Whether `h` is one `a` asks for.
#[must_use]
pub fn wanted(a: &ListArg, h: &HitRow) -> bool {
    let (from, to) = (
        a.from.unwrap_or(f64::NEG_INFINITY),
        a.to.unwrap_or(f64::INFINITY),
    );
    let bars = match a.bars.as_slice() {
        [] => None,
        [one] => Some((*one, *one)),
        [first, last, ..] => Some((*first.min(last), *first.max(last))),
    };
    h.at() >= from
        && h.at() <= to
        && (!a.playing.unwrap_or(false) || h.now.is_some())
        && bars.is_none_or(|(first, last)| h.bar >= first && h.bar <= last)
        && a.take
            .as_ref()
            .is_none_or(|t| h.take.eq_ignore_ascii_case(t.trim()))
        && a.flag
            .as_ref()
            .is_none_or(|f| h.flags.iter().any(|x| x.eq_ignore_ascii_case(f)))
        && a.origin
            .as_ref()
            .is_none_or(|o| h.origin.eq_ignore_ascii_case(o))
        && a.heard
            .as_ref()
            .is_none_or(|w| h.heard.eq_ignore_ascii_case(w))
}

/// One hit as a compact row, in [`HIT_COLS`]' order.
#[must_use]
pub fn hit_row(h: &HitRow) -> Value {
    json!([
        h.id,
        h.piece,
        h.take,
        round(h.time, 4),
        h.now.map(|n| round(n, 4)),
        h.bar,
        round(h.beat, 3),
        round(h.peak_db, 1),
        h.heard,
        h.flags.join(","),
    ])
}

/// The page `a` asks for of `all` (every hit, in time order): objects, or
/// with `compact` a header and rows. `next` is the offset of the page
/// after, when there is one.
///
/// # Errors
/// The page would not serialise.
pub fn list_page(a: &ListArg, all: Vec<HitRow>) -> Result<String, String> {
    let picked: Vec<HitRow> = all.into_iter().filter(|h| wanted(a, h)).collect();
    let total = u32::try_from(picked.len()).unwrap_or(u32::MAX);
    let offset = a.offset.unwrap_or(0);
    let compact = a.compact.unwrap_or(false);
    let limit = if compact {
        a.limit.unwrap_or(1_000).min(MOST_ROWS)
    } else {
        a.limit.unwrap_or(100).min(MOST_OBJECTS)
    };
    let page = picked
        .into_iter()
        .skip(usize::try_from(offset).unwrap_or(usize::MAX))
        .take(usize::try_from(limit).unwrap_or(usize::MAX));
    let next = offset
        .saturating_add(limit)
        .lt(&total)
        .then(|| offset.saturating_add(limit));
    crate::edit_trace::count_here("edit.hits", usize::try_from(total).unwrap_or(usize::MAX));
    if compact {
        let rows: Vec<Value> = page.map(|h| hit_row(&h)).collect();
        return Ok(json!({
            "total": total,
            "offset": offset,
            "next": next,
            "cols": HIT_COLS,
            "rows": rows,
        })
        .to_string());
    }
    facet_json::to_string(&ListPage {
        total,
        offset,
        hits: page.collect(),
    })
    .map_err(|e| e.to_string())
}

/// `HITS_LIST`: see its description.
///
/// # Errors
/// No song open.
pub fn hits_list(a: &ListArg) -> Result<RawJson, String> {
    let traced = crate::edit_trace::Traced::enter(crate::edit_trace::edit_span!("HITS_LIST"));
    let (daw, project) = crate::actions::song()?;
    let step = tracing::info_span!("hits.read").entered();
    let all = crate::hits::hits(daw, &project, &a.pieces);
    drop(step);
    traced.count("edit.hits", all.len());
    let step = tracing::info_span!("hits.page").entered();
    let out = list_page(a, all);
    drop(step);
    traced.outcome(&out);
    out.map(RawJson::from)
}

// ── DRUMS_BARS ────────────────────────────────────────────────────────

static BARS: ActionMeta = crate::actions::meta(
    "DRUMS_BARS",
    "drums_bars",
    "The drums bar by bar",
    "Bar by bar from `from_bar` to `to_bar` (as the song counts them; at most 64 bars a call): its start and end (seconds), its section (the SECTIONS lane), the takes that play it (their passes, as COMP_CANDIDATES and COMP_CHOOSE name them), the zones over it (fills and no-quantize exceptions: kind, origin, start, end, note), the hits that play in it now (`hit_cols`: id, piece, beat where it plays, now, recorded, take, flags) and the drum MIDI there as pieces (`midi_cols`: piece — or `#pitch` when the map has none — beat, time, velocity; the map as DRUMS_REFERENCE_MAP has it, a hand-set one first). One call for what HITS_LIST, DRUMS_REFERENCE, ZONES_LIST, COMP_CANDIDATES and SESSION_SECTIONS said apart. Read-only.",
    "Drums",
    "",
);

/// Which bars.
#[derive(Facet)]
struct BarsArg {
    from_bar: i32,
    to_bar: Option<i32>,
}

/// The most bars one call reads: a section or two.
const MOST_BARS: i32 = 64;

/// The columns of a bar's hits and notes.
const BAR_HIT_COLS: [&str; 7] = ["id", "piece", "beat", "now", "recorded", "take", "flags"];
const BAR_MIDI_COLS: [&str; 4] = ["piece", "beat", "time", "velocity"];

/// One bar's span.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarSpan {
    pub bar: i32,
    pub start: f64,
    pub end: f64,
}

/// The bar holding each of `times` (by time, the bars in order); `None`
/// outside them all.
#[must_use]
pub fn bar_of(bars: &[BarSpan], t: f64) -> Option<usize> {
    let i = bars.partition_point(|b| b.end <= t);
    bars.get(i).filter(|b| t >= b.start).map(|_| i)
}

/// `DRUMS_BARS`: see its description.
fn drums_bars(a: &BarsArg) -> Result<RawJson, String> {
    let traced = crate::edit_trace::Traced::enter(crate::edit_trace::edit_span!("DRUMS_BARS"));
    let out = drums_bars_traced(a, &traced);
    traced.outcome(&out);
    out
}

fn drums_bars_traced(a: &BarsArg, traced: &crate::edit_trace::Traced) -> Result<RawJson, String> {
    let (daw, project) = crate::actions::song()?;
    let last = a.to_bar.unwrap_or(a.from_bar);
    let (first, last) = (a.from_bar.min(last), a.from_bar.max(last));
    if last - first >= MOST_BARS {
        return Err(format!(
            "{} bars is more than {MOST_BARS} a call: read a section at a time",
            last - first + 1
        ));
    }
    let step = tracing::info_span!("bars.read").entered();
    let bars: Vec<BarSpan> = (first..=last)
        .map(|bar| BarSpan {
            bar,
            start: crate::song_bars::time(daw, &project, bar, 1, 0.0),
            end: crate::song_bars::time(daw, &project, bar.saturating_add(1), 1, 0.0),
        })
        .collect();
    let (Some(lo), Some(hi)) = (bars.first().map(|b| b.start), bars.last().map(|b| b.end)) else {
        return Err("no bars".to_owned());
    };
    let hits: Vec<HitRow> = crate::hits::hits(daw, &project, &[])
        .into_iter()
        .filter(|h| h.now.is_some_and(|n| n >= lo && n < hi))
        .collect();
    let zones = crate::hits::zones(daw, &project);
    let sections = sections(daw, &project);
    drop(step);
    let step = tracing::info_span!("bars.midi").entered();
    let midi = written(daw, &project, (lo, hi));
    drop(step);
    traced.count("edit.hits", hits.len());
    let step = tracing::info_span!("bars.table").entered();
    let mut rows: Vec<(Vec<Value>, Vec<Value>, std::collections::BTreeSet<String>)> =
        vec![(Vec::new(), Vec::new(), std::collections::BTreeSet::new()); bars.len()];
    for h in &hits {
        let Some(now) = h.now else { continue };
        let Some(i) = bar_of(&bars, now) else {
            continue;
        };
        let Some(row) = rows.get_mut(i) else { continue };
        row.0.push(json!([
            h.id,
            h.piece,
            round(h.beat, 3),
            round(now, 4),
            round(h.time, 4),
            h.take,
            h.flags.join(","),
        ]));
        row.2.insert(h.take.clone());
    }
    for (piece, time, velocity) in &midi {
        let Some(i) = bar_of(&bars, *time) else {
            continue;
        };
        let (_, beat, frac) = crate::song_bars::musical(daw, &project, *time);
        if let Some(row) = rows.get_mut(i) {
            row.1.push(json!([
                piece,
                round(f64::from(beat) + frac, 3),
                round(*time, 4),
                velocity
            ]));
        }
    }
    let out: Vec<Value> = bars
        .iter()
        .zip(rows)
        .map(|(b, (hit_rows, midi_rows, takes))| {
            let over: Vec<Value> = zones
                .iter()
                .filter(|z| z.start < b.end && z.end > b.start)
                .map(|z| {
                    json!({
                        "guid": z.guid,
                        "kind": format!("{:?}", z.kind),
                        "origin": format!("{:?}", z.origin),
                        "start": round(z.start, 4),
                        "end": round(z.end, 4),
                        "note": z.note,
                    })
                })
                .collect();
            let section = sections
                .iter()
                .find(|(_, from, to)| b.start >= from - 1e-6 && b.start < *to)
                .map(|(name, _, _)| name.clone());
            json!({
                "bar": b.bar,
                "start": round(b.start, 4),
                "end": round(b.end, 4),
                "section": section,
                "takes": takes.into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>(),
                "zones": over,
                "hits": hit_rows,
                "midi": midi_rows,
            })
        })
        .collect();
    drop(step);
    Ok(RawJson::from(
        json!({ "hit_cols": BAR_HIT_COLS, "midi_cols": BAR_MIDI_COLS, "bars": out }).to_string(),
    ))
}

/// The song's sections, by start: name, start, end.
fn sections(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
) -> Vec<(String, f64, f64)> {
    let mut out: Vec<(String, f64, f64)> = daw::service::Regions::all(daw, project.clone())
        .into_iter()
        .filter(|r| {
            r.lane
                .is_none_or(|l| l == engine_domain::ruler_lanes::CoreLane::Sections.lane_index())
        })
        .map(|r| {
            (
                r.name.clone(),
                r.time_range.start_seconds(),
                r.time_range.end_seconds(),
            )
        })
        .collect();
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    out
}

/// The drum MIDI over `lo..hi` as pieces: `(piece or #pitch, time,
/// velocity)`, through the map (a hand-set one first).
fn written(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (lo, hi): (f64, f64),
) -> Vec<(String, f64, u8)> {
    let Some(track) = crate::drum_reference::find_track(daw, project) else {
        return Vec::new();
    };
    let notes = crate::drum_reference::notes(daw, project, &track);
    let pieces: Vec<(String, Vec<f64>)> = crate::hits::hits(daw, project, &[])
        .into_iter()
        .fold(
            std::collections::BTreeMap::<String, Vec<f64>>::new(),
            |mut by, h| {
                by.entry(h.piece.clone()).or_default().push(h.time);
                by
            },
        )
        .into_iter()
        .map(|(name, mut times)| {
            times.sort_by(f64::total_cmp);
            (name, times)
        })
        .collect();
    let map = crate::drum_map_set::map_for(daw, project, &notes, &pieces, NOTE_WINDOW);
    notes
        .iter()
        .filter(|n| n.time >= lo && n.time < hi)
        .map(|n| {
            let piece = map
                .iter()
                .find(|m| m.pitch == n.pitch)
                .and_then(|m| m.piece.clone())
                .unwrap_or_else(|| format!("#{}", n.pitch));
            (piece, n.time, n.velocity)
        })
        .collect()
}

/// How near a note a hit is that note's, seconds — the learned map's.
pub const NOTE_WINDOW: f64 = 0.06;

// ── SLICES_LIST ───────────────────────────────────────────────────────

static SLICES: ActionMeta = crate::actions::meta(
    "SLICES_LIST",
    "slices_list",
    "The drum edit's slices",
    "The EDIT lane's slices on the kit over `from`/`to` (seconds; the whole song when left out): each its start and length (seconds), the take it plays (its pass, as COMP_CANDIDATES names it), its offset into the recording, its speed (1 as recorded; with stretch markers, the slowest and fastest of them — `rate_min`, `rate_max`), its fades and the hits it plays. A slice slower than 0.8× or faster than 1.25× is flagged (`slow`, `fast`): an edit that drew a slice out rather than moving what follows. `track` (a name or GUID) reads that mic; else the kit's first mic with an EDIT lane — the kit is cut as one, so every mic's slices are alike. `flagged` lists only the flagged. Compact: `cols` and a row per slice. Read-only.",
    "Drums",
    "",
);

/// Which slices.
#[derive(Facet, Default)]
struct SlicesArg {
    from: Option<f64>,
    to: Option<f64>,
    track: Option<String>,
    flagged: Option<bool>,
}

/// A slice slower than this, or faster than [`FASTEST`], is flagged.
pub const SLOWEST: f64 = 0.8;
pub const FASTEST: f64 = 1.25;

const SLICE_COLS: [&str; 11] = [
    "start", "length", "take", "offset", "rate", "rate_min", "rate_max", "fade_in", "fade_out",
    "hits", "flag",
];

/// One slice's speed: its take's rate, and through its stretch markers the
/// slowest and fastest of its stretches.
#[must_use]
pub fn speeds(play_rate: f64, markers: &[daw_proto::StretchMarker]) -> (f64, f64, f64) {
    let rate = if play_rate > 0.0 { play_rate } else { 1.0 };
    let (mut lo, mut hi) = (rate, rate);
    for w in markers.windows(2) {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else {
            continue;
        };
        let dt = b.position - a.position;
        if dt <= 1e-9 {
            continue;
        }
        // Positions are in the slice's playback time: its speed there is
        // how much recording plays per second of it.
        let r = (b.source_position - a.source_position) / dt;
        lo = lo.min(r);
        hi = hi.max(r);
    }
    (rate, lo, hi)
}

/// `slow`, `fast`, or nothing: how a slice's speeds read.
#[must_use]
pub fn speed_flag((_, lo, hi): (f64, f64, f64)) -> &'static str {
    if lo < SLOWEST {
        "slow"
    } else if hi > FASTEST {
        "fast"
    } else {
        ""
    }
}

/// One slice as read off the state.
struct SliceRead {
    start: f64,
    length: f64,
    file: Option<String>,
    offset: f64,
    speeds: (f64, f64, f64),
    fades: (f64, f64),
}

/// `SLICES_LIST`: see its description.
fn slices_list(a: &SlicesArg) -> Result<RawJson, String> {
    let traced = crate::edit_trace::Traced::enter(crate::edit_trace::edit_span!("SLICES_LIST"));
    let out = slices_traced(a, &traced);
    traced.outcome(&out);
    out
}

fn slices_traced(a: &SlicesArg, traced: &crate::edit_trace::Traced) -> Result<RawJson, String> {
    use daw::service::Tracks as _;
    let (daw, project) = crate::actions::song()?;
    let song = crate::song::Song::current()?;
    let (from, to) = (
        a.from.unwrap_or(f64::NEG_INFINITY),
        a.to.unwrap_or(f64::INFINITY),
    );
    let step = tracing::info_span!("slices.read").entered();
    let wanted = a
        .track
        .as_ref()
        .map(|t| crate::actions::resolve(daw, &project, t))
        .transpose()?;
    let mic = song
        .daw()
        .all(song.context())
        .into_iter()
        .filter(|t| !crate::hits_midi::is_view(t))
        .filter(|t| wanted.as_ref().is_none_or(|w| *w == t.guid))
        .find_map(|t| {
            let edit = crate::drum_lanes::lane_named(&t, crate::drum_lanes::EDIT)?;
            Some((t.guid.clone(), t.name.clone(), edit))
        })
        .ok_or("no track of the kit has an EDIT lane (DRUMS_EDIT_LANES first)")?;
    let read: Vec<SliceRead> = song
        .read(|p| {
            let mut out: Vec<SliceRead> = p
                .items_by_track
                .get(&mic.0)
                .into_iter()
                .flatten()
                .filter_map(|guid| p.items.get(guid))
                .map(|entry| &entry.item)
                .filter(|item| item.fixed_lane.unwrap_or(0) == mic.2)
                .filter(|item| {
                    let at = item.position.as_seconds();
                    at < to && at + item.length.as_seconds() > from
                })
                .map(|item| {
                    let take = p
                        .takes
                        .get(&item.guid)
                        .and_then(|l| l.takes.get(usize::try_from(l.active_idx).ok()?));
                    let markers = take
                        .and_then(|t| p.stretch_markers.get(&t.guid))
                        .cloned()
                        .unwrap_or_default();
                    SliceRead {
                        start: item.position.as_seconds(),
                        length: item.length.as_seconds(),
                        file: take.and_then(|t| t.source_file_path.clone()),
                        offset: take.map_or(item.start_offset.as_seconds(), |t| {
                            t.start_offset.as_seconds()
                        }),
                        speeds: speeds(take.map_or(1.0, |t| t.play_rate), &markers),
                        fades: (
                            item.fade_in_length.as_seconds(),
                            item.fade_out_length.as_seconds(),
                        ),
                    }
                })
                .collect();
            out.sort_by(|x, y| x.start.total_cmp(&y.start));
            out
        })
        .ok_or("the song closed")?;
    let mut playing: Vec<(f64, String)> = crate::hits::hits(daw, &project, &[])
        .into_iter()
        .filter_map(|h| h.now.map(|n| (n, h.id)))
        .collect();
    playing.sort_by(|x, y| x.0.total_cmp(&y.0));
    drop(step);
    let step = tracing::info_span!("slices.table").entered();
    let (mut flagged, mut rows) = (0_usize, Vec::new());
    for s in &read {
        let flag = speed_flag(s.speeds);
        if !flag.is_empty() {
            flagged = flagged.saturating_add(1);
        }
        if a.flagged.unwrap_or(false) && flag.is_empty() {
            continue;
        }
        let lo = playing.partition_point(|(n, _)| *n < s.start);
        let hi = playing.partition_point(|(n, _)| *n < s.start + s.length);
        let on: Vec<&str> = playing
            .get(lo..hi)
            .unwrap_or_default()
            .iter()
            .map(|(_, id)| id.as_str())
            .collect();
        let take = s
            .file
            .as_deref()
            .map(|f| {
                crate::take_colors::stamp(f).unwrap_or_else(|| {
                    std::path::Path::new(f)
                        .file_stem()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
                })
            })
            .unwrap_or_default();
        rows.push(json!([
            round(s.start, 4),
            round(s.length, 4),
            take,
            round(s.offset, 4),
            round(s.speeds.0, 4),
            round(s.speeds.1, 4),
            round(s.speeds.2, 4),
            round(s.fades.0, 4),
            round(s.fades.1, 4),
            on,
            flag,
        ]));
    }
    traced.count("edit.slices", read.len());
    traced.count("edit.stretched", flagged);
    drop(step);
    Ok(RawJson::from(
        json!({
            "track": mic.1,
            "slices": read.len(),
            "flagged": flagged,
            "cols": SLICE_COLS,
            "rows": rows,
        })
        .to_string(),
    ))
}

// ── QUANTIZE's detail ─────────────────────────────────────────────────

/// A table: a header and a row of numbers each.
#[derive(Facet, Default, Debug, Clone, PartialEq)]
pub struct Columns {
    pub cols: Vec<String>,
    pub rows: Vec<Vec<f64>>,
}

/// The hits held for one reason.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct HeldGroup {
    /// Why: `exception zone`, `fill`, `outside the fills`, `written fill:
    /// not in the chart`, `no line in reach` (`window_ms`, `sure_ms`),
    /// `not the locked piece`, `within tolerance`, `span edge: the next
    /// span's`.
    pub why: String,
    pub count: u32,
    /// Each held hit: `time`, `bar`, `beat` (where it plays).
    pub hits: Columns,
}

/// A zone that held hits.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ZoneHeld {
    pub guid: String,
    pub kind: String,
    pub start: f64,
    pub end: f64,
    pub note: String,
    pub hits: u32,
}

/// What one span of a quantize moved and held, complete.
#[derive(Facet, Default, Debug, Clone, PartialEq)]
pub struct SpanDetail {
    /// Every move: `time` (where the hit plays), `bar`, `beat`, `to`
    /// (where it is sent), `off_ms` (target minus hit), `shift_ms` (how far
    /// it moves, strength applied).
    pub moves: Columns,
    /// The hits held, by reason.
    pub held: Vec<HeldGroup>,
    /// The zones that held hits.
    pub zones_held: Vec<ZoneHeld>,
}

/// Why a quantize left a hit where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Held {
    Exception,
    Fill,
    OutsideFills,
    WrittenFill,
    NoLine,
    NotLocked,
    Tolerance,
    SpanEdge,
}

impl Held {
    #[must_use]
    pub const fn why(self) -> &'static str {
        match self {
            Self::Exception => "exception zone",
            Self::Fill => "fill",
            Self::OutsideFills => "outside the fills (fills: Only)",
            Self::WrittenFill => "written fill: not in the chart",
            Self::NoLine => "no line in reach (window_ms, sure_ms)",
            Self::NotLocked => "not the locked piece",
            Self::Tolerance => "within tolerance",
            Self::SpanEdge => "span edge: the next span's",
        }
    }

    /// Whether a zone holds it, so the zone is named.
    #[must_use]
    pub const fn by_zone(self) -> bool {
        matches!(self, Self::Exception | Self::Fill | Self::WrittenFill)
    }
}

/// The detail of a span from its decisions: `moves` (`(hit, target)`, the
/// ones moved), `held` (`(hit, why)`), at `place(t) -> (bar, beat)`, with
/// `strength`; the zones holding a zone-held hit named from `zones`.
#[must_use]
pub fn detail(
    (moves, held): (&[(f64, f64)], &[(f64, Held)]),
    strength: f64,
    zones: &[crate::hits::Zone],
    place: impl Fn(f64) -> (i32, f64),
) -> SpanDetail {
    let rows = moves
        .iter()
        .map(|&(h, t)| {
            let (bar, beat) = place(h);
            vec![
                round(h, 4),
                f64::from(bar),
                round(beat, 3),
                round(t, 4),
                round((t - h) * 1e3, 1),
                round((t - h) * strength * 1e3, 1),
            ]
        })
        .collect();
    let mut groups: std::collections::BTreeMap<Held, Vec<Vec<f64>>> =
        std::collections::BTreeMap::new();
    let mut by_zone: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for &(h, why) in held {
        let (bar, beat) = place(h);
        groups
            .entry(why)
            .or_default()
            .push(vec![round(h, 4), f64::from(bar), round(beat, 3)]);
        if why.by_zone() {
            let wanted = match why {
                Held::Exception => Some(crate::hits::ZoneKind::Exception),
                _ => Some(crate::hits::ZoneKind::Fill),
            };
            // The innermost zone of that kind holding it.
            if let Some(z) = zones
                .iter()
                .filter(|z| Some(z.kind) == wanted && z.origin != crate::hits::ZoneOrigin::Rejected)
                .filter(|z| h >= z.start - 0.1 && h < z.end)
                .min_by(|a, b| (a.end - a.start).total_cmp(&(b.end - b.start)))
            {
                let n = by_zone.entry(z.guid.clone()).or_default();
                *n = n.saturating_add(1);
            }
        }
    }
    let cols = |names: &[&str]| names.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>();
    SpanDetail {
        moves: Columns {
            cols: cols(&["time", "bar", "beat", "to", "off_ms", "shift_ms"]),
            rows,
        },
        held: groups
            .into_iter()
            .map(|(why, rows)| HeldGroup {
                why: why.why().to_owned(),
                count: u32::try_from(rows.len()).unwrap_or(u32::MAX),
                hits: Columns {
                    cols: cols(&["time", "bar", "beat"]),
                    rows,
                },
            })
            .collect(),
        zones_held: zones
            .iter()
            .filter_map(|z| {
                by_zone.get(&z.guid).map(|n| ZoneHeld {
                    guid: z.guid.clone(),
                    kind: format!("{:?}", z.kind),
                    start: z.start,
                    end: z.end,
                    note: z.note.clone(),
                    hits: *n,
                })
            })
            .collect(),
    }
}

impl SpanDetail {
    /// Counts only: the tables emptied, how many each held kept.
    pub fn brief(&mut self) {
        self.moves.rows.clear();
        for g in &mut self.held {
            g.hits.rows.clear();
        }
    }
}

// ── ZONES_LIST ────────────────────────────────────────────────────────

/// The song's zones: the fills, and apart from them the no-quantize
/// exceptions — what keeps a quantize away.
#[derive(Facet)]
pub struct ZonesOut {
    /// No-quantize zones (not turned down): every quantize leaves these as
    /// played, whatever is asked of the fills.
    pub exceptions: Vec<crate::hits::Zone>,
    /// The fills, and exceptions turned down (holding nothing).
    pub zones: Vec<crate::hits::Zone>,
}

/// `zones` split into the exceptions that hold and the rest.
#[must_use]
pub fn zones_out(zones: Vec<crate::hits::Zone>) -> ZonesOut {
    let (exceptions, zones) = zones.into_iter().partition(|z| {
        z.kind == crate::hits::ZoneKind::Exception && z.origin != crate::hits::ZoneOrigin::Rejected
    });
    ZonesOut { exceptions, zones }
}

/// `DRUMS_BARS` and `SLICES_LIST`.
///
/// # Errors
/// When either is already registered.
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    // A raw answer (a header and rows), so through `register_with`.
    list.register_with(&BARS, Effect::ReadOnly, |_: &ActionCaller, a: BarsArg| {
        drums_bars(&a)
    })?;
    list.register_with(
        &SLICES,
        Effect::ReadOnly,
        |_: &ActionCaller, a: SlicesArg| slices_list(&a),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
