//! Serialization logic for REAPER menu/toolbar configuration files.
//!
//! The output uses `\r\n` (CRLF) line endings, matching what REAPER writes.
//! Sections are separated by a single blank line.

use super::types::*;

impl MenuConfig {
    /// Serialize this [`MenuConfig`] back to the `reaper-menu.ini` text format.
    ///
    /// Each line is terminated with `\r\n` (CRLF) to match REAPER's convention.
    /// Sections are separated by a single blank line.
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        for (i, section) in self.sections.iter().enumerate() {
            if i > 0 {
                out.push_str("\r\n");
            }
            serialize_section(section, &mut out);
        }
        out
    }
}

fn serialize_section(section: &MenuSection, out: &mut String) {
    // Header line
    out.push('[');
    out.push_str(section.id.to_header());
    out.push_str("]\r\n");

    // Items – we emit them 0-based and in order.
    // Icon lines must appear *before* the matching item lines in the output.
    // Collect icon entries first so we can emit them together.
    for (n, item) in section.items.iter().enumerate() {
        // Emit icon line if this item carries one (toolbar items).
        if let MenuItem::Action(a) = item
            && let Some(icon) = &a.icon
        {
            out.push_str(&format!("icon_{}={}\r\n", n, icon.to_token()));
        }
        // Emit the item line.
        out.push_str(&serialize_item(n, item));
    }

    // Optional title line (written after items, matching REAPER's output order).
    if let Some(title) = &section.title {
        out.push_str(&format!("title={}\r\n", title));
    }
}

fn serialize_item(n: usize, item: &MenuItem) -> String {
    match item {
        MenuItem::Separator => format!("item_{}=-1\r\n", n),

        MenuItem::SubMenuStart { title } => {
            // Use the `-2 <title>` form (works for both menus and toolbars).
            if title.is_empty() {
                format!("item_{}=-2\r\n", n)
            } else {
                format!("item_{}=-2 {}\r\n", n, title)
            }
        }

        MenuItem::SubMenuEnd => format!("item_{}=-3\r\n", n),

        MenuItem::Label { text } => {
            if text.is_empty() {
                format!("item_{}=-4\r\n", n)
            } else {
                format!("item_{}=-4 {}\r\n", n, text)
            }
        }

        MenuItem::Action(a) => serialize_action_item(n, a),
    }
}

fn serialize_action_item(n: usize, a: &ActionItem) -> String {
    let cmd = a.command.to_token();

    // Toolbar items (icon present or non-zero flags) use `<flags> <cmd> [title]` format.
    // Menu items use `<cmd> [title]` format.
    let is_toolbar_style = a.icon.is_some() || a.flags != 0;

    let title_part = match &a.title {
        None => String::new(),
        Some(t) => {
            // Only quote if the title contains whitespace or special chars.
            if t.contains('"') || t.is_empty() || needs_quoting(t) {
                format!(" \"{}\"", t)
            } else {
                format!(" {}", t)
            }
        }
    };

    if is_toolbar_style {
        format!("item_{}={} {}{}\r\n", n, a.flags, cmd, title_part)
    } else {
        format!("item_{}={}{}\r\n", n, cmd, title_part)
    }
}

/// Returns `true` if a title string should be wrapped in double-quotes.
///
/// We quote when the title starts with a digit (could be confused with a
/// command ID) or contains characters that would break the simple split.
fn needs_quoting(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------
