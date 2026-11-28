//! A strip's controls, shared by every view that draws one: what a click
//! lands on ([`Control`]), the edit a press or a drag on one makes
//! ([`click`], [`drag`]), the strip's measured sizes, its ground, and the
//! knob. The mixer's strips and the track control panel's buttons are both
//! these, so a theme's `mute` is the same control in both.

use anyrender::PaintScene;
use daw_proto::Track;
use daw_ui::art::geometry::mcp as g;
use vello::kurbo::{Affine, Line, Stroke};
use vello::peniko::Color;

use crate::palette::Palette;

/// REAPER's wide strip, and the width everything in the geometry was
/// measured against.
///
/// Narrow mode's 54 is a separate layout rather than this one scaled, so
/// it is not modelled by shrinking this. What IS modelled by shrinking
/// is `Track::width`: a strip narrower than this keeps its controls in
/// their measured columns and loses the room to the right of them.
pub const STRIP_W: f64 = 86.0;

const _: () = assert!(
    g::STRIP_W.to_bits() == 86.0_f32.to_bits(),
    "mcp::STRIP_W must match the measured geometry"
);

/// The gap between strips, so two adjacent ones read as two.
pub const STRIP_GAP: f64 = 1.0;

/// The widest a touchscreen's strip is (before its zoom): mute and solo
/// side by side across it, the fader under them — thinner than REAPER's
/// 86, so more of a phone's or a tablet's mixer is on screen at once.
pub const TOUCH_STRIP_W: f64 = 62.0;

/// How much shorter each level of nesting makes a strip.
///
/// Folder depth reads off the BOTTOM of the mixer: strips share a top
/// edge and their bottoms step up with depth, so a folder's children sit
/// visibly inside it. Brackets across the top said the same thing and
/// were removed — that band is wanted for something else, and a
/// staircase costs no height that the strips were using.
pub const INDENT_STEP: f64 = 12.0;

/// How tall the mixer panel opens by default.
///
/// REAPER's own default, and a *default* is all it is: the panel is
/// draggable and a strip fills whatever height it is given. There is no
/// fixed REAPER strip height to match.
///
/// This was briefly 310, measured off a running REAPER — wrongly. That
/// REAPER had "show multiple rows of tracks" ON, so its mixer had
/// wrapped the session into two rows, and what I measured was the ROW
/// PITCH of a wrapped layout, not the height of a strip. With the
/// option off there is one row and each strip is as tall as the panel.
///
/// The width measured the same way IS right — 86, confirmed twice — and
/// that is the difference worth remembering: a strip's width is a real
/// constant, its height is whatever the dock gives it.
pub const DEFAULT_HEIGHT: f64 = 371.0;

/// A control on a strip.
///
/// What a click can land on, as opposed to what it can land IN — the
/// strip is `hit::Target::Track`, and this narrows it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Control {
    Fx,
    Pan,
    RecArm,
    Mute,
    Solo,
    Routing,
    /// The fader, including its cap. Dragged, not clicked.
    Volume,
    /// The name plate. Double-clicked to rename.
    Name,
    /// Input monitoring, under the record arm.
    ///
    /// Its own control rather than part of the arm: what a track
    /// records and whether you hear it while it does are two decisions,
    /// and REAPER gives them two buttons for that reason.
    Monitor,
    /// The clip latch at the top of the meter — clicked to clear it.
    ///
    /// Its rect is always here, because geometry is worked out without
    /// knowing what the signal did. Whether it is actually a target is
    /// the window's call: with nothing clipped there is nothing to
    /// clear, and the band falls through to the fader underneath it so
    /// the strip behaves exactly as it would without a latch.
    Clip,
    /// The fold at the foot of a folder's strip — clicked to collapse
    /// the folder to one strip, or open it again. Only a target on a
    /// folder; the rect is there on every strip because geometry is
    /// worked out without knowing what the track is.
    Folder,
}

impl Control {
    /// Whether this control is set by DRAGGING rather than clicking.
    ///
    /// The distinction the gesture layer needs: a press on a knob is
    /// the start of a drag and must not act until the pointer moves or
    /// is released, where a press on a mute is a click waiting to
    /// happen.
    #[must_use]
    pub const fn is_continuous(self) -> bool {
        matches!(self, Self::Volume | Self::Pan)
    }
}

/// What the rack paints under its chain's end: the track's colour when
/// the mixer is set to fill the rack that way, else nothing.
///
/// One answer for the recording and the live pass, so a rack that
/// starts moving does not change colour under its chain.
#[must_use]
pub fn rack_ground(palette: &Palette, track: &Track) -> Option<Color> {
    wash(crate::layout::rack_fill(), palette, track)
}

/// The strip's ground: the panel's grey, or the track's wash when the
/// mixer is set to paint the strip in it — the FX section, the space
/// beside the fader, all of it, so the strip is one surface with its
/// band and its rack.
#[must_use]
pub fn strip_ground(palette: &Palette, track: &Track) -> Color {
    wash(crate::layout::strip_fill(), palette, track).unwrap_or(palette.tcp_tint)
}

/// The rule up a strip's left edge in the track's colour, when the mixer
/// is set to draw one — see `layout::strip_edge`.
#[must_use]
pub fn strip_edge(palette: &Palette, track: &Track) -> Option<Color> {
    wash(crate::layout::strip_edge(), palette, track)
}

/// A wash setting as a colour for this track: nothing, the band's
/// tint, or the track's own colour.
pub fn wash(wash: crate::layout::Wash, palette: &Palette, track: &Track) -> Option<Color> {
    match wash {
        crate::layout::Wash::Off => None,
        crate::layout::Wash::Tint => Some(crate::tcp::row_tint(palette, track)),
        crate::layout::Wash::Full => Some(crate::tcp::track_color(palette, track)),
    }
}

/// How wide the fold target at the foot of a folder's strip is.
pub const FOLD_W: u32 = 14;

/// A strip height, for the f32 the theme's geometry is written in.
pub const fn f64_to_f32(value: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "a pixel height; f32 holds it exactly and the theme's API takes one"
    )]
    let narrowed = value as f32;
    narrowed
}

/// What a press on a strip control runs: the named action that does it
/// (`TRACK_SET`, `TRACK_ARM`, `SESSION_SELECT_TRACK`), and the edit the
/// strip shows before the engine says so.
#[derive(Clone, Debug, PartialEq)]
pub struct Press {
    pub id: &'static str,
    pub args: serde_json::Value,
    /// Shown at once (`mixer_panel::predict`); in the browser build, which
    /// has no Action List, sent as it is.
    pub preview: crate::engine::Edit,
}

/// A track selected — alone, or (`add`) added to the selection: a click
/// on a strip's name or its ground.
#[must_use]
pub fn select(guid: &str, add: bool) -> Press {
    let guid = guid.to_owned();
    Press {
        id: "SESSION_SELECT_TRACK",
        args: serde_json::json!({ "track": guid, "add": add }),
        preview: if add {
            crate::engine::Edit::AddToSelection(guid)
        } else {
            crate::engine::Edit::Select(guid)
        },
    }
}

/// Run `press`'s action through the Action List ([`crate::press::press`]:
/// the desktop's, or the page's). Its preview is the caller's to show.
pub fn run(press: &Press) {
    let _ = crate::press::press(press.id, &press.args);
}

/// What a click on a strip control runs.
#[must_use]
pub fn click(control: Control, guid: &str, from: &daw_proto::Track, add: bool) -> Option<Press> {
    use crate::engine::Edit;
    let id = guid.to_owned();
    let set = |preview: Edit, args: serde_json::Value| Press {
        id: "TRACK_SET",
        args,
        preview,
    };
    match control {
        Control::Mute => Some(set(
            Edit::ToggleMute(id),
            serde_json::json!({ "track": guid, "mute": !from.muted }),
        )),
        Control::Solo => Some(set(
            Edit::ToggleSolo(id),
            serde_json::json!({ "track": guid, "solo": !from.soloed }),
        )),
        Control::RecArm => Some(Press {
            id: "TRACK_ARM",
            args: serde_json::json!({ "track": guid, "on": !from.armed }),
            preview: Edit::ToggleArm(id),
        }),
        // Clicking the name selects the track; DOUBLE-clicking renames
        // it, which is a different gesture and a different edit.
        Control::Name => Some(select(guid, add)),
        // The routing widget opens the track's routing close-up
        // (`SESSION_CLOSEUP {routing}`: its sends, receives and hardware
        // outputs), which is the window's and not an edit — so nothing
        // to show before it: `Done` is a barrier no strip predicts.
        Control::Routing => Some(Press {
            id: "SESSION_CLOSEUP",
            args: serde_json::json!({ "track": guid, "routing": true, "name": from.name }),
            preview: Edit::Done(0),
        }),
        // The FX button opens the track's chain close-up
        // (`SESSION_CLOSEUP {chain}`: every FX on it, added, removed,
        // reordered and bypassed, and any plugin's parameters) — the
        // window's, not an edit, as the routing widget's is.
        Control::Fx => Some(Press {
            id: "SESSION_CLOSEUP",
            args: serde_json::json!({ "track": guid, "chain": true, "name": from.name }),
            preview: Edit::Done(0),
        }),
        Control::Volume | Control::Pan => None,
        // Clearing a clip changes nothing about the track — it is the
        // window forgetting something, not the engine being told
        // something. Handled where the latch lives.
        Control::Clip => None,
        // Folding is the window's: it changes which strips exist, not
        // what any track is — see the window's fold.
        Control::Folder => None,
        // The lamp is only drawn on an armed track, and what is not
        // drawn is not clicked.
        Control::Monitor => from.armed.then(|| {
            let next = crate::engine::next_monitor(from.input_monitor);
            let word = match next {
                daw_proto::track::InputMonitoringMode::Off => "off",
                daw_proto::track::InputMonitoringMode::Normal => "on",
                daw_proto::track::InputMonitoringMode::NotWhenPlaying => "auto",
            };
            set(
                Edit::SetInputMonitor(id, next),
                serde_json::json!({ "track": guid, "monitor": word }),
            )
        }),
    }
}

/// What a drag on a control means, given how far it moved.
///
/// `fraction` is the drag as a share of the control's travel, already
/// signed so that up is more — see `gesture::drag_fraction`.
#[must_use]
pub fn drag(
    control: Control,
    guid: &str,
    from: &daw_proto::Track,
    fraction: f64,
) -> Option<crate::engine::Edit> {
    match control {
        Control::Volume => {
            // Through the fader's own taper, so dragging a third of the
            // way up moves it a third of the way up the SCALE rather
            // than a third of the way up the gain — which at the top of
            // a fader is most of its range and at the bottom is none.
            let at = daw_ui::art::paint::tcp::gain_norm(from.volume);
            let to = (at + fraction).clamp(0.0, 1.0);
            let db = daw_ui::art::paint::tcp::fader_db(to);
            Some(crate::engine::Edit::SetVolume(
                guid.to_owned(),
                crate::engine::db_to_gain(db),
            ))
        }
        Control::Pan => {
            // Pan is linear and two-sided, so the same drag is worth
            // twice as much of its range as it is of a fader's.
            let to = (from.pan + fraction * 2.0).clamp(-1.0, 1.0);
            Some(crate::engine::Edit::SetPan(guid.to_owned(), to))
        }
        _ => None,
    }
}
/// One knob: its track, its travel, its pointer. The vocal leveler's
/// stage dials are these too.
pub fn knob_arc(
    scene: &mut impl PaintScene,
    palette: &Palette,
    centre: (f64, f64),
    r: f64,
    value: f64,
    tint: Color,
    held: bool,
) {
    use std::f64::consts::PI;
    // Seven o'clock to five o'clock, clockwise: 270 degrees of travel.
    let start = PI * 0.75;
    let sweep = PI * 1.5;
    let arc = |from: f64, to: f64| vello::kurbo::Arc::new(centre, (r, r), from, to - from, 0.0);
    scene.stroke(
        &Stroke::new(if held { 2.4 } else { 1.6 }),
        Affine::IDENTITY,
        palette.grid_beat,
        None,
        &arc(start, start + sweep),
    );
    let to = value.clamp(0.0, 1.0).mul_add(sweep, start);
    if value > 0.005 {
        scene.stroke(
            &Stroke::new(if held { 2.4 } else { 1.6 }),
            Affine::IDENTITY,
            tint,
            None,
            &arc(start, to),
        );
    }
    let (px, py) = (to.cos().mul_add(r, centre.0), to.sin().mul_add(r, centre.1));
    scene.stroke(
        &Stroke::new(1.2),
        Affine::IDENTITY,
        if held { palette.text } else { palette.text_dim },
        None,
        &Line::new(centre, (px, py)),
    );
}
/// What fits in a strip of a given width.
///
/// The width counterpart of [`Collapse`], which does the same job for a
/// strip's height. REAPER needs no such thing because every strip there
/// is one width; ours are not, so the measured columns — the button
/// column at 55, the fader at 30 — stop being reachable long before the
/// strip stops being useful.
///
/// The rule is the same one the track panel follows: shed controls in
/// the order they stop earning their space, and never let one overflow
/// its strip. What a narrow strip keeps is what you look at a send or a
/// trigger for — its level, its mute, and which track it is.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Squeeze {
    /// Everything REAPER's strip has, at the columns it measures them
    /// to. The button column sits at 55 and the routing widget after it,
    /// so this is the only width where they are reachable.
    Full,
    /// The head of the strip — its FX pill and its pan — with the meter
    /// beside the fader. Routing goes first, because it is the control
    /// you set once and then read off the arrangement.
    Head,
    /// The fader down the middle, mute and solo stacked over it, and the
    /// name. What you look at a send or a trigger for: its level, its
    /// mute, and which track it is.
    Level,
}

impl Squeeze {
    /// What fits in `width`.
    ///
    /// Thresholds are what each tier NEEDS: the button column plus its
    /// own 21 for `Full`, the meter's 26 and the fader's 22 with their
    /// gaps for `Head`.
    #[must_use]
    pub fn at(width: f64) -> Self {
        if width >= 78.0 {
            Self::Full
        } else if width >= 60.0 {
            Self::Head
        } else {
            Self::Level
        }
    }

    /// The pan knob and the FX pill, which share a threshold: both want
    /// a cell and enough either side to read as placed rather than
    /// wedged.
    #[must_use]
    pub fn head(self) -> bool {
        self <= Self::Head
    }

    /// The meter, which shares the stretch with the fader.
    #[must_use]
    pub fn meter(self) -> bool {
        self <= Self::Head
    }

    /// Routing, and the measured button column it sits in.
    #[must_use]
    pub fn columns(self) -> bool {
        self == Self::Full
    }
}

/// Where a strip's columns fall, for a strip of this width.
///
/// REAPER has one strip width, so its geometry is a list of fixed
/// offsets. Ours are not one width — a piece is wider than a mic, and a
/// selected strip is wider again — and fixed offsets meant every extra
/// pixel piled up as dead space on the right: the fader stayed a third
/// of the way across, the arm sat where an 86-wide strip puts it, and
/// the meter never grew.
///
/// So the offsets become rules, chosen so that **a strip of exactly
/// [`STRIP_W`] reproduces REAPER's own layout**. That is the constraint
/// that keeps this honest: the measured numbers are still the numbers,
/// they are just expressed as where-they-come-from rather than as
/// where-they-landed.
///
/// | | rule | at 86 | REAPER |
/// |---|---|---|---|
/// | button column | right-aligned, REAPER's margin | 55 | 55 |
/// | fader | centred | 32 | 31 |
/// | scale | left, fixed width | 2 | ~4 |
/// | meter | fills what is left between them | ~0 | not drawn |
#[derive(Clone, Copy, Debug)]
pub struct Columns {
    pub scale_x: f64,
    pub scale_w: f64,
    pub fader_x: f64,
    pub fader_w: f64,
    /// The left edge of the button column.
    pub column_x: f64,
    /// Its centre — what the record arm hangs off.
    pub column_axis: f64,
}

impl Columns {
    pub fn at(x: f64, w: f64) -> Self {
        // Right-aligned by REAPER's own right margin, so the buttons
        // keep their distance from the edge instead of their distance
        // from the left.
        let margin = f64::from(g::STRIP_W - g::COLUMN);
        let column_x = if Squeeze::at(w).columns() {
            (x + w - margin).max(x)
        } else {
            x + (w - f64::from(g::BUTTON_W)).max(0.0) / 2.0
        };
        let fader_x = x + (w - FADER_W) / 2.0;
        // Hard against the meter, because the numbers and the column
        // are one instrument: a tick runs from each number to the
        // meter's edge, and a gap the width of the strip's margin
        // would make that tick a bridge over nothing.
        let scale_x = (fader_x - SCALE_W).max(x + 1.0);
        Self {
            scale_x,
            scale_w: SCALE_W,
            fader_x,
            fader_w: FADER_W,
            column_x,
            column_axis: column_x + f64::from(g::BUTTON_W) / 2.0,
        }
    }
}
/// The narrowest rack that gives the panel over to the plugin's own
/// editing surface.
///
/// Below this the plugin's grid crowds, its labels collide and its
/// nodes — authored for a graph eight hundred pixels wide — overlap
/// each other. Above it there is room for all three, and the rack stops
/// abbreviating.
///
/// Well under a focus width (618 on a 1440p panel), because a focused
/// strip is not the only way to get here: two of them side by side on
/// an ultrawide, or a mixer of six tracks, land in the same place and
/// should get the same panel.
pub const FOCUSED: f64 = 260.0;
/// The fader's own width. Fixed: a fader is a fader, and a wider strip
/// wants a longer scale, not a fatter handle.
///
/// Wider than the 22 it was, because the column is the METER now — the
/// groove is lit by the signal, the cap rides over it as glass, and the
/// meter is wider than the cap so each channel shows down its own side
/// of it. The cap itself is still 22 (`paint::cap_w`); what grew is the
/// scale it is read against, not the handle.
///
/// There is no second column to size against. That is what lets a narrow strip have both: they were never
/// two things that needed two columns, and REAPER drew no meter at all
/// on an 86-wide strip because there was nowhere left to put one.
pub const FADER_W: f64 = 30.0;

/// How much room the scale's numbers need. Fixed, because `-54-` is
/// `-54-` at any strip width.
pub const SCALE_W: f64 = 24.0;
