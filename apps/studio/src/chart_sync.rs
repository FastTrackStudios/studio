//! A song's chart, engraved to follow its recording: Keyflow's SVG with a
//! time on every bar (`engraver::export::follow`), the times taken from
//! the song's session so the chart keeps to the recording the player plays.
//!
//! The session knows where each section starts and ends in the recording
//! (its regions); the chart knows how many bars each section has. When
//! the two agree on how many sections the song plays, each bar is placed
//! inside its section evenly — a tempo change, a held chord or a slower
//! bridge stay in step. When they do not, the chart's own tempo runs
//! from the session's first section.

use keyflow::engraver::api::pipeline::ChartPipeline;
use keyflow::engraver::export::follow::{Follow, add_follow};
use keyflow::engraver::layout::chart::LayoutMode;
use keyflow::text::bar_times::{BarClock, BarTime, bar_times};

use crate::session_file::Section;

/// Width the chart is laid out at (points); the page scales it.
const LAYOUT_WIDTH: f64 = 800.0;
const PADDING: f64 = 6.0;

/// A chart ready to show.
#[derive(Clone, Debug, PartialEq)]
pub struct Engraved {
    /// The SVG, its bars marked `class="kf-bar"` with `data-at` (seconds).
    pub svg: String,
    /// How its bars were timed.
    pub timing: Timing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timing {
    /// Bar by bar inside the session's sections.
    Sections,
    /// The chart's tempo, from the session's first section.
    Tempo,
}

/// Engrave `source` timed by `sections` (the session's, in seconds).
pub fn engrave(source: &str, sections: &[Section]) -> Result<Engraved, String> {
    let chart = keyflow::text::chart::parse_chart(source).map_err(|e| e.to_string())?;
    let nominal = bar_times(&chart, BarClock::at(0.0));
    let measured = fit(&nominal, sections);
    let timing = if measured.is_some() {
        Timing::Sections
    } else {
        Timing::Tempo
    };
    let clock = BarClock {
        downbeat: sections.first().map_or(0.0, |s| s.start),
        measured: measured.as_deref(),
    };
    let follow = Follow::new(&chart, clock, None);
    let pipeline = pipeline()?;
    let result = pipeline.layout(
        &chart,
        &LayoutMode::ContinuousScroll {
            width: LAYOUT_WIDTH,
        },
    );
    let svg = pipeline.export_svg_snippet(&result, PADDING);
    Ok(Engraved {
        svg: add_follow(&svg, &result, &follow, None),
        timing,
    })
}

/// The `@font-face` rules the engraved SVGs name their fonts by; put in
/// the page once.
pub fn font_css() -> Result<String, String> {
    Ok(pipeline()?.font_face_css())
}

fn pipeline() -> Result<&'static ChartPipeline, String> {
    ChartPipeline::shared().map_err(|e| e.to_string())
}

/// Each played bar's start, when the chart plays as many sections as the
/// session has: a section's bars share its time evenly.
fn fit(bars: &[BarTime], sections: &[Section]) -> Option<Vec<f64>> {
    let runs = runs(bars);
    if runs.is_empty() || runs.len() != sections.len() {
        return None;
    }
    let mut out = Vec::with_capacity(bars.len());
    for (n, s) in runs.iter().zip(sections) {
        let each = (s.end - s.start) / *n as f64;
        for j in 0..*n {
            out.push(s.start + each * j as f64);
        }
    }
    Some(out)
}

/// How many bars each played section has, in playing order: a new one
/// starts where the section changes or its bars start over (a repeat).
fn runs(bars: &[BarTime]) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    let mut last: Option<(usize, usize, u32)> = None;
    for b in bars {
        let same = last.is_some_and(|(sec, index, pass)| {
            sec == b.section && b.index > index && pass == b.pass
        });
        if same {
            if let Some(n) = out.last_mut() {
                *n += 1;
            }
        } else {
            out.push(1);
        }
        last = Some((b.section, b.index, b.pass));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHART: &str = "Song\n#C 120bpm 4/4\n\nIN 2\n1 4\n\nVS 4\n1 4 5 1\n\nCH 2\n4 5\n";

    fn sec(name: &str, start: f64, end: f64) -> Section {
        Section {
            name: name.into(),
            start,
            end,
        }
    }

    #[test]
    fn bars_sit_inside_the_sessions_sections() {
        let sections = [
            sec("Intro", 1.0, 5.0),
            sec("Verse", 5.0, 13.0),
            sec("Chorus", 13.0, 19.0),
        ];
        let e = engrave(CHART, &sections).unwrap();
        assert_eq!(e.timing, Timing::Sections);
        let at: Vec<f64> = e
            .svg
            .match_indices("data-at=\"")
            .map(|(i, m)| {
                let rest = &e.svg[i + m.len()..];
                rest[..rest.find('"').unwrap()].parse().unwrap()
            })
            .collect();
        // Two intro bars in 4 s, four verse bars in 8 s, two chorus bars in 6 s.
        let bars: Vec<f64> = at.iter().copied().filter(|t| *t >= 1.0).collect();
        for want in [1.0, 3.0, 5.0, 7.0, 9.0, 11.0, 13.0, 16.0] {
            assert!(
                bars.iter().any(|t| (t - want).abs() < 0.01),
                "{want} in {bars:?}"
            );
        }
    }

    #[test]
    fn a_different_form_keeps_the_charts_tempo() {
        let e = engrave(CHART, &[sec("Intro", 2.0, 6.0)]).unwrap();
        assert_eq!(e.timing, Timing::Tempo);
        assert!(e.svg.contains("data-at=\"2"), "bar 1 at the first section");
    }

    /// Real charts against their sessions: `STUDIO_CHART=… STUDIO_SESSION_FILE=…`.
    #[test]
    #[ignore]
    fn real_chart() {
        let chart = std::fs::read_to_string(std::env::var("STUDIO_CHART").unwrap()).unwrap();
        let t = crate::session_file::read(
            &std::fs::read(std::env::var("STUDIO_SESSION_FILE").unwrap()).unwrap(),
            120.0,
        )
        .unwrap();
        let parsed = keyflow::text::chart::parse_chart(&chart).unwrap();
        let r = runs(&bar_times(&parsed, BarClock::at(0.0)));
        let e = engrave(&chart, &t.sections).unwrap();
        std::fs::write(
            std::env::temp_dir().join("studio-chart.txt"),
            format!(
                "chart runs {} {:?}\nsession {} {:?}\ntiming {:?}\n",
                r.len(),
                r,
                t.sections.len(),
                t.sections
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>(),
                e.timing
            ),
        )
        .unwrap();
    }
}
