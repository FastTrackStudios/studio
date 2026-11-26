//! What a view draws with, read off its source, to hold it to what it
//! declares (`daw_theme::Uses`).
//!
//! Each view crate declares the tokens, pages and fields it uses, and a
//! test of its own reads its source with these and fails when it paints
//! with a name it did not declare. Read, not run: a painter's colour is a
//! field of the [`Palette`](crate::palette::Palette), so the tokens a view
//! reads are the palette fields its code names (through [`super::PALETTE`]
//! and [`super::CHROME`]), and the names it writes out in full (`"mcp.strip"`,
//! `"editor.key_white"`, `"track.name"`).
//!
//! What it sees: `palette.<field>` (any binding ending in `palette`),
//! `chrome.<field>`, a whole `palette.chrome` handed to the art (every
//! chrome token), and every string literal that is a spec name with a dot
//! in it. A field reached another way (`p.text` on a palette called `p`)
//! is not seen, so name the palette `palette`.

use std::collections::BTreeSet;

use daw_theme::spec;

pub use daw_theme::Uses;

/// The midi editor's use of the spec: `expression` and `editor_view`,
/// which go to the midi-editor crate (extraction plan, step 5) and take
/// this with them.
pub const EDITOR: daw_theme::Uses = daw_theme::Uses {
    view: "editor",
    tokens: &[
        "editor.bg",
        "editor.row_a",
        "editor.row_b",
        "editor.grid_beat",
        "editor.grid_sub",
        "editor.panel",
        "editor.panel_border",
        "editor.border_strong",
        "editor.surface_inset",
        "editor.surface_bar",
        "editor.gutter_bg",
        "editor.key_white",
        "editor.key_black",
        "editor.key_label",
        "editor.text",
        "editor.text_dim",
        "editor.text_bright",
        "editor.text_faint",
        "editor.accent",
        "editor.selected",
        "editor.playhead",
        "editor.razor",
        "editor.gold",
        "editor.zone",
        "editor.reference",
        "editor.handle",
        "editor.control",
        "editor.control_active",
        "editor.control_hover",
        "editor.control_selected",
        "editor.octave_line",
        "editor.peaks",
        "editor.pitch_0",
        "editor.pitch_1",
        "editor.pitch_2",
        "editor.pitch_3",
        "editor.pitch_4",
        "editor.pitch_5",
        "editor.pitch_6",
        "editor.pitch_7",
        "editor.pitch_8",
        "editor.pitch_9",
        "editor.pitch_10",
        "editor.pitch_11",
        "editor.drum_kick",
        "editor.drum_snare",
        "editor.drum_tom",
        "editor.drum_hat",
        "editor.drum_ride",
        "editor.drum_cymbal",
        "editor.drum_other",
        // The palette the editor's look is mapped from, before its own
        // tokens (`expression::look_of`).
        "arrange.ground",
        "arrange.row",
        "arrange.row_alt",
        "arrange.divider",
        "grid.bar",
        "grid.beat",
        "grid.sub",
        "text",
        "text.dim",
        "text.faint",
        "accent",
        "solo",
        "rec",
        "meter.warn",
        "rail.ground",
        "rail.rule",
        "rail.button",
        "tcp.tint",
        "tcp.column",
        "field",
        "hardware",
        "hardware.edge",
        "hardware.mark",
        "surface",
        "surface.raised",
        "surface.sunken",
        "border",
        "selected",
        "clip.edge",
    ],
    parts: &[
        "editor.bar",
        "editor.button",
        "editor.keys",
        "editor.key",
        "editor.keygroup",
        "editor.lane",
        "editor.ruler",
        "editor.tick",
        "editor.marker",
        "editor.strip",
        "editor.menu",
        "editor.menuitem",
    ],
    fields: &[
        "button.label",
        "button.label_size",
        "button.on",
        "button.hover",
        "button.pressed",
        "button.has_icon",
        "button.enabled",
        "button.readout",
        "key.black",
        "key.piano",
        "key.label",
        "key.labelled",
        "group.label",
        "lane.name",
        "lane.active",
        "lane.reference",
        "lane.tinted",
        "lane.tint",
        "tick.bar",
        "tick.label",
        "tick.labelled",
        "marker.label",
        "strip.label",
        "menu.label",
        "menu.shortcut",
        "menu.enabled",
        "menu.hover",
        "menu.rule",
    ],
};

/// The tokens `src` reads: through the palette's fields, and named.
#[must_use]
pub fn tokens_read(src: &str) -> BTreeSet<&'static str> {
    let mut out = BTreeSet::new();
    for (field, rest) in after(src, "palette.") {
        if field == "chrome" && !rest.starts_with('.') {
            out.extend(super::CHROME.iter().map(|(_, t)| *t));
        }
        if let Some((_, t)) = super::PALETTE.iter().find(|(f, _)| *f == field) {
            out.insert(*t);
        }
        match field {
            "track_tint" => {
                out.insert("tint.strength");
            }
            "clip_alpha" => {
                out.insert("clip.alpha");
            }
            "clips" => out.extend(super::CLIPS),
            _ => {}
        }
    }
    for (field, _) in after(src, "chrome.") {
        if let Some((_, t)) = super::CHROME.iter().find(|(f, _)| *f == field) {
            out.insert(*t);
        }
    }
    out.extend(
        literals(src)
            .filter(|l| l.contains('.'))
            .filter_map(|l| spec::token(l).map(|t| t.name)),
    );
    out
}

/// The pages `src` asks a theme for: by name, or by the kit's names for
/// them.
#[must_use]
pub fn parts_asked(src: &str) -> BTreeSet<&'static str> {
    let mut out: BTreeSet<&'static str> = literals(src)
        .filter_map(spec::part)
        .filter(|p| p.kind == spec::PartKind::Page)
        .map(|p| p.name)
        .collect();
    // `fx.macro.{phase}`: the family, built at run time.
    if src.contains("\"fx.macro.") || src.contains("\"fx.macro\"") {
        out.insert("fx.macro");
    }
    for (constant, page) in [
        ("skin::STRIP", super::STRIP),
        ("skin::RACKBACK", super::RACKBACK),
        ("skin::STEP", super::STEP),
        ("skin::ROW", super::ROW),
    ] {
        if src.contains(constant)
            && let Some(p) = spec::part(page)
        {
            out.insert(p.name);
        }
    }
    out
}

/// The fields `src` hands a page, by name.
#[must_use]
pub fn fields_given(src: &str) -> BTreeSet<&'static str> {
    literals(src)
        .filter_map(spec::field_named)
        .map(|f| f.name)
        .collect()
}

/// Every place `src` names `prefix` and an identifier after it, with
/// what follows the identifier. `prefix` must start a binding: a word
/// that ends in it (`self.palette.`, `song.palette.`) counts, one that
/// merely contains it does not.
fn after<'a>(src: &'a str, prefix: &'a str) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
    src.match_indices(prefix).filter_map(move |(at, _)| {
        let before = src.get(..at)?.chars().next_back();
        // `x_palette.` names some other palette; `.palette.`, ` palette.`,
        // `(palette.` and `&palette.` are a palette.
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            return None;
        }
        let rest = src.get(at.checked_add(prefix.len())?..)?;
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let (field, rest) = rest.split_at(end);
        (!field.is_empty()).then_some((field, rest))
    })
}

/// Every string literal in `src`, as written (escapes and all); comments
/// and char literals (`'"'`) skipped.
fn literals(src: &str) -> impl Iterator<Item = &str> {
    let mut chars = src.char_indices().peekable();
    let mut out = Vec::new();
    while let Some((at, c)) = chars.next() {
        match c {
            '/' if chars.peek().is_some_and(|(_, n)| *n == '/') => {
                for (_, n) in chars.by_ref() {
                    if n == '\n' {
                        break;
                    }
                }
            }
            // A char literal: `'x'`, `'\''`, `'"'`; a lifetime has no
            // closing quote and is left alone.
            '\'' => {
                let mut ahead = chars.clone();
                match (ahead.next(), ahead.next(), ahead.next()) {
                    (Some((_, '\\')), Some(_), Some((_, '\''))) => {
                        chars = ahead;
                    }
                    (Some(_), Some((_, '\'')), _) => {
                        chars.next();
                        chars.next();
                    }
                    _ => {}
                }
            }
            '"' => {
                let start = at.saturating_add(1);
                let mut end = src.len();
                while let Some((i, n)) = chars.next() {
                    match n {
                        '\\' => {
                            chars.next();
                        }
                        '"' => {
                            end = i;
                            break;
                        }
                        _ => {}
                    }
                }
                if let Some(l) = src.get(start..end) {
                    out.push(l);
                }
            }
            _ => {}
        }
    }
    out.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_fields_chrome_and_names_are_read() {
        let src = r#"
            painter.fill(self.palette.mute, song.palette.tcp_gutter);
            let c = palette.chrome.hardware_mark;
            art(&palette.chrome);
            let other = my_palette.solo;
            theme.token("editor.key_white");
            has_role("mcp.strip"); has_role(crate::skin::ROW);
            fields(&[("track.name", v)]);
            let q = '"'; // "not.a.literal"
            says(t, "strip.live");
        "#;
        let t = tokens_read(src);
        for want in [
            "mute",
            "rail.ground",
            "hardware.mark",
            "surface.raised",
            "editor.key_white",
        ] {
            assert!(t.contains(want), "{want} in {t:?}");
        }
        assert!(!t.contains("solo"), "my_palette is not the palette");
        let p = parts_asked(src);
        assert!(p.contains("mcp.strip") && p.contains("tcp.row"), "{p:?}");
        assert_eq!(
            fields_given(src).into_iter().collect::<Vec<_>>(),
            ["track.name"]
        );
        assert!(
            t.contains("strip.live"),
            "a char literal must not shift the quotes"
        );
    }

    #[test]
    fn the_palette_tables_name_spec_tokens_and_real_fields() {
        let mut p = crate::palette::Palette::from_theme(&daw_ui::theming::Theme::dark());
        for (field, token) in super::super::PALETTE {
            assert!(spec::token(token).is_some(), "{token}");
            assert!(
                super::super::palette_field(&mut p, field).is_some(),
                "{field}"
            );
        }
        for (field, token) in super::super::CHROME {
            assert!(spec::token(token).is_some(), "{token}");
            assert!(
                super::super::chrome_field(&mut p.chrome, field).is_some(),
                "{field}"
            );
        }
        for token in super::super::CLIPS {
            assert!(spec::token(token).is_some(), "{token}");
        }
    }

    #[test]
    fn the_editor_declares_names_the_spec_has() {
        assert_eq!(EDITOR.check(), Vec::new());
    }

    /// The editor's source (`expression`, `editor_view`) paints only with
    /// what [`EDITOR`] declares.
    #[test]
    fn the_editor_paints_with_what_it_declares() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../views/midi/view/src");
        let src: String = ["expression.rs", "editor_view.rs"]
            .iter()
            .map(|f| std::fs::read_to_string(dir.join(f)).expect("source"))
            .collect();
        for t in tokens_read(&src) {
            assert!(
                EDITOR.reads(t),
                "the editor reads `{t}` and does not declare it"
            );
        }
        for p in parts_asked(&src) {
            assert!(
                EDITOR.asks_for(p),
                "the editor asks for `{p}` and does not declare it"
            );
        }
        for f in fields_given(&src) {
            assert!(
                EDITOR.fields.contains(&f),
                "the editor gives `{f}` and does not declare it"
            );
        }
    }
}
