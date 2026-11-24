//! What the mixer draws with from a theme pack (daw-theme, daw#52): the
//! tokens its painters read, the pages it asks for (the strip, the rack,
//! the rails, the macros) and the fields it hands them. A pack implements
//! these; the test below fails when the mixer paints with a name it does
//! not declare here, or declares one the spec lacks.

use crate::skin::uses::Uses;

/// The mixer's use of the spec, its rails included (drawn by the kit's
/// `rails` until they move here).
pub const USES: Uses = Uses {
    view: "mixer",
    tokens: &[
        "text",
        "text.dim",
        "text.faint",
        "accent",
        "selected",
        "surface",
        "surface.raised",
        "surface.sunken",
        "border",
        "hardware",
        "hardware.edge",
        "hardware.mark",
        "plate",
        "sheen",
        "field",
        "combo",
        "mute",
        "solo",
        "rec",
        "pan",
        "monitor.tape",
        "route.out",
        "route.send",
        "route.recv",
        "meter.safe",
        "meter.warn",
        "meter.danger",
        "meter.well",
        "tcp.tint",
        "tcp.column",
        "rail.ground",
        "rail.rule",
        "rail.button",
        "rack.well",
        "tint.strength",
        "strip.live",
        "strip.touch",
        "arrange.ground",
        "arrange.row",
        "arrange.row_alt",
        "arrange.divider",
        "grid.bar",
        "grid.beat",
        "grid.sub",
    ],
    parts: &[
        "mcp.strip",
        "mcp.rackback",
        "mcp.step",
        "rails",
        "rail.button",
        "rack.unit",
        "rack.rail",
        "rack.phase",
        "fx.phase",
        "fx.macro",
    ],
    fields: &[
        "track.name",
        "track.label",
        "track.number",
        "track.color",
        "track.has_color",
        "track.selected",
        "track.folder",
        "track.collapsed",
        "track.depth",
        "track.input",
        "track.ground",
        "track.edge",
        "track.has_edge",
        "track.tint",
        "track.band",
        "track.label_size",
        "track.parent_send",
        "track.mute",
        "track.solo",
        "track.armed",
        "track.monitor",
        "track.phase",
        "track.fader",
        "track.volume",
        "track.pan",
        "track.fx",
        "track.sends",
        "track.receives",
        "track.volume_label",
        "track.volume_label_w",
        "meter.left",
        "meter.right",
        "meter.hold_left",
        "meter.hold_right",
        "meter.clip",
        "meter.peak",
        "ui.hover",
        "ui.pressed",
        "ui.recording",
        "ui.mixing",
        "ui.rack",
        "folder.tint",
        "folder.level",
        "mixer.h",
        "button.label",
        "button.label_size",
        "button.on",
        "button.hover",
        "button.pressed",
        "button.has_icon",
        "button.enabled",
        "button.readout",
        "unit.name",
        "unit.value",
        "unit.bypassed",
        "unit.glyph",
        "unit.expands",
        "unit.zoom_lit",
        "phase.name",
        "phase.tint",
        "phase.blank",
        "phase.shut",
        "phase.held",
        "macro.name",
        "macro.value",
        "macro.says",
        "macro.tint",
        "macro.held",
    ],
};

#[cfg(test)]
mod tests {
    use super::USES;
    use crate::skin::uses::{fields_given, parts_asked, tokens_read};

    /// Every `.rs` under this crate's `src` but this one, read whole.
    fn source() -> String {
        fn walk(dir: &std::path::Path, out: &mut String) {
            for entry in std::fs::read_dir(dir).expect("src").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs")
                    // This file names everything it declares.
                    && !path.ends_with("src/theme.rs")
                {
                    out.push_str(&std::fs::read_to_string(&path).expect("read"));
                }
            }
        }
        let mut out = String::new();
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut out,
        );
        out
    }

    #[test]
    fn the_spec_has_every_name_the_mixer_declares() {
        assert_eq!(USES.check(), Vec::new());
    }

    #[test]
    fn the_mixer_paints_only_with_what_it_declares() {
        let src = source();
        for t in tokens_read(&src) {
            assert!(
                USES.reads(t),
                "the mixer reads `{t}` and does not declare it"
            );
        }
        for p in parts_asked(&src) {
            assert!(
                USES.asks_for(p),
                "the mixer asks for `{p}` and does not declare it"
            );
        }
        for f in fields_given(&src) {
            assert!(
                USES.fields.contains(&f),
                "the mixer gives `{f}` and does not declare it"
            );
        }
    }
}
