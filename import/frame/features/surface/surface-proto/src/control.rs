//! Controls: what makes a piece of a design *do* something.
//!
//! A control is attached to a design node (a component instance, a frame —
//! anything drawn). It names its kind (knob, fader, footswitch…), what it
//! is bound to in an engine's [`Catalog`](crate::Catalog), and which of its
//! descendant nodes the runtime drives (the needle, the arc, the readout).
//! The look is entirely the design's; the behavior is entirely the control's.

use facet::Facet;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::curve::Curve;
use crate::param::{Catalog, EntryId, ParamAddr, ParamKind, StreamKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Orientation {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum ButtonMode {
    /// Press flips the value.
    #[default]
    Toggle,
    /// On while held.
    Momentary,
    /// Fires an action; no state.
    Trigger,
}

/// The behavior of a control. Each kind has a fixed interaction contract
/// (see the `surface` spec) and a fixed set of slot roles it drives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum ControlKind {
    /// Rotary; vertical drag, fine with Shift, wheel, double-click reset.
    Knob {
        /// Sweep in degrees (default 270), centered on straight up.
        sweep_degrees: f64,
    },
    /// Linear slider/fader.
    Fader { orientation: Orientation },
    Button { mode: ButtonMode },
    /// Stage footswitch: tap and hold are different gestures.
    Footswitch {
        /// Hold threshold (default 500 ms).
        hold_ms: u32,
    },
    /// Level / gain-reduction meter, driven by a stream.
    Meter { orientation: Orientation },
    /// Two parameters at once (x → `binding`, y → `y_binding`).
    XyPad,
    /// Pick one of a choice parameter's options (segmented, dropdown, radio).
    Selector,
    /// Text readout of a bound value or stream.
    Readout,
}

impl ControlKind {
    #[must_use]
    pub const fn knob() -> Self {
        Self::Knob { sweep_degrees: 270.0 }
    }

    #[must_use]
    pub const fn fader() -> Self {
        Self::Fader { orientation: Orientation::Vertical }
    }

    #[must_use]
    pub const fn footswitch() -> Self {
        Self::Footswitch { hold_ms: 500 }
    }

    /// Short lowercase name, as used in markup and lint messages.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Knob { .. } => "knob",
            Self::Fader { .. } => "fader",
            Self::Button { .. } => "button",
            Self::Footswitch { .. } => "footswitch",
            Self::Meter { .. } => "meter",
            Self::XyPad => "xy-pad",
            Self::Selector => "selector",
            Self::Readout => "readout",
        }
    }

    /// Whether the control writes a value (vs. only displaying one).
    #[must_use]
    pub const fn is_input(&self) -> bool {
        !matches!(self, Self::Meter { .. } | Self::Readout)
    }
}

/// What a control is wired to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Source {
    /// Read and write an engine parameter.
    Param { addr: ParamAddr },
    /// Fire an engine action (buttons, footswitch taps).
    Action { id: EntryId },
    /// Display a telemetry stream (meters, readouts). `channel` picks one
    /// channel of a multi-channel level stream.
    Stream { id: EntryId, channel: Option<u8> },
    /// Select within a collection (songs, patches).
    Collection { id: EntryId },
}

/// A control ⇄ engine edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Binding {
    pub source: Source,
    /// Shapes control position → value sent (and back for display).
    pub curve: Curve,
    /// Sub-range of the target a full control sweep covers (normalized).
    pub min: f64,
    pub max: f64,
}

impl Binding {
    #[must_use]
    pub fn param(addr: impl Into<String>) -> Self {
        Self { source: Source::Param { addr: ParamAddr::new(addr) }, curve: Curve::Linear, min: 0.0, max: 1.0 }
    }

    #[must_use]
    pub fn action(id: impl Into<String>) -> Self {
        Self { source: Source::Action { id: EntryId::new(id) }, curve: Curve::Linear, min: 0.0, max: 1.0 }
    }

    #[must_use]
    pub fn stream(id: impl Into<String>) -> Self {
        Self { source: Source::Stream { id: EntryId::new(id), channel: None }, curve: Curve::Linear, min: 0.0, max: 1.0 }
    }

    /// Control position (0..1) → target value (normalized).
    #[must_use]
    pub fn to_target(&self, position: f64) -> f64 {
        self.min + (self.max - self.min) * self.curve.apply(position)
    }

    /// Target value (normalized) → control position (0..1).
    #[must_use]
    pub fn to_position(&self, value: f64) -> f64 {
        let span = self.max - self.min;
        if span == 0.0 {
            return 0.0;
        }
        self.curve.invert(((value - self.min) / span).clamp(0.0, 1.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum MidiMessage {
    Cc { number: u8 },
    Note { number: u8 },
    PitchBend,
    Aftertouch,
}

/// A hardware control driving this control.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct MidiMapping {
    /// 1..=16; `None` = omni.
    pub channel: Option<u8>,
    pub message: MidiMessage,
    pub curve: Curve,
}

/// Which part of a control's drawing a descendant node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum SlotRole {
    /// Rotates with the value (knob pointer).
    Needle,
    /// Its outline is replaced by an arc from the start angle to the value.
    ValueArc,
    /// Arc showing the modulation range.
    ModArc,
    /// Grows along the control's axis with the value (fader fill, meter bar).
    Fill,
    /// Moves along the axis with the value (fader cap).
    Thumb,
    /// Its text becomes the formatted value.
    Value,
    /// Its text becomes the bound parameter's name (unless authored).
    Label,
    /// Visible (full opacity) when on, dimmed when off.
    Led,
    /// Peak-hold marker on meters.
    PeakHold,
}

/// "Node `node` plays `role`." `node` is a design node id (uuid).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Slot {
    pub role: SlotRole,
    pub node: Uuid,
}

/// Everything behavioral about a design node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Control {
    pub kind: ControlKind,
    pub binding: Option<Binding>,
    /// The second axis of an [`ControlKind::XyPad`].
    pub y_binding: Option<Binding>,
    pub midi: Option<MidiMapping>,
    /// Position (0..1) a reset returns to; `None` = the parameter default.
    pub default: Option<f64>,
    pub slots: Vec<Slot>,
}

impl Control {
    #[must_use]
    pub fn new(kind: ControlKind) -> Self {
        Self { kind, binding: None, y_binding: None, midi: None, default: None, slots: Vec::new() }
    }

    #[must_use]
    pub fn bound(mut self, binding: Binding) -> Self {
        self.binding = Some(binding);
        self
    }

    #[must_use]
    pub fn slot(mut self, role: SlotRole, node: Uuid) -> Self {
        self.slots.push(Slot { role, node });
        self
    }

    pub fn slots_of(&self, role: SlotRole) -> impl Iterator<Item = Uuid> + '_ {
        self.slots.iter().filter(move |s| s.role == role).map(|s| s.node)
    }
}

/// A problem with a control's wiring, relative to a catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum BindingIssue {
    Unbound,
    UnknownParam { addr: ParamAddr },
    UnknownAction { id: EntryId },
    UnknownStream { id: EntryId },
    UnknownCollection { id: EntryId },
    /// The source type doesn't suit the control (a meter bound to an action…).
    Mismatch { detail: String },
}

impl std::fmt::Display for BindingIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unbound => f.write_str("control is not bound to anything"),
            Self::UnknownParam { addr } => write!(f, "no parameter `{addr}` in the catalog"),
            Self::UnknownAction { id } => write!(f, "no action `{}` in the catalog", id.0),
            Self::UnknownStream { id } => write!(f, "no stream `{}` in the catalog", id.0),
            Self::UnknownCollection { id } => write!(f, "no collection `{}` in the catalog", id.0),
            Self::Mismatch { detail } => f.write_str(detail),
        }
    }
}

impl Control {
    /// Check this control's bindings against `catalog`.
    #[must_use]
    pub fn validate(&self, catalog: &Catalog) -> Vec<BindingIssue> {
        let mut issues = Vec::new();
        let Some(b) = &self.binding else {
            issues.push(BindingIssue::Unbound);
            return issues;
        };
        let mut check = |b: &Binding| match &b.source {
            Source::Param { addr } => match catalog.param(addr) {
                None => issues.push(BindingIssue::UnknownParam { addr: addr.clone() }),
                Some(p) => {
                    if matches!(self.kind, ControlKind::Meter { .. }) {
                        issues.push(BindingIssue::Mismatch { detail: format!("meter bound to parameter `{addr}`; meters display streams") });
                    }
                    if matches!(self.kind, ControlKind::Selector) && !matches!(p.kind, ParamKind::Choice { .. } | ParamKind::Toggle) {
                        issues.push(BindingIssue::Mismatch { detail: format!("selector bound to continuous parameter `{addr}`") });
                    }
                }
            },
            Source::Action { id } => {
                if catalog.action(id).is_none() {
                    issues.push(BindingIssue::UnknownAction { id: id.clone() });
                } else if !matches!(self.kind, ControlKind::Button { .. } | ControlKind::Footswitch { .. }) {
                    issues.push(BindingIssue::Mismatch { detail: format!("{} bound to action `{}`; only buttons and footswitches fire actions", self.kind.name(), id.0) });
                }
            }
            Source::Stream { id, .. } => match catalog.stream(id) {
                None => issues.push(BindingIssue::UnknownStream { id: id.clone() }),
                Some(s) => {
                    if self.kind.is_input() {
                        issues.push(BindingIssue::Mismatch { detail: format!("{} bound to read-only stream `{}`", self.kind.name(), id.0) });
                    }
                    if matches!(self.kind, ControlKind::Meter { .. }) && matches!(s.kind, StreamKind::Spectrum { .. } | StreamKind::Tuner) {
                        issues.push(BindingIssue::Mismatch { detail: format!("meter bound to non-level stream `{}`", id.0) });
                    }
                }
            },
            Source::Collection { id } => {
                if catalog.collection(id).is_none() {
                    issues.push(BindingIssue::UnknownCollection { id: id.clone() });
                }
            }
        };
        check(b);
        if let Some(y) = &self.y_binding {
            check(y);
        }
        issues
    }
}
