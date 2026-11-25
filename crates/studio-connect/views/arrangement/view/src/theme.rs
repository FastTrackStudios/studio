//! What the arrangement draws with from a theme pack (daw-theme, daw#52):
//! the tokens its painters read, the pages it asks for (the track panel's
//! row) and the fields it hands them. A pack implements these; the test
//! below fails when the arrangement paints with a name it does not
//! declare here, or declares one the spec lacks.

use crate::skin::uses::Uses;

/// The arrangement's use of the spec, the track control panel's included.
pub const USES: Uses = Uses {
    view: "arrangement",
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
        "field",
        "mute",
        "solo",
        "rec",
        "pan",
        "meter.safe",
        "meter.warn",
        "meter.danger",
        "tcp.tint",
        "tcp.column",
        "rail.ground",
        "rail.rule",
        "rail.button",
        "combo",
        "rack.well",
        "tint.strength",
        "arrange.ground",
        "arrange.row",
        "arrange.row_alt",
        "arrange.divider",
        "grid.bar",
        "grid.beat",
        "grid.sub",
        "ruler.ground",
        "ruler.ink",
        "ruler.ink_dim",
        "playhead",
        "cursor.edit",
        "clip.edge",
        "clip.alpha",
        "clip.0",
        "clip.1",
        "clip.2",
        "clip.3",
        "clip.4",
        "clip.5",
        "clip.6",
        "clip.7",
        "clip.8",
        "clip.9",
        "clip.10",
        "clip.11",
        "clip.12",
        "clip.13",
        "clip.14",
        "clip.15",
    ],
    parts: &["tcp.row"],
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
        "row.indent",
        "row.number",
        "row.number_size",
        "row.number_x",
        "row.number_baseline",
        "row.has_number",
        "row.icon",
        "row.icon_x",
        "row.icon_y",
        "row.icon_size",
        "row.name",
        "row.name_size",
        "row.input",
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
    fn the_spec_has_every_name_the_arrangement_declares() {
        assert_eq!(USES.check(), Vec::new());
    }

    #[test]
    fn the_arrangement_paints_only_with_what_it_declares() {
        let src = source();
        for t in tokens_read(&src) {
            assert!(
                USES.reads(t),
                "the arrangement reads `{t}` and does not declare it"
            );
        }
        for p in parts_asked(&src) {
            assert!(
                USES.asks_for(p),
                "the arrangement asks for `{p}` and does not declare it"
            );
        }
        for f in fields_given(&src) {
            assert!(
                USES.fields.contains(&f),
                "the arrangement gives `{f}` and does not declare it"
            );
        }
    }
}
