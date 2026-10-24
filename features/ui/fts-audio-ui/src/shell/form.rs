//! Editor form factors — the preset window sizes a plugin can wear.
//!
//! A face is a fixed drawing, so "what size is this editor" is a real choice
//! rather than a slider: an outboard unit has a size, and an FTS plugin should
//! be able to claim the same ones. The vocabulary and the ratios come from
//! `audio_controls::core::layout::FormFactor`, which laid this out first (19"
//! rack at 482 x 44.45 mm per U, a 500-series module at 38 x 133 mm); this is
//! that table as *editor* sizes, plus the rule for what a face does when its
//! drawing cannot fit the box.
//!
//! # The rule
//!
//! Every form is available to every face. What changes is how the face draws
//! itself: while the panel fits at a legible scale it is drawn as the panel,
//! and when it cannot — a 500-series module is portrait, and no 3:1 rack
//! drawing goes in there — the face flows its controls into the space instead
//! ([`EditorForm::wants_panel`]). One drawing, two renderings, so a new unit
//! gets the narrow sizes for free rather than needing a second layout table.

/// A preset editor size.
///
/// Persisted by [`id`](EditorForm::id), never by index — see the note on the
/// plugins' `profile_id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EditorForm {
    /// The size the current face asks for, and freely resizable from there.
    /// The default, and the only form that changes with the face.
    #[default]
    Responsive,
    /// 19" rack, one unit: wide and very short.
    Rack1U,
    /// Two units.
    Rack2U,
    /// Three units — the closest rack size to how the faces are drawn.
    Rack3U,
    /// A single 500-series module: portrait, one column.
    Series500,
    /// Two 500-series slots side by side.
    Series500Double,
    /// The smallest useful view: the controls, nothing else.
    Mini,
}

/// Every form, in the order the rail cycles them.
pub static EDITOR_FORMS: &[EditorForm] = &[
    EditorForm::Responsive,
    EditorForm::Rack1U,
    EditorForm::Rack2U,
    EditorForm::Rack3U,
    EditorForm::Series500,
    EditorForm::Series500Double,
    EditorForm::Mini,
];

/// How many rack units a face's own drawing is.
///
/// The panels are drawn as 2U units — an LA-2A or an 1176 sitting in a rack —
/// so the face's preferred size *is* the 2U size, and the other rack forms are
/// that scaled by their unit count. Deriving them from the 19"-at-2px/mm table
/// instead gave a 1U editor 89px tall: physically true, and unrelated to the
/// artwork it had to draw.
pub const FACE_RACK_UNITS: f64 = 2.0;

/// A 500-series module, in millimetres.
const MODULE_W_MM: f64 = 38.0;
const MODULE_H_MM: f64 = 133.0;

/// A 500-series module drawn at a usable width rather than a true 76 px.
const MODULE_PX_PER_MM: f64 = 7.4;

impl EditorForm {
    /// Stable id — what a session persists.
    pub fn id(self) -> &'static str {
        match self {
            Self::Responsive => "responsive",
            Self::Rack1U => "rack_1u",
            Self::Rack2U => "rack_2u",
            Self::Rack3U => "rack_3u",
            Self::Series500 => "series_500",
            Self::Series500Double => "series_500_double",
            Self::Mini => "mini",
        }
    }

    /// The form an id names, if this build has it.
    pub fn from_id(id: &str) -> Option<Self> {
        EDITOR_FORMS.iter().copied().find(|f| f.id() == id)
    }

    /// Full name, for the tooltip.
    pub fn label(self) -> &'static str {
        match self {
            Self::Responsive => "Responsive — the size this face asks for",
            Self::Rack1U => "Rack 1U — 19\" rack, one unit",
            Self::Rack2U => "Rack 2U — 19\" rack, two units",
            Self::Rack3U => "Rack 3U — 19\" rack, three units",
            Self::Series500 => "500 Series — one module, portrait",
            Self::Series500Double => "500 Series ×2 — two slots",
            Self::Mini => "Mini — controls only",
        }
    }

    /// Rail badge.
    pub fn badge(self) -> &'static str {
        match self {
            Self::Responsive => "RSP",
            Self::Rack1U => "1U",
            Self::Rack2U => "2U",
            Self::Rack3U => "3U",
            Self::Series500 => "500",
            Self::Series500Double => "5x2",
            Self::Mini => "MIN",
        }
    }

    /// The editor size this form asks the host for, given the rail's width and
    /// what the face would ask for on its own.
    ///
    /// `Responsive` is the face's own answer; the rest are the hardware sizes,
    /// which is the whole point — a 1U editor is short because 1U is short.
    pub fn editor_size(self, rail_w: f64, face_preferred: (u32, u32)) -> (u32, u32) {
        // A rack is one width: units add height and nothing else, so 1U is
        // exactly half of 2U with the same window width.
        let rack = |units: f64| {
            let (w, h) = face_preferred;
            (w, (h as f64 * units / FACE_RACK_UNITS) as u32)
        };
        match self {
            Self::Responsive => face_preferred,
            Self::Rack1U => rack(1.0),
            Self::Rack2U => rack(2.0),
            Self::Rack3U => rack(3.0),
            Self::Series500 => (
                (MODULE_W_MM * MODULE_PX_PER_MM + rail_w) as u32,
                (MODULE_H_MM * MODULE_PX_PER_MM) as u32,
            ),
            Self::Series500Double => (
                (MODULE_W_MM * 2.0 * MODULE_PX_PER_MM + rail_w) as u32,
                (MODULE_H_MM * MODULE_PX_PER_MM) as u32,
            ),
            Self::Mini => ((260.0 + rail_w) as u32, 200),
        }
    }

    /// The smallest, and largest, box any form asks for.
    ///
    /// These are what an editor should declare as its resize bounds, and
    /// declaring anything narrower is not a stricter policy — it is a broken
    /// size button. The host clamps a resize request to the bounds it was
    /// given, so a 300px floor silently turned both 1U (89px) and 2U (178px)
    /// into the same 300px window, and a 720px width floor turned a portrait
    /// 500-series module into a landscape one. The forms are physical sizes;
    /// if a form is offered, its size has to be reachable.
    ///
    /// `Responsive` is excluded — it has no size of its own, it defers to the
    /// face, and faces are checked against these bounds separately.
    pub fn size_bounds(rail_w: f64, face_preferred: (u32, u32)) -> ((u32, u32), (u32, u32)) {
        let sizes = || {
            EDITOR_FORMS
                .iter()
                .filter(|f| **f != Self::Responsive)
                .map(|f| f.editor_size(rail_w, face_preferred))
        };
        let min = (
            sizes().map(|(w, _)| w).min().unwrap_or(0),
            sizes().map(|(_, h)| h).min().unwrap_or(0),
        );
        let max = (
            sizes().map(|(w, _)| w).max().unwrap_or(0),
            sizes().map(|(_, h)| h).max().unwrap_or(0),
        );
        (min, max)
    }

    /// Whether a face should draw its panel at this form, or flow its controls
    /// into the space instead.
    ///
    /// The panel is drawn while it fits at a legible scale. A portrait module
    /// and a 1U sliver are not failures of the drawing — they are sizes the
    /// drawing was never for, and the honest answer is a different rendering
    /// of the same controls.
    pub fn wants_panel(self, design_w: f64, design_h: f64, avail_w: f64, avail_h: f64) -> bool {
        if design_w <= 0.0 || design_h <= 0.0 {
            return false;
        }
        let scale = (avail_w / design_w).min(avail_h / design_h);
        scale >= PANEL_LEGIBLE_SCALE
    }
}

/// Below this the silkscreen stops being readable and a panel is worse than a
/// plain row of controls. Deliberately far below the hardware kit's old
/// minimum: a resized-down window should get a smaller PANEL (the panel
/// scales all the way, `panel_svg::MIN_SCALE`), and the flow fallback is
/// only for genuinely tiny or wildly mismatched boxes.
pub const PANEL_LEGIBLE_SCALE: f64 = 0.32;
