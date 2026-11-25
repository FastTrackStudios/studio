//! The mixer strip, composed from the primitives.
//!
//! This is the web mixer *and* the reference for what REAPER should look
//! like. REAPER composites its own MCP from ~96 separate images per the
//! WALTER layout, so it never renders this component — but every piece it
//! does blit is one of the same primitives, which is what keeps the two
//! surfaces honest.
//!
//! Layout is HTML/flex rather than one big SVG: the web wants a real,
//! resizable, interactive mixer, and the primitives are already
//! self-contained SVGs that drop into a flex box.

use crate::theme::{Color, Theme};
use dioxus::prelude::*;

use crate::art::primitives::{Button, ControlState, Groove, Meter, Thumb};

/// One channel of the mixer.
#[derive(Props, Clone, PartialEq)]
pub struct StripProps {
    pub name: String,
    /// Track colour — the tint across the top of the strip.
    #[props(default)]
    pub color: Option<Color>,
    /// Fader position, 0–1.
    #[props(default = 0.75)]
    pub fader: f32,
    /// Meter level, 0–1.
    #[props(default = 0.0)]
    pub level: f32,
    #[props(default)]
    pub muted: bool,
    #[props(default)]
    pub soloed: bool,
    /// Strip width in px.
    #[props(default = 76)]
    pub width: u32,
}

#[component]
pub fn MixerStrip(props: StripProps) -> Element {
    let t = Theme::default();
    let c = &t.chrome;
    let accent = props.color.unwrap_or(t.signal.neutral_track);

    let fader_h = 150u32;
    let thumb_h = 14u32;
    // The thumb travels the groove minus its own height, so at 1.0 its top
    // edge sits flush rather than half off the end.
    let travel = fader_h.saturating_sub(thumb_h) as f32;
    let thumb_top = ((1.0 - props.fader.clamp(0.0, 1.0)) * travel).round();

    rsx! {
        div {
            style: "display: flex; flex-direction: column; align-items: center; \
                    width: {props.width}px; padding: 6px; gap: 6px; \
                    background: {c.surface_raised.css()}; \
                    border: 1px solid {c.border.css()}; \
                    border-radius: {t.metrics.radius}px;",

            // Track colour tab.
            div {
                style: "width: 100%; height: 14px; border-radius: 3px; \
                        background: {accent.css()};",
            }

            // Fader and meter side by side.
            div {
                style: "display: flex; gap: 6px; align-items: flex-start;",

                // The fader: a groove with a thumb positioned over it.
                div {
                    style: "position: relative; width: 22px; height: {fader_h}px;",
                    Groove { width: 22, height: fader_h }
                    div {
                        style: "position: absolute; left: 1px; top: {thumb_top}px;",
                        Thumb { width: 20, height: thumb_h, accent: Some(accent) }
                    }
                }

                Meter { width: 8, height: fader_h, level: props.level }
            }

            // Mute / solo. Each keeps its own engaged colour rather than
            // both lighting the accent, so the two stay tellable apart.
            div {
                style: "display: flex; gap: 4px;",
                Button {
                    width: 22, height: 16, label: Some("M".into()),
                    state: if props.muted { ControlState::On } else { ControlState::Off },
                    on_color: Some(t.signal.mute),
                }
                Button {
                    width: 22, height: 16, label: Some("S".into()),
                    state: if props.soloed { ControlState::On } else { ControlState::Off },
                    on_color: Some(t.signal.solo),
                }
            }

            div {
                style: "color: {c.text.css()}; font-family: Fira Sans, sans-serif; \
                        font-size: 11px; max-width: 100%; overflow: hidden; \
                        text-overflow: ellipsis; white-space: nowrap;",
                "{props.name}"
            }
        }
    }
}

/// A row of strips — the mixer.
#[derive(Props, Clone, PartialEq)]
pub struct MixerProps {
    pub strips: Vec<StripProps>,
}

#[component]
pub fn Mixer(props: MixerProps) -> Element {
    let t = Theme::default();
    rsx! {
        div {
            style: "display: flex; gap: 6px; padding: 8px; \
                    background: {t.chrome.surface.css()};",
            for (i, strip) in props.strips.iter().enumerate() {
                MixerStrip { key: "{i}", ..strip.clone() }
            }
        }
    }
}
