//! A song's chart, engraved to follow its recording: Keyflow's SVG with a
//! time on every bar (`engraver::export::follow`), the times taken from
//! the song's session so the chart keeps to the recording the player plays.
//!
//! The session knows where each section starts and ends in the recording
//! (its regions); the chart knows how many bars each section has. The two
//! need not list the same sections: a chart may leave out an intro the
//! session has, or write as one section (`BR 16`) what the session keeps
//! as two (Bridge 1, Bridge 2). So they are aligned by length: each chart
//! section takes one or more consecutive session sections of the same
//! length, and a session section the chart does not have is passed over.
//! Each bar then sits inside its sections evenly — a tempo change, a held
//! chord or a slower bridge stay in step. When nothing aligns, the
//! chart's own tempo runs from the session's first section.

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
    /// Where the chart's form and the session's differ, said plainly
    /// ("The chart has no Intro").
    pub notes: Vec<String>,
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
    let names: Vec<String> = chart.sections.iter().map(section_name).collect();
    let fitted = fit(&nominal, sections, &names);
    let (measured, notes, timing) = match fitted {
        Some((m, notes)) => (Some(m), notes, Timing::Sections),
        None => (None, Vec::new(), Timing::Tempo),
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
        notes,
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

/// A chart section's name (`Verse 1`, `Chorus`).
fn section_name(s: &keyflow::ChartSection) -> String {
    s.section.display_name()
}

/// One section of the chart as it is played: which chart section, how
/// many bars, and how long at the chart's tempo.
struct Run {
    section: usize,
    bars: usize,
    secs: f64,
}

/// What a step of the alignment did.
#[derive(Clone, Copy)]
enum Step {
    /// Chart run `i` took session sections `j..k`.
    Take { i: usize, j: usize, k: usize },
    /// The session's section `j` is not in the chart.
    PassSession(usize),
    /// Chart run `i` is not in the session.
    PassChart(usize),
}

/// The most session sections one chart section may stand for.
const MERGE: usize = 4;
/// What leaving a section out costs, against a length off by `err`
/// (0 to 1): a section the chart does not have is cheaper to leave out
/// than to fold into its neighbour, unless the lengths agree.
const PASS: f64 = 0.25;
/// What each extra session section one chart section stands for costs:
/// an exact split (BR 16 = Bridge 1 + Bridge 2) still wins, a fold that
/// only roughly fits does not.
const JOIN: f64 = 0.1;

/// Each played bar's start, the chart aligned to the session by length,
/// and what differed. `None` when too little of it lines up.
fn fit(
    bars: &[BarTime],
    sections: &[Section],
    names: &[String],
) -> Option<(Vec<f64>, Vec<String>)> {
    let runs = runs(bars);
    let (r, n) = (runs.len(), sections.len());
    if r == 0 || n == 0 {
        return None;
    }
    let err = |a: f64, b: f64| (a - b).abs() / a.max(b).max(1e-6);
    // cost[i][j]: the best alignment of the first i runs and j sections.
    let mut cost = vec![vec![f64::INFINITY; n + 1]; r + 1];
    let mut back: Vec<Vec<Option<Step>>> = vec![vec![None; n + 1]; r + 1];
    cost[0][0] = 0.0;
    for i in 0..=r {
        for j in 0..=n {
            let here = cost[i][j];
            if !here.is_finite() {
                continue;
            }
            let mut relax = |i2: usize, j2: usize, c: f64, step: Step| {
                if c < cost[i2][j2] {
                    cost[i2][j2] = c;
                    back[i2][j2] = Some(step);
                }
            };
            if j < n {
                relax(i, j + 1, here + PASS, Step::PassSession(j));
            }
            if i < r {
                relax(i + 1, j, here + PASS, Step::PassChart(i));
                for k in j + 1..=(j + MERGE).min(n) {
                    let len = sections[k - 1].end - sections[j].start;
                    let e = err(runs[i].secs, len) + JOIN * (k - j - 1) as f64;
                    relax(i + 1, k, here + e, Step::Take { i, j, k });
                }
            }
        }
    }
    // Walk back.
    let mut steps = Vec::new();
    let (mut i, mut j) = (r, n);
    while i > 0 || j > 0 {
        let step = back[i][j]?;
        steps.push(step);
        match step {
            Step::Take { j: j0, .. } => {
                i -= 1;
                j = j0;
            }
            Step::PassSession(_) => j -= 1,
            Step::PassChart(_) => i -= 1,
        }
    }
    steps.reverse();
    let taken: Vec<(usize, usize, usize)> = steps
        .iter()
        .filter_map(|s| match s {
            Step::Take { i, j, k } => Some((*i, *j, *k)),
            _ => None,
        })
        .collect();
    // Most of the chart has to land on sections of its own length.
    let good = taken
        .iter()
        .filter(|(i, j, k)| err(runs[*i].secs, sections[*k - 1].end - sections[*j].start) < 0.15)
        .count();
    if good * 2 < r {
        return None;
    }
    let mut starts = vec![None; r];
    let mut notes = Vec::new();
    for s in &steps {
        match *s {
            Step::Take { i, j, k } => {
                let (a, b) = (sections[j].start, sections[k - 1].end);
                starts[i] = Some((a, (b - a) / runs[i].bars as f64));
                if k - j > 1 {
                    let parts: Vec<&str> = sections[j..k].iter().map(|x| x.name.as_str()).collect();
                    notes.push(format!(
                        "The chart's {} is the session's {}",
                        names.get(runs[i].section).map_or("section", String::as_str),
                        parts.join(" + ")
                    ));
                }
            }
            Step::PassSession(j) => notes.push(format!(
                "The chart has no {} (the session's {} to {})",
                sections[j].name,
                crate::songs::clock(sections[j].start),
                crate::songs::clock(sections[j].end)
            )),
            Step::PassChart(i) => notes.push(format!(
                "The session has no {} the chart plays",
                names.get(runs[i].section).map_or("section", String::as_str)
            )),
        }
    }
    // A chart section the session has none of plays at the chart's tempo
    // from where the last one ended.
    let mut out = Vec::with_capacity(bars.len());
    let mut t = sections[0].start;
    for (i, run) in runs.iter().enumerate() {
        let (start, each) = starts[i].unwrap_or((t, run.secs / run.bars as f64));
        for b in 0..run.bars {
            out.push(start + each * b as f64);
        }
        t = start + each * run.bars as f64;
    }
    Some((out, notes))
}

/// Each section as it is played, in order: a new one starts where the
/// chart's section changes or its bars start over (a repeat).
fn runs(bars: &[BarTime]) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    let mut last: Option<(usize, usize, u32)> = None;
    for b in bars {
        let same = last.is_some_and(|(sec, index, pass)| {
            sec == b.section && b.index > index && pass == b.pass
        });
        match out.last_mut() {
            Some(r) if same => {
                r.bars += 1;
                r.secs += b.end - b.start;
            }
            _ => out.push(Run {
                section: b.section,
                bars: 1,
                secs: b.end - b.start,
            }),
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
    fn a_missing_intro_and_merged_sections_still_line_up() {
        // The chart starts at the verse and writes the chorus as one 2-bar
        // section; the session has an intro and splits the chorus in two.
        let chart = "Song\n#C 120bpm 4/4\n\nVS 4\n1 4 5 1\n\nCH 2\n4 5\n";
        let sections = [
            sec("Intro", 0.0, 4.0),
            sec("Verse", 4.0, 12.0),
            sec("Chorus 1", 12.0, 14.0),
            sec("Chorus 2", 14.0, 16.0),
        ];
        let e = engrave(chart, &sections).unwrap();
        assert_eq!(e.timing, Timing::Sections);
        assert_eq!(e.notes.len(), 2, "{:?}", e.notes);
        assert!(
            e.notes[0].starts_with("The chart has no Intro"),
            "{:?}",
            e.notes
        );
        assert!(e.notes[1].contains("Chorus 1 + Chorus 2"), "{:?}", e.notes);
        assert!(
            e.svg.contains("data-at=\"4\"") || e.svg.contains("data-at=\"4.0"),
            "verse at 4 s"
        );
        assert!(e.svg.contains("data-at=\"14"), "second chorus bar at 14 s");
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
        let r: Vec<usize> = runs(&bar_times(&parsed, BarClock::at(0.0)))
            .iter()
            .map(|r| r.bars)
            .collect();
        let e = engrave(&chart, &t.sections).unwrap();
        std::fs::write(
            std::env::temp_dir().join("studio-chart.txt"),
            format!(
                "chart runs {} {:?}\nsession {} {:?}\ntiming {:?}\nnotes {:?}\n",
                r.len(),
                r,
                t.sections.len(),
                t.sections
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>(),
                e.timing,
                e.notes
            ),
        )
        .unwrap();
    }
}
