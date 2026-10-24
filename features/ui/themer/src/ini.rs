//! Round-trip editing of a `.ReaperTheme` ini.
//!
//! `daw_ui::reaper_theme::Palette` parses the file into a `HashMap`, which is all
//! a *reader* needs. An editor needs more: REAPER rewrites this file itself
//! (every color you change in its own dialogs), so we must not reorder keys,
//! drop the `[REAPER]` font blobs, or normalise anything we didn't touch —
//! otherwise every save produces a diff full of noise and a merge conflict
//! against REAPER's own writes.
//!
//! So the file is kept as a `Vec` of verbatim lines and edits rewrite exactly
//! the one line that changed. A key that doesn't exist yet is appended to its
//! section rather than dropped.

#[cfg(feature = "fs")]
use anyhow::{Context, Result};
use std::collections::HashMap;
#[cfg(feature = "fs")]
use std::path::Path;

use crate::color::Rgb;

/// The colors section. Every other section is preserved but not indexed.
const COLOR_SECTION: &str = "color theme";

/// A `.ReaperTheme` file held as lines, with an index into the color section.
#[derive(Clone, Debug)]
pub struct ThemeIni {
    lines: Vec<String>,
    /// `key` (lowercased) → index into `lines`, for `[color theme]` only.
    index: HashMap<String, usize>,
    /// Line index just past the last entry of `[color theme]`, for appends.
    section_end: usize,
    /// Whether the file ended with a newline, so we can put it back.
    trailing_newline: bool,
}

impl ThemeIni {
    /// Parse from ini text.
    pub fn parse(text: &str) -> Self {
        let trailing_newline = text.ends_with('\n');
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        let mut index = HashMap::new();
        let mut in_colors = false;
        let mut section_end = lines.len();

        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if let Some(section) = t.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                if in_colors {
                    // We just walked off the end of the color section.
                    section_end = i;
                }
                in_colors = section.eq_ignore_ascii_case(COLOR_SECTION);
                continue;
            }
            if in_colors && let Some((key, _)) = t.split_once('=') {
                index.insert(key.trim().to_ascii_lowercase(), i);
                section_end = i + 1;
            }
        }

        Self {
            lines,
            index,
            section_end,
            trailing_newline,
        }
    }

    /// Read a `.ReaperTheme` from disk.
    #[cfg(feature = "fs")]
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        Ok(Self::parse(&text))
    }

    /// Serialize back to ini text.
    pub fn to_text(&self) -> String {
        let mut out = self.lines.join("\n");
        if self.trailing_newline {
            out.push('\n');
        }
        out
    }

    /// Write back to disk.
    #[cfg(feature = "fs")]
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        std::fs::write(path, self.to_text()).with_context(|| format!("write {}", path.display()))
    }

    /// Raw integer for a color-section key.
    pub fn int(&self, key: &str) -> Option<i32> {
        let line = self
            .lines
            .get(*self.index.get(&key.to_ascii_lowercase())?)?;
        line.split_once('=')?.1.trim().parse().ok()
    }

    /// Decoded color for a key.
    pub fn color(&self, key: &str) -> Option<Rgb> {
        self.int(key).map(Rgb::from_colorref)
    }

    /// Set a raw integer, appending the key if it's new.
    pub fn set_int(&mut self, key: &str, value: i32) {
        let lower = key.to_ascii_lowercase();
        if let Some(&i) = self.index.get(&lower) {
            self.lines[i] = format!("{key}={value}");
            return;
        }
        self.lines
            .insert(self.section_end, format!("{key}={value}"));
        // Everything at or past the insert point shifted down by one.
        for i in self.index.values_mut() {
            if *i >= self.section_end {
                *i += 1;
            }
        }
        self.index.insert(lower, self.section_end);
        self.section_end += 1;
    }

    /// Set a color, preserving REAPER's flag byte on an existing key.
    pub fn set_color(&mut self, key: &str, color: Rgb) {
        let previous = self.int(key).unwrap_or(0);
        self.set_int(key, color.to_colorref_preserving(previous));
    }

    /// Every color-section key, in file order.
    /// Does this theme define `key` at all?
    ///
    /// REAPER ignores keys it does not know, so writing one is silent —
    /// which makes a typo in an override indistinguishable from a colour
    /// that "didn't take". Callers check first and report instead.
    pub fn has(&self, key: &str) -> bool {
        self.int(key).is_some()
    }

    pub fn keys(&self) -> Vec<&str> {
        let mut keys: Vec<(usize, &str)> = self
            .index
            .values()
            .filter_map(|&i| {
                let line = self.lines.get(i)?;
                Some((i, line.split_once('=')?.0.trim()))
            })
            .collect();
        keys.sort_unstable();
        keys.into_iter().map(|(_, k)| k).collect()
    }

    /// A `[REAPER]`-section value (e.g. `ui_img`), searched verbatim.
    pub fn reaper_value(&self, key: &str) -> Option<&str> {
        let mut in_reaper = false;
        for line in &self.lines {
            let t = line.trim();
            if let Some(section) = t.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                in_reaper = section.eq_ignore_ascii_case("REAPER");
                continue;
            }
            if in_reaper
                && let Some((k, v)) = t.split_once('=')
                && k.trim().eq_ignore_ascii_case(key)
            {
                return Some(v.trim());
            }
        }
        None
    }

    /// Number of indexed color keys.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}
