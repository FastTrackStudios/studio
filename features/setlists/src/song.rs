//! A song: its note (`type: song`) and everything it keeps: its chart,
//! engraved pages and PDF, LRC, session, other media, and arrangements.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::frontmatter::{self, link_target, wikilink};
use crate::layout::{Library, MEDIA};
use crate::lyrics::Block;

pub const KIND: &str = "song";

/// The fence a chart is drawn from on a song's page.
///
/// Its body is the Keyflow source, its info the recording it follows
/// (`kf media=Media/Take%201.ogg`, relative to the song's folder,
/// `%`-escaped). Studio's share renderer
/// draws it as a chart whose bars follow the playhead
/// (`setlists::share`); where nothing draws it, it reads as the chart's
/// text.
pub const CHART_FENCE: &str = "kf";

/// What a song's page shows that its properties do not: read from its
/// files when its note is written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SongPage {
    /// Its lyrics as text, for a song with no timed lyrics.
    pub lyrics: Vec<Block>,
    /// Its own chart's source: drawn as a chart that follows playback.
    pub chart: Option<String>,
    /// Each arrangement's chart source, by the arrangement's name.
    pub arrangements: Vec<(String, String)>,
    /// LRC made from its chart's timed lyric lane, for a song with no LRC
    /// of its own (one source of timed lyrics a song: its LRC, else its
    /// chart's lane).
    pub chart_lrc: Option<String>,
}

/// Whether `name` is audio or video a page plays, by its extension (what
/// Architect's sites draw as a player).
#[must_use]
pub fn is_playable(name: &str) -> bool {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "mp3"
            | "m4a"
            | "aac"
            | "wav"
            | "ogg"
            | "oga"
            | "opus"
            | "flac"
            | "mp4"
            | "m4v"
            | "webm"
            | "mov"
    )
}

/// `path` with `%` and whitespace escaped, to sit in a fence's info.
#[must_use]
pub fn escape_info(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        match c {
            '%' => out.push_str("%25"),
            ' ' => out.push_str("%20"),
            '\t' => out.push_str("%09"),
            c => out.push(c),
        }
    }
    out
}

/// [`escape_info`] undone.
#[must_use]
pub fn unescape_info(text: &str) -> String {
    text.replace("%20", " ")
        .replace("%09", "\t")
        .replace("%25", "%")
}

/// A fenced block of `body` with `info`, its fence longer than any run of
/// backticks in it.
fn fence(info: &str, body: &str) -> String {
    let mut longest = 0_usize;
    let mut run = 0_usize;
    for c in body.chars() {
        run = if c == '`' { run.saturating_add(1) } else { 0 };
        longest = longest.max(run);
    }
    let ticks = "`".repeat(longest.saturating_add(1).max(3));
    let body = body.trim_end_matches('\n');
    format!("{ticks}{info}\n{body}\n{ticks}\n\n")
}

/// How a chart is drawn: the song's own, or an arrangement's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chart {
    /// As the chart writes it (`A`, `F#m`).
    pub key: Option<String>,
    pub tempo: Option<f64>,
    pub meter: Option<String>,
    /// The source (`<Song>.kf`), relative to the song's folder.
    pub source: Option<String>,
    /// Its engraved pages, in order, relative to the song's folder.
    pub pages: Vec<String>,
    /// Its engraved PDF, relative to the song's folder.
    pub pdf: Option<String>,
}

impl Chart {
    /// `A · 83 bpm · 3/4`, what is known of it.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(key) = &self.key {
            parts.push(key.clone());
        }
        if let Some(tempo) = self.tempo {
            parts.push(format!("{} bpm", trim_float(tempo)));
        }
        if let Some(meter) = &self.meter {
            parts.push(meter.clone());
        }
        parts.join(" · ")
    }
}

/// An arrangement of a song: a chart of its own.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Arrangement {
    pub name: String,
    pub chart: Chart,
}

/// A song of a library. File names are relative to the song's folder.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Song {
    pub title: String,
    pub artist: Option<String>,
    /// The library's categories it is in (several allowed).
    pub categories: Vec<String>,
    /// Its own chart.
    pub chart: Chart,
    pub arrangements: Vec<Arrangement>,
    /// The LRC.
    pub lrc: Option<String>,
    /// The session file (`.RPP`, `.session`).
    pub session: Option<String>,
    /// Everything else of the song's, in `Media/`.
    pub media: Vec<String>,
    /// How long its recording runs, in seconds ([`crate::media::length`]).
    pub length: Option<f64>,
}

/// An arrangement as its note's property writes it.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ArrangementFields {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tempo: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    meter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chart: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pages: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pdf: Option<String>,
}

/// A song note's properties, as written.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Fields {
    #[serde(rename = "type")]
    kind: String,
    title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artist: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    length: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tempo: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    meter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chart: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pages: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pdf: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    arrangements: Vec<ArrangementFields>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lyrics: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    media: Vec<String>,
    /// The library it is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    library: Option<String>,
}

impl Song {
    /// `A · 83 bpm · 3/4` of its own chart.
    #[must_use]
    pub fn summary(&self) -> String {
        self.chart.summary()
    }

    /// The recording its page plays and its chart follows: its first
    /// audio or video file, relative to its folder.
    #[must_use]
    pub fn recording(&self) -> Option<&str> {
        self.media
            .iter()
            .map(String::as_str)
            .find(|m| is_playable(m))
    }

    /// The chart of `arrangement` (None: its own); None when it has no
    /// such arrangement.
    #[must_use]
    pub fn chart_of(&self, arrangement: Option<&str>) -> Option<&Chart> {
        arrangement.map_or(Some(&self.chart), |name| {
            self.arrangements
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case(name))
                .map(|a| &a.chart)
        })
    }

    /// Its note in `lib`: the properties, then its page: the summary and
    /// downloads, its recording, its chart (a fence that follows playback,
    /// [`CHART_FENCE`]), its lyrics (timed when it has them), its
    /// arrangements (a heading each, which a setlist links to), its media.
    #[must_use]
    pub fn note(&self, lib: &Library, page: &SongPage) -> String {
        let file = |name: &str| lib.song_file(&self.title, name);
        let link = |name: &Option<String>| name.as_deref().map(|n| wikilink(&file(n), base(n)));
        let fields = Fields {
            kind: KIND.to_owned(),
            title: self.title.clone(),
            artist: self.artist.clone(),
            categories: self.categories.clone(),
            length: self.length,
            key: self.chart.key.clone(),
            tempo: self.chart.tempo,
            meter: self.chart.meter.clone(),
            chart: link(&self.chart.source),
            pages: self
                .chart
                .pages
                .iter()
                .map(|p| wikilink(&file(p), base(p)))
                .collect(),
            pdf: link(&self.chart.pdf),
            arrangements: self
                .arrangements
                .iter()
                .map(|a| ArrangementFields {
                    name: a.name.clone(),
                    key: a.chart.key.clone(),
                    tempo: a.chart.tempo,
                    meter: a.chart.meter.clone(),
                    chart: link(&a.chart.source),
                    pages: a
                        .chart
                        .pages
                        .iter()
                        .map(|p| wikilink(&file(p), base(p)))
                        .collect(),
                    pdf: link(&a.chart.pdf),
                })
                .collect(),
            lyrics: link(&self.lrc),
            session: link(&self.session),
            media: self
                .media
                .iter()
                .map(|m| wikilink(&file(m), base(m)))
                .collect(),
            library: Some(wikilink(lib.note().trim_end_matches(".md"), &lib.name)),
        };
        frontmatter::write(&fields, &self.body(lib, page))
    }

    /// Its chart on its page: the source as a fence that follows its
    /// recording, else (no source read) its engraved pages as images.
    fn chart_block(&self, out: &mut String, chart: &Chart, source: Option<&str>, lib: &Library) {
        match source {
            Some(source) => {
                let mut info = CHART_FENCE.to_owned();
                if let Some(recording) = self.recording() {
                    let _ = write!(info, " media={}", escape_info(recording));
                }
                out.push_str(&fence(&info, source));
            }
            None => {
                for page in &chart.pages {
                    let _ = writeln!(out, "![[{}]]\n", lib.song_file(&self.title, page));
                }
            }
        }
    }

    fn body(&self, lib: &Library, page: &SongPage) -> String {
        let file = |name: &str| lib.song_file(&self.title, name);
        let mut out = format!("# {}\n\n", self.title);
        if let Some(artist) = &self.artist {
            let _ = writeln!(out, "*{artist}*\n");
        }
        if !self.categories.is_empty() {
            let _ = writeln!(out, "In {}\n", self.categories.join(", "));
        }
        let summary = self.summary();
        if !summary.is_empty() {
            let _ = writeln!(out, "**{summary}**\n");
        }
        let mut downloads = chart_downloads(&self.chart, &file);
        if let Some(session) = &self.session {
            downloads.push(wikilink(&file(session), &format!("Session ({session})")));
        }
        if let Some(lrc) = &self.lrc {
            downloads.push(wikilink(&file(lrc), "Synced lyrics (LRC)"));
        }
        if !downloads.is_empty() {
            let _ = writeln!(out, "**Downloads:** {}\n", downloads.join(" · "));
        }
        if let Some(recording) = self.recording() {
            let _ = writeln!(out, "![[{}]]\n", file(recording));
        }
        self.chart_block(&mut out, &self.chart, page.chart.as_deref(), lib);
        if let Some(lrc) = &self.lrc {
            let _ = writeln!(out, "## Lyrics\n\n![[{}]]\n", file(lrc));
        } else if let Some(lrc) = &page.chart_lrc {
            out.push_str("## Lyrics\n\n");
            out.push_str(&fence("lrc", lrc));
        } else if !page.lyrics.is_empty() {
            out.push_str("## Lyrics\n\n");
            for block in &page.lyrics {
                if !block.section.is_empty() {
                    let _ = writeln!(out, "### {}\n", block.section);
                }
                out.push_str(&block.lines.join("\\\n"));
                out.push_str("\n\n");
            }
        }
        if !self.arrangements.is_empty() {
            out.push_str("## Arrangements\n\n");
            for a in &self.arrangements {
                let _ = writeln!(out, "### {}\n", a.name);
                let summary = a.chart.summary();
                if !summary.is_empty() {
                    let _ = writeln!(out, "**{summary}**\n");
                }
                let downloads = chart_downloads(&a.chart, &file);
                if !downloads.is_empty() {
                    let _ = writeln!(out, "**Downloads:** {}\n", downloads.join(" · "));
                }
                let source = page
                    .arrangements
                    .iter()
                    .find(|(name, _)| *name == a.name)
                    .map(|(_, source)| source.as_str());
                self.chart_block(&mut out, &a.chart, source, lib);
            }
        }
        if !self.media.is_empty() {
            out.push_str("## Media\n\n");
            for m in &self.media {
                let _ = writeln!(out, "- {}", wikilink(&file(m), base(m)));
            }
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "In the library {}.",
            wikilink(lib.note().trim_end_matches(".md"), &lib.name)
        );
        out
    }

    /// The song a note describes; None when it is not a song note.
    #[must_use]
    pub fn from_note(lib: &Library, text: &str) -> Option<Self> {
        let f: Fields = frontmatter::read(text)?;
        if f.kind != KIND {
            return None;
        }
        let dir = format!("{}/", lib.song_dir(&f.title));
        let rel = |link: &str| {
            let target = link_target(link);
            target.strip_prefix(&dir).unwrap_or(target).to_owned()
        };
        let name = |link: Option<String>| link.as_deref().map(rel);
        Some(Self {
            chart: Chart {
                key: f.key,
                tempo: f.tempo,
                meter: f.meter,
                source: name(f.chart),
                pages: f.pages.iter().map(|p| rel(p)).collect(),
                pdf: name(f.pdf),
            },
            arrangements: f
                .arrangements
                .into_iter()
                .map(|a| Arrangement {
                    name: a.name,
                    chart: Chart {
                        key: a.key,
                        tempo: a.tempo,
                        meter: a.meter,
                        source: name(a.chart),
                        pages: a.pages.iter().map(|p| rel(p)).collect(),
                        pdf: name(a.pdf),
                    },
                })
                .collect(),
            title: f.title,
            artist: f.artist,
            categories: f.categories,
            length: f.length.filter(|l| l.is_finite() && *l > 0.0),
            lrc: name(f.lyrics),
            session: name(f.session),
            media: f.media.iter().map(|m| rel(m)).collect(),
        })
    }

    /// The media file `name` as kept in the song's folder.
    #[must_use]
    pub fn media_path(name: &str) -> String {
        format!("{MEDIA}/{name}")
    }
}

fn chart_downloads(chart: &Chart, file: &impl Fn(&str) -> String) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(pdf) = &chart.pdf {
        out.push(wikilink(&file(pdf), "Chart (PDF)"));
    }
    if let Some(source) = &chart.source {
        out.push(wikilink(&file(source), "Chart source (Keyflow)"));
    }
    out
}

/// A path's last part.
fn base(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `83` for 83.0, `82.5` for 82.5.
#[must_use]
pub fn trim_float(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song() -> Song {
        Song {
            title: "Silent Night".to_owned(),
            artist: Some("Traditional".to_owned()),
            categories: vec!["Christmas".to_owned(), "Hymns".to_owned()],
            chart: Chart {
                key: Some("A".to_owned()),
                tempo: Some(83.0),
                meter: Some("3/4".to_owned()),
                source: Some("Silent Night.kf".to_owned()),
                pages: vec!["Silent Night.chart-1.png".to_owned()],
                pdf: Some("Silent Night - Master Rhythm.pdf".to_owned()),
            },
            arrangements: vec![Arrangement {
                name: "Acoustic".to_owned(),
                chart: Chart {
                    key: Some("G".to_owned()),
                    source: Some("Arrangements/Acoustic/Silent Night (Acoustic).kf".to_owned()),
                    pages: vec![
                        "Arrangements/Acoustic/Silent Night (Acoustic).chart-1.png".to_owned(),
                    ],
                    ..Chart::default()
                },
            }],
            lrc: None,
            session: Some("Silent Night.RPP".to_owned()),
            media: vec!["Media/Click.wav".to_owned()],
            length: Some(186.5),
        }
    }

    #[test]
    fn a_song_note_reads_back_as_the_song() {
        let lib = Library::named("Christmas");
        let note = song().note(&lib, &SongPage::default());
        assert!(note.starts_with("---\ntype: song\n"), "{note}");
        assert!(
            note.contains("![[Libraries/Christmas/Silent Night/Silent Night.chart-1.png]]"),
            "{note}"
        );
        assert!(
            note.contains("## Arrangements\n\n### Acoustic\n\n**G**"),
            "{note}"
        );
        assert!(
            note.contains("- [[Libraries/Christmas/Silent Night/Media/Click.wav|Click.wav]]"),
            "{note}"
        );
        assert_eq!(Song::from_note(&lib, &note), Some(song()));
        assert_eq!(song().summary(), "A · 83 bpm · 3/4");
        assert_eq!(
            song()
                .chart_of(Some("acoustic"))
                .and_then(|c| c.key.clone())
                .as_deref(),
            Some("G")
        );
        assert!(song().chart_of(Some("Electric")).is_none());
    }

    #[test]
    fn other_notes_are_not_songs() {
        let lib = Library::named("X");
        assert_eq!(
            Song::from_note(&lib, "---\ntype: setlist\ntitle: x\n---\n"),
            None
        );
        assert_eq!(Song::from_note(&lib, "# no properties\n"), None);
    }
}
