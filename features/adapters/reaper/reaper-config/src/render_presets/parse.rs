//! Parsing logic for REAPER render preset (`reaper-render.ini`) files.

use super::types::*;
use thiserror::Error;

/// Errors that can occur while parsing a `reaper-render.ini` file.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("not enough fields on {kind} line {line}: {content}")]
    TooFewFields {
        kind: &'static str,
        line: usize,
        content: String,
    },

    #[error("invalid integer on line {line}: {source}")]
    InvalidInt {
        line: usize,
        #[source]
        source: std::num::ParseIntError,
    },

    #[error("invalid float on line {line}: {source}")]
    InvalidFloat {
        line: usize,
        #[source]
        source: std::num::ParseFloatError,
    },

    #[error("unterminated block <{kind}> starting at line {line}")]
    UnterminatedBlock { kind: String, line: usize },

    #[error("unterminated quoted string on line {line}")]
    UnterminatedQuote { line: usize },

    #[error("unexpected content inside block at line {line}: {content}")]
    UnexpectedBlockContent { line: usize, content: String },
}

// ---------------------------------------------------------------------------
// Token helpers
// ---------------------------------------------------------------------------

/// Parse a preset name token from the front of `s`.
///
/// REAPER quotes names that contain spaces: `"my preset"`. Unquoted names are
/// terminated by the next whitespace character.
///
/// Returns `(name, rest_of_line)`.
fn parse_name(s: &str, line_num: usize) -> Result<(String, &str), ParseError> {
    let s = s.trim_start();
    if let Some(after_open) = s.strip_prefix('"') {
        // Quoted name.
        if let Some(close) = after_open.find('"') {
            let name = after_open[..close].to_string();
            let rest = after_open[close + 1..].trim_start();
            Ok((name, rest))
        } else {
            Err(ParseError::UnterminatedQuote { line: line_num })
        }
    } else {
        // Unquoted: everything up to the next whitespace.
        let end = s.find(|c: char| c.is_whitespace()).unwrap_or(s.len());
        let name = s[..end].to_string();
        let rest = s[end..].trim_start();
        Ok((name, rest))
    }
}

/// Parse a quoted-or-bare string token (used for filenames / directories).
///
/// - If the remaining text starts with `"`, read until the matching `"`.
/// - Otherwise consume all remaining text as a single token (no embedded spaces).
///
/// Returns `(value, rest_of_line)`.
fn parse_string_token(s: &str, line_num: usize) -> Result<(String, &str), ParseError> {
    let s = s.trim_start();
    if let Some(after_open) = s.strip_prefix('"') {
        if let Some(close) = after_open.find('"') {
            let value = after_open[..close].to_string();
            let rest = after_open[close + 1..].trim_start();
            Ok((value, rest))
        } else {
            Err(ParseError::UnterminatedQuote { line: line_num })
        }
    } else {
        // Consume until whitespace.
        let end = s.find(|c: char| c.is_whitespace()).unwrap_or(s.len());
        let value = s[..end].to_string();
        let rest = s[end..].trim_start();
        Ok((value, rest))
    }
}

/// Split `s` on whitespace, returning the first token and the remainder.
fn next_token(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    let end = s.find(|c: char| c.is_whitespace()).unwrap_or(s.len());
    Some((&s[..end], s[end..].trim_start()))
}

/// Parse an integer from the next whitespace-delimited token.
fn parse_u32(s: &str, line_num: usize) -> Result<(u32, &str), ParseError> {
    let (tok, rest) = next_token(s).ok_or_else(|| ParseError::TooFewFields {
        kind: "integer",
        line: line_num,
        content: s.to_string(),
    })?;
    let n = tok.parse::<u32>().map_err(|e| ParseError::InvalidInt {
        line: line_num,
        source: e,
    })?;
    Ok((n, rest))
}

/// Parse a float from the next whitespace-delimited token.
fn parse_f64(s: &str, line_num: usize) -> Result<(f64, &str), ParseError> {
    let (tok, rest) = next_token(s).ok_or_else(|| ParseError::TooFewFields {
        kind: "float",
        line: line_num,
        content: s.to_string(),
    })?;
    let f = tok.parse::<f64>().map_err(|e| ParseError::InvalidFloat {
        line: line_num,
        source: e,
    })?;
    Ok((f, rest))
}

// ---------------------------------------------------------------------------
// Block parsers
// ---------------------------------------------------------------------------

/// Parse a `<RENDERPRESET ...>` block.
///
/// `header` is the text after `<RENDERPRESET ` on the opening line.
/// `body_line` is the single indented line inside the block.
/// `line_num` is the 1-based line number of the opening `<RENDERPRESET` line.
fn parse_renderpreset_block(
    header: &str,
    body_line: &str,
    line_num: usize,
) -> Result<RenderPreset, ParseError> {
    let (name, rest) = parse_name(header, line_num)?;
    let (sample_rate, rest) = parse_u32(rest, line_num)?;
    let (channels, rest) = parse_u32(rest, line_num)?;
    let (offline_mode, rest) = parse_u32(rest, line_num)?;
    let (use_project_sample_rate, rest) = parse_u32(rest, line_num)?;
    let (resample_mode, rest) = parse_u32(rest, line_num)?;
    let (various_checkboxes, rest) = parse_u32(rest, line_num)?;
    let (various_checkboxes2, _rest) = parse_u32(rest, line_num)?;

    let render_cfg = body_line.trim().to_string();

    Ok(RenderPreset {
        name,
        sample_rate,
        channels,
        offline_mode,
        use_project_sample_rate,
        resample_mode,
        various_checkboxes,
        various_checkboxes2,
        render_cfg,
    })
}

/// Parse a `<RENDERPRESET2 ...>` block.
fn parse_renderpreset2_block(
    header: &str,
    body_line: &str,
    line_num: usize,
) -> Result<RenderPreset2, ParseError> {
    let (name, _rest) = parse_name(header, line_num)?;
    let render_cfg = body_line.trim().to_string();
    Ok(RenderPreset2 { name, render_cfg })
}

// ---------------------------------------------------------------------------
// Flat-line parsers
// ---------------------------------------------------------------------------

/// Parse a `RENDERPRESET_OUTPUT ...` line.
///
/// Format:
/// ```text
/// RENDERPRESET_OUTPUT presetname bounds start end source unknown filename tail dir tail_ms
/// ```
fn parse_renderpreset_output(
    rest: &str,
    line_num: usize,
) -> Result<RenderPresetOutput, ParseError> {
    let (name, rest) = parse_name(rest, line_num)?;
    let (bounds, rest) = parse_u32(rest, line_num)?;
    let (start, rest) = parse_f64(rest, line_num)?;
    let (end, rest) = parse_f64(rest, line_num)?;
    let (source, rest) = parse_u32(rest, line_num)?;
    let (unknown, rest) = parse_u32(rest, line_num)?;
    let (filename, rest) = parse_string_token(rest, line_num)?;
    let (tail, rest) = parse_u32(rest, line_num)?;
    let (directory, rest) = parse_string_token(rest, line_num)?;
    let (tail_ms, _rest) = parse_u32(rest, line_num)?;

    Ok(RenderPresetOutput {
        name,
        bounds,
        start,
        end,
        source,
        unknown,
        filename,
        tail,
        directory,
        tail_ms,
    })
}

/// Parse a `RENDERPRESET_EXT ...` line.
///
/// Format:
/// ```text
/// RENDERPRESET_EXT presetname normalize_mode normalize_target brickwall_target
///   fadein_length fadeout_length fadein_shape fadeout_shape
///   trim_lead trim_trail pad_start pad_end
/// ```
fn parse_renderpreset_ext(rest: &str, line_num: usize) -> Result<RenderPresetExt, ParseError> {
    let (name, rest) = parse_name(rest, line_num)?;
    let (normalize_mode, rest) = parse_u32(rest, line_num)?;
    let (normalize_target, rest) = parse_f64(rest, line_num)?;
    let (brickwall_target, rest) = parse_f64(rest, line_num)?;
    let (fadein_length, rest) = parse_f64(rest, line_num)?;
    let (fadeout_length, rest) = parse_f64(rest, line_num)?;
    let (fadein_shape, rest) = parse_u32(rest, line_num)?;
    let (fadeout_shape, rest) = parse_u32(rest, line_num)?;
    let (trim_lead, rest) = parse_f64(rest, line_num)?;
    let (trim_trail, rest) = parse_f64(rest, line_num)?;
    let (pad_start, rest) = parse_f64(rest, line_num)?;
    let (pad_end, _rest) = parse_f64(rest, line_num)?;

    Ok(RenderPresetExt {
        name,
        normalize_mode,
        normalize_target,
        brickwall_target,
        fadein_length,
        fadeout_length,
        fadein_shape,
        fadeout_shape,
        trim_lead,
        trim_trail,
        pad_start,
        pad_end,
    })
}

// ---------------------------------------------------------------------------
// Top-level parser
// ---------------------------------------------------------------------------

/// Parse the full contents of a `reaper-render.ini` file.
pub fn parse(input: &str) -> Result<RenderPresetFile, ParseError> {
    let mut entries = Vec::new();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let raw_line = lines[i];
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim();
        let line_num = i + 1;

        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        if let Some(header) = trimmed.strip_prefix("<RENDERPRESET2 ") {
            // Multi-line block: read body line and closing `>`.
            let block_start = line_num;
            i += 1;
            let body_line = if i < lines.len() {
                lines[i].trim_end_matches('\r')
            } else {
                return Err(ParseError::UnterminatedBlock {
                    kind: "RENDERPRESET2".to_string(),
                    line: block_start,
                });
            };
            i += 1;
            // Expect closing `>`.
            if i >= lines.len() || lines[i].trim_end_matches('\r').trim() != ">" {
                return Err(ParseError::UnterminatedBlock {
                    kind: "RENDERPRESET2".to_string(),
                    line: block_start,
                });
            }
            let preset2 = parse_renderpreset2_block(header, body_line, block_start)?;
            entries.push(RenderPresetEntry::Preset2(preset2));
        } else if let Some(header) = trimmed.strip_prefix("<RENDERPRESET ") {
            // Multi-line block: read body line and closing `>`.
            let block_start = line_num;
            i += 1;
            let body_line = if i < lines.len() {
                lines[i].trim_end_matches('\r')
            } else {
                return Err(ParseError::UnterminatedBlock {
                    kind: "RENDERPRESET".to_string(),
                    line: block_start,
                });
            };
            i += 1;
            // Expect closing `>`.
            if i >= lines.len() || lines[i].trim_end_matches('\r').trim() != ">" {
                return Err(ParseError::UnterminatedBlock {
                    kind: "RENDERPRESET".to_string(),
                    line: block_start,
                });
            }
            let preset = parse_renderpreset_block(header, body_line, block_start)?;
            entries.push(RenderPresetEntry::Preset(preset));
        } else if let Some(rest) = trimmed.strip_prefix("RENDERPRESET_OUTPUT ") {
            let output = parse_renderpreset_output(rest, line_num)?;
            entries.push(RenderPresetEntry::Output(output));
        } else if let Some(rest) = trimmed.strip_prefix("RENDERPRESET_EXT ") {
            let ext = parse_renderpreset_ext(rest, line_num)?;
            entries.push(RenderPresetEntry::Ext(ext));
        }
        // Silently skip unrecognized lines.

        i += 1;
    }

    Ok(RenderPresetFile { entries })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
