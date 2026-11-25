//! Writing component-drawn artwork into a REAPER theme.
//!
//! `restyle` recolours the *inherited* art; this replaces it. Each image is
//! rendered from its Dioxus component at 100/150/200 % — from the vector
//! each time, not upscaled — and written over the corresponding PNG.
//!
//! Generated images are the only ones that don't need a pristine snapshot:
//! they have no prior state to preserve, because their source is code.

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::theme::{SCALES, ThemeDir};

/// What [`generate`] wrote.
#[derive(Debug, Clone, Default)]
pub struct GenerateReport {
    pub written: Vec<PathBuf>,
    /// Of `written`, those drawn from a true vector control rather than
    /// from traced rects — worth reporting because only these get sharper
    /// at 150/200%, and a control silently falling back to its trace is
    /// invisible in the output.
    pub vectorised: Vec<PathBuf>,
    pub failed: Vec<(String, String)>,
}

/// Render every component-drawn image into `theme`.
///
/// Each image's geometry is **measured from the art being replaced** — the
/// pristine snapshot `restyle` keeps, falling back to what's live. Nothing
/// about size or marker layout is authored here, because REAPER blits these
/// where WALTER expects and only reads magenta as geometry when it lands
/// where it expects. Getting that wrong renders the guides as visible
/// magenta in the mixer.
pub fn generate(theme: &ThemeDir, dry_run: bool, vectors_only: bool) -> Result<GenerateReport> {
    let mut report = GenerateReport::default();

    for (prefix, _scale) in SCALES {
        let dir = theme.scale_dir(prefix);
        if !dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "png") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            // The traced art is keyed on the 100% names; a DPI variant
            // finds the same drawing and renders it at the size that
            // folder wants.
            let Some(art) = daw_ui::art::generated::by_name(name) else {
                continue;
            };

            // Geometry still comes from the image being replaced, not from
            // the traced source: REAPER's 150% art is not reliably 1.5x the
            // 100%, and blitting at the wrong size is what renders markers
            // as visible magenta.
            //
            // From the **pristine snapshot** where there is one, because
            // measuring the live file makes generation self-referential:
            // once a run split a strip wrongly and wrote a stretched
            // button over it, every later run measured the stretched
            // button, agreed it was a single cell, and reproduced the
            // mistake. That looks exactly like a fix not taking effect.
            let measured_from = pristine(&dir, name).unwrap_or_else(|| path.clone());
            let measured = match image::open(&measured_from) {
                Ok(img) => daw_ui::art::DerivedSpec::from_image(&img.to_rgba8()),
                Err(e) => {
                    report.failed.push((name.into(), format!("read: {e}")));
                    continue;
                }
            };

            // A vector control if one draws this image, the trace
            // otherwise. Both stamp the same measured markers back.
            let vector = daw_ui::art::cell_markup(name, Default::default()).is_some();
            // The traced half rewrites art nothing here authored, through
            // a ramp built from the theme's own palette. Skipping it is
            // the default; see the `--traced` flag for why.
            if vectors_only && !vector {
                continue;
            }
            let drawn = if vector {
                daw_ui::art::render_control(name, &measured).map_err(|e| format!("{e}"))
            } else {
                render_art(art, &measured)
            };

            match drawn {
                Ok(img) => {
                    if !dry_run
                        && let Err(e) = img
                            .save(&path)
                            .with_context(|| format!("write {}", path.display()))
                    {
                        report.failed.push((name.into(), format!("{e:#}")));
                        continue;
                    }
                    if vector {
                        report.vectorised.push(path.clone());
                    }
                    report.written.push(path);
                }
                Err(e) => report.failed.push((name.into(), e)),
            }
        }
    }
    report.written.sort();
    report.vectorised.sort();
    Ok(report)
}

/// The untouched original of `name`, if the theme kept one.
///
/// `restyle` snapshots every image into `.source-art/` before its first
/// pass, so this is what the theme shipped rather than what the last run
/// left behind.
fn pristine(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
    let p = dir
        .join(crate::restyle::SOURCE_DIR)
        .join(format!("{name}.png"));
    p.is_file().then_some(p)
}

/// Render traced art at the geometry measured from the image it replaces,
/// stamping that image's marker pixels back.
///
/// Cell by cell, because `ArtImage` draws **one** sprite cell — asking it
/// for the full strip width does not draw the strip, it stretches cell 0
/// across all of it. That produces a plausible-looking blurry button
/// rather than an obviously wrong one, so it is worth being explicit
/// about here.
fn render_art(
    art: daw_ui::art::ArtData,
    spec: &daw_ui::art::DerivedSpec,
) -> std::result::Result<image::RgbaImage, String> {
    use daw_ui::art::art_data::{ArtImage, ArtImageProps, ColorMode};

    // Split by the *trace's* cell count, not the measured one.
    //
    // `ArtImage` shifts its viewBox by `cell * (art.width / art.cells)`,
    // so it can only address the cells the trace knows about. When the two
    // disagreed — the index built with an older detector, the spec
    // measured with the current one — every cell index clamped to 0 and
    // the whole drawing was composited three times side by side, which is
    // what put three copies of a toolbar icon inside one icon.
    let mut spec = spec.clone();
    let n = art.cells.max(1);
    if spec.cells.len() as u32 != n {
        let w = spec.width / n;
        spec.cells = (0..n).map(|i| (i * w, w)).collect();
    }

    daw_ui::art::composite_cells(&spec, |i, w| {
        Ok(daw_ui::art::render_svg(
            ArtImage,
            ArtImageProps {
                art,
                width: Some(w),
                height: Some(spec.height),
                mode: ColorMode::Themed,
                cell: i,
            },
        ))
    })
    .map_err(|e| format!("{e}"))
}
