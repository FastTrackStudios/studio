//! A song's chart engraved, and a song brought into a library.
//!
//! The engraving is its Master Rhythm pages (one PNG each, as `kf png`
//! draws them) and PDF (`kf pdf`), the timed lyric lane included: what
//! players download and print.
//!
//! The pages are raster, not SVG: an SVG page that shows anywhere as an
//! image has to carry its fonts, about 4 MB a page. What a share page
//! shows is the chart drawn to follow playback ([`crate::follow`]).

use std::path::Path;
use std::sync::OnceLock;

use keyflow::engraver::api::pipeline::ChartPipeline;
use keyflow::engraver::layout::chart::{ChartLayoutConfig, ChartLayoutResult, LayoutMode};
use keyflow::engraver::style::MStyle;

use crate::layout::{ARRANGEMENTS, Library};
use crate::lyrics::{self, Block};
use crate::song::{Arrangement, Chart as SongChart, Song, SongPage};
use crate::vault::Vault;
use crate::{Error, Result, check_name};

/// What a chart's header says, and its engraving.
#[derive(Debug, Clone)]
pub struct Engraved {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub key: Option<String>,
    /// The key it ends in, when another (its last key in force).
    pub key_end: Option<String>,
    /// Not plainly in its key ([`crate::keys`]).
    pub key_outside: bool,
    pub tempo: Option<f64>,
    pub meter: Option<String>,
    /// One PNG per page.
    pub pages: Vec<Vec<u8>>,
    /// The PDF; empty on iOS, where the engraver writes none.
    pub pdf: Vec<u8>,
}

/// The pipeline `kf` draws with: the shared fonts, `MStyle::new()`.
fn pipeline() -> Result<&'static ChartPipeline> {
    static PIPELINE: OnceLock<std::result::Result<ChartPipeline, String>> = OnceLock::new();
    PIPELINE
        .get_or_init(|| {
            let style: &'static MStyle = Box::leak(Box::new(MStyle::new()));
            ChartPipeline::with_style(style).map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| Error::Engrave(e.clone()))
}

/// Chart `chart` laid out as it is printed: A4 pages, the Master Rhythm
/// preset. The pipeline it was laid out with comes along.
///
/// # Errors
///
/// The engraver's fonts did not load.
pub(crate) fn layout(
    chart: &keyflow::Chart,
) -> Result<(&'static ChartPipeline, ChartLayoutResult)> {
    let pipeline = pipeline()?;
    let config = ChartLayoutConfig::master_rhythm().with_page_offsets(true);
    Ok((
        pipeline,
        pipeline.layout_with_config(chart, &LayoutMode::paginated_a4(), &config),
    ))
}

/// Engrave chart `source`: A4 pages, the Master Rhythm preset.
///
/// # Errors
///
/// It does not parse, or the engraver could not write its PDF.
pub fn engrave(source: &str) -> Result<Engraved> {
    let chart = keyflow::parse(source).map_err(|e| Error::Engrave(e.to_string()))?;
    let (pipeline, layout) = layout(&chart)?;
    let pages = pipeline
        .export_svg_pages_linked(&layout)
        .iter()
        .map(|svg| raster(svg))
        .collect::<Result<Vec<_>>>()?;
    // No PDF on iOS: the engraver's PDF writer is not built there
    // (`Cargo.toml`).
    #[cfg(not(target_os = "ios"))]
    let pdf = pipeline.export_pdf(&layout).map_err(Error::Engrave)?;
    #[cfg(target_os = "ios")]
    let pdf = Vec::new();
    let meta = &chart.metadata;
    let facts = crate::keys::facts(&chart);
    Ok(Engraved {
        key_end: facts.as_ref().and_then(|f| f.end.clone()),
        key_outside: facts.as_ref().is_some_and(|f| f.outside),
        title: meta.title.clone().filter(|t| !t.trim().is_empty()),
        artist: meta.artist.clone().filter(|a| !a.trim().is_empty()),
        key: chart
            .initial_key
            .as_ref()
            .map(|k| key_label(&k.to_string())),
        tempo: chart.tempo.as_ref().map(|t| t.bpm),
        meter: chart
            .initial_time_signature
            .as_ref()
            .or(chart.time_signature.as_ref())
            .map(ToString::to_string),
        pages,
        pdf,
    })
}

/// The engraver's own fonts, for drawing its pages without it: as pixels
/// (the PNG pages) or as outlines (the share page's chart, [`crate::follow`]).
///
/// # Errors
///
/// The engraver's fonts did not load.
pub(crate) fn fontdb() -> Result<std::sync::Arc<resvg::usvg::fontdb::Database>> {
    static FONTS: OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    let pipeline = pipeline()?;
    Ok(std::sync::Arc::clone(FONTS.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        for (label, bytes) in pipeline.fonts().embeddable_fonts_for_raster() {
            db.load_font_data(bytes.to_vec());
            // The generic families the engraving names (a rehearsal mark's
            // `sans-serif`) are the face the engraver files under them:
            // fontdb maps a generic family by name only.
            let family = db
                .faces()
                .last()
                .and_then(|f| f.families.first())
                .map(|(name, _)| name.clone());
            match (label, family) {
                ("sans-serif", Some(name)) => db.set_sans_serif_family(name),
                ("serif", Some(name)) => db.set_serif_family(name),
                _ => {}
            }
        }
        std::sync::Arc::new(db)
    })))
}

/// How many pixels a point is on a page: A4 at 2× is 1190 px wide.
const SCALE: f32 = 2.0;

/// An engraved page as a PNG, drawn with the engraver's own fonts.
fn raster(svg: &str) -> Result<Vec<u8>> {
    use resvg::{tiny_skia, usvg};
    let opts = usvg::Options {
        fontdb: fontdb()?,
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_str(svg, &opts).map_err(|e| Error::Engrave(e.to_string()))?;
    let size = tree.size().to_int_size().scale_by(SCALE);
    let mut pixmap = size
        .and_then(|s| tiny_skia::Pixmap::new(s.width(), s.height()))
        .ok_or_else(|| Error::Engrave("an empty page".to_owned()))?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(SCALE, SCALE),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|e| Error::Engrave(e.to_string()))
}

/// A key as a band reads it: `A Ionian` → `A`, `F# Aeolian` → `F#m`.
#[must_use]
pub fn key_label(key: &str) -> String {
    let key = key.trim();
    if let Some(root) = key
        .strip_suffix(" Ionian")
        .or_else(|| key.strip_suffix(" major"))
    {
        return root.to_owned();
    }
    if let Some(root) = key
        .strip_suffix(" Aeolian")
        .or_else(|| key.strip_suffix(" minor"))
    {
        return format!("{root}m");
    }
    key.to_owned()
}

/// The files a song is brought in from, and where they go.
#[derive(Debug, Clone, Default)]
pub struct SongFiles<'a> {
    /// A chart (`.kf`): the song's own, or `arrangement`'s.
    pub chart: Option<&'a Path>,
    /// The arrangement `chart` is (None: the song's own chart).
    pub arrangement: Option<&'a str>,
    /// Its line-synced lyrics (`.lrc`).
    pub lrc: Option<&'a Path>,
    /// Its session (`.RPP`, `.session`).
    pub session: Option<&'a Path>,
    /// Anything else of the song's: stems, references, artwork. Into `Media/`.
    pub media: &'a [&'a Path],
    /// Categories of its library to put it in (added to those it has).
    pub categories: &'a [String],
}

fn read_text(p: &Path) -> Result<String> {
    std::fs::read_to_string(p).map_err(|source| Error::Io {
        path: p.display().to_string(),
        source,
    })
}

/// Bring a song into `lib`: its files copied into its folder, its chart
/// engraved, its note written.
///
/// `title` names it; else the chart's title, else the chart's file name.
/// A song already there is updated: what is given replaces or adds to
/// what it has, the rest stays.
///
/// # Errors
///
/// No title could be found, a file could not be read or written, or the
/// chart did not engrave.
pub fn import_song(
    vault: &Vault,
    lib: &Library,
    files: &SongFiles<'_>,
    title: Option<&str>,
) -> Result<Song> {
    let source = files.chart.map(read_text).transpose()?;
    let engraved = source.as_deref().map(engrave).transpose()?;
    let title = title
        .map(str::to_owned)
        .or_else(|| engraved.as_ref().and_then(|e| e.title.clone()))
        .or_else(|| stem(files.chart?))
        .ok_or_else(|| Error::BadName(String::new()))?;
    let title = check_name(&title)?.to_owned();
    if let Some(a) = files.arrangement {
        check_name(a)?;
    }

    vault.ensure_library(lib)?;
    let mut song = vault.song(lib, &title).unwrap_or_else(|_| Song {
        title: title.clone(),
        ..Song::default()
    });
    for c in files.categories {
        if !song.categories.iter().any(|x| x.eq_ignore_ascii_case(c)) {
            song.categories.push(c.trim().to_owned());
        }
    }
    if let (Some(text), Some(engraved)) = (&source, engraved) {
        let (dir, stem) = files.arrangement.map_or_else(
            || (String::new(), title.clone()),
            |a| (format!("{ARRANGEMENTS}/{a}/"), format!("{title} ({a})")),
        );
        let source = format!("{dir}{stem}.kf");
        vault.write(&lib.song_file(&title, &source), text.as_bytes())?;
        if engraved.artist.is_some() && files.arrangement.is_none() {
            song.artist.clone_from(&engraved.artist);
        }
        let chart = match files.arrangement {
            Some(a) => {
                if !song.arrangements.iter().any(|x| x.name == a) {
                    song.arrangements.push(Arrangement {
                        name: a.to_owned(),
                        chart: SongChart::default(),
                    });
                }
                &mut song
                    .arrangements
                    .iter_mut()
                    .find(|x| x.name == a)
                    .ok_or_else(|| Error::Engrave(format!("no arrangement {a}")))?
                    .chart
            }
            None => &mut song.chart,
        };
        chart.source = Some(source);
        write_engraving(vault, lib, &title, &dir, &stem, chart, engraved)?;
    }
    if let Some(lrc) = files.lrc {
        let name = format!("{title}.lrc");
        vault.copy_in(lrc, &lib.song_file(&title, &name))?;
        song.lrc = Some(name);
    }
    if let Some(session) = files.session {
        let ext = session
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("RPP");
        let name = format!("{title}.{ext}");
        vault.copy_in(session, &lib.song_file(&title, &name))?;
        song.session = Some(name);
    }
    for m in files.media {
        let name = m
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| Error::BadName(m.display().to_string()))?;
        let rel = Song::media_path(name);
        vault.copy_in(m, &lib.song_file(&title, &rel))?;
        if !song.media.contains(&rel) {
            song.media.push(rel);
        }
    }
    song.length = recording_length(vault, lib, &song).or(song.length);
    let page = song_page(vault, lib, &song);
    vault.write_song(lib, &song, &page)?;
    Ok(song)
}

/// How long `song`'s recording runs, from its file's header.
fn recording_length(vault: &Vault, lib: &Library, song: &Song) -> Option<f64> {
    let recording = song.recording()?;
    crate::media::length(&vault.path(&lib.song_file(&song.title, recording)))
}

/// Engrave song `title`'s charts again (its own and every arrangement's)
/// from the `.kf` files in its folder, after an edit, and rewrite its note.
///
/// # Errors
///
/// A chart did not engrave, or a file could not be written.
pub fn reengrave(vault: &Vault, lib: &Library, title: &str) -> Result<Song> {
    let mut song = vault.song(lib, title)?;
    song.length = recording_length(vault, lib, &song).or(song.length);
    let mut charts: Vec<&mut SongChart> = vec![&mut song.chart];
    charts.extend(song.arrangements.iter_mut().map(|a| &mut a.chart));
    for chart in charts {
        let Some(source) = chart.source.clone() else {
            continue;
        };
        let text = vault.read(&lib.song_file(title, &source))?;
        let (dir, stem) = match source.rsplit_once('/') {
            Some((d, f)) => (format!("{d}/"), f.trim_end_matches(".kf").to_owned()),
            None => (String::new(), source.trim_end_matches(".kf").to_owned()),
        };
        write_engraving(vault, lib, title, &dir, &stem, chart, engrave(&text)?)?;
    }
    let page = song_page(vault, lib, &song);
    vault.write_song(lib, &song, &page)?;
    Ok(song)
}

/// Write `engraved`'s pages and PDF into `dir` of song `title`'s folder,
/// named after `stem`; its header and files into `chart`.
fn write_engraving(
    vault: &Vault,
    lib: &Library,
    title: &str,
    dir: &str,
    stem: &str,
    chart: &mut SongChart,
    engraved: Engraved,
) -> Result<()> {
    let mut pages = Vec::with_capacity(engraved.pages.len());
    for (n, png) in (1_u32..).zip(&engraved.pages) {
        let name = format!("{dir}{stem}.chart-{n}.png");
        vault.write(&lib.song_file(title, &name), png)?;
        pages.push(name);
    }
    // Pages a longer chart had before.
    for old in chart.pages.iter().filter(|p| !pages.contains(p)) {
        vault.remove(&lib.song_file(title, old))?;
    }
    chart.pages = pages;
    // No PDF (iOS): the one there was is kept.
    if !engraved.pdf.is_empty() {
        let pdf = format!("{dir}{stem} - Master Rhythm.pdf");
        vault.write(&lib.song_file(title, &pdf), &engraved.pdf)?;
        chart.pdf = Some(pdf);
    }
    chart.key = engraved.key.or_else(|| chart.key.take());
    chart.key_end = engraved.key_end;
    chart.key_outside = engraved.key_outside;
    chart.tempo = engraved.tempo.or(chart.tempo);
    chart.meter = engraved.meter.or_else(|| chart.meter.take());
    Ok(())
}

/// What the song's page shows from its files: its charts' sources (drawn
/// as charts that follow playback), its timed lyrics from its chart when it
/// has no LRC, and its lyrics as text (its chart's lanes, else its LRC's
/// lines).
fn song_page(vault: &Vault, lib: &Library, song: &Song) -> SongPage {
    let file = |name: &Option<String>| {
        name.as_ref()
            .and_then(|n| vault.read(&lib.song_file(&song.title, n)).ok())
    };
    let chart = file(&song.chart.source);
    let arrangements = song
        .arrangements
        .iter()
        .filter_map(|a| Some((a.name.clone(), file(&a.chart.source)?)))
        .collect();
    let chart_lrc = if song.lrc.is_some() {
        None
    } else {
        chart
            .as_deref()
            .and_then(|c| crate::follow::chart_lrc(c).ok().flatten())
    };
    let from_chart = chart.as_deref().map(lyrics::from_chart).unwrap_or_default();
    let lyrics = if from_chart.is_empty() {
        file(&song.lrc)
            .map(|l| lyrics::from_lrc(&l))
            .filter(|lines| !lines.is_empty())
            .map(|lines| {
                vec![Block {
                    section: String::new(),
                    lines,
                }]
            })
            .unwrap_or_default()
    } else {
        from_chart
    };
    SongPage {
        lyrics,
        chart,
        arrangements,
        chart_lrc,
    }
}

fn stem(path: &Path) -> Option<String> {
    path.file_stem()?.to_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::key_label;

    #[test]
    fn keys_read_as_a_band_writes_them() {
        assert_eq!(key_label("A Ionian"), "A");
        assert_eq!(key_label("F# Aeolian"), "F#m");
        assert_eq!(key_label("Bb minor"), "Bbm");
        assert_eq!(key_label("D Dorian"), "D Dorian");
    }
}
