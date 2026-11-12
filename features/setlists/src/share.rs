//! A setlist's share page, as an Architect node draws it: the blocks
//! Studio's notes claim, drawn at build time into what Architect's page
//! player follows.
//!
//! - **A song's chart** (```` ```kf ````, [`crate::song::CHART_FENCE`]):
//!   the chart engraved as the PDF prints it, its bars timed on the
//!   chart's own clock ([`crate::follow`]), tied to the song's recording
//!   (`data-media`). As the recording plays, the page marks the bar being
//!   played and keeps it in view; a click on a bar seeks there.
//! - **A set's songs** (```` ```studio-setlist ````,
//!   [`crate::setlist::SETLIST_FENCE`]): each song a row that plays its
//!   recording from the top (and is marked while it plays) with a link to
//!   its page, and a "Play this set" at the head; the last set's ends with
//!   the link's files page and "Download all" (the node's `_files/` and
//!   `_archive.zip` routes of the link: every chart PDF, part and
//!   recording the link's audience may have, the node deciding which).
//!
//! The rest of the page is Architect's: a song's recording embedded as a
//! player, its LRC (or the LRC made from its chart's lyric lane) drawn as
//! lines that follow, the link's queue (every song's recording in running
//! order, the next starting when one ends), the files page and "Download
//! all".
//!
//! What is drawn goes through Architect's sanitiser like any block's
//! (`architect_sites::sanitize_block`): it keeps the SVG, its `data-*` and
//! its flat stylesheet, and every URL here is relative, so nothing loads
//! from elsewhere. A node draws these blocks when it is given
//! [`blocks`] (`Distribution::block`, or `node::start_with`); where none
//! does, the fences read as text (an installed process extension cannot
//! draw them on a node yet: FastTrackStudios/architect#547).
//!
//! **URLs.** A block knows the vault path of its note, not where the site
//! is mounted, so it links relative to its page: a note
//! `A/B/C.md` is published at `<base>/a/b/c/`, a file at
//! `<base>/<its vault path, %-encoded>` (what the node's queue names it,
//! so a block's `data-media` and the queue agree).

use std::fmt::Write as _;

use architect_sites::{
    Block, Blocks, Claim, Drawn, FnBlock, encode_path, fragment_id, slug_segment,
};

use crate::follow::follow_chart;
use crate::setlist::{Line, SETLIST_FENCE};
use crate::song::{CHART_FENCE, unescape_info};

/// Who draws: what a drawn block is marked with (`data-by`).
pub const NAME: &str = "studio.setlists";

/// The renderers of Studio's blocks, for a node or a site build.
#[must_use]
pub fn blocks() -> Blocks {
    Blocks::new().with(FnBlock::new(NAME, &[CHART_FENCE, SETLIST_FENCE], draw))
}

fn draw(block: &Block<'_>) -> Result<Drawn, String> {
    match block.claim {
        Claim::Fence(lang) if lang.eq_ignore_ascii_case(CHART_FENCE) => chart(block),
        Claim::Fence(lang) if lang.eq_ignore_ascii_case(SETLIST_FENCE) => {
            Ok(Drawn::Html(set(block)))
        }
        other => Err(format!("{NAME} does not draw {other}")),
    }
}

/// The folder a vault path is in (`A/B` for `A/B/C.md`; empty at the top).
fn folder(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// From page `page` (a note's vault path) up to the site's base.
fn up(page: &str) -> String {
    let depth = page.split('/').filter(|p| !p.is_empty()).count();
    "../".repeat(depth)
}

/// The URL of file `file` (a vault path) from page `page`.
#[must_use]
pub fn file_url(page: &str, file: &str) -> String {
    format!("{}{}", up(page), encode_path(file))
}

/// The URL of note `note` (a vault path) from page `page`.
#[must_use]
pub fn note_url(page: &str, note: &str) -> String {
    let stem = note.strip_suffix(".md").unwrap_or(note);
    let mut url = up(page);
    for part in stem.split('/').filter(|p| !p.is_empty()) {
        url.push_str(&slug_segment(part));
        url.push('/');
    }
    url
}

/// A chart fence: its source engraved to follow its recording, in a box
/// that scrolls it (so the page keeps the bar being played in view
/// without moving itself).
fn chart(block: &Block<'_>) -> Result<Drawn, String> {
    let recording = block
        .info
        .split_whitespace()
        .skip(1)
        .find_map(|word| word.strip_prefix("media="))
        .map(unescape_info)
        .or_else(|| header_media(block.source));
    let media = recording.map(|r| {
        let dir = folder(block.page);
        let path = if dir.is_empty() {
            r
        } else {
            format!("{dir}/{r}")
        };
        file_url(block.page, &path)
    });
    let chart = follow_chart(block.source, media.as_deref()).map_err(|e| e.to_string())?;
    // The song's now-playing view (Architect's `[data-now-playing]`): what
    // a setlist's player shows while its recording plays.
    let mut view = String::from("<div class=\"studio-now\" data-now-playing=\"\"");
    if let Some(media) = &media {
        let _ = write!(view, " data-media=\"{}\"", escape(media));
    }
    let _ = write!(
        view,
        " style=\"display:flex;flex-direction:column;height:100%\"><div class=\"studio-chart\" style=\"flex:1;min-height:0;max-height:85vh;overflow-y:auto;background:#ffffff;border-radius:6px\">{}</div></div>",
        chart.svg
    );
    Ok(Drawn::Html(view))
}

/// The recording the chart's own header names (`\media = "<file>"`),
/// relative to its folder.
fn header_media(source: &str) -> Option<String> {
    keyflow::parse(source).ok()?.settings.media
}

/// The play icon of a playlist's Play (Architect's player draws its own).
const PLAY_ICON: &str = "<svg viewBox=\"0 0 24 24\" width=\"18\" height=\"18\" aria-hidden=\"true\"><path d=\"M7 4.5v15L20 12z\" fill=\"currentColor\"/></svg>";

/// `m:ss` of `seconds`.
fn length(seconds: f64) -> String {
    let whole = seconds.max(0.0).round();
    let minutes = (whole / 60.0).floor();
    format!("{minutes:.0}:{:02.0}", minutes.mul_add(-60.0, whole))
}

/// A set's fence, as a playlist (Architect's `[data-playlist]`): its Play
/// (the first set's the big one: it plays the setlist through), then each
/// song a row (`[data-track]`, its recording and its page, where its chart
/// is the now-playing view): number, title, artist · key · tempo · meter,
/// length, and a link to its page. A tap on a row plays it and the page
/// stays; the songs follow each other without a page load.
fn set(block: &Block<'_>) -> String {
    let lines: Vec<Line> = block.source.lines().filter_map(Line::parse).collect();
    let page = block.page;
    let first_set = lines.first().is_some_and(|l| l.number == 1);
    let mut html = String::from("<div class=\"studio-setlist\" data-playlist=\"\">");
    if let Some(first) = lines.iter().find_map(|l| l.recording.as_deref()) {
        let (label, style) = if first_set {
            (
                "Play",
                "display:inline-flex;align-items:center;gap:8px;margin:4px 0 12px;padding:12px 28px;border-radius:999px;border:0;background:#22c55e;color:#04210f;font:inherit;font-size:16px;font-weight:700;cursor:pointer",
            )
        } else {
            (
                "Play this set",
                "display:inline-flex;align-items:center;gap:6px;margin:0 0 8px;padding:8px 16px;border-radius:999px;border:1px solid currentColor;background:transparent;color:inherit;font:inherit;font-weight:600;cursor:pointer",
            )
        };
        // Also a follow box whose one line is the first song's start: an
        // Architect without playlists (v0.12.2) plays it from there.
        let _ = write!(
            html,
            "<div data-follow-playhead=\"\" data-media=\"{}\"><button type=\"button\" data-playlist-play=\"\" data-at=\"0\" style=\"{style}\">{PLAY_ICON}{label}</button></div>",
            escape(&file_url(page, first))
        );
    }
    html.push_str("<ol style=\"list-style:none;margin:0;padding:0\">");
    for line in &lines {
        let mut href = note_url(page, &line.note);
        if let Some(a) = &line.arrangement {
            let _ = write!(href, "#{}", fragment_id(a));
        }
        let mut about: Vec<&str> = Vec::new();
        if let Some(artist) = &line.artist {
            about.push(artist);
        }
        if !line.summary.is_empty() {
            about.push(&line.summary);
        }
        let about = about.join(" · ");
        match &line.recording {
            Some(recording) => {
                // The row is a follow box too, its song's start its one
                // line: without playlists, a tap on it plays from there.
                let _ = write!(
                    html,
                    "<li data-track=\"\" data-follow-playhead=\"\" data-media=\"{0}\" data-page=\"{1}\" data-title=\"{2}\" data-subtitle=\"{3}\" style=\"display:flex;align-items:center;gap:12px;padding:10px 8px;border-radius:8px;min-height:44px\"><span data-at=\"0\" style=\"flex:1;min-width:0;display:flex;align-items:center;gap:12px;cursor:pointer\">",
                    escape(&file_url(page, recording)),
                    escape(&href),
                    escape(&line.label),
                    escape(&about),
                );
            }
            None => html.push_str(
                "<li style=\"display:flex;align-items:center;gap:12px;padding:10px 8px;min-height:44px;opacity:0.55\"><span style=\"flex:1;min-width:0;display:flex;align-items:center;gap:12px\">",
            ),
        }
        let _ = write!(
            html,
            "<span data-track-mark=\"\" style=\"flex:none;width:22px;text-align:right;font-variant-numeric:tabular-nums;opacity:0.6\">{}</span><span style=\"flex:1;min-width:0;display:flex;flex-direction:column;gap:2px\"><span data-track-title=\"\" style=\"font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap\">{}</span><span style=\"font-size:13px;opacity:0.7;overflow:hidden;text-overflow:ellipsis;white-space:nowrap\">{}</span></span>",
            line.number,
            escape(&line.label),
            if line.recording.is_none() && about.is_empty() {
                "No recording".to_owned()
            } else if line.recording.is_none() {
                format!("{} · no recording", escape(&about))
            } else {
                escape(&about)
            },
        );
        if let Some(l) = line.length {
            let _ = write!(
                html,
                "<span style=\"flex:none;font-size:13px;font-variant-numeric:tabular-nums;opacity:0.7\">{}</span>",
                length(l)
            );
        }
        let _ = write!(
            html,
            "</span><a href=\"{}\" style=\"flex:none;padding:8px 4px;font-size:13px;opacity:0.8;white-space:nowrap\">Chart</a></li>",
            escape(&href)
        );
    }
    html.push_str("</ol>");
    if block.info.split_whitespace().any(|w| w == "downloads") {
        let root = up(page);
        let _ = write!(
            html,
            "<p style=\"margin:16px 0 0\"><a href=\"{root}_files/\">Files</a> · <a href=\"{root}_archive.zip\">Download all</a></p>"
        );
    }
    html.push_str("</div>");
    html
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
    fn urls_are_relative_to_the_page() {
        let page = "Libraries/Christmas/Silent Night/Silent Night.md";
        assert_eq!(
            file_url(page, "Libraries/Christmas/Silent Night/Media/Take 1.ogg"),
            "../../../../Libraries/Christmas/Silent%20Night/Media/Take%201.ogg"
        );
        assert_eq!(
            note_url("Setlists/Eve.md", page),
            "../../libraries/christmas/silent-night/silent-night/"
        );
    }
}
