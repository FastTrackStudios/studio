//! A song's timing, as the engine reads it, written into its session
//! document: its tempo changes (with their meters), its markers, its
//! sections as regions, and `bar_one`.
//!
//! The desktop engine writes it into the session it keeps for the song as
//! the song opens (daw-extension's `session_doc`). The web build writes the
//! same into a session it makes in memory for a song it opened, and the
//! chart page draws the chart Keyflow generates from it (studio#742): one
//! writer, so a page and `SESSION_CHART` count the same bars.

use daw_view::song::project::{Project, TempoChange};
use fts_session::doc::SessionDoc;
use fts_session::time::{Position, TempoPoint, TimePosition, TimeSignature};
use fts_session::{MarkId, Marker, Region, TempoMap, Time};

/// A session in memory holding `project`'s timing and nothing else: what a
/// page draws a song's generated chart from when the song came as a
/// project, not a session.
///
/// # Errors
///
/// The document refused a write.
pub fn session_of(project: &Project) -> Result<SessionDoc, String> {
    let doc = SessionDoc::new();
    write(&doc, project)?;
    Ok(doc)
}

/// `project`'s timing written into `doc`, in place of the timing it had.
///
/// # Errors
///
/// The document refused a write.
pub fn write(doc: &SessionDoc, project: &Project) -> Result<(), String> {
    let mut changes: Vec<_> = project.tempo.iter().collect();
    changes.sort_by(|a, b| a.at.total_cmp(&b.at));
    let first = changes.first();
    let meter = |c: &TempoChange| TimeSignature::new(c.beats_per_bar.max(1), c.beat_unit.max(1));
    let tempo = TempoMap::new(
        first.map_or(project.bpm, |c| c.bpm),
        first.map_or_else(|| TimeSignature::new(4, 4), |c| meter(c)),
        changes
            .iter()
            .map(|c| {
                TempoPoint::with_time_signature(
                    Position::from_time(TimePosition::from_seconds(c.at)),
                    c.bpm,
                    meter(c),
                )
            })
            .collect(),
    );
    let err = |e: &dyn std::fmt::Display| e.to_string();
    doc.set_tempo(&tempo).map_err(|e| err(&e))?;
    for id in doc.markers().keys() {
        doc.remove_marker(id).map_err(|e| err(&e))?;
    }
    for id in doc.regions().keys() {
        doc.remove_region(id).map_err(|e| err(&e))?;
    }
    for m in &project.markers {
        doc.put_marker(
            &MarkId(format!("marker-{}", m.idx)),
            &Marker {
                at: Time::Seconds(m.at),
                name: m.name.clone(),
                color: color(m.color.as_deref()),
                ruler: Some(m.lane),
            },
        )
        .map_err(|e| err(&e))?;
    }
    for s in &project.sections {
        doc.put_region(
            &MarkId(format!("region-{}", s.id)),
            &Region {
                start: Time::Seconds(s.start),
                end: Time::Seconds(s.end),
                name: s.name.clone(),
                color: color(s.color.as_deref()),
                ruler: Some(s.lane),
            },
        )
        .map_err(|e| err(&e))?;
    }
    doc.set_bar_one(bar_one(project).map(Time::Seconds))
        .map_err(|e| err(&e))
}

/// `#rrggbb` as the session's colour.
fn color(css: Option<&str>) -> Option<u32> {
    let hex = css?.strip_prefix('#')?;
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

/// The song's bar 1: a `SONGSTART` marker, else the first section that is
/// not an intro or a count-in, else the bar `FTS_DAW_BAR_ONE` names, else
/// none.
fn bar_one(project: &Project) -> Option<f64> {
    if let Some(m) = project
        .markers
        .iter()
        .find(|m| m.name.trim().eq_ignore_ascii_case("songstart"))
    {
        return Some(m.at);
    }
    let mut sections: Vec<_> = project.sections.iter().collect();
    sections.sort_by(|a, b| a.start.total_cmp(&b.start));
    let lead_in = |name: &str| {
        let n = name.trim().to_ascii_lowercase();
        n.starts_with("intro") || n.starts_with("count") || n.starts_with("click")
    };
    let from_sections = match sections.first() {
        Some(first) if lead_in(&first.name) => {
            sections.iter().find(|s| !lead_in(&s.name)).map(|s| s.start)
        }
        _ => None,
    };
    // A song that marks neither (no SONGSTART, no named regions: sections
    // as text items on a track, `>` markers): where the person opening it
    // says, `FTS_DAW_BAR_ONE` (a bar of the song, from 1), on the song's
    // own tempo. A page has no environment: none there.
    from_sections.or_else(|| {
        let bar: u32 = std::env::var("FTS_DAW_BAR_ONE").ok()?.trim().parse().ok()?;
        Some(seconds_at_bar(project, bar))
    })
}

/// Where bar `bar` (from 1) starts, seconds, on `project`'s tempo changes.
fn seconds_at_bar(project: &Project, bar: u32) -> f64 {
    let mut changes: Vec<_> = project.tempo.iter().collect();
    changes.sort_by(|a, b| a.at.total_cmp(&b.at));
    let (mut at, mut bars) = (0.0_f64, 0.0_f64);
    let (mut bpm, mut beats) = changes.first().map_or((project.bpm, 4.0), |c| {
        (c.bpm, f64::from(c.beats_per_bar.max(1)))
    });
    let want = f64::from(bar.max(1).saturating_sub(1));
    for c in changes.iter().skip(1) {
        let bar_len = beats * 60.0 / bpm.max(1.0);
        let span = (c.at - at) / bar_len;
        if bars + span >= want {
            break;
        }
        bars += span;
        at = c.at;
        bpm = c.bpm;
        beats = f64::from(c.beats_per_bar.max(1));
    }
    at + (want - bars) * beats * 60.0 / bpm.max(1.0)
}
