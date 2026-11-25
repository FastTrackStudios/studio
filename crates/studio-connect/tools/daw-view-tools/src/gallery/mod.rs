//! The gallery: a theme pack's showcase shots, drawn from the golden
//! session.
//!
//! `daw_theme::gallery` names the shots (their view, frame and size, and
//! what each shows); this draws them for every theme of a pack, or the
//! built-in look, and writes `<theme>/<shot>.png` and `README.md` into a
//! folder a pack repository can commit as it is (`gallery/`). The same
//! pack and session give the same bytes: the raster is the CPU's, the
//! meters are frozen, nothing reads the clock.
//!
//! The views draw from process-wide state (the loaded theme, the folds,
//! the automation lanes), so a gallery runs in a process of its own —
//! `daw-view-tools gallery`, which Studio's `THEME_GALLERY` starts —
//! rather than inside a window whose look it would change.

mod editor;
mod editor_states;
pub mod render;
pub mod verify;

use std::path::{Path, PathBuf};

use daw_theme::gallery::{self as manifest, Shot};
use daw_theme::{Pack, Source, ThemeDef};

pub use render::{Picture, Planned, Stage};

use crate::golden::{self, Golden};

/// Why a gallery could not be made.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the shots are written for golden session {wants}, this is session {has}")]
    Session { wants: u32, has: u32 },
    #[error("no shot named `{0}`")]
    NoShot(String),
    #[error(transparent)]
    Source(#[from] daw_theme::source::Error),
    #[error(transparent)]
    Pack(#[from] daw_theme::pack::Error),
    #[error("theme `{theme}` did not load: {why}")]
    Theme { theme: String, why: String },
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("{0}: {1}")]
    Png(PathBuf, image::ImageError),
}

/// What to picture: the built-in look, or a pack's themes.
pub enum Looks {
    /// The views' own look, no pack (`--pack none`).
    BuiltIn,
    /// These themes of `pack`.
    Pack { pack: Pack, themes: Vec<ThemeDef> },
}

/// The name a gallery files the built-in look under.
pub const BUILT_IN: &str = "built-in";

impl Looks {
    /// The pack `link` names (`none`, a first-party name, `owner/repo[@rev]`,
    /// a GitHub URL or a folder), installed into `cache` when it is a link;
    /// `theme` alone, or all of its themes.
    ///
    /// # Errors
    /// The link is not one, it could not be fetched, it is not a pack, or
    /// it has no theme `theme`.
    pub fn resolve(link: &str, theme: Option<&str>, cache: &Path) -> Result<Self, Error> {
        if link.is_empty() || link == "none" {
            return Ok(Self::BuiltIn);
        }
        let source = Source::parse(daw_theme::source::first_party(link))?;
        let pack = daw_theme::install(&source, cache)?.pack;
        let themes = match theme {
            Some(name) => vec![pack.theme(Some(name))?.clone()],
            None => pack.themes.clone(),
        };
        Ok(Self::Pack { pack, themes })
    }

    /// The pack's title, as its gallery page is headed.
    #[must_use]
    pub fn title(&self) -> &str {
        match self {
            Self::BuiltIn => "Built-in look",
            Self::Pack { pack, .. } => &pack.title,
        }
    }

    /// The themes pictured, by name and title.
    #[must_use]
    pub fn pictured(&self) -> Vec<manifest::Pictured<'_>> {
        match self {
            Self::BuiltIn => vec![manifest::Pictured {
                name: BUILT_IN,
                title: "Built-in",
            }],
            Self::Pack { themes, .. } => themes
                .iter()
                .map(|t| manifest::Pictured {
                    name: &t.name,
                    title: &t.title,
                })
                .collect(),
        }
    }
}

/// The shots named, or every shot.
///
/// # Errors
/// A name that is no shot's.
pub fn shots(names: &[String]) -> Result<Vec<&'static Shot>, Error> {
    if names.is_empty() {
        return Ok(manifest::SHOTS.iter().collect());
    }
    names
        .iter()
        .map(|n| Shot::named(n).ok_or_else(|| Error::NoShot(n.clone())))
        .collect()
}

/// The golden session, made ready to draw: the process-wide state the
/// views read set from it (the folds, the automation lanes), and planned
/// as the windows plan it.
///
/// # Errors
/// The manifest's shots are written for another session.
pub fn stage_session() -> Result<(Golden, Planned), Error> {
    if manifest::SESSION != golden::VERSION {
        return Err(Error::Session {
            wants: manifest::SESSION,
            has: golden::VERSION,
        });
    }
    let golden = golden::song();
    // Consumed first, so an `FTS_THEME` in the environment can never
    // reach a shot later: each theme is set explicitly.
    daw_view_kit::skin::from_env();
    // Every folder open but those the session shows shut.
    daw_view_kit::folds::set_tops(false);
    for guid in &golden.shut {
        daw_view_kit::folds::toggle(guid, true);
    }
    daw_view_kit::automation::replace(golden.lanes.clone());
    let planned = plan(&golden);
    Ok((golden, planned))
}

/// The golden session's rows and strips, folded the way the windows fold.
fn plan(golden: &Golden) -> Planned {
    let kinds = studio_engine::plan::Kinds::default();
    let (project, rows) = studio_engine::song_model::plan_rows_with(&golden.project, None, &kinds);
    let strips = studio_engine::song_model::plan_strips_with(&golden.project, None, &kinds);
    Planned {
        project,
        rows,
        strips: daw_ui::studio::RowsRef(std::sync::Arc::new(strips)),
        previews: golden.previews(),
    }
}

/// Show `theme` (or the built-in look) in every view, and what the shots
/// draw with under it.
///
/// # Errors
/// The theme did not compile.
pub fn stage_theme(pack: Option<(&Pack, &ThemeDef)>) -> Result<Stage, Error> {
    match pack {
        None => daw_view_kit::skin::clear(),
        Some((pack, theme)) => {
            daw_view_kit::skin::load_pack(pack, theme).map_err(|why| Error::Theme {
                theme: theme.name.clone(),
                why,
            })?;
        }
    }
    let base = arrangement_view::arrangement::Palette::from_theme(&daw_ui::theming::Theme::dark());
    Ok(Stage {
        palette: daw_view_kit::skin::palette(&base),
        font: daw_view_kit::text::Font::embedded().map_err(|e| Error::Theme {
            theme: "font".to_owned(),
            why: e.to_string(),
        })?,
        layout: daw_view_kit::layout::Layout::default(),
    })
}

/// What a gallery run made.
#[derive(Debug)]
pub struct Made {
    /// Every picture written, relative to the output folder.
    pub pictures: Vec<String>,
    /// The page, when every shot was drawn.
    pub readme: Option<PathBuf>,
    /// How long each theme's shots took to draw and write, in milliseconds.
    pub per_theme_ms: Vec<(String, u128)>,
}

/// Draw `shots` for every theme `looks` holds into `out`, and the page
/// when the run is the whole set.
///
/// # Errors
/// A theme did not load, or a picture or the page could not be written.
pub fn run(looks: &Looks, shots: &[&Shot], out: &Path) -> Result<Made, Error> {
    let (golden, planned) = stage_session()?;
    let themes: Vec<Option<&ThemeDef>> = match looks {
        Looks::BuiltIn => vec![None],
        Looks::Pack { themes, .. } => themes.iter().map(Some).collect(),
    };
    let mut made = Made {
        pictures: Vec::new(),
        readme: None,
        per_theme_ms: Vec::new(),
    };
    for theme in themes {
        let began = std::time::Instant::now();
        let pack = match (looks, theme) {
            (Looks::Pack { pack, .. }, Some(t)) => Some((pack, t)),
            _ => None,
        };
        let name = theme.map_or(BUILT_IN, |t| t.name.as_str());
        let stage = stage_theme(pack)?;
        for shot in shots {
            let picture = render::shot(&golden, &planned, &stage, shot);
            let rel = manifest::image_path(name, shot.name);
            write_png(&out.join(&rel), &picture)?;
            made.pictures.push(rel);
        }
        made.per_theme_ms
            .push((name.to_owned(), began.elapsed().as_millis()));
    }
    if shots.len() == manifest::SHOTS.len() {
        let page = manifest::readme(looks.title(), &looks.pictured(), shots);
        let path = out.join("README.md");
        std::fs::write(&path, page).map_err(|e| Error::Io(path.clone(), e))?;
        made.readme = Some(path);
    }
    Ok(made)
}

/// `picture` as a PNG at `path`, compressed hard: a gallery is committed.
///
/// # Errors
/// The folder or the file could not be written.
pub fn write_png(path: &Path, picture: &Picture) -> Result<(), Error> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Error::Io(dir.to_owned(), e))?;
    }
    let bytes = encode_png(picture).map_err(|e| Error::Png(path.to_owned(), e))?;
    std::fs::write(path, bytes).map_err(|e| Error::Io(path.to_owned(), e))
}

/// `picture` encoded as an opaque RGB PNG, best compression, adaptive
/// filters: the same bytes for the same pixels.
///
/// # Errors
/// The encoder refused the buffer.
pub fn encode_png(picture: &Picture) -> Result<Vec<u8>, image::ImageError> {
    use image::ImageEncoder as _;
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};

    // The ground is opaque, so the alpha channel is all 255: dropped.
    let rgb: Vec<u8> = picture
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|px| px.iter().take(3).copied())
        .collect();
    let mut bytes = Vec::new();
    PngEncoder::new_with_quality(&mut bytes, CompressionType::Best, FilterType::Adaptive)
        .write_image(
            &rgb,
            picture.width,
            picture.height,
            image::ExtendedColorType::Rgb8,
        )?;
    Ok(bytes)
}
