//! Tools, selection, and hit testing.
//!
//! Hit testing is shared by every tool on purpose. The rule that makes
//! the canvas predictable is that a note's full top-to-bottom rectangle
//! belongs to that note's pitch row, and clicking an existing note
//! always selects it rather than creating something underneath — so
//! Select, Curve, and Note Draw all agree about what is under the
//! pointer.

use crate::camera::{Camera, Viewport};
use crate::doc::{Dimension, ExpressionDoc, NoteId, Target};

/// The active drawing tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Tool {
    /// The default, and the only tool that claims no gestures: an editor
    /// constructed without choosing one behaves exactly as the mouse map
    /// says. Any other default would mean a fresh `Editor` silently
    /// reassigned the plain drag — which is what Curve, the previous
    /// default, started doing the moment tools began claiming gestures.
    #[default]
    Select,
    Pen,
    Curve,
    Eraser,
    NoteDraw,
    NoteErase,
    /// Change the view, not the material.
    ///
    /// Drag up and down to zoom pitch, left and right to zoom time —
    /// so the direction you drag is the axis you get. Held `z` arms it
    /// for the length of the hold; it is also a tool like any other, so
    /// it can be clicked and left on.
    Zoom,
    /// Sweep out time × row areas to operate on.
    ///
    /// A razor selects a *region of the canvas* rather than a set of
    /// notes: it says "these rows, this span", and what follows — carve,
    /// delete, move contents, clear a lane — applies to whatever falls
    /// inside, including the halves of notes that straddle an edge.
    /// `Ctrl` reaches it from any tool; arming it puts the same thing on
    /// the plain drag, which is what you want when cutting several areas
    /// in a row.
    Razor,
    /// Vertical drags set velocity.
    ///
    /// The habit this replaces is REAPER's Alt+drag-on-a-note. A held
    /// `v` arms it the way a held `z` arms zoom, which is better than a
    /// modifier for the same reason zoom is: `v` is also a which-key
    /// prefix, so the one key is both "shape velocity by hand" and the
    /// door to every velocity command there is.
    Velocity,
}

impl Tool {
    /// The gestures this tool takes over from the base map.
    ///
    /// Plain (unmodified) gestures only. A tool claiming
    /// `Ctrl+drag` would silently take the pen override, or the razor,
    /// or whatever the user had bound there — the modified gestures are
    /// the map's and stay the map's.
    ///
    /// Empty means "the base map already does the right thing", which is
    /// true of Select: a plain drag already marquees on the roll and
    /// moves a note.
    pub fn claims(self) -> &'static [(crate::mouse::Context, crate::mouse::Gesture)] {
        use crate::mouse::{Context as C, Gesture as G};
        match self {
            // The base map is already this tool.
            Tool::Select => &[],
            // The expression tools want the roll *and* notes: a freehand
            // stroke that stopped at the edge of a note would be useless.
            Tool::Pen | Tool::Curve | Tool::Eraser => {
                &[(C::PianoRoll, G::Drag), (C::Note, G::Drag)]
            }
            // Drawing owns empty roll; a drag that starts on a note still
            // moves it, which is what every DAW does with a pencil.
            Tool::NoteDraw => &[(C::PianoRoll, G::Drag)],
            // Erasing owns both, or sweeping across notes would move the
            // first one it touched instead of deleting it.
            Tool::NoteErase => &[(C::PianoRoll, G::Drag), (C::Note, G::Drag)],
            // Zoom owns every drag it can reach. It is a *view* tool:
            // while it is armed nothing about the material should
            // change, so a drag that started on a note must zoom rather
            // than move the note.
            Tool::Zoom => &[(C::PianoRoll, G::Drag), (C::Note, G::Drag)],
            // Roll, notes, and note *edges*. A razor has to cut through
            // material rather than pick it up, and an edge is the one
            // place a sweep is most likely to start: on dense music
            // every few pixels is somebody's edge, so leaving them to
            // the map meant a cut that began a hair off resized a note
            // instead.
            //
            // Deliberately NOT `RazorArea` or `RazorEdge`. An area you
            // have already drawn keeps the map's gestures, so dragging
            // its contents and resizing it still work while the tool is
            // armed — claiming those would leave the tool unable to use
            // its own output.
            Tool::Razor => &[
                (C::PianoRoll, G::Drag),
                (C::Note, G::Drag),
                (C::NoteEdge, G::Drag),
            ],
            // Notes and their edges. Not the empty roll: a drag that
            // starts on nothing has no note to set the velocity of, and
            // stealing the marquee there would cost you the only way to
            // *choose* the notes you are about to shape.
            Tool::Velocity => &[(C::Note, G::Drag), (C::NoteEdge, G::Drag)],
        }
    }

    pub const ALL: [Tool; 9] = [
        Tool::Select,
        Tool::Pen,
        Tool::Curve,
        Tool::Eraser,
        Tool::NoteDraw,
        Tool::NoteErase,
        Tool::Razor,
        Tool::Velocity,
        Tool::Zoom,
    ];

    /// Whether this tool changes the view rather than the material.
    ///
    /// A view tool is safe to arm at any moment and safe to leave armed:
    /// it cannot alter the document, so nothing it does needs undo and
    /// nothing it does can be lost. That is what makes it reasonable to
    /// spring-load one onto a held key.
    pub fn is_view(self) -> bool {
        matches!(self, Tool::Zoom)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Pen => "Pen",
            Tool::Curve => "Curve",
            Tool::Eraser => "Eraser",
            Tool::NoteDraw => "Note Draw",
            Tool::NoteErase => "Note Erase",
            Tool::Razor => "Razor",
            Tool::Velocity => "Velocity",
            Tool::Zoom => "Zoom",
        }
    }

    /// Tools that author expression rather than notes.
    pub fn edits_expression(&self) -> bool {
        matches!(self, Tool::Pen | Tool::Curve | Tool::Eraser)
    }

    pub fn edits_notes(&self) -> bool {
        matches!(self, Tool::NoteDraw | Tool::NoteErase)
    }
}

/// A tool takes the plain gestures it lists ([`Tool::claims`]) ahead of
/// the mouse map.
impl timeline::mouse::ToolClaims<crate::mouse::Context> for Tool {
    fn claims(&self, context: &crate::mouse::Context, gesture: crate::mouse::Gesture) -> bool {
        Tool::claims(*self).contains(&(*context, gesture))
    }
}

pub use timeline::input::Mods;

/// What the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// A note body. `zone` is the zone under the pointer.
    Note { id: NoteId, zone: usize },
    /// A note edge, within the resize handle width.
    NoteEdge { id: NoteId, start_edge: bool },
    /// A Q-zone split handle in the bottom strip or note body.
    ZoneSplit { id: NoteId, index: usize, t: f64 },
    /// A point on the active dimension's curve.
    CurvePoint {
        id: NoteId,
        dimension: Dimension,
        t: f64,
    },
    /// Empty canvas at this time and pitch.
    Empty { t: f64, pitch: f64 },
}

/// Pixel tolerances for hit testing.
#[derive(Clone, Copy, Debug)]
pub struct HitConfig {
    pub edge_px: f64,
    pub split_px: f64,
    pub point_px: f64,
}

impl Default for HitConfig {
    fn default() -> Self {
        Self {
            edge_px: timeline::hit::EDGE_PX,
            split_px: 8.0,
            point_px: 5.0,
        }
    }
}

/// Resolve what is under `(x, y)`.
///
/// Order is deliberate: split handles beat note edges, which beat note
/// bodies, which beat curve points. Handles are small and intentional;
/// bodies are large and easy to hit by accident.
pub fn hit_test(
    doc: &ExpressionDoc,
    camera: &Camera,
    vp: Viewport,
    dimension: Dimension,
    x: f64,
    y: f64,
    cfg: HitConfig,
) -> Hit {
    let t = camera.t_at(x);
    let pitch = camera.pitch_at(y, vp);
    let row = pitch.round() as i32;

    for n in &doc.notes {
        for (i, &s) in n.splits.iter().enumerate() {
            if (camera.x(s) - x).abs() <= cfg.split_px && n.row == row {
                return Hit::ZoneSplit {
                    id: n.id,
                    index: i,
                    t: s,
                };
            }
        }
    }

    for n in &doc.notes {
        if n.row != row {
            continue;
        }
        // The shared edge rule (`timeline::hit::part`), the start first.
        match timeline::hit::part(x, camera.x(n.start), camera.x(n.end), cfg.edge_px) {
            Some(timeline::hit::Part::Start) => {
                return Hit::NoteEdge {
                    id: n.id,
                    start_edge: true,
                };
            }
            Some(timeline::hit::Part::End) => {
                return Hit::NoteEdge {
                    id: n.id,
                    start_edge: false,
                };
            }
            _ => {}
        }
    }

    for n in &doc.notes {
        // The full row height belongs to the note, so a pointer
        // anywhere in the row band selects it.
        if n.row == row && t >= n.start && t <= n.end {
            return Hit::Note {
                id: n.id,
                zone: n.zone_at(t),
            };
        }
    }

    for n in &doc.notes {
        if t < n.start || t > n.end {
            continue;
        }
        let curve = n.curve(dimension);
        for p in curve.points() {
            let py = match dimension {
                Dimension::Pitch => camera.y(n.row as f64 + p.value, vp),
                _ => lane_box_y(camera, vp, n.row, p.value),
            };
            if (camera.x(p.t) - x).abs() <= cfg.point_px && (py - y).abs() <= cfg.point_px {
                return Hit::CurvePoint {
                    id: n.id,
                    dimension,
                    t: p.t,
                };
            }
        }
    }

    Hit::Empty { t, pitch }
}

/// Pressure and Timbre draw inside a fixed box one semitone above and
/// below the note's row, at every zoom level — so a normalized gesture
/// keeps the same shape whether you are zoomed to a bar or to a beat.
pub fn lane_box_y(camera: &Camera, vp: Viewport, row: i32, normalized: f64) -> f64 {
    let top = camera.y(row as f64 + 1.0, vp);
    let bottom = camera.y(row as f64 - 1.0, vp);
    top + (bottom - top) * (1.0 - normalized.clamp(0.0, 1.0))
}

/// Inverse of [`lane_box_y`].
pub fn lane_box_value(camera: &Camera, vp: Viewport, row: i32, y: f64) -> f64 {
    let top = camera.y(row as f64 + 1.0, vp);
    let bottom = camera.y(row as f64 - 1.0, vp);
    if (bottom - top).abs() < 1e-9 {
        return 0.5;
    }
    (1.0 - (y - top) / (bottom - top)).clamp(0.0, 1.0)
}

/// Selected notes and, within them, selected curve points.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selection {
    pub notes: Vec<NoteId>,
    /// `(note, dimension, t)` of individually selected points.
    pub points: Vec<(NoteId, Dimension, f64)>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty() && self.points.is_empty()
    }

    pub fn clear(&mut self) {
        self.notes.clear();
        self.points.clear();
    }

    pub fn contains(&self, id: NoteId) -> bool {
        self.notes.contains(&id)
    }

    pub fn set_single(&mut self, id: NoteId) {
        self.notes.clear();
        self.points.clear();
        self.notes.push(id);
    }

    pub fn toggle(&mut self, id: NoteId) {
        match self.notes.iter().position(|&n| n == id) {
            Some(i) => {
                self.notes.remove(i);
            }
            None => self.notes.push(id),
        }
    }

    pub fn add(&mut self, id: NoteId) {
        if !self.contains(id) {
            self.notes.push(id);
        }
    }

    /// The selection `marquee` leaves, from `base` (the notes selected
    /// when it was pressed): the notes it touches, combined by its mode —
    /// replace, add or toggle — the arrangement's marquee over items too
    /// (`timeline::gestures::Marquee`). A note sits on the band half a row
    /// either side of its row; a touch at all counts.
    pub fn marquee(
        &mut self,
        doc: &ExpressionDoc,
        camera: &Camera,
        vp: Viewport,
        marquee: &timeline::gestures::Marquee,
        base: &[NoteId],
    ) {
        let ((x0, y0), (x1, y1)) = marquee.rect();
        let span = timeline::gestures::Span::new(
            (camera.t_at(x0), camera.pitch_at(y0, vp)),
            (camera.t_at(x1), camera.pitch_at(y1, vp)),
        );
        let hits = doc.notes.iter().filter_map(|n| {
            let row = f64::from(n.row);
            span.touches(n.start, n.end, row - 0.5, row + 0.5)
                .then_some(n.id)
        });
        let base: std::collections::HashSet<NoteId> = base.iter().copied().collect();
        let chosen = marquee.select(&base, hits);
        // In the document's order, as a selection built note by note is.
        self.notes = doc
            .notes
            .iter()
            .map(|n| n.id)
            .filter(|id| chosen.contains(id))
            .collect();
    }
}

fn min_max(a: f64, b: f64) -> (f64, f64) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Clicking a note's active zone toggles between targeting that one
/// zone and targeting the whole note.
///
/// The visual contract: either exactly one segment is highlighted, or
/// every segment is.
pub fn toggle_target(current: Target, clicked_zone: usize) -> Target {
    match current {
        Target::Zone(i) if i == clicked_zone => Target::WholeNote,
        _ => Target::Zone(clicked_zone),
    }
}

/// Which notes a gesture writes to.
///
/// A drag that starts over an unselected note while others are selected
/// still belongs to the selection — the note under the pointer is only
/// selected if the gesture turns out to be a click.
pub fn gesture_targets(selection: &Selection, under_pointer: Option<NoteId>) -> Vec<NoteId> {
    if !selection.notes.is_empty() {
        return selection.notes.clone();
    }
    under_pointer.into_iter().collect()
}

/// Directional clamping for a gesture that begins outside the target
/// span.
///
/// Starting to the left extends only to the left boundary; the right
/// side of the gesture is left alone, and vice versa. Starting *above*
/// or *below* is still an ordinary interval edit — only horizontal
/// overshoot clamps.
pub fn clamp_gesture(span: (f64, f64), start_t: f64, t0: f64, t1: f64) -> (f64, f64) {
    let (lo, hi) = span;
    let (mut a, mut b) = min_max(t0, t1);
    if start_t < lo {
        a = lo;
        b = b.min(hi);
    } else if start_t > hi {
        b = hi;
        a = a.max(lo);
    } else {
        a = a.max(lo);
        b = b.min(hi);
    }
    (a, b)
}

/// Local grid, private to the editor — it never touches the project or
/// MIDI-editor grid. The shared setting: division, feel, snap, and how it
/// follows the zoom.
pub use timeline::grid::{Feel, Grid};
