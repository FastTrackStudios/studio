//! Theming the colour literals inside `rtconfig.txt`.
//!
//! The fourth place REAPER takes colour from, and the least obvious:
//! WALTER scripts carry **hardcoded RGB literals**.
//!
//! ```text
//! set mcp_bg_color  theme_version>1 [0 0 0 0 61 61 61] [0 0 0 0 51 51 51]
//! ```
//!
//! `61 61 61` is the mixer strip body. It is not a palette key, not an
//! image, and not SWELL — so a theme can have all three of those perfectly
//! dark and still render its mixer in the original's grey, which is exactly
//! what happened here.
//!
//! # Only `*color*` assignments
//!
//! WALTER's bracket syntax is overloaded: `[x y w h ls ts rs bs]` is a
//! rectangle and `[r g b]` is a colour, with no syntactic difference. So
//! rewriting every bracket would silently relayout the entire theme.
//!
//! Only lines assigning a variable whose **name contains `color`** are
//! touched, and within those only the trailing 3-tuples — which covers the
//! `[r g b]`, `[r g b a]` and `[0 0 0 0 r g b]` forms REAPER uses without
//! having to parse WALTER properly.

use anyhow::{Context, Result};
use std::path::Path;

use daw_ui::theme::Ramp;

/// A literal that was rewritten.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub line: usize,
    pub before: String,
    pub after: String,
}

/// Rewrite the colour literals in `text` through `ramp`.
pub fn retint(text: &str, ramp: &Ramp) -> (String, Vec<Change>) {
    let mut changes = Vec::new();
    let out: Vec<String> = text
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if !is_color_assignment(line) {
                return line.to_string();
            }
            let rewritten = rewrite_line(line, ramp);
            if rewritten != line {
                changes.push(Change {
                    line: i + 1,
                    before: line.trim().to_string(),
                    after: rewritten.trim().to_string(),
                });
            }
            rewritten
        })
        .collect();

    let mut joined = out.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    (joined, changes)
}

/// Does this line assign to a variable whose name says it is a colour?
fn is_color_assignment(line: &str) -> bool {
    let t = line.trim_start();
    let Some(rest) = t.strip_prefix("set ") else {
        return false;
    };
    let Some(name) = rest.split_whitespace().next() else {
        return false;
    };
    // `set mcp.volume.color …` and `set gl_pan_color …` both qualify;
    // `set tcp.volume [x y w h]` does not.
    name.contains("color") || name.contains("colour")
}

/// Rewrite every bracketed literal on one line.
fn rewrite_line(line: &str, ramp: &Ramp) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;

    while let Some(open) = rest.find('[') {
        let Some(close_rel) = rest[open..].find(']') else {
            break;
        };
        let close = open + close_rel;
        out.push_str(&rest[..=open]);
        let inner = &rest[open + 1..close];
        out.push_str(&rewrite_literal(inner, ramp));
        out.push(']');
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Rewrite the numbers inside one `[...]`.
///
/// Only *all-numeric* literals are touched: WALTER also puts variable names
/// in brackets (`[trackcolor_r trackcolor_g trackcolor_b]`), and those are
/// already dynamic — rewriting them would replace a live track colour with
/// a constant.
fn rewrite_literal(inner: &str, ramp: &Ramp) -> String {
    let parts: Vec<&str> = inner.split_whitespace().collect();
    let nums: Option<Vec<u32>> = parts.iter().map(|p| p.parse::<u32>().ok()).collect();
    let Some(nums) = nums else {
        return inner.to_string();
    };
    if nums.iter().any(|n| *n > 255) {
        // Out of colour range — a coordinate or a flag word.
        return inner.to_string();
    }

    // The RGB triple is the last three values: `[r g b]`, `[r g b a]` has
    // alpha last so is handled below, and `[0 0 0 0 r g b]` prefixes it.
    let (rgb_at, keeps_alpha) = match nums.len() {
        3 => (0, false),
        4 => (0, true),
        7 => (4, false),
        _ => return inner.to_string(),
    };

    let c = daw_ui::theme::Color::rgb(
        nums[rgb_at] as u8,
        nums[rgb_at + 1] as u8,
        nums[rgb_at + 2] as u8,
    );
    let m = ramp.apply(c);

    let mut out: Vec<String> = nums.iter().map(|n| n.to_string()).collect();
    out[rgb_at] = m.r.to_string();
    out[rgb_at + 1] = m.g.to_string();
    out[rgb_at + 2] = m.b.to_string();
    let _ = keeps_alpha; // alpha is at index 3 and deliberately untouched
    out.join(" ")
}

/// Where the pristine rtconfig is kept, beside the pristine artwork.
///
/// A luminance ramp is not idempotent — it compounds on its own output, so
/// a second run darkens again and there is no way back to try a different
/// palette. Exactly the problem `restyle` has with images, and it takes
/// exactly the same answer: always retint from an untouched copy.
pub const SOURCE_NAME: &str = "rtconfig.txt";

/// Retint the rtconfig at `path`, always from the pristine copy in
/// `source_dir`, which is created from `path` on first use.
pub fn retint_file_from(
    path: &Path,
    source_dir: &Path,
    ramp: &Ramp,
    dry_run: bool,
) -> Result<Vec<Change>> {
    let pristine = source_dir.join(SOURCE_NAME);
    if !pristine.is_file() {
        if dry_run {
            anyhow::bail!(
                "no pristine rtconfig yet — run without --dry-run once to create {}",
                pristine.display()
            );
        }
        std::fs::create_dir_all(source_dir)?;
        std::fs::copy(path, &pristine).with_context(|| format!("snapshot {}", path.display()))?;
    }

    let text = std::fs::read_to_string(&pristine)
        .with_context(|| format!("read {}", pristine.display()))?;
    let (out, changes) = retint(&text, ramp);

    // Compare against what is live, not against the pristine source, or
    // every run reports the whole file as changed.
    let current = std::fs::read_to_string(path).unwrap_or_default();
    if out == current {
        return Ok(Vec::new());
    }
    if !dry_run {
        std::fs::write(path, out).with_context(|| format!("write {}", path.display()))?;
    }
    Ok(changes)
}
