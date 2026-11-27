//! The chart the current song shows (`SESSION_CHART`, studio#693): its
//! written chart, or, when it has none, the one Keyflow generates from its
//! markers.
//!
//! The chart pane draws a session from the session alone
//! (`keyflow_session::shown`): a session somebody has charted shows its
//! typed chart, one nobody has charted shows its **generated chart**, its
//! road map: the session's sections (its regions) with their bars and
//! measure numbers, its tempos and meters, its markers as cues, every bar
//! a bar of slashes. This answers with that same chart, read from the
//! session document the engine keeps for the song (`crate::session_doc`),
//! so what an agent or a test reads over MCP is what the pane draws.
//!
//! A song with a `.kf` beside it that is not yet typed into its session
//! has a written chart all the same: the app seeds it into the session as
//! the chart pane follows the song (`KEYFLOW_SESSION_FOLLOW`'s
//! `chart_path`), and `SESSION_CHART_AT` counts from it. That file is the
//! chart answered then, as written.

use std::path::{Path, PathBuf};

use architect::action::{ActionError, ActionList, ActionMeta, Effect};
use facet::Facet;
use keyflow_session::shown::RoadMap;
use keyflow_session::typed::Held;

static CHART: ActionMeta = ActionMeta {
    id: "SESSION_CHART",
    trait_name: "",
    method_name: "chart",
    display_name: "The chart",
    description: "The chart the current song shows, in Keyflow's language (`text`), and its `sections` (each its `name`, `first_bar` and `bars`: the measure numbers the chart prints, bar 1 the first after its count-in). `source` says whose it is: `chart`, written (typed into the session, or a `.kf` beside the song); `generated`, made by Keyflow from the song's own markers because it has none: its sections and measure numbers, every bar slashes, no chords until someone writes them. `count_in` is the bars before bar 1. SESSION_CHART_AT says where it is at the playhead. Read-only.",
    category: "View",
    group: "",
    shortcut: "",
    toggleable: false,
    undo: false,
};

/// One section of the chart shown, as it numbers its bars.
#[derive(Facet, Debug, Clone, PartialEq, Eq)]
pub struct ChartSection {
    /// As the song names it (`Verse 1`): the chart heads it so (`VS`,
    /// numbered 1).
    pub name: String,
    /// Its first bar's measure number (1 the first after the count-in).
    pub first_bar: i32,
    pub bars: u32,
}

/// The chart the current song shows.
#[derive(Facet, Debug, Clone)]
pub struct ChartOut {
    /// `chart` (written) or `generated` (from the song's markers).
    pub source: String,
    pub title: String,
    pub count_in: u32,
    pub sections: Vec<ChartSection>,
    /// The chart in Keyflow's language.
    pub text: String,
}

/// Puts `SESSION_CHART` in the engine's Action List (once: a second call
/// finds it there).
pub fn register(list: &ActionList) {
    match list.register(&CHART, Effect::ReadOnly, |(): ()| shown()) {
        Ok(()) | Err(ActionError::Duplicate(_)) => {}
        Err(e) => tracing::error!(error = %e, "daw: SESSION_CHART did not register"),
    }
}

/// The current song's chart: the reads first (the song, its session, a
/// `.kf` beside it), then the chart from them.
fn shown() -> Result<ChartOut, String> {
    let (project, title) = current().ok_or("no song is open")?;
    let (doc, kept) = crate::session_doc::doc_of(&project)
        .ok_or("the song's session is not open yet: ask again in a moment")?;
    let held = Held::of_doc(&doc);
    let written = if held.is_typed() {
        None
    } else {
        kept.parent()
            .and_then(chart_in)
            .map(|file| {
                std::fs::read_to_string(&file)
                    .map(|text| (file, text))
                    .map_err(|e| e.to_string())
            })
            .transpose()?
    };
    match written {
        Some((file, text)) => from_file(&title, &file, text),
        None => from_session(&title, &held),
    }
}

/// The current song: its project id and its title.
fn current() -> Option<(String, String)> {
    studio_engine::studio_engine::set::peek(|view| {
        let set = view.setlist.as_ref()?;
        let song = set.songs.get(set.at)?;
        Some((song.project.clone(), song.name.clone()))
    })
}

/// The one `.kf` in `dir`, if there is exactly one (as
/// `SESSION_CHART_AT` finds a song's chart).
fn chart_in(dir: &Path) -> Option<PathBuf> {
    let mut found = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "kf"));
    let one = found.next()?;
    found.next().is_none().then_some(one)
}

/// The chart the session shows: typed into it (`chart`), or generated
/// from its markers (`generated`), titled `title`.
///
/// # Errors
///
/// The chart composed does not read back as the road map's bars: a chart
/// the pane could not draw as the session has it.
pub fn from_session(title: &str, held: &Held) -> Result<ChartOut, String> {
    let road = held.road_map();
    let text = held.shown_source(title);
    check_reads_back(&road, &text)?;
    Ok(ChartOut {
        source: if held.is_typed() {
            "chart"
        } else {
            "generated"
        }
        .to_owned(),
        title: title.to_owned(),
        count_in: road.lead_in,
        sections: road
            .sections
            .iter()
            .map(|s| ChartSection {
                name: s.name.clone(),
                first_bar: s.first_bar,
                bars: s.bars,
            })
            .collect(),
        text,
    })
}

/// A written chart's file: its sections as it writes them.
fn from_file(title: &str, file: &Path, text: String) -> Result<ChartOut, String> {
    let chart = keyflow_lang::text::chart::parse_chart(&text)
        .map_err(|e| format!("{}: {e}", file.display()))?;
    let mut next = 1_i32;
    let mut count_in = 0;
    let mut sections = Vec::new();
    for s in &chart.sections {
        let bars = u32::try_from(s.measures().len()).unwrap_or(u32::MAX);
        if s.section.section_type.is_compact() {
            count_in += bars;
            continue;
        }
        sections.push(ChartSection {
            name: s.section.display_name(),
            first_bar: next,
            bars,
        });
        next = next.saturating_add(i32::try_from(bars).unwrap_or(i32::MAX));
    }
    Ok(ChartOut {
        source: "chart".to_owned(),
        title: title.to_owned(),
        count_in,
        sections,
        text,
    })
}

/// `text` (composed from `road`) must read as `road`'s sections and bars.
fn check_reads_back(road: &RoadMap, text: &str) -> Result<(), String> {
    let chart = keyflow_lang::text::chart::parse_chart(text)
        .map_err(|e| format!("the chart shown does not parse: {e}\n{text}"))?;
    let read: Vec<usize> = chart
        .sections
        .iter()
        .filter(|s| !s.section.section_type.is_compact())
        .map(|s| s.measures().len())
        .filter(|n| *n > 0)
        .collect();
    let made: Vec<usize> = road
        .sections
        .iter()
        .map(|s| s.bars as usize)
        .filter(|n| *n > 0)
        .collect();
    if read == made {
        Ok(())
    } else {
        Err(format!(
            "the chart shown reads as sections of {read:?} bars, the session has {made:?}\n{text}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fts_session::doc::SessionDoc;
    use fts_session::time::TimeSignature;
    use fts_session::{MarkId, Region, TempoMap, Time};

    /// 90 bpm in 3/4 (2 s bars): `Verse 1` bars 1-4, `Bridge` 5-6, as the
    /// markers-only fixture's markers lay it out.
    fn markers_only() -> SessionDoc {
        let doc = SessionDoc::new();
        let map = TempoMap::new(
            90.0,
            TimeSignature::new(3, 4),
            vec![TempoMap::point(0.0, 90.0, Some(TimeSignature::new(3, 4)))],
        );
        doc.set_tempo(&map).unwrap();
        let region = |name: &str, start: f64, end: f64| Region {
            start: Time::Seconds(start),
            end: Time::Seconds(end),
            name: name.into(),
            color: None,
            ruler: None,
        };
        doc.put_region(&MarkId::from("a"), &region("Verse 1", 0.0, 8.0))
            .unwrap();
        doc.put_region(&MarkId::from("b"), &region("Bridge", 8.0, 12.0))
            .unwrap();
        doc
    }

    #[test]
    fn a_session_with_markers_and_no_chart_gets_a_generated_chart() {
        let out = from_session("Markers", &Held::of_doc(&markers_only())).unwrap();
        assert_eq!(out.source, "generated");
        assert_eq!(out.count_in, 0);
        assert_eq!(
            out.sections,
            [
                ChartSection {
                    name: "Verse 1".into(),
                    first_bar: 1,
                    bars: 4
                },
                ChartSection {
                    name: "Bridge".into(),
                    first_bar: 5,
                    bars: 2
                },
            ]
        );
        assert!(out.text.contains("\\title \"Markers\""), "{}", out.text);
        assert!(out.text.contains("90bpm 3/4"), "{}", out.text);
    }

    #[test]
    fn a_session_with_a_typed_chart_keeps_it() {
        let doc = markers_only();
        let written = "Markers - Test\n90bpm 3/4\n\nVS 4\nC\nG\nAm\nF\n\nBR 2\nDm\nG\n";
        keyflow_session::typed::import(
            &doc,
            &keyflow_lang::text::chart::parse_chart(written).unwrap(),
        )
        .unwrap();
        let out = from_session("Markers", &Held::of_doc(&doc)).unwrap();
        assert_eq!(out.source, "chart");
        assert!(out.text.contains("Am"), "{}", out.text);
    }

    #[test]
    fn a_written_file_is_answered_as_written() {
        let text = "Song - Artist\n120bpm 4/4 #C\n\ncount 1\n\nVS 2\nC G\n\nCH 2\nF G\n";
        let out = from_file("Song", Path::new("song.kf"), text.to_owned()).unwrap();
        assert_eq!(out.source, "chart");
        assert_eq!(out.text, text);
        assert_eq!(out.count_in, 1);
        assert_eq!(
            out.sections
                .iter()
                .map(|s| (s.first_bar, s.bars))
                .collect::<Vec<_>>(),
            [(1, 2), (3, 2)]
        );
    }
}
