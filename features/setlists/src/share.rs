//! A setlist's share page, as an Architect node draws it: the blocks
//! Studio's notes claim, drawn at build time into what Architect's page
//! player follows.
//!
//! - **A song's chart** (```` ```kf ````, [`crate::song::CHART_FENCE`]):
//!   the chart engraved as the PDF prints it, its bars timed on the
//!   chart's own clock ([`crate::follow`]), tied to the song's recording
//!   (`data-media`). As the recording plays, the page marks the bar being
//!   played and keeps it in view; a click on a bar seeks there.
//! - **A setlist's songs** (```` ```studio-setlist ````,
//!   [`crate::setlist::SETLIST_FENCE`]): one list, as a streaming app's
//!   playlist: each set's name a quiet label in it, each song a row (its
//!   number, its art's tile, its title over who it is by, its length, a way
//!   to its page) that plays its recording where the page is (and is marked
//!   while it plays); it ends with the link's files page and "Download all"
//!   (the node's `_files/` and `_archive.zip` routes of the link: every
//!   chart PDF, part and recording the link's audience may have, the node
//!   deciding which). A page written with a fence a set (under `##`
//!   headings) is joined into one list by the page's player.
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
pub use crate::row::{RowKey, row_key, signed, tile_hue};
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
    // Its sections as played, for a progress bar over it (Studio's
    // island, #445): `[{name, start, end, color}]`, seconds.
    let sections: Vec<serde_json::Value> = chart
        .sections
        .iter()
        .map(|p| serde_json::json!({ "name": p.name, "start": p.start, "end": p.end, "color": p.color }))
        .collect();
    let _ = write!(
        view,
        " data-sections=\"{}\"",
        escape(&serde_json::Value::Array(sections).to_string())
    );
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

/// A row's menu button: three dots, drawn by its own attributes.
const MORE_ICON: &str = "<svg viewBox=\"0 0 24 24\" width=\"20\" height=\"20\" fill=\"currentColor\" aria-hidden=\"true\"><circle cx=\"5\" cy=\"12\" r=\"2\"/><circle cx=\"12\" cy=\"12\" r=\"2\"/><circle cx=\"19\" cy=\"12\" r=\"2\"/></svg>";

/// A tile's glyph for a song with neither art nor key: Lucide's music note
/// (the page's icon set), never letters.
const NOTE_ICON: &str = "<svg viewBox=\"0 0 24 24\" width=\"22\" height=\"22\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M9 18V5l12-2v13\"/><circle cx=\"6\" cy=\"18\" r=\"3\"/><circle cx=\"18\" cy=\"16\" r=\"3\"/></svg>";

/// A row's tile, on a colour from the song's title (the page's player lays
/// its art under, dimmed): the song's key, the only text on it, and in its
/// corner by how far the setlist moves it (`+4`); with no key, a music
/// note.
fn tile(line: &Line, hue: u32) -> String {
    let Some(key) = row_key(line) else {
        return format!(
            "<span data-track-art=\"\" data-hue=\"{hue}\" aria-hidden=\"true\" style=\"background:hsl({hue} 42% 40%)\">{NOTE_ICON}</span>"
        );
    };
    let said = key.spoken();
    let mark = key
        .moved
        .as_ref()
        .map_or_else(String::new, |(_, semitones)| {
            format!(
                "<span data-track-transposed=\"\" aria-hidden=\"true\">{}</span>",
                signed(*semitones)
            )
        });
    let size = key.label.size();
    let size = if size.is_empty() {
        String::new()
    } else {
        format!(" data-key-size=\"{size}\"")
    };
    format!(
        "<span data-track-art=\"\" data-hue=\"{hue}\" data-key=\"\"{size} role=\"img\" aria-label=\"{}\" style=\"background:hsl({hue} 42% 40%)\"><span data-track-key=\"\" aria-hidden=\"true\">{}</span>{mark}</span>",
        escape(&said),
        escape(&key.label.text()),
    )
}

/// A setlist's fence, as one playlist (Architect's `[data-playlist]`):
/// each set's name a section label (`[data-playlist-section]`, no track),
/// each song a row: its number, its tile (its art, when the link has it,
/// laid over it by the page's player), its title over who it is by and
/// its key, tempo and meter, its length, and a way to its page (its
/// chart). A row with a recording is a track (`[data-track]`: a tap plays
/// it and the page stays); one without is in its place, quieter, and opens
/// its page. The page's player and sheet draw the rest.
fn set(block: &Block<'_>) -> String {
    let page = block.page;
    // Its list's own reset, inline: a player that styles no playlist (an
    // Architect before this) still draws it as rows, not a numbered list.
    let mut html = String::from(
        "<div class=\"studio-setlist\" data-playlist=\"\" data-playlist-join=\"\"><ol style=\"list-style:none;margin:0;padding:0\">",
    );
    for text in block.source.lines() {
        if let Some(name) = crate::setlist::section_line(text) {
            let _ = write!(html, "<li data-playlist-section=\"\">{}</li>", escape(name));
            continue;
        }
        let Some(line) = Line::parse(text) else {
            continue;
        };
        let mut href = note_url(page, &line.note);
        if let Some(a) = &line.arrangement {
            let _ = write!(href, "#{}", fragment_id(a));
        }
        // Who it is by, and nothing else: the key is on its art, its tempo
        // and meter on its chart.
        let about = line.artist.clone().unwrap_or_default();
        let meta = match (&line.recording, about.is_empty()) {
            (Some(_), _) => about.clone(),
            (None, true) => "No recording".to_owned(),
            (None, false) => format!("{about} · No recording"),
        };
        match &line.recording {
            Some(recording) => {
                let _ = write!(
                    html,
                    "<li data-track=\"\" data-media=\"{}\" data-page=\"{}\" data-title=\"{}\" data-subtitle=\"{}\">",
                    escape(&file_url(page, recording)),
                    escape(&href),
                    escape(&line.label),
                    escape(&about),
                );
            }
            None => {
                let _ = write!(
                    html,
                    "<li data-page=\"{}\" data-title=\"{}\">",
                    escape(&href),
                    escape(&line.label),
                );
            }
        }
        let hue = tile_hue(&line.label);
        let _ = write!(
            html,
            "<span data-track-mark=\"\">{}</span>{}<span data-track-text=\"\"><span data-track-title=\"\">{}</span><span data-track-meta=\"\">{}</span></span>",
            line.number,
            tile(&line, hue),
            escape(&line.label),
            escape(&meta),
        );
        // Its menu (Architect's player fills it: its chart, its lyrics, its
        // page and downloads).
        let _ = write!(
            html,
            "<button type=\"button\" data-track-more=\"\" aria-label=\"More: {}\" title=\"More\">{MORE_ICON}</button></li>",
            escape(&line.label),
        );
    }
    html.push_str("</ol>");
    if block.info.split_whitespace().any(|w| w == "downloads") {
        let root = up(page);
        let _ = write!(
            html,
            "<p data-playlist-files=\"\"><a href=\"{root}_files/\">Files</a> · <a href=\"{root}_archive.zip\">Download all</a></p>"
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
