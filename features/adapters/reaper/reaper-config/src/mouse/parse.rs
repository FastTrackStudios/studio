//! INI parser for `reaper-mouse.ini` and `.ReaperMouseMap` files.

use super::types::{
    ActionValue, ContextKind, HasImportedEntry, ModifierIndex, MouseBinding, MouseConfig,
    MouseContext,
};
use thiserror::Error;

/// Errors that can occur while parsing a mouse config file.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("invalid integer on line {line}: {source}")]
    InvalidInt {
        line: usize,
        #[source]
        source: std::num::ParseIntError,
    },

    #[error("missing '=' on line {line}: {content:?}")]
    MissingEquals { line: usize, content: String },

    #[error("invalid mm_N key on line {line}: {content:?}")]
    InvalidModifierKey { line: usize, content: String },

    #[error("invalid binding value on line {line}: {content:?}")]
    InvalidBindingValue { line: usize, content: String },
}

/// Parse an `ActionValue` from the right-hand side of a `mm_N=…` entry.
fn parse_action_value(rhs: &str, line: usize) -> Result<ActionValue, ParseError> {
    let rhs = rhs.trim();

    // Named action command ID.
    if rhs.starts_with('_') {
        return Ok(ActionValue::Named(rhs.to_string()));
    }

    // Split on whitespace.
    let mut parts = rhs.splitn(2, ' ');
    let num_str = parts.next().unwrap_or("").trim();
    let suffix = parts.next().map(|s| s.trim());

    let n = num_str
        .parse::<i32>()
        .map_err(|e| ParseError::InvalidInt { line, source: e })?;

    match suffix {
        Some("m") | None => Ok(ActionValue::MouseBehavior(n)),
        Some("c") => Ok(ActionValue::CommandId(n)),
        Some(_) => Err(ParseError::InvalidBindingValue {
            line,
            content: rhs.to_string(),
        }),
    }
}

/// Try to parse a `mm_N` key into its modifier index.
fn parse_modifier_key(key: &str, line: usize) -> Result<Option<u8>, ParseError> {
    let lower = key.to_ascii_lowercase();
    if !lower.starts_with("mm_") {
        return Ok(None);
    }
    let index_str = &lower[3..];
    let idx = index_str
        .parse::<u8>()
        .map_err(|e| ParseError::InvalidInt { line, source: e })?;
    Ok(Some(idx))
}

/// Parse the contents of a `reaper-mouse.ini` or `.ReaperMouseMap` file.
pub fn parse(input: &str) -> Result<MouseConfig, ParseError> {
    let mut config = MouseConfig::new_mouse_map();

    enum Section {
        None,
        HasImported,
        Context(MouseContext),
    }

    let mut state = Section::None;

    let lines: Vec<&str> = input.lines().collect();

    let flush = |state: Section, config: &mut MouseConfig| {
        if let Section::Context(ctx) = state {
            config.contexts.push(ctx);
        }
    };

    for (idx, raw_line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let line = raw_line.trim_end_matches('\r').trim();

        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let section_name = &line[1..line.len() - 1];

            let prev = std::mem::replace(&mut state, Section::None);
            flush(prev, &mut config);

            if section_name.eq_ignore_ascii_case("hasimported") {
                config.has_imported = Some(Vec::new());
                state = Section::HasImported;
            } else {
                let kind = ContextKind::from_reaper_str(section_name);
                state = Section::Context(MouseContext::new(kind));
            }
            continue;
        }

        let eq_pos = line.find('=').ok_or_else(|| ParseError::MissingEquals {
            line: line_num,
            content: line.to_string(),
        })?;

        let key = line[..eq_pos].trim();
        let value = line[eq_pos + 1..].trim();

        match &mut state {
            Section::None => {}

            Section::HasImported => {
                let n = value.parse::<i32>().map_err(|e| ParseError::InvalidInt {
                    line: line_num,
                    source: e,
                })?;
                if let Some(ref mut entries) = config.has_imported {
                    entries.push(HasImportedEntry {
                        key: key.to_string(),
                        value: n,
                    });
                }
            }

            Section::Context(ctx) => {
                if key.eq_ignore_ascii_case("name") {
                    ctx.name = Some(value.to_string());
                } else {
                    if let Some(idx) = parse_modifier_key(key, line_num)? {
                        let action = parse_action_value(value, line_num)?;
                        ctx.bindings.push(MouseBinding {
                            modifier: ModifierIndex(idx),
                            action,
                        });
                    }
                }
            }
        }
    }

    flush(state, &mut config);

    Ok(config)
}
