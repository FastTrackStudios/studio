//! A chart a share page follows: its engraving as one SVG whose bars carry
//! the time each is played, and its timed lyric lane as LRC.
//!
//! **The contract** is Architect's page player
//! (`architect-sites`' `assets/player.js`, Keyflow's
//! `engraver::export::follow`): a box marked `data-follow-playhead` (and
//! `data-media`, the recording its times belong to) whose `[data-at]`
//! elements (seconds, in ascending document order) are marked
//! `aria-current="time"` at the playhead and seek there on a click. Here
//! the box is the chart's own `<svg>`, and each bar a transparent
//! `<rect class="kf-bar" data-at>` over it, one per time it is played.
//!
//! **The clock** is the chart's own: its tempo, tempo changes and meters,
//! bar 1 at its `\downbeat` (Keyflow's [`bar_times`]), so the bars, the
//! lyric lane and the recording named by `\media` (or the song's) agree.
//!
//! **What is drawn** is the engraving the PDF prints (A4, the Master Rhythm
//! preset), its pages stacked top to bottom in one SVG so the bars stay in
//! playing order across pages. Its text is drawn as outlines: a page that
//! shows a block cannot load the engraver's fonts (Architect's sanitiser
//! keeps no `@font-face`, and a share's CSP loads fonts from its own origin
//! only), so the glyphs travel as paths and the chart reads the same
//! anywhere.

use std::fmt::Write as _;

use keyflow::engraver::export::follow::{BarBox, bar_boxes};
use keyflow::text::bar_times::{BarClock, BarTime, bar_times};
use resvg::usvg;

use crate::chart::{fontdb, layout};
use crate::{Error, Result};

/// The space between two stacked pages, in points.
const PAGE_GAP: f64 = 16.0;

/// The marks' look, as Keyflow's follow export writes it: invisible until
/// current; a click anywhere on a bar lands on it. Flat rules, no quotes:
/// what Architect's sanitiser keeps (scoped to the block).
const STYLE: &str = "<style>.kf-bar{fill:#f5a623;fill-opacity:0;pointer-events:all;cursor:pointer}.kf-bar[aria-current=time]{fill-opacity:0.22}</style>";

/// A chart engraved to follow its recording.
#[derive(Debug, Clone)]
pub struct FollowChart {
    /// The SVG: every page, one `kf-bar` rect per bar played.
    pub svg: String,
    /// Every bar in playing order, as timed in `svg`.
    pub bars: Vec<BarTime>,
    /// How many pages it has.
    pub pages: usize,
}

/// The chart's clock: bar 1 at its `\downbeat` (0 without one).
fn clock(chart: &keyflow::Chart) -> BarClock<'static> {
    BarClock::at(chart.settings.downbeat.unwrap_or(0.0))
}

/// Chart `source` engraved to follow the recording at `media` (a URL, as
/// the page that shows it reaches it; None: whatever its page plays).
///
/// # Errors
///
/// It does not parse, or a page did not draw.
pub fn follow_chart(source: &str, media: Option<&str>) -> Result<FollowChart> {
    let chart = keyflow::parse(source).map_err(|e| Error::Engrave(e.to_string()))?;
    let (pipeline, layout) = layout(&chart)?;
    let bars = bar_times(&chart, clock(&chart));
    let pages = pipeline.export_svg_pages_linked(&layout);
    let (width, height) = layout
        .pages
        .first()
        .map_or((595.28, 841.89), |p| (p.width, p.height));
    let pitch = height + PAGE_GAP;
    let count = pages.len().max(1);
    let total = pitch.mul_add(f64::from(u32::try_from(count).unwrap_or(1)), -PAGE_GAP);

    let mut svg = String::with_capacity(pages.len().saturating_mul(140_000));
    let _ = write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"kf-chart\" viewBox=\"0 0 {width:.2} {total:.2}\" width=\"100%\" style=\"display:block;width:100%;height:auto;max-width:100%\" data-follow-playhead=\"\""
    );
    if let Some(media) = media {
        let _ = write!(svg, " data-media=\"{}\"", escape(media));
    }
    svg.push('>');
    for (n, page) in (0_u32..).zip(&pages) {
        let outlined = outline(page, n)?;
        let _ = write!(
            svg,
            "<g class=\"kf-page\" transform=\"translate(0 {:.2})\">{outlined}</g>",
            pitch * f64::from(n)
        );
    }
    svg.push_str("<g class=\"kf-follow\">");
    svg.push_str(STYLE);
    let boxes = bar_boxes(&layout);
    let offsets: Vec<(f64, f64)> = layout
        .pages
        .iter()
        .map(|p| (p.x_offset, p.y_offset))
        .collect();
    for bar in &bars {
        let Some(b) = boxes.get(&bar.bar) else {
            continue;
        };
        let Some((x, y)) = place(b, &offsets, pitch) else {
            continue;
        };
        let _ = write!(
            svg,
            "<rect class=\"kf-bar\" data-at=\"{:.3}\" data-bar=\"{}\" data-pass=\"{}\" x=\"{x:.2}\" y=\"{y:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"2\"/>",
            bar.start,
            bar.bar.saturating_add(1),
            bar.pass,
            b.width,
            b.height,
        );
    }
    svg.push_str("</g></svg>");
    Ok(FollowChart {
        svg,
        bars,
        pages: pages.len(),
    })
}

/// Where bar box `b` is in the stacked SVG: its page's offset in the
/// engraver's scene taken off, its page's place in the stack put on.
fn place(b: &BarBox, offsets: &[(f64, f64)], pitch: f64) -> Option<(f64, f64)> {
    let index = b.page.checked_sub(1)?;
    let (ox, oy) = *offsets.get(usize::try_from(index).ok()?)?;
    Some((b.x - ox, pitch.mul_add(f64::from(index), b.y - oy)))
}

/// Page `svg` with its text drawn as outlines, as the inside of an `<svg>`
/// in the page's own coordinates (its top left at 0,0). Its ids are made
/// the page's own (`kfp<n>-`), so stacked pages never share one.
fn outline(svg: &str, page: u32) -> Result<String> {
    let options = usvg::Options {
        fontdb: fontdb()?,
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_str(svg, &options).map_err(|e| Error::Engrave(e.to_string()))?;
    let written = tree.to_string(&usvg::WriteOptions {
        id_prefix: Some(format!("kfp{page}-")),
        preserve_text: false,
        coordinates_precision: 2,
        transforms_precision: 3,
        ..usvg::WriteOptions::default()
    });
    // What is inside its root element.
    let open = written.find("<svg").and_then(|at| {
        let end = written.get(at..)?.find('>')?;
        Some(at.saturating_add(end).saturating_add(1))
    });
    let close = written.rfind("</svg>");
    match (open, close) {
        (Some(open), Some(close)) if open <= close => Ok(written
            .get(open..close)
            .unwrap_or_default()
            .trim()
            .to_owned()),
        _ => Err(Error::Engrave("a page drew no SVG".to_owned())),
    }
}

/// The chart's timed lyric lanes as LRC: a line per sung line, at the time
/// its first word is sung on the chart's clock. None when it has no timed
/// lane.
///
/// A lane places words on bars and beats of its section; a section played
/// twice (a repeat) sings its lane each time. A line ends where the source
/// line does, or where the singing rests for more than a bar.
///
/// # Errors
///
/// It does not parse.
pub fn chart_lrc(source: &str) -> Result<Option<String>> {
    let chart = keyflow::parse(source).map_err(|e| Error::Engrave(e.to_string()))?;
    let bars = bar_times(&chart, clock(&chart));
    let mut lines: Vec<(f64, String)> = Vec::new();
    for (s, section) in chart.sections.iter().enumerate() {
        let Some(lane) = section
            .lyrics_track()
            .and_then(|t| t.lyrics.as_ref())
            .filter(|l| l.timed)
        else {
            continue;
        };
        let measures = section.measures();
        let beats_in = |index: usize| {
            measures
                .get(index)
                .map_or(4.0, |m| f64::from(m.time_signature.0.max(1)))
        };
        for pass in bars.iter().filter(|b| b.section == s).map(|b| b.pass).fold(
            Vec::new(),
            |mut passes, p| {
                if !passes.contains(&p) {
                    passes.push(p);
                }
                passes
            },
        ) {
            let at = |index: usize, beat: f32| {
                bars.iter()
                    .find(|b| b.section == s && b.index == index && b.pass == pass)
                    .map(|b| b.start + (b.end - b.start) * f64::from(beat) / beats_in(index))
            };
            let mut words = String::new();
            let mut start: Option<f64> = None;
            let mut last_bar: Option<usize> = None;
            let mut joining = false;
            for syllable in &lane.syllables {
                let text = syllable.text.trim();
                let rested = last_bar.is_some_and(|l| syllable.measure_index > l.saturating_add(1));
                if rested && !joining && !words.is_empty() {
                    if let Some(t) = start.take() {
                        lines.push((t, std::mem::take(&mut words)));
                    }
                }
                last_bar = Some(syllable.measure_index);
                if text.is_empty() || text.chars().all(|c| c == '_') {
                    continue;
                }
                if start.is_none() {
                    start = at(syllable.measure_index, syllable.beat);
                }
                if !words.is_empty() && !joining {
                    words.push(' ');
                }
                words.push_str(text.trim_end_matches('-'));
                joining = syllable.hyphen_after || text.ends_with('-');
                if syllable.line_break_after {
                    if let Some(t) = start.take() {
                        lines.push((t, std::mem::take(&mut words)));
                    }
                    joining = false;
                }
            }
            if let Some(t) = start {
                if !words.is_empty() {
                    lines.push((t, words));
                }
            }
        }
    }
    if lines.is_empty() {
        return Ok(None);
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut lrc = String::new();
    if let Some(title) = chart
        .metadata
        .title
        .as_deref()
        .filter(|t| !t.trim().is_empty())
    {
        let _ = writeln!(lrc, "[ti:{}]", title.trim());
    }
    for (at, text) in lines {
        let _ = writeln!(lrc, "{}{text}", stamp(at));
    }
    Ok(Some(lrc))
}

/// `[mm:ss.xx]` for `seconds`.
fn stamp(seconds: f64) -> String {
    // Rounded to the hundredth first, so 59.996 s is 01:00.00.
    let centis = (seconds.max(0.0) * 100.0).round();
    let minutes = (centis / 6000.0).floor();
    let rest = minutes.mul_add(-6000.0, centis) / 100.0;
    format!("[{minutes:02.0}:{rest:05.2}]")
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_read_as_lrc_writes_them() {
        assert_eq!(stamp(0.0), "[00:00.00]");
        assert_eq!(stamp(83.456), "[01:23.46]");
    }
}
