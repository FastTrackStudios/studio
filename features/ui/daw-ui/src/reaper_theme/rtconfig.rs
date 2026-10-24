//! `rtconfig.txt` — global directives + `define_parameter` knobs.
//!
//! Only the *global* (pre-WALTER) layer is parsed here: the top-of-file
//! directives (`mcp_min_height`, `*_zeroline` colors, `misc_dpi_translate`, …)
//! and every `define_parameter` line wherever it appears. The WALTER layout
//! program itself (macros / `set` / `Layout` blocks) is a later phase.

use crate::reaper_theme::palette::Rgba;
use std::collections::HashMap;

/// One `define_parameter "name" "description" default min max` knob.
#[derive(Clone, PartialEq, Debug)]
pub struct ThemeParamDef {
    pub name: String,
    pub desc: String,
    pub default: f32,
    pub min: f32,
    pub max: f32,
}

/// Parsed rtconfig globals.
#[derive(Clone, Debug, Default)]
pub struct RtConfig {
    /// Raw global directives: first token → remaining tokens. Repeated keys
    /// keep the *first* occurrence (REAPER reads top-down), except
    /// `misc_dpi_translate` which accumulates in [`RtConfig::dpi_translate`].
    pub globals: HashMap<String, Vec<String>>,
    /// All `define_parameter` knobs, in file order.
    pub params: Vec<ThemeParamDef>,
    /// `misc_dpi_translate <min_dpi%> <subfolder>` lines, in order.
    pub dpi_translate: Vec<(f32, String)>,
}

impl RtConfig {
    pub fn parse(text: &str) -> Self {
        let mut cfg = Self::default();
        let mut in_layout_or_macro = 0usize;

        for raw_line in text.lines() {
            // Strip comments (`;`), keeping quoted strings intact is not
            // needed for globals — REAPER's own globals never quote `;`.
            let line = match raw_line.split_once(';') {
                Some((before, _)) => before,
                None => raw_line,
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let mut tokens = tokenize(line);
            if tokens.is_empty() {
                continue;
            }
            let head = tokens.remove(0);
            let head_lc = head.to_ascii_lowercase();

            match head_lc.as_str() {
                // Track nesting so we stop collecting globals inside blocks
                // (define_parameter is still collected anywhere).
                "layout" | "macro" => in_layout_or_macro += 1,
                "endlayout" | "endmacro" => {
                    in_layout_or_macro = in_layout_or_macro.saturating_sub(1)
                }
                "define_parameter" => {
                    if let Some(p) = parse_param(&tokens) {
                        cfg.params.push(p);
                    }
                }
                "misc_dpi_translate" => {
                    if let (Some(dpi), Some(folder)) = (
                        tokens.first().and_then(|t| t.parse::<f32>().ok()),
                        tokens.get(1),
                    ) {
                        cfg.dpi_translate.push((dpi, folder.clone()));
                    }
                }
                _ if in_layout_or_macro == 0 && !head_lc.starts_with("set ") => {
                    cfg.globals.entry(head_lc).or_insert(tokens);
                }
                _ => {}
            }
        }
        cfg
    }

    /// A global's first value as an f32 (`mcp_min_height`, `tcp_vupeakwidth`…).
    pub fn global_f32(&self, key: &str) -> Option<f32> {
        self.globals.get(key)?.first()?.parse().ok()
    }

    /// A global's first value as an `AABBGGRR` hex color (`*_zeroline`,
    /// `item_volknobfg`…).
    pub fn global_color(&self, key: &str) -> Option<Rgba> {
        let hex = self.globals.get(key)?.first()?;
        u32::from_str_radix(hex, 16).ok().map(Rgba::from_aabbggrr)
    }
}

/// Split a line into tokens, honouring single/double quotes.
fn tokenize(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '\'' || c == '"' => quote = Some(c),
            None if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_param(tokens: &[String]) -> Option<ThemeParamDef> {
    let name = tokens.first()?.clone();
    let desc = tokens.get(1)?.clone();
    let num = |i: usize| tokens.get(i).and_then(|t| t.parse::<f32>().ok());
    Some(ThemeParamDef {
        name,
        desc,
        default: num(2)?,
        min: num(3)?,
        max: num(4)?,
    })
}
