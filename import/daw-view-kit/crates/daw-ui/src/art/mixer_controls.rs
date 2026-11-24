//! The mixer's controls, as components with real state.
//!
//! REAPER ships one image per control *per state* — `mcp_solo_off`,
//! `mcp_solo_on`, `mcp_solodefeat_on` — which is the right shape for a DAW
//! blitting bitmaps and the wrong shape for a UI. A component per image
//! would be 176 of them and would push the state machine onto every caller.
//!
//! So there is one component per **control**, taking the state as a prop
//! and selecting the traced artwork itself. `SoloButton` knows solo has
//! three states; nothing outside it needs to.
//!
//! Each control's art is traced from the original (see [`crate::art::trace`]) and
//! drawn in the theme palette, so these are 1:1 with Reapertips today and
//! are the place to change that when you want to. Replacing one means
//! rewriting its `rsx!` — the state API stays the same and every caller
//! keeps working.

use dioxus::prelude::*;

use crate::art::art_data::{ArtImage, ColorMode};
use crate::art::generated;

/// Which sprite cell of a control strip to draw.
///
/// REAPER packs these side by side in one image, so this is a cell index,
/// not a style — the artwork for hover already exists and simply has to be
/// selected.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Interaction {
    #[default]
    Normal,
    Hover,
    Pressed,
}

impl Interaction {
    fn cell(self) -> usize {
        match self {
            Self::Normal => 0,
            Self::Hover => 1,
            Self::Pressed => 2,
        }
    }
}

/// Draw a traced image by name, or nothing if the theme lacks it.
///
/// Returning empty rather than panicking matters: a theme is allowed not
/// to ship every image, and a missing `mcp_recarm_auto` should cost you
/// that one indicator, not the whole mixer.
fn art(name: &str, width: Option<u32>, height: Option<u32>, at: Interaction) -> Element {
    let Some(art) = generated::by_name(name) else {
        return rsx! {};
    };
    rsx! {
        ArtImage { art, width, height, mode: ColorMode::Themed, cell: at.cell() }
    }
}

// ── record arm ───────────────────────────────────────────────────────────

/// Record-arm state. REAPER separates *armed* from *auto* (arm follows
/// selection) and from *norec* (armed but this track cannot record).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RecordArm {
    #[default]
    Off,
    On,
    /// Armed, but recording is disabled for the track — REAPER draws this
    /// differently precisely so you can tell it from armed-and-ready.
    NoRecord,
    /// Arm follows track selection.
    Auto,
    AutoOn,
    AutoNoRecord,
}

#[derive(Props, Clone, PartialEq)]
pub struct RecordArmProps {
    #[props(default)]
    pub state: RecordArm,
    /// Draw size in px. `None` uses the artwork's own size.
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Which sprite cell — normal, hover, pressed.
    #[props(default)]
    pub at: Interaction,
}

#[component]
pub fn RecordArmButton(props: RecordArmProps) -> Element {
    let name = match props.state {
        RecordArm::Off => "mcp_recarm_off",
        RecordArm::On => "mcp_recarm_on",
        RecordArm::NoRecord => "mcp_recarm_norec",
        RecordArm::Auto => "mcp_recarm_auto",
        RecordArm::AutoOn => "mcp_recarm_auto_on",
        RecordArm::AutoNoRecord => "mcp_recarm_auto_norec",
    };
    art(name, props.width, props.height, props.at)
}

// ── mute ─────────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct ToggleProps {
    #[props(default)]
    pub on: bool,
    /// Draw size in px. `None` uses the artwork's own size.
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Which sprite cell — normal, hover, pressed.
    #[props(default)]
    pub at: Interaction,
}

#[component]
pub fn MuteButton(props: ToggleProps) -> Element {
    let name = if props.on {
        "mcp_mute_on"
    } else {
        "mcp_mute_off"
    };
    art(name, props.width, props.height, props.at)
}

// ── solo ─────────────────────────────────────────────────────────────────

/// Solo has a third state: *defeat*, meaning this track ignores other
/// tracks' solos. It is not "more soloed" — it is a different thing, and
/// conflating it with `On` is why it gets its own variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Solo {
    #[default]
    Off,
    On,
    Defeat,
}

#[derive(Props, Clone, PartialEq)]
pub struct SoloProps {
    #[props(default)]
    pub state: Solo,
    /// Draw size in px. `None` uses the artwork's own size.
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Which sprite cell — normal, hover, pressed.
    #[props(default)]
    pub at: Interaction,
}

#[component]
pub fn SoloButton(props: SoloProps) -> Element {
    let name = match props.state {
        Solo::Off => "mcp_solo_off",
        Solo::On => "mcp_solo_on",
        Solo::Defeat => "mcp_solodefeat_on",
    };
    art(name, props.width, props.height, props.at)
}

// ── routing ──────────────────────────────────────────────────────────────

/// What the routing button reports. Sends and receives are independent, so
/// this is two booleans rather than an enum of four — the component picks
/// the image.
#[derive(Props, Clone, PartialEq)]
pub struct RoutingProps {
    #[props(default)]
    pub has_sends: bool,
    #[props(default)]
    pub has_receives: bool,
    /// Routing unavailable (e.g. the master's).
    #[props(default)]
    pub disabled: bool,
    /// Draw size in px. `None` uses the artwork's own size.
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Which sprite cell — normal, hover, pressed.
    #[props(default)]
    pub at: Interaction,
}

#[component]
pub fn RoutingButton(props: RoutingProps) -> Element {
    let name = match (props.has_sends, props.has_receives, props.disabled) {
        (true, true, false) => "mcp_io_s_r",
        (true, true, true) => "mcp_io_s_r_dis",
        (true, false, false) => "mcp_io_s",
        (true, false, true) => "mcp_io_s_dis",
        (false, true, false) => "mcp_io_r",
        (false, true, true) => "mcp_io_r_dis",
        (false, false, false) => "mcp_io",
        (false, false, true) => "mcp_io_dis",
    };
    art(name, props.width, props.height, props.at)
}

// ── input monitoring ─────────────────────────────────────────────────────

/// Input monitoring: off, on, or automatic (monitor only while stopped).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Monitoring {
    #[default]
    Off,
    On,
    Auto,
}

#[derive(Props, Clone, PartialEq)]
pub struct MonitoringProps {
    #[props(default)]
    pub state: Monitoring,
    /// Draw size in px. `None` uses the artwork's own size.
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Which sprite cell — normal, hover, pressed.
    #[props(default)]
    pub at: Interaction,
}

#[component]
pub fn InputMonitorIndicator(props: MonitoringProps) -> Element {
    let name = match props.state {
        Monitoring::Off => "mcp_monitor_off",
        Monitoring::On => "mcp_monitor_on",
        Monitoring::Auto => "mcp_monitor_auto",
    };
    art(name, props.width, props.height, props.at)
}

// ── FX ───────────────────────────────────────────────────────────────────

/// The FX button's state, which is about the *chain*, not a toggle.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FxChain {
    /// No FX on the track.
    #[default]
    Empty,
    /// FX present and active.
    Active,
    /// FX present, chain bypassed.
    Bypassed,
}

#[derive(Props, Clone, PartialEq)]
pub struct FxProps {
    #[props(default)]
    pub state: FxChain,
    /// Draw size in px. `None` uses the artwork's own size.
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Which sprite cell — normal, hover, pressed.
    #[props(default)]
    pub at: Interaction,
}

#[component]
pub fn FxButton(props: FxProps) -> Element {
    let name = match props.state {
        FxChain::Empty => "mcp_fx_empty",
        FxChain::Active => "mcp_fx_norm",
        FxChain::Bypassed => "mcp_fx_dis",
    };
    art(name, props.width, props.height, props.at)
}

// ── pan ──────────────────────────────────────────────────────────────────

/// The pan knob. REAPER ships two sizes and a *frame stack* — 128 rendered
/// rotations — rather than one rotatable image, so `position` picks a
/// frame rather than applying a transform.
#[derive(Props, Clone, PartialEq)]
pub struct PanProps {
    /// -1 hard left, 0 centre, +1 hard right.
    #[props(default = 0.0)]
    pub position: f32,
    /// Use the larger knob artwork.
    #[props(default)]
    pub large: bool,
    /// Draw size in px. `None` uses the artwork's own size.
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Which sprite cell — normal, hover, pressed.
    #[props(default)]
    pub at: Interaction,
}

#[component]
pub fn PanningKnob(props: PanProps) -> Element {
    // The stack is a column of frames; the visible one is chosen by
    // clipping, which the traced renderer cannot express yet. Until then
    // the static knob art stands in — see the note on `KNOB_FRAMES`.
    let name = if props.large {
        "mcp_pan_knob_large"
    } else {
        "mcp_pan_knob_small"
    };
    let _ = props.position;
    art(name, props.width, props.height, props.at)
}

/// How many rotations a knob stack holds.
///
/// The stacks are 128 frames tall and stay traced data rather than
/// components — nobody hand-edits a sprite animation, and inlining 30k
/// rects of it would dominate this crate's compile time.
pub const KNOB_FRAMES: u32 = 128;

// ── fader ────────────────────────────────────────────────────────────────

/// The fader cap. `accent` is how REAPER's per-colour variants work: the
/// theme ships `<colour>/mcp_volthumb.png` folders, and the cap picks up
/// the track's accent.
#[derive(Props, Clone, PartialEq)]
pub struct FaderCapProps {
    /// Draw size in px. `None` uses the artwork's own size.
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Which sprite cell — normal, hover, pressed.
    #[props(default)]
    pub at: Interaction,
}

#[component]
pub fn VolumeFaderCap(props: FaderCapProps) -> Element {
    art("mcp_volthumb", props.width, props.height, props.at)
}

/// The trough the cap runs in.
#[component]
pub fn VolumeFaderTrack(props: FaderCapProps) -> Element {
    art("mcp_volbg", props.width, props.height, props.at)
}
