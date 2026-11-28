//! The song's timeline on the Action List: its tempo, the time selection,
//! and the sections and markers the Organize toolbar inserts.
//!
//! - `TEMPO_SET` sets the tempo at a bar or a time. While the song has no
//!   audio its markers, sections and the edit cursor keep their bars — the
//!   structure is musical before anything is recorded against the clock —
//!   and once it has, the caller says which (`follow`). The guide is
//!   generated again, so the click follows (below the guide's threshold
//!   its eighths come in by themselves).
//! - `TEMPO_MAP_SET` replaces the whole map: every tempo point, with the
//!   meters that change along it. What a tempo-mapped recording gets (one
//!   point per bar, `studio.tempo-map`).
//! - `TIME_SELECTION_SET` sets the time selection, in the engine and in
//!   the arrangement (`crate::edit_state::ask`), or clears it.
//! - `SECTION_INSERT` and `MARKER_INSERT` are the toolbar's buttons
//!   (`crate::organize_edit::insert`, one path for both): a section fills the
//!   time selection, or its default length from the edit cursor, and the
//!   guide is generated again with its cue.
//!
//! Bars are the song's (`crate::fx_actions::moment`): bar 1 is the one
//! SONGSTART falls in, the count-in before it bars −1 and −2.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::transport::service::Transport as _;
use daw::service::{ItemRef, ProjectContext, TakeRef, Takes as _, TempoMap as _};
use facet::Facet;
use fts_session::section_kinds::{MarkerKind, SectionKind};

use crate::edit_state::{Edit, Span};
use crate::organize_edit::Button;

static TEMPO: ActionMeta = crate::actions::meta(
    "TEMPO_SET",
    "tempo_set",
    "Set the tempo",
    "Set the tempo to `bpm` at `at` — a bar (`33`, `33:2.5`) or a time (`12.5s`); the song's start when left out. While the song has no audio its markers, sections and the edit cursor keep their bars; once it has, say `follow: true` for that or `follow: false` to leave them at their times. The click and guide are generated again (below 75 bpm the click plays eighths).",
    "Tempo",
    "",
);

static TEMPO_MAP: ActionMeta = crate::actions::meta(
    "TEMPO_MAP_SET",
    "tempo_map_set",
    "Set the tempo map",
    "Replace the song's whole tempo map with `points`: each `at` (seconds), `bpm` (quarter notes a minute) and, where the meter is set or changes, `numerator` and `denominator`. Every point the song had is removed first; a point at 0 is the song's own tempo and meter. `follow: true` keeps the markers, sections and the edit cursor on their bars (they move in time); left out or false they stay at their times, as a recording's do. `bar_one` (seconds) is where the song's bar 1 is under the new map: its SONGSTART is moved there (made, when the song has none), so the ruler and everything that takes a bar count from it. The click and guide are generated again. Says how many points and meter changes were set.",
    "Tempo",
    "",
);

static TEMPO_READ: ActionMeta = crate::actions::meta(
    "SESSION_TEMPO",
    "session_tempo",
    "The tempo map",
    "The song's tempo map as the window's ruler reads it: every change, in order — `at` (seconds), `bpm`, its `meter` (`3/4`; a change that sets none keeps the one before it) and the `bar` it falls on (`5.1`: bar and beat, counted as the ruler numbers them, from the song's bar 1). `at`: times (seconds) to say the bar and beat of too, in `bars`. Read only; ARRANGE_TEMPO and TEMPO_SET change it.",
    "Tempo",
    "",
);

static SELECTION: ActionMeta = crate::actions::meta(
    "TIME_SELECTION_SET",
    "time_selection_set",
    "Set the time selection",
    "Select `from` to `to` — each a bar (`3`, `3:2.5`), a time (`12.5s`) or a section (`VS 1`, `VS 1 end`) — with the edit cursor at its start, as a drag across the ruler does. Both left out (or null) clears the selection.",
    "View",
    "",
);

static SECTION: ActionMeta = crate::actions::meta(
    "SECTION_INSERT",
    "section_insert",
    "Insert a section",
    "Insert a `kind` of section (intro, verse, pre-chorus, chorus, bridge, outro, instrumental, solo, hits, interlude, breakdown, vamp, refrain, tag, turnaround, ending) as the Organize toolbar's button does: over `from`..`to` when given (bars, times or sections as TIME_SELECTION_SET), else over the time selection, else its default length (intro and outro 4 bars, verse and chorus 8) from the edit cursor, up to the next section. Sections it overlaps are cut back. The edit cursor moves to its end, so the next goes after it; SONGEND moves out to it when it runs past; the guide is generated again with its cue. Returns the section.",
    "Structure",
    "",
);

static MARKER: ActionMeta = crate::actions::meta(
    "MARKER_INSERT",
    "marker_insert",
    "Insert a marker",
    "Insert a song marker — `kind`: count-in, start (=START), songstart, songend or end (=END) — at `at` (a bar, a time or a section, as TIME_SELECTION_SET) or the edit cursor, as the Organize toolbar's button does. A COUNT-IN moves the edit cursor two bars on, where SONGSTART goes. The guide is generated again. Says where it went.",
    "Structure",
    "",
);

/// A tempo.
#[derive(Facet)]
struct TempoArg {
    bpm: f64,
    at: Option<String>,
    follow: Option<bool>,
}

/// One point of a tempo map.
#[derive(Facet, Clone)]
struct TempoMapPoint {
    /// Seconds.
    at: f64,
    /// Quarter notes a minute.
    bpm: f64,
    numerator: Option<u32>,
    denominator: Option<u32>,
}

/// A whole tempo map.
#[derive(Facet)]
struct TempoMapArg {
    points: Vec<TempoMapPoint>,
    follow: Option<bool>,
    /// Where the song's bar 1 is, seconds.
    bar_one: Option<f64>,
}

/// Times to say the bars of.
#[derive(Facet)]
struct TempoReadArg {
    #[facet(default)]
    at: Vec<f64>,
}

/// One change of the tempo map, as the ruler reads it.
#[derive(Facet)]
struct TempoChangeOut {
    at: f64,
    bpm: f64,
    meter: String,
    bar: String,
}

/// A time and its bar.
#[derive(Facet)]
struct BarOut {
    at: f64,
    bar: String,
}

/// The tempo map, and the bars asked for.
#[derive(Facet)]
struct TempoOut {
    changes: Vec<TempoChangeOut>,
    bars: Vec<BarOut>,
}

/// A span, or none.
#[derive(Facet)]
struct SelectionArg {
    from: Option<String>,
    to: Option<String>,
}

/// A section to insert.
#[derive(Facet)]
struct SectionArg {
    kind: String,
    from: Option<String>,
    to: Option<String>,
}

/// A marker to insert.
#[derive(Facet)]
struct MarkerArg {
    kind: String,
    at: Option<String>,
}

/// The slowest and fastest tempo taken.
const BPM: std::ops::RangeInclusive<f64> = 10.0..=400.0;

/// How close two times are to be one tempo point, seconds.
const SAME_POINT: f64 = 0.001;

/// The section kinds by what they are called, squashed (letters and
/// digits only, lower case).
const SECTIONS: &[(&str, SectionKind)] = &[
    ("intro", SectionKind::Intro),
    ("in", SectionKind::Intro),
    ("verse", SectionKind::Verse),
    ("vs", SectionKind::Verse),
    ("prechorus", SectionKind::PreChorus),
    ("pre", SectionKind::PreChorus),
    ("chorus", SectionKind::Chorus),
    ("ch", SectionKind::Chorus),
    ("bridge", SectionKind::Bridge),
    ("br", SectionKind::Bridge),
    ("outro", SectionKind::Outro),
    ("instrumental", SectionKind::Instrumental),
    ("solo", SectionKind::Solo),
    ("hits", SectionKind::Hits),
    ("interlude", SectionKind::Interlude),
    ("breakdown", SectionKind::Breakdown),
    ("vamp", SectionKind::Vamp),
    ("refrain", SectionKind::Refrain),
    ("tag", SectionKind::Tag),
    ("turnaround", SectionKind::Turnaround),
    ("ending", SectionKind::End),
    ("end", SectionKind::End),
];

/// The song markers by what they are called, squashed, and as written.
const MARKERS: &[(&str, MarkerKind, &str)] = &[
    ("countin", MarkerKind::CountIn, "COUNT-IN"),
    ("start", MarkerKind::Start, "=START"),
    ("songstart", MarkerKind::SongStart, "SONGSTART"),
    ("songend", MarkerKind::SongEnd, "SONGEND"),
    ("end", MarkerKind::End, "=END"),
];

/// `text` with only its letters and digits, lower case: `Pre-Chorus` and
/// `pre chorus` are one name.
fn squashed(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn section_kind(text: &str) -> Result<SectionKind, String> {
    let want = squashed(text);
    SECTIONS
        .iter()
        .find(|(name, _)| *name == want)
        .map(|(_, kind)| *kind)
        .ok_or_else(|| {
            format!(
                "no section kind {text}: one of {:?}",
                SECTIONS.iter().map(|(name, _)| *name).collect::<Vec<_>>()
            )
        })
}

fn marker_kind(text: &str) -> Result<(MarkerKind, &'static str), String> {
    let want = squashed(text);
    MARKERS
        .iter()
        .find(|(name, ..)| *name == want)
        .map(|(_, kind, written)| (*kind, *written))
        .ok_or_else(|| {
            format!(
                "no marker kind {text}: one of {:?}",
                MARKERS
                    .iter()
                    .map(|(.., written)| *written)
                    .collect::<Vec<_>>()
            )
        })
}

/// Where `text` is, seconds — a bar, a time or a section, as `FX_AUTOMATE`
/// reads them.
fn moment(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    text: &str,
) -> Result<f64, String> {
    crate::fx_actions::moment(daw, project, text)
}

/// `from`..`to`, both given or neither.
fn span(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<Option<Span>, String> {
    match (from, to) {
        (None, None) => Ok(None),
        (Some(from), Some(to)) => {
            let span = Span::between(moment(daw, project, from)?, moment(daw, project, to)?);
            if span.is_meaningful() {
                Ok(Some(span))
            } else {
                Err(format!("{from}..{to} is no span"))
            }
        }
        _ => Err("a span is `from` and `to` together".to_owned()),
    }
}

/// Where the edit cursor and selection are: the arrangement's, as last
/// drawn or asked for; the engine's when there is no arrangement.
fn edit_now(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Edit {
    crate::edit_state::current().unwrap_or_else(|| crate::organize_edit::engine_edit(daw, project))
}

/// The click, count and cues generated again over the song as it is now.
fn regenerate_guide(daw: &daw::standalone::Standalone) {
    if let Err(e) = engine_domain::guide::Guide::new(daw.clone())
        .with_instrument(crate::prepare::GUIDE_INSTRUMENT)
        .generate(engine_domain::guide::GuideScope::All)
    {
        tracing::debug!(error = %e, "timeline: no guide to regenerate yet");
    }
}

static WHAT_IS_SELECTED: ActionMeta = crate::actions::meta(
    "SESSION_SELECTION",
    "selection",
    "What is selected",
    "What the person has selected in the window, to know exactly what they mean: the time selection (seconds, and bars as the ruler writes them, with the sections it overlaps), the edit cursor, the selected tracks (paths) and items (track, where, how long; the first 100), the kit's hits inside the time selection by drum, and the track regions it overlaps. Read-only.",
    "Session",
    "",
);

/// A stretch of time, said both ways.
#[derive(Facet)]
struct StretchOut {
    from: f64,
    to: f64,
    seconds: f64,
    /// Where it starts and ends, as the ruler writes them: `78.3 – 79.1`.
    bars: String,
    sections: Vec<String>,
}

/// A selected item.
#[derive(Facet)]
struct SelectedItem {
    guid: String,
    track: String,
    at: f64,
    length: f64,
}

/// One drum's hits in the time selection.
#[derive(Facet)]
struct HitsIn {
    piece: String,
    hits: u32,
}

/// A track region the time selection overlaps.
#[derive(Facet)]
struct RegionIn {
    track: String,
    label: String,
    bars: String,
}

/// What is selected.
#[derive(Facet)]
struct SelectionOut {
    time_selection: Option<StretchOut>,
    cursor: f64,
    cursor_bar: String,
    tracks: Vec<String>,
    items: Vec<SelectedItem>,
    items_selected: u32,
    hits: Vec<HitsIn>,
    regions: Vec<RegionIn>,
}

/// The most selected items said one by one.
const MOST_ITEMS: usize = 100;

fn selection(daw: &daw::standalone::Standalone, project: &ProjectContext) -> SelectionOut {
    use daw::service::{Items as _, Tracks};
    let edit = edit_now(daw, project);
    let tracks = Tracks::all(daw, project.clone());
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let path = |guid: &str| {
        by_guid
            .get(guid)
            .map(|t| crate::gain_stage::path_of(t, &by_guid).join("/"))
            .unwrap_or_default()
    };
    let span = edit.selection.map(|sel| (sel.start, sel.end));
    let time_selection = span.map(|(from, to)| StretchOut {
        from,
        to,
        seconds: to - from,
        bars: format!(
            "{} – {}",
            crate::track_regions::bar_of(daw, project, from),
            crate::track_regions::bar_of(daw, project, to)
        ),
        sections: crate::actions::sections(daw, project)
            .into_iter()
            .filter(|s| s.start < to && s.end > from)
            .map(|s| s.name)
            .collect(),
    });
    let selected: Vec<daw_proto::Item> = daw
        .get_all_items(project.clone())
        .into_iter()
        .filter(|i| i.selected)
        .collect();
    let items = selected
        .iter()
        .take(MOST_ITEMS)
        .map(|i| SelectedItem {
            guid: i.guid.clone(),
            track: path(&i.track_guid),
            at: i.position.as_seconds(),
            length: i.length.as_seconds(),
        })
        .collect();
    let mut hits: Vec<HitsIn> = Vec::new();
    let mut regions = Vec::new();
    if let Some((from, to)) = span {
        for hit in crate::hits::playback(daw, project)
            .iter()
            .filter(|h| h.time >= from && h.time < to)
        {
            match hits.iter_mut().find(|h| h.piece == hit.piece) {
                Some(h) => h.hits = h.hits.saturating_add(1),
                None => hits.push(HitsIn {
                    piece: hit.piece.clone(),
                    hits: 1,
                }),
            }
        }
        for (track, marks) in crate::track_regions::all(daw, project) {
            for r in marks
                .regions
                .iter()
                .filter(|r| r.start < to && r.end > from)
            {
                regions.push(RegionIn {
                    track: path(&track),
                    label: r.label(),
                    bars: format!(
                        "{} – {}",
                        crate::track_regions::bar_of(daw, project, r.start),
                        crate::track_regions::bar_of(daw, project, r.end)
                    ),
                });
            }
        }
    }
    SelectionOut {
        time_selection,
        cursor: edit.at,
        cursor_bar: crate::track_regions::bar_of(daw, project, edit.at),
        tracks: tracks
            .iter()
            .filter(|t| t.selected)
            .map(|t| path(&t.guid))
            .collect(),
        items,
        items_selected: u32::try_from(selected.len()).unwrap_or(u32::MAX),
        hits,
        regions,
    }
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&WHAT_IS_SELECTED, Effect::ReadOnly, |(): ()| {
        let (daw, project) = crate::actions::song()?;
        Ok(selection(daw, &project))
    })?;
    list.register(&TEMPO, Effect::Mutating, |a: TempoArg| {
        if !BPM.contains(&a.bpm) {
            return Err(format!(
                "{} bpm is not a tempo: {} to {}",
                a.bpm,
                BPM.start(),
                BPM.end()
            ));
        }
        let (daw, project) = crate::actions::song()?;
        let at = a
            .at
            .as_deref()
            .map(|text| moment(daw, &project, text))
            .transpose()?
            .unwrap_or(0.0);
        let follow = match (a.follow, has_audio(daw, &project)) {
            (Some(follow), _) => follow,
            (None, false) => true,
            (None, true) => {
                return Err(
                    "the song has audio, recorded against the clock: say `follow: true` to keep the markers and sections on their bars (they move in time), or `follow: false` to leave them where they are"
                        .to_owned(),
                );
            }
        };
        // One undo step: the engine's undo keeps the tempo map, and a
        // later undo of an earlier edit must not take this tempo with it.
        let set = one_step(daw, &project, "Set the tempo", || {
            set_tempo(daw, &project, a.bpm, at, follow)
        });
        set?;
        regenerate_guide(daw);
        crate::song_model::request_resync();
        crate::actions::list().changed("tempo", &format!("{} bpm", a.bpm));
        Ok(format!(
            "{} bpm from {at:.3} s{}; the guide follows",
            a.bpm,
            if follow {
                ", the markers and sections on their bars"
            } else {
                ""
            }
        ))
    })?;
    list.register(&TEMPO_READ, Effect::ReadOnly, |a: TempoReadArg| {
        let (daw, project) = crate::actions::song()?;
        Ok::<_, String>(tempo_read(daw, &project, &a.at))
    })?;
    list.register(&TEMPO_MAP, Effect::Mutating, |a: TempoMapArg| {
        let (daw, project) = crate::actions::song()?;
        if a.bar_one.is_some_and(|at| !at.is_finite() || at < 0.0) {
            return Err("`bar_one` is a time in the song, seconds".to_owned());
        }
        // One undo step, as TEMPO_SET is.
        let meters = one_step(daw, &project, "Set the tempo map", || {
            set_tempo_map(daw, &project, &a.points, a.follow.unwrap_or(false))
        })?;
        let started = a
            .bar_one
            .map(|at| set_song_start(daw, &project, at))
            .transpose()?;
        regenerate_guide(daw);
        crate::song_model::request_resync();
        crate::actions::list().changed("tempo", &format!("{} points", a.points.len()));
        let first = a.points.iter().map(|p| p.at).fold(f64::INFINITY, f64::min);
        let last = a.points.iter().map(|p| p.at).fold(0.0, f64::max);
        Ok(format!(
            "{} tempo points from {first:.3} s to {last:.3} s, {meters} meter{} set{}; the guide follows",
            a.points.len(),
            if meters == 1 { "" } else { "s" },
            started.map_or_else(String::new, |at| format!(", bar 1 (SONGSTART) at {at:.3} s"))
        ))
    })?;
    list.register(&SELECTION, Effect::Mutating, |a: SelectionArg| {
        let (daw, project) = crate::actions::song()?;
        let Some(chosen) = span(daw, &project, a.from.as_deref(), a.to.as_deref())? else {
            daw.clear_time_selection(project.clone())
                .map_err(|e| e.to_string())?;
            let at = edit_now(daw, &project).at;
            crate::edit_state::ask(Edit {
                at,
                selection: None,
            });
            return Ok("no time selection".to_owned());
        };
        daw.set_time_selection(project.clone(), chosen.start, chosen.end)
            .and_then(|()| daw.set_position(project.clone(), chosen.start))
            .map_err(|e| e.to_string())?;
        crate::edit_state::ask(Edit {
            at: chosen.start,
            selection: Some(chosen),
        });
        Ok(format!(
            "{:.3} s to {:.3} s selected, the edit cursor at its start",
            chosen.start, chosen.end
        ))
    })?;
    list.register(&SECTION, Effect::Mutating, |a: SectionArg| {
        let kind = section_kind(&a.kind)?;
        let (daw, project) = crate::actions::song()?;
        let edit = span(daw, &project, a.from.as_deref(), a.to.as_deref())?.map_or_else(
            || edit_now(daw, &project),
            |over| Edit {
                at: over.start,
                selection: Some(over),
            },
        );
        crate::organize_edit::insert(daw, Button::Section(kind), false, Some(edit));
        crate::song_model::request_resync();
        crate::actions::list().changed("markers", &a.kind);
        // The section ends where the insert left the cursor.
        let end = crate::organize_edit::engine_edit(daw, &project).at;
        crate::actions::sections(daw, &project)
            .into_iter()
            .find(|s| (s.end - end).abs() < SAME_POINT)
            .ok_or_else(|| format!("no {} was inserted at {:.3} s", a.kind, edit.at))
    })?;
    list.register(&MARKER, Effect::Mutating, |a: MarkerArg| {
        let (kind, written) = marker_kind(&a.kind)?;
        let (daw, project) = crate::actions::song()?;
        let mut edit = edit_now(daw, &project);
        if let Some(at) = a.at.as_deref() {
            edit.at = moment(daw, &project, at)?;
        }
        crate::organize_edit::insert(daw, Button::Marker(kind), false, Some(edit));
        crate::song_model::request_resync();
        crate::actions::list().changed("markers", written);
        Ok(format!("{written} at {:.3} s", edit.at))
    })?;
    Ok(())
}

/// The tempo map as the window reads it (`daw_view::song::project::
/// tempo_changes`, what its read-back holds), each change on its bar, and
/// the bars of `times`.
fn tempo_read(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    times: &[f64],
) -> TempoOut {
    let points = daw.get_tempo_points(project.clone());
    let bpm = daw.get_default_tempo(project.clone());
    let (num, den) = daw.get_time_signature_at(project.clone(), 0.0);
    let meter = (
        u32::try_from(num.max(1)).unwrap_or(4),
        u32::try_from(den.max(1)).unwrap_or(4),
    );
    let bar = |at: f64| crate::track_regions::bar_of(daw, project, at);
    TempoOut {
        changes: daw_view::song::project::tempo_changes(&points, (bpm, meter))
            .into_iter()
            .map(|change| TempoChangeOut {
                at: change.at,
                bpm: change.bpm,
                meter: format!("{}/{}", change.beats_per_bar, change.beat_unit),
                bar: bar(change.at),
            })
            .collect(),
        bars: times
            .iter()
            .map(|&at| BarOut { at, bar: bar(at) })
            .collect(),
    }
}

/// Run `f` as the undo step `label`, kept apart in the song's history: the
/// steps before it recorded first, and it recorded before the action
/// returns (`crate::history_marks::settle`, as the FX actions do). Inside
/// a step already open (an agent's group, a caller's), it is part of that
/// one.
fn one_step<R>(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    label: &str,
    f: impl FnOnce() -> R,
) -> R {
    let own = !daw.undo_step_open(project);
    if own {
        crate::history_marks::settle(crate::history_marks::SETTLE);
        daw::service::Projects::begin_undo_block(daw, project.clone(), label);
    }
    let out = f();
    if own {
        daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
        crate::history_marks::settle(crate::history_marks::SETTLE);
    }
    out
}

/// Whether anything in the song is audio — a take recorded or imported
/// against the clock, which a tempo change cannot move without moving
/// what it plays. The guide's MIDI is not.
fn has_audio(daw: &daw::standalone::Standalone, project: &ProjectContext) -> bool {
    let audio = |item: &daw_proto::Item| {
        let source = daw.get_source_type(
            project.clone(),
            ItemRef::Guid(item.guid.clone()),
            TakeRef::Active,
        );
        item.take_count > 0
            && matches!(
                source,
                daw_proto::SourceType::Audio | daw_proto::SourceType::Video
            )
    };
    daw::service::Items::get_all_items(daw, project.clone())
        .iter()
        .any(audio)
}

/// A moment as the tempo map counts it: measure, beat, fraction of a beat.
type Musical = (i32, i32, f64);

/// Where the markers, the sections, the edit cursor and the time
/// selection are in bars, read before the tempo changes.
struct Places {
    marks: Vec<(u32, Musical)>,
    spans: Vec<(u32, Musical, Musical)>,
    cursor: Musical,
    selection: Option<(Musical, Musical)>,
}

/// The bars everything is on now.
fn places(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Places {
    let musical = |seconds: f64| daw.time_to_musical(project.clone(), seconds);
    let edit = edit_now(daw, project);
    Places {
        marks: daw::service::Markers::all(daw, project.clone())
            .into_iter()
            .filter_map(|m| Some((m.id?, musical(m.position.seconds()?))))
            .collect(),
        spans: daw::service::Regions::all(daw, project.clone())
            .into_iter()
            .filter_map(|r| {
                Some((
                    r.id?,
                    musical(r.time_range.start_seconds()),
                    musical(r.time_range.end_seconds()),
                ))
            })
            .collect(),
        cursor: musical(edit.at),
        selection: edit.selection.map(|s| (musical(s.start), musical(s.end))),
    }
}

/// Puts everything back on the bars it was on, under the tempo map as it
/// is now.
fn keep_places(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    was: Places,
) -> Result<(), String> {
    let time = |(measure, beat, fraction): Musical| {
        daw.musical_to_time(project.clone(), measure, beat, fraction)
    };
    for (id, bar) in was.marks {
        daw::service::Markers::set_position(daw, project.clone(), id, time(bar))
            .map_err(|e| format!("a marker did not keep its bar: {e}"))?;
    }
    for (id, start, end) in was.spans {
        daw::service::Regions::set_bounds(daw, project.clone(), id, time(start), time(end))
            .map_err(|e| format!("a section did not keep its bars: {e}"))?;
    }
    let edit = Edit {
        at: time(was.cursor),
        selection: was
            .selection
            .map(|(start, end)| Span::between(time(start), time(end))),
    };
    daw.set_position(project.clone(), edit.at)
        .map_err(|e| e.to_string())?;
    if let Some(span) = edit.selection {
        daw.set_time_selection(project.clone(), span.start, span.end)
            .map_err(|e| e.to_string())?;
    }
    crate::edit_state::ask(edit);
    Ok(())
}

/// `bpm` from `at`: the tempo point there set, the song's own tempo when
/// `at` is its start and there is none, else a point added. With
/// `follow`, the markers, sections and edit cursor go back onto the bars
/// they were on.
fn set_tempo(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    bpm: f64,
    at: f64,
    follow: bool,
) -> Result<(), String> {
    let was = follow.then(|| places(daw, project));
    let points = daw.get_tempo_points(project.clone());
    let here = points.iter().position(|p| {
        p.position
            .seconds()
            .is_some_and(|s| (s - at).abs() < SAME_POINT)
    });
    match here {
        Some(index) => {
            let index = u32::try_from(index).map_err(|e| e.to_string())?;
            daw.set_tempo_at_point(project.clone(), index, bpm)
        }
        None if at < SAME_POINT => daw.set_default_tempo(project.clone(), bpm),
        None => daw.add_tempo_point(project.clone(), at, bpm).map(|_| ()),
    }
    .map_err(|e| format!("the tempo was refused: {e}"))?;
    match was {
        Some(was) => keep_places(daw, project, was),
        None => Ok(()),
    }
}

/// Puts the song's bar 1 at `at`: its `SONGSTART` moved there, any other
/// marker of that name removed (bar 1 is the bar the first one falls in),
/// or one made when the song has none.
fn set_song_start(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    at: f64,
) -> Result<f64, String> {
    use daw::service::Markers;
    let refused = |e: daw_proto::DawError| format!("SONGSTART was refused: {e}");
    let mut ids = Markers::all(daw, project.clone())
        .into_iter()
        .filter(|m| m.name == "SONGSTART")
        .filter_map(|m| m.id);
    match ids.next() {
        Some(id) => {
            Markers::set_position(daw, project.clone(), id, at).map_err(refused)?;
            for extra in ids {
                Markers::remove(daw, project.clone(), extra).map_err(refused)?;
            }
        }
        None => {
            Markers::add(daw, project.clone(), at, "SONGSTART").map_err(refused)?;
        }
    }
    crate::actions::list().changed("markers", "SONGSTART");
    Ok(at)
}

/// A meter a tempo point may set: 1 to 64 over a power of two up to 64.
fn meter_of(point: &TempoMapPoint) -> Result<Option<(i32, i32)>, String> {
    match (point.numerator, point.denominator) {
        (None, None) => Ok(None),
        (Some(n), Some(d)) if (1..=64).contains(&n) && d.is_power_of_two() && d <= 64 => {
            Ok(Some((
                i32::try_from(n).map_err(|e| e.to_string())?,
                i32::try_from(d).map_err(|e| e.to_string())?,
            )))
        }
        (n, d) => Err(format!(
            "{} over {} at {:.3} s is not a meter: `numerator` 1 to 64 and `denominator` a power of two, together",
            n.map_or_else(|| "nothing".to_owned(), |n| n.to_string()),
            d.map_or_else(|| "nothing".to_owned(), |d| d.to_string()),
            point.at
        )),
    }
}

/// `points` in time order, each a tempo and a time a map can hold, no two
/// at one time.
fn checked(points: &[TempoMapPoint]) -> Result<Vec<TempoMapPoint>, String> {
    if points.is_empty() {
        return Err("no points: a tempo map has at least one".to_owned());
    }
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.at.total_cmp(&b.at));
    let mut before: Option<f64> = None;
    for p in &sorted {
        if !p.at.is_finite() || p.at < 0.0 {
            return Err(format!("{} s is not a time in the song", p.at));
        }
        if !BPM.contains(&p.bpm) {
            return Err(format!(
                "{} bpm at {:.3} s is not a tempo: {} to {}",
                p.bpm,
                p.at,
                BPM.start(),
                BPM.end()
            ));
        }
        if before.is_some_and(|b| p.at - b < SAME_POINT) {
            return Err(format!("two points at {:.3} s", p.at));
        }
        meter_of(p)?;
        before = Some(p.at);
    }
    Ok(sorted)
}

/// Replaces the song's tempo map with `points`: the ones it had removed,
/// a point at 0 made the song's own tempo and meter, the rest added with
/// their meters. With how many meters were set.
fn set_tempo_map(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    points: &[TempoMapPoint],
    follow: bool,
) -> Result<usize, String> {
    let points = checked(points)?;
    let was = follow.then(|| places(daw, project));
    let refused = |e: daw_proto::DawError| format!("the tempo map was refused: {e}");
    for index in (0..daw.tempo_point_count(project.clone())).rev() {
        daw.remove_tempo_point(project.clone(), index)
            .map_err(refused)?;
    }
    let mut meters = 0_usize;
    for (i, point) in points.iter().enumerate() {
        let meter = meter_of(point)?;
        if point.at < SAME_POINT {
            daw.set_default_tempo(project.clone(), point.bpm)
                .map_err(refused)?;
            if let Some((n, d)) = meter {
                daw.set_default_time_signature(project.clone(), n, d)
                    .map_err(refused)?;
                meters = meters.saturating_add(1);
            }
            continue;
        }
        if i == 0 {
            // No point at 0: the song opens at the first point's tempo.
            daw.set_default_tempo(project.clone(), point.bpm)
                .map_err(refused)?;
        }
        let index = daw
            .add_tempo_point(project.clone(), point.at, point.bpm)
            .map_err(refused)?;
        if let Some((n, d)) = meter {
            daw.set_time_signature_at_point(project.clone(), index, n, d)
                .map_err(refused)?;
            meters = meters.saturating_add(1);
        }
    }
    if let Some(was) = was {
        keep_places(daw, project, was)?;
    }
    Ok(meters)
}
