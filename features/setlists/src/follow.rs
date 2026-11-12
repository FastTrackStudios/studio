//! A chart a share page follows: its engraving as one SVG whose bars carry
//! the time each is played, and its timed lyric lane as LRC.
//!
//! **The contract** is Architect's page player
//! (`architect-sites`' `assets/player.js`, Keyflow's
//! `engraver::export::follow`): a box marked `data-follow-playhead` (and
//! `data-media`, the recording its times belong to) whose `[data-at]`
//! elements (seconds, in ascending document order) are marked
//! `aria-current="time"` at the playhead and seek there on a click. Here
//! the boxes are two groups of the chart's own `<svg>` (below), each bar a
//! transparent `<rect class="kf-bar" data-at>` over it, one per time it is
//! played, and each beat a `<rect class="kf-beat" data-at>`.
//!
//! **The clock** is the chart's own: its tempo, tempo changes and meters,
//! bar 1 at its `\downbeat` (Keyflow's [`bar_times`]), so the bars, the
//! lyric lane and the recording named by `\media` (or the song's) agree.
//!
//! **The cursor** is Session's ([`CursorConfig::playback`], the look
//! every chart pane of Studio's draws): a wash over the bar being played
//! and a line at the beat being played. The page player marks one item
//! per follow box, so the bars and the beats are two boxes in the one
//! SVG: `kf-bars` (a rect per bar played, the wash, a click seeks to the
//! bar) and `kf-beats` (a line per beat played, at the x Keyflow's
//! [`ChartCursor`] puts the playhead at that beat's start). A beat's time
//! is its bar's on the chart's clock ([`bar_times`]), split as the layout
//! splits the bar: the beats and the bars are one clock.
//!
//! **What is drawn** is the engraving the PDF prints (A4, the Master Rhythm
//! preset), its pages stacked top to bottom in one SVG so the bars stay in
//! playing order across pages. Its text is drawn as outlines: a page that
//! shows a block cannot load the engraver's fonts (Architect's sanitiser
//! keeps no `@font-face`, and a share's CSP loads fonts from its own origin
//! only), so the glyphs travel as paths and the chart reads the same
//! anywhere. A glyph drawn more than once is drawn once, in `<defs>`, and
//! placed with `<use>` (This Christmas: 11.1 MB of SVG to 2.3 MB).
//!
//! [`ChartCursor`]: keyflow::engraver::layout::chart::cursor::ChartCursor

use std::fmt::Write as _;

use std::collections::HashMap;

use keyflow::engraver::export::follow::bar_boxes;
use keyflow::engraver::layout::chart::cursor::{CursorConfig, CursorStyle};
use keyflow::engraver::layout::chart::types::{BeatPosition, ChartLayoutResult};
use keyflow::text::bar_times::{BarClock, BarTime, bar_times};
use resvg::usvg;

use crate::chart::{fontdb, layout};
use crate::{Error, Result};

/// The space between two stacked pages, in points.
const PAGE_GAP: f64 = 16.0;

/// A path drawn at least this many bytes long is worth a `<use>`.
const SHARED_PATH_MIN: usize = 64;

/// The marks' look: Session's playback cursor ([`CursorConfig::playback`]),
/// invisible until current; a click anywhere on a bar lands on it, a beat's
/// line takes no clicks. Flat rules, no quotes: what Architect's sanitiser
/// keeps (scoped to the block).
fn style(cursor: &CursorConfig) -> String {
    let [r, g, b, a] = cursor.accent_color;
    let line = f64::from(a) / 255.0;
    let wash = line * f64::from(cursor.fill_alpha);
    format!(
        "<style>.kf-bar{{fill:rgb({r},{g},{b});fill-opacity:0;pointer-events:all;cursor:pointer}}.kf-bar[aria-current=time]{{fill-opacity:{wash:.3}}}.kf-beat{{fill:rgb({r},{g},{b});fill-opacity:0;pointer-events:none}}.kf-beat[aria-current=time]{{fill-opacity:{line:.3}}}</style>"
    )
}

/// One beat played: when, and where its line is drawn (page-stacked SVG
/// coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeatMark {
    /// Seconds, on the chart's clock.
    pub at: f64,
    /// The engraver's bar index.
    pub bar: usize,
    /// The beat within its bar (0-based).
    pub beat: usize,
    /// The line: left edge, top, width, height.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A chart engraved to follow its recording.
#[derive(Debug, Clone)]
pub struct FollowChart {
    /// The SVG: every page, one `kf-bar` rect per bar played.
    pub svg: String,
    /// Every bar in playing order, as timed in `svg`.
    pub bars: Vec<BarTime>,
    /// Every beat in playing order, as timed in `svg`.
    pub beats: Vec<BeatMark>,
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
        "<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"kf-chart\" viewBox=\"0 0 {width:.2} {total:.2}\" width=\"100%\" style=\"display:block;width:100%;height:auto;max-width:100%\""
    );
    svg.push('>');
    let mut drawn = String::with_capacity(pages.len().saturating_mul(600_000));
    for (n, page) in (0_u32..).zip(&pages) {
        let outlined = outline(page, n)?;
        let _ = write!(
            drawn,
            "<g class=\"kf-page\" transform=\"translate(0 {:.2})\">{outlined}</g>",
            pitch * f64::from(n)
        );
    }
    let (defs, drawn) = share_paths(&drawn);
    svg.push_str(&defs);
    svg.push_str(&drawn);
    let cursor = CursorConfig::playback();
    let boxes = bar_boxes(&layout);
    let offsets: Vec<(f64, f64)> = layout
        .pages
        .iter()
        .map(|p| (p.x_offset, p.y_offset))
        .collect();
    let follow_box = |class: &str| {
        let mut g = format!("<g class=\"{class}\" data-follow-playhead=\"\"");
        if let Some(media) = media {
            let _ = write!(g, " data-media=\"{}\"", escape(media));
        }
        g.push('>');
        g
    };
    svg.push_str("<g class=\"kf-follow\">");
    svg.push_str(&style(&cursor));
    svg.push_str(&follow_box("kf-bars"));
    for bar in &bars {
        let Some(b) = boxes.get(&bar.bar) else {
            continue;
        };
        let Some((x, y)) = place(b.page, b.x, b.y, &offsets, pitch) else {
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
    svg.push_str("</g>");
    let beats = beat_marks(&layout, &bars, &cursor, &offsets, pitch);
    svg.push_str(&follow_box("kf-beats"));
    for beat in &beats {
        let _ = write!(
            svg,
            "<rect class=\"kf-beat\" data-at=\"{:.3}\" data-bar=\"{}\" data-beat=\"{}\" x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\"/>",
            beat.at,
            beat.bar.saturating_add(1),
            beat.beat.saturating_add(1),
            beat.x,
            beat.y,
            beat.width,
            beat.height,
        );
    }
    svg.push_str("</g></g></svg>");
    Ok(FollowChart {
        svg,
        bars,
        beats,
        pages: pages.len(),
    })
}

/// Where point `x`, `y` of 1-based page `page` of the engraver's scene is
/// in the stacked SVG: its page's offset in the scene taken off, its
/// page's place in the stack put on.
fn place(page: u32, x: f64, y: f64, offsets: &[(f64, f64)], pitch: f64) -> Option<(f64, f64)> {
    let index = page.checked_sub(1)?;
    let (ox, oy) = *offsets.get(usize::try_from(index).ok()?)?;
    Some((x - ox, pitch.mul_add(f64::from(index), y - oy)))
}

/// Every beat played, in playing order.
///
/// Each played bar's beats as the layout draws them, timed on the chart's
/// clock (the bar's start and end from [`bar_times`], split where the
/// layout splits the bar), the line where [`ChartCursor`] draws the
/// playhead at the beat's start, as tall as its line
/// ([`CursorStyle::MeasureWithLine`]).
///
/// [`ChartCursor`]: keyflow::engraver::layout::chart::cursor::ChartCursor
fn beat_marks(
    layout: &ChartLayoutResult,
    bars: &[BarTime],
    cursor: &CursorConfig,
    offsets: &[(f64, f64)],
    pitch: f64,
) -> Vec<BeatMark> {
    let (width, overhang) = match cursor.style {
        CursorStyle::MeasureWithLine {
            line_width,
            overhang,
        } => (line_width, overhang),
        CursorStyle::VerticalLine { width } => (width, 0.0),
        _ => (1.5, 4.0),
    };
    // The bar's beats in layout order (count-in beats, at negative ticks,
    // are not bars).
    let mut by_bar: HashMap<usize, Vec<&BeatPosition>> = HashMap::new();
    for beat in layout
        .beat_positions
        .iter()
        .filter(|b| b.absolute_tick >= 0)
    {
        by_bar.entry(beat.measure).or_default().push(beat);
    }
    let mut marks = Vec::new();
    for bar in bars {
        let Some(beats) = by_bar.get(&bar.bar) else {
            continue;
        };
        let (Some(first), Some(last)) = (beats.first(), beats.last()) else {
            continue;
        };
        let (from, to) = (first.time_start, last.time_end);
        let span = to - from;
        for beat in beats {
            let part = if span > 0.0 {
                ((beat.time_start - from) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let at = (bar.end - bar.start).mul_add(part, bar.start);
            let x = beat.x_at_time(beat.time_start) - width / 2.0;
            let Some((x, y)) = place(beat.page, x, beat.staff_y - overhang, offsets, pitch) else {
                continue;
            };
            marks.push(BeatMark {
                at,
                bar: bar.bar,
                beat: beat.beat,
                x,
                y,
                width,
                height: 2.0f64.mul_add(overhang, beat.staff_height),
            });
        }
    }
    marks
}

/// `drawn` with each path drawn more than once (a glyph: every chord
/// symbol's letters, every slash) moved into `<defs>` once and placed with
/// `<use>`: (the `<defs>`, what is left of `drawn`). A path keeps its other
/// attributes on its `<use>`; its definition is its outline alone.
fn share_paths(drawn: &str) -> (String, String) {
    // Each path element: (where it starts, where it ends, its outline).
    let mut found: Vec<(usize, usize, &str)> = Vec::new();
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut from = 0;
    while let Some(at) = drawn.get(from..).and_then(|r| r.find("<path ")) {
        let start = from.saturating_add(at);
        let Some(close) = drawn.get(start..).and_then(|r| r.find('>')) else {
            break;
        };
        let mut end = start.saturating_add(close).saturating_add(1);
        let tag = drawn.get(start..end).unwrap_or_default();
        from = end;
        let self_closing = tag.ends_with("/>");
        if !self_closing {
            if drawn.get(end..).is_some_and(|r| r.starts_with("</path>")) {
                end = end.saturating_add("</path>".len());
                from = end;
            } else {
                continue;
            }
        }
        let Some(d) = attr(tag, "d") else {
            continue;
        };
        if d.len() < SHARED_PATH_MIN {
            continue;
        }
        let count = seen.entry(d).or_default();
        *count = count.saturating_add(1);
        found.push((start, end, d));
    }
    let mut ids: HashMap<&str, usize> = HashMap::new();
    let mut defs = String::new();
    let mut out = String::with_capacity(drawn.len());
    let mut kept = 0;
    for (start, end, d) in found {
        if seen.get(d).copied().unwrap_or(0) < 2 {
            continue;
        }
        let next = ids.len();
        let id = *ids.entry(d).or_insert_with(|| {
            let _ = write!(defs, "<path id=\"kfg{next}\" d=\"{d}\"/>");
            next
        });
        out.push_str(drawn.get(kept..start).unwrap_or_default());
        let tag = drawn.get(start..end).unwrap_or_default();
        let open = tag.find('>').map_or(tag, |c| tag.get(..c).unwrap_or(tag));
        let rest = open
            .trim_end_matches('/')
            .trim_start_matches("<path")
            .replace(&format!(" d=\"{d}\""), "");
        let _ = write!(out, "<use href=\"#kfg{id}\"{rest}/>");
        kept = end;
    }
    out.push_str(drawn.get(kept..).unwrap_or_default());
    if defs.is_empty() {
        (defs, out)
    } else {
        (format!("<defs>{defs}</defs>"), out)
    }
}

/// The value of attribute `name` in start tag `tag`, as written.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!(" {name}=\"");
    let at = tag.find(&needle)?.saturating_add(needle.len());
    let rest = tag.get(at..)?;
    rest.get(..rest.find('"')?)
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
#[allow(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::arithmetic_side_effects,
    clippy::suboptimal_flops,
    clippy::unwrap_used
)]
mod tests {
    use super::*;

    use keyflow::engraver::layout::chart::cursor::ChartCursor;

    /// A tune in 3/4 at 90 bpm, its first bar at 0.5 s: no repeats, so the
    /// layout's own clock is the chart's less its downbeat.
    const WALTZ: &str = "Waltz - Test\n90bpm 3/4 #G\n\\downbeat = 0.5\n\nVS 8\nG G C D\nG Em C D\n\nCH 4\nC D G G\n";

    #[test]
    fn beats_are_where_keyflows_cursor_puts_the_playhead() {
        let chart = keyflow::parse(WALTZ).unwrap();
        let (_, layout) = layout(&chart).unwrap();
        let drawn = follow_chart(WALTZ, None).unwrap();
        let downbeat = 0.5;
        let beats_per_bar = 3;
        assert_eq!(drawn.beats.len(), drawn.bars.len() * beats_per_bar);
        let cursor = ChartCursor::new(CursorConfig::playback());
        let offsets: Vec<(f64, f64)> = layout
            .pages
            .iter()
            .map(|p| (p.x_offset, p.y_offset))
            .collect();
        let half = 1.5 / 2.0;
        for pair in drawn.beats.windows(2) {
            let (beat, next) = (pair[0], pair[1]);
            // One clock: 90 bpm is 2/3 s a beat, from the downbeat.
            let n = drawn.beats.iter().position(|b| *b == beat).unwrap();
            let want = downbeat + n as f64 * 60.0 / 90.0;
            assert!(
                (beat.at - want).abs() < 1e-6,
                "beat {n}: {} not {want}",
                beat.at
            );
            // Anywhere in the beat, the line is where Keyflow's cursor is at
            // its start, on the same page.
            for part in [0.0, 0.5, 0.99] {
                let t = beat.at + (next.at - beat.at) * part;
                let current = drawn.beats.iter().rev().find(|b| b.at <= t).unwrap();
                assert_eq!(*current, beat);
                let state = cursor.compute_at_time(&layout, beat.at - downbeat).unwrap();
                let (ox, _) = offsets[(state.page - 1) as usize];
                assert!(
                    (current.x + half - (state.cursor_x - ox)).abs() < 1e-6,
                    "beat {n} at {t:.3}s: line at {} , Keyflow's cursor at {}",
                    current.x + half,
                    state.cursor_x - ox
                );
            }
        }
        // Each bar's first beat is the bar's time.
        for bar in &drawn.bars {
            let first = drawn.beats.iter().find(|b| b.bar == bar.bar).unwrap();
            assert!((first.at - bar.start).abs() < 1e-9);
        }
    }

    #[test]
    fn glyphs_drawn_twice_are_drawn_once() {
        let drawn = follow_chart(WALTZ, None).unwrap();
        assert!(
            drawn.svg.contains("<defs><path id=\"kfg0\""),
            "shared glyphs"
        );
        assert!(drawn.svg.contains("<use href=\"#kfg0\""));
        // Every glyph used is defined.
        let mut at = 0;
        while let Some(i) = drawn.svg[at..].find("href=\"#kfg") {
            let start = at + i + "href=\"#".len();
            let end = start + drawn.svg[start..].find('"').unwrap();
            let id = &drawn.svg[start..end];
            assert!(
                drawn.svg.contains(&format!("<path id=\"{id}\"")),
                "{id} defined"
            );
            at = end;
        }
        // Two follow boxes: bars and beats, both tied to the recording.
        let drawn = follow_chart(WALTZ, Some("rec.mp4")).unwrap();
        assert_eq!(drawn.svg.matches("data-follow-playhead").count(), 2);
        assert_eq!(drawn.svg.matches("data-media=\"rec.mp4\"").count(), 2);
    }

    #[test]
    fn stamps_read_as_lrc_writes_them() {
        assert_eq!(stamp(0.0), "[00:00.00]");
        assert_eq!(stamp(83.456), "[01:23.46]");
    }
}
