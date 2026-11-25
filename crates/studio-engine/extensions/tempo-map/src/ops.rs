//! What each action does, given the song it is about: JSON in, JSON out,
//! nothing of the host or the engine (the extension's `lib.rs` resolves
//! the song and talks to the engine).

use std::path::Path;

use serde_json::{Value as Json, json};
use tempo_map::analysis::{self, Analysis, Options};
use tempo_map::bench;
use tempo_map::eval;
use tempo_map::map::TempoPoint;
use tempo_map::meter::{Meter, MeterChange};
use tempo_map::track::TrackParams;

use crate::store::Store;

fn number(args: &Json, name: &str) -> Option<f64> {
    args.get(name).and_then(Json::as_f64)
}

fn index(args: &Json, name: &str) -> Result<Option<usize>, String> {
    match args.get(name) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| format!("`{name}` is a whole number, not {v}")),
    }
}

fn meter(text: &str) -> Result<Meter, String> {
    Meter::parse(text).ok_or_else(|| format!("{text} is not a meter (`3/4`, `6/8`)"))
}

/// `[{bar, meter}]`.
fn changes(args: &Json) -> Result<Vec<MeterChange>, String> {
    let Some(list) = args.get("changes").and_then(Json::as_array) else {
        return Ok(Vec::new());
    };
    list.iter()
        .map(|c| {
            let bar = c
                .get("bar")
                .and_then(Json::as_u64)
                .and_then(|b| u32::try_from(b).ok())
                .ok_or("a change is `{bar, meter}`: no bar")?;
            let text = c
                .get("meter")
                .and_then(Json::as_str)
                .ok_or("a change is `{bar, meter}`: no meter")?;
            Ok(MeterChange {
                bar,
                meter: meter(text)?,
            })
        })
        .collect()
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<Json, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

/// The first bars of a map, for an answer a person reads.
fn first_bars(found: &Analysis) -> Json {
    let map = found.map();
    json!({
        "bars": map.bars.len(),
        "count_in": map.count_in,
        "first": map.bars.iter().take(4).collect::<Vec<_>>(),
        "tempo": map.tempo_range(),
    })
}

/// `STUDIO_TEMPO_MAP_ANALYSE`.
///
/// # Errors
///
/// No stems, a meter that is not one, or a file that cannot be written.
pub fn analyse(store: &Store, song: &str, args: &Json) -> Result<Json, String> {
    let stems = args
        .get("stems")
        .and_then(Json::as_str)
        .ok_or("no `stems`: the folder of `<song> (<Name>).wav` files")?;
    let defaults = TrackParams::default();
    let options = Options {
        track: TrackParams {
            bpm: number(args, "bpm").unwrap_or(defaults.bpm),
            tightness: number(args, "tightness").unwrap_or(defaults.tightness),
            from: number(args, "from").unwrap_or(defaults.from),
            ..defaults
        },
        meter: args
            .get("meter")
            .and_then(Json::as_str)
            .map(meter)
            .transpose()?,
        changes: changes(args)?,
        no_repair: args.get("repair").and_then(Json::as_bool) == Some(false),
    };
    let measured = store.measured(Path::new(stems))?;
    let found = analysis::analyse(&measured, &options);
    store.put(song, &found)?;
    let measure = found.evaluate(&measured);
    Ok(json!({
        "song": song,
        "summary": found.summary(found.phase_line(&measured)),
        "meter_guess": found.meter_guess,
        "repair": found.repair,
        "candidates": found.candidates.iter().take(4).collect::<Vec<_>>(),
        "grid_error_ms": measure.grid_error,
        "beat_error_ms": measure.beat_error,
        "on_hits": measure.on_hits,
        "confidence": measure.confidence,
        "flagged": measure.flagged,
    }))
}

/// `STUDIO_TEMPO_MAP_CANDIDATES`.
///
/// # Errors
///
/// No analysis of the song.
pub fn candidates(store: &Store, song: &str) -> Result<Json, String> {
    let found = store.get(song)?;
    Ok(json!({
        "song": song,
        "bar1": found.bar1,
        "meter": found.layout.meter.to_string(),
        "candidates": found.candidates,
    }))
}

/// `STUDIO_TEMPO_MAP_SET_BAR1`.
///
/// # Errors
///
/// No analysis of the song, or no such beat.
pub fn set_bar1(store: &Store, song: &str, args: &Json) -> Result<Json, String> {
    let mut found = store.get(song)?;
    let beat = if let Some(beat) = index(args, "beat")? {
        beat
    } else if let Some(time) = number(args, "time") {
        found.beat_near(time).ok_or("the song has no beats")?
    } else if let Some(place) = index(args, "candidate")? {
        place
            .checked_sub(1)
            .and_then(|i| found.candidates.get(i))
            .map(|c| c.beat)
            .ok_or_else(|| format!("no candidate {place}: there are {}", found.candidates.len()))?
    } else {
        return Err("say which: `beat`, `time` or `candidate`".to_owned());
    };
    found.set_bar1(beat)?;
    store.put(song, &found)?;
    Ok(json!({
        "song": song,
        "bar1": beat,
        "time": found.beats.get(beat),
        "map": first_bars(&found),
    }))
}

/// `STUDIO_TEMPO_MAP_EDIT`.
///
/// # Errors
///
/// No analysis of the song, no fix named, or one that does not fit it.
pub fn edit(store: &Store, song: &str, args: &Json) -> Result<Json, String> {
    let mut found = store.get(song)?;
    let mut did = false;
    if let Some(span) = args.get("respace").filter(|v| !v.is_null()) {
        let from = index(span, "from")?.ok_or("respace: no `from`")?;
        let to = index(span, "to")?.ok_or("respace: no `to`")?;
        let beats = index(span, "beats")?.ok_or("respace: no `beats`")?;
        let snap = span.get("snap").and_then(Json::as_bool) != Some(false);
        let measured = store.measured(Path::new(&found.stems))?;
        found.respace(&measured, from, to, beats, snap)?;
        did = true;
    }
    if let Some(text) = args.get("meter").and_then(Json::as_str) {
        let measured = store.measured(Path::new(&found.stems))?;
        found.set_meter(&measured, meter(text)?, changes(args)?);
        did = true;
    } else if args.get("changes").is_some_and(Json::is_array) {
        let measured = store.measured(Path::new(&found.stems))?;
        let first = found.layout.meter;
        found.set_meter(&measured, first, changes(args)?);
        did = true;
    }
    if let Some(placed) = args.get("place").filter(|v| !v.is_null()) {
        let from = index(placed, "from")?.ok_or("place: no `from`")?;
        let times: Vec<f64> = placed
            .get("times")
            .and_then(Json::as_array)
            .ok_or("place: no `times`")?
            .iter()
            .filter_map(Json::as_f64)
            .collect();
        found.place(from, &times)?;
        did = true;
    }
    if let Some(raw) = args.get("raw_from_bar") {
        let bar = match raw {
            Json::Null => None,
            v => Some(
                v.as_u64()
                    .and_then(|b| u32::try_from(b).ok())
                    .ok_or("`raw_from_bar` is a song bar, or null")?,
            ),
        };
        found.set_raw_from(bar);
        did = true;
    }
    if !did {
        return Err(
            "no fix named: `respace`, `meter` (with `changes`), `raw_from_bar` or `place`"
                .to_owned(),
        );
    }
    store.put(song, &found)?;
    Ok(json!({
        "song": song,
        "beats": found.beats.len(),
        "bar1": found.bar1,
        "edits": found.edits,
        "map": first_bars(&found),
    }))
}

/// `STUDIO_TEMPO_MAP_MAP`.
///
/// # Errors
///
/// No analysis of the song.
pub fn map(store: &Store, song: &str) -> Result<Json, String> {
    let found = store.get(song)?;
    let map = found.map();
    Ok(json!({
        "song": song,
        "bar1": found.bar1_beat(),
        "bar1_chosen": found.bar1.is_some(),
        "tempo": map.tempo_range(),
        // Where the audio would sit in a session for one whole count-in
        // bar at bar 1's tempo (STUDIO_TEMPO_MAP_APPLY's `offset`).
        "count_in_offset": map.count_in_offset(),
        "points": map.points(),
        "map": map,
    }))
}

/// The tempo points of the song's map for a session whose audio starts
/// `offset` seconds in, as the engine's `TEMPO_MAP_SET` takes them. With
/// bar 1's downbeat in the session.
///
/// # Errors
///
/// No analysis of the song, or a map with no bars.
pub fn points_for_engine(store: &Store, song: &str, offset: f64) -> Result<(Json, f64), String> {
    let found = store.get(song)?;
    let map = found.map();
    let first = map.bars.first().ok_or("the map has no bars")?.time;
    let point = |p: &TempoPoint| match p.meter {
        Some(m) => json!({
            "at": p.time,
            "bpm": p.bpm,
            "numerator": m.numerator,
            "denominator": m.denominator,
        }),
        None => json!({ "at": p.time, "bpm": p.bpm }),
    };
    let points: Vec<Json> = map.points_at(offset).iter().map(point).collect();
    Ok((Json::Array(points), first + offset))
}

/// `STUDIO_TEMPO_MAP_EVALUATE`.
///
/// # Errors
///
/// No analysis of the song, or stems that cannot be read.
pub fn evaluate(store: &Store, song: &str, args: &Json) -> Result<Json, String> {
    let found = store.get(song)?;
    let measured = store.measured(Path::new(&found.stems))?;
    let mut result = match args.get("map").and_then(Json::as_str) {
        Some(other) => eval::evaluate(&store.map_of(other)?, &measured, None),
        None => found.evaluate(&measured),
    };
    if args.get("bars").and_then(Json::as_bool) != Some(true) {
        result.per_bar.clear();
    }
    to_json(&result)
}

/// `STUDIO_TEMPO_MAP_COMPARE`.
///
/// # Errors
///
/// A map that is neither a file nor a song analysed.
pub fn compare(store: &Store, args: &Json) -> Result<Json, String> {
    let named = |which: &str| {
        args.get(which)
            .and_then(Json::as_str)
            .ok_or_else(|| format!("no `{which}`: a song analysed here, or a file"))
    };
    let (a, b) = (store.map_of(named("a")?)?, store.map_of(named("b")?)?);
    let tolerance = number(args, "tolerance_ms").unwrap_or(bench::TOLERANCE_MS);
    let mut diff = eval::compare(&a, &b, tolerance);
    if args.get("bars").and_then(Json::as_bool) != Some(true) {
        diff.per_bar.retain(|d| d.ms.abs() > tolerance);
    }
    to_json(&diff)
}

/// A kit piece's hits to write on a track.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackHits {
    pub piece: String,
    pub track: String,
    /// Seconds of the session.
    pub at: Vec<f64>,
}

/// `STUDIO_TEMPO_MAP_HITS`: the answer, and what `write` puts on tracks.
///
/// # Errors
///
/// No analysis of the song.
pub fn hits(store: &Store, song: &str, args: &Json) -> Result<(Json, Vec<TrackHits>), String> {
    let found = store.get(song)?;
    let only = args.get("piece").and_then(Json::as_str);
    let from = number(args, "from").unwrap_or(f64::NEG_INFINITY);
    let to = number(args, "to").unwrap_or(f64::INFINITY);
    let floor = number(args, "min_strength").unwrap_or(0.0);
    let offset = number(args, "offset").unwrap_or(0.0);
    let mut pieces = serde_json::Map::new();
    let mut tracks = Vec::new();
    for (piece, marks) in found.hit_marks() {
        if only.is_some_and(|p| !p.eq_ignore_ascii_case(&piece)) {
            continue;
        }
        let kept: Vec<_> = marks
            .into_iter()
            .filter(|h| h.time >= from && h.time < to && f64::from(h.strength) >= floor)
            .collect();
        let track = args
            .get("tracks")
            .and_then(|t| t.get(&piece))
            .and_then(Json::as_str)
            .unwrap_or(&piece)
            .to_owned();
        tracks.push(TrackHits {
            piece: piece.clone(),
            track,
            at: kept.iter().map(|h| h.time + offset).collect(),
        });
        pieces.insert(piece, to_json(&kept)?);
    }
    if let Some(piece) = only.filter(|_| pieces.is_empty()) {
        return Err(format!(
            "no hits of `{piece}`: the song has {}",
            found.hits.keys().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    Ok((
        json!({
            "song": song,
            "bar1_chosen": found.bar1.is_some(),
            "hits": pieces,
        }),
        tracks,
    ))
}

/// `STUDIO_TEMPO_MAP_BENCH`.
///
/// # Errors
///
/// No song with stems and an accepted project under `songs`.
pub fn benchmark(args: &Json) -> Result<Json, String> {
    let folder = |name: &str| args.get(name).and_then(Json::as_str).map(Path::new);
    let songs = folder("songs").ok_or("no `songs`: a folder per song")?;
    let report = bench::run(songs, folder("alt"), folder("scripts"), |_| {});
    if report.rows.is_empty() {
        return Err(format!(
            "{}: no song with stems and a `*Tempo Mapped*.RPP`",
            songs.display()
        ));
    }
    Ok(json!({
        "markdown": report.markdown(),
        "rows": report.rows,
        "agreement": report.agreement,
        "skipped": report.skipped,
    }))
}

/// `STUDIO_TEMPO_MAP_STATE`.
#[must_use]
pub fn state(store: &Store, build: &str) -> Json {
    let songs: Vec<Json> = store
        .keys()
        .into_iter()
        .filter_map(|key| {
            let found = store.get(&key).ok()?;
            Some(json!({
                "song": key,
                "stems": found.stems,
                "beats": found.beats.len(),
                "meter": found.layout.meter.to_string(),
                "bar1": found.bar1,
                "edits": found.edits.len(),
            }))
        })
        .collect();
    json!({
        "build": build,
        "analysis_version": analysis::VERSION,
        "kept_in": store.dir().display().to_string(),
        "songs": songs,
    })
}
