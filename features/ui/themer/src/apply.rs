//! Painting a REAPER theme from the canonical FTS theme.
//!
//! [`daw_ui::theme::Theme`] is the authored source; this writes the keys it
//! determines into a `.ReaperTheme`. Everything else in the file — the keys
//! the palette has no opinion about, the blend words, the font blobs — is
//! left exactly as it was, so applying is a **merge onto** a theme rather
//! than a replacement of one. That is what lets an FTS palette be applied on
//! top of a hand-tuned theme like the Reapertips fork without flattening the
//! parts nobody has modelled yet.

use anyhow::{Context, Result};
use std::path::Path;

use crate::color::Rgb;
use crate::theme::ThemeDir;

/// What [`apply_theme`] changed.
#[derive(Debug, Clone, Default)]
pub struct ApplyReport {
    /// Keys whose value actually changed.
    pub changed: Vec<(String, Rgb, Rgb)>,
    /// Keys the theme determined that already held that colour.
    pub unchanged: usize,
    /// Where the SWELL theme was written, if anywhere.
    pub swell: Option<std::path::PathBuf>,
    /// Overridden keys this REAPER theme does not have.
    ///
    /// Worth surfacing rather than writing: a key REAPER never reads is a
    /// typo that looks exactly like a colour that "didn't take".
    pub unknown: Vec<String>,
    /// Overridden keys that are not colours — `*_drawmode` and friends.
    ///
    /// Writing a colour over a blend mode changes how a layer composites,
    /// which shows up as a rendering bug a long way from its cause.
    pub not_a_colour: Vec<String>,
}

impl ApplyReport {
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty()
    }
}

/// Write `theme`'s palette into the REAPER theme at `dir`.
///
/// `dry_run` computes the diff without touching the file — worth having,
/// because this rewrites a file the user may have hand-edited.
pub fn apply_theme(dir: &Path, theme: &daw_ui::theme::Theme, dry_run: bool) -> Result<ApplyReport> {
    apply_theme_to(dir, theme, dry_run, None)
}

/// As [`apply_theme`], also writing `libSwell.colortheme` into
/// `resource_dir` when one is given.
///
/// SWELL's theme is a separate file in REAPER's *resource* directory, not
/// in the theme directory, and it paints the menu bar, dialogs, buttons and
/// list controls — none of which the `.ReaperTheme` palette can reach.
/// Skipping it is why a "finished" dark theme keeps a grey bar across the
/// top and grey dialogs throughout.
pub fn apply_theme_to(
    dir: &Path,
    theme: &daw_ui::theme::Theme,
    dry_run: bool,
    resource_dir: Option<&Path>,
) -> Result<ApplyReport> {
    let mut target = ThemeDir::open(dir)?;
    let mut report = ApplyReport::default();

    for assignment in theme.reaper_palette() {
        // Guarded only for overrides, and deliberately so.
        //
        // A *derived* key the theme happens not to define is normal — a
        // theme sets what it cares about and REAPER defaults the rest, so
        // adding `envcp_bg` to a theme that omits it is the feature, not a
        // mistake. An *override* naming an unknown key is almost always a
        // typo, and REAPER ignores it silently, so it looks exactly like a
        // colour that refused to take effect. Same for a `*_drawmode`:
        // writing a colour over a blend mode changes how a layer
        // composites and surfaces a long way from the cause.
        if theme.overrides.contains_key(assignment.key.as_ref()) {
            if !target.ini().has(&assignment.key) {
                report.unknown.push(assignment.key.to_string());
                continue;
            }
            if !crate::groups::is_color(&assignment.key) {
                report.not_a_colour.push(assignment.key.to_string());
                continue;
            }
        }
        // daw-theme and fts-themer each have their own colour type — one is
        // the authoring vocabulary, the other the REAPER wire format — so
        // convert rather than leaking one into the other's API.
        let want = Rgb::new(assignment.color.r, assignment.color.g, assignment.color.b);
        match target.ini().color(&assignment.key) {
            Some(have) if have == want => report.unchanged += 1,
            have => {
                report.changed.push((
                    assignment.key.to_string(),
                    have.unwrap_or(Rgb::new(0, 0, 0)),
                    want,
                ));
                target.ini_mut().set_color(&assignment.key, want);
            }
        }
    }

    if !dry_run && !report.is_empty() {
        target
            .save_ini()
            .with_context(|| format!("write {}", target.ini_path().display()))?;
    }

    if let Some(res) = resource_dir {
        let path = res.join("libSwell.colortheme");
        if !dry_run {
            std::fs::write(&path, theme.swell_colortheme())
                .with_context(|| format!("write {}", path.display()))?;
        }
        report.swell = Some(path);
    }

    Ok(report)
}

/// Load a canonical theme from a styx file, or the built-in default.
pub fn load_theme(path: Option<&Path>) -> Result<daw_ui::theme::Theme> {
    match path {
        Some(p) => {
            let text =
                std::fs::read_to_string(p).with_context(|| format!("read {}", p.display()))?;
            daw_ui::theme::Theme::from_styx(&text)
                .map_err(|e| anyhow::anyhow!("parse {}: {e}", p.display()))
        }
        None => Ok(daw_ui::theme::Theme::default()),
    }
}
