//! The library's small parts: its type scale, the press sheet, and the
//! pieces every screen repeats (a sync mark, a player's initials, a
//! button, a heading with its trailing action).

use app_shell::tokens as t;
use dioxus::prelude::*;
use lucide_dioxus::{Check, Cloud, CloudDownload, HardDriveDownload, RefreshCw};

use crate::model::{Ready, Sync};

pub const DESK: &str = t::DESK;
pub const SHEET: &str = t::SHEET;
pub const SHEET_2: &str = t::SHEET_2;
pub const RULE: &str = t::RULE;
pub const RULE_STRONG: &str = t::RULE_STRONG;
pub const INK: &str = t::INK;
pub const INK_2: &str = t::INK_2;
pub const INK_3: &str = t::INK_3;
pub const DIM: &str = t::DIM;
pub const LIVE: &str = t::LIVE;
pub const WARN: &str = t::MODIFIED;
pub const FONT: &str = t::FONT;
/// Blue picks: the selected row, the current tab, the one solid button.
pub const PICK: &str = "#bfdbfe";
pub const PICK_BG: &str = "rgba(96,165,250,0.12)";
pub const PRIMARY: &str = "#2563eb";

/// Touch targets: 48 minimum, rows 60, the bar 56.
pub const HIT: u32 = 48;
pub const ROW: u32 = 60;
pub const BAR: u32 = 56;

/// Press and focus feedback, the one stylesheet (a static string, as
/// Blitz wants). `.lp` on anything pressable.
pub const SHEET_CSS: &str = "\
.lp { cursor: pointer; transition: background-color 120ms ease-out, transform 120ms ease-out; }\
.lp:hover { background-color: rgba(255,255,255,0.035); }\
.lp:active { background-color: rgba(255,255,255,0.07); transform: scale(0.995); }\
.lp:focus-visible { outline: 2px solid #bfdbfe; outline-offset: -2px; }\
.lp-solid:hover { background-color: #1d4ed8; }\
.lp-solid:active { background-color: #1e40af; }\
";

/// The text styles, as inline fragments.
pub const TITLE: &str =
    "font-size: 28px; font-weight: 750; letter-spacing: -0.02em; line-height: 1.15;";
pub const HEADING: &str = "font-size: 17px; font-weight: 700; letter-spacing: -0.01em;";
pub const BODY: &str = "font-size: 15px; font-weight: 560;";
pub const META: &str = "font-size: 13px; font-weight: 500;";
/// Column headers of a table: small, caps, quiet.
pub const COLUMN: &str =
    "font-size: 11px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase;";
/// One line that clips instead of wrapping (Blitz has no ellipsis).
pub const ONE_LINE: &str = "white-space: nowrap; overflow: hidden; min-width: 0;";

/// A heading over a block, with an optional action at its end.
#[component]
pub fn Heading(text: String, #[props(default)] trailing: Option<Element>) -> Element {
    rsx! {
        div {
            style: "display: flex; align-items: center; gap: 12px; padding: 28px 0 10px;",
            span { style: "{HEADING} color: {INK};", "{text}" }
            span { style: "flex: 1;" }
            {trailing}
        }
    }
}

/// A quiet link-button at a heading's end ("See all").
#[component]
pub fn TextButton(label: String, on_press: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: "lp",
            style: "height: 36px; padding: 0 10px; border: none; border-radius: 8px; background: transparent; \
                    color: {PICK}; font-family: {FONT}; {META} font-weight: 650; display: flex; align-items: center;",
            onclick: move |_| on_press.call(()),
            "{label}"
        }
    }
}

/// How prominent a button is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    /// The one solid button of a screen.
    Primary,
    /// An outlined one.
    Secondary,
}

/// A button: a label, an optional leading icon, 44 tall (48 hit with its
/// gap).
#[component]
pub fn Button(
    label: String,
    weight: Weight,
    #[props(default)] icon: Option<Element>,
    on_press: EventHandler<()>,
) -> Element {
    let (class, look) = match weight {
        Weight::Primary => (
            "lp lp-solid",
            format!("background: {PRIMARY}; color: #fff; border: 1px solid {PRIMARY};"),
        ),
        Weight::Secondary => (
            "lp",
            format!("background: transparent; color: {INK}; border: 1px solid {RULE_STRONG};"),
        ),
    };
    rsx! {
        button {
            class,
            style: "height: 44px; padding: 0 16px; border-radius: 10px; display: flex; align-items: center; \
                    justify-content: center; gap: 8px; font-family: {FONT}; font-size: 15px; font-weight: 650; \
                    white-space: nowrap; {look}",
            onclick: move |_| on_press.call(()),
            {icon}
            "{label}"
        }
    }
}

/// A person's initials in a circle, tinted by their name so a band's
/// members are told apart at a glance.
#[component]
pub fn Avatar(name: String, #[props(default = 28)] size: u32) -> Element {
    const TINTS: [(&str, &str); 5] = [
        ("#1e3a8a", "#dbeafe"),
        ("#14532d", "#dcfce7"),
        ("#7c2d12", "#ffedd5"),
        ("#581c87", "#f3e8ff"),
        ("#134e4a", "#ccfbf1"),
    ];
    let hash = name.bytes().fold(0usize, |h, b| {
        h.wrapping_mul(31).wrapping_add(usize::from(b))
    });
    let (bg, fg) = TINTS[hash % TINTS.len()];
    let initials: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect();
    let font = size * 2 / 5;
    rsx! {
        span {
            style: "width: {size}px; height: {size}px; border-radius: {size}px; flex-shrink: 0; display: flex; \
                    align-items: center; justify-content: center; background: {bg}; color: {fg}; \
                    font-size: {font}px; font-weight: 750;",
            "{initials}"
        }
    }
}

/// How a doc stands, as a mark and a word: on this iPad, synced, catching
/// up, or in the cloud only.
#[component]
pub fn SyncMark(sync: Sync, #[props(default)] quiet: bool) -> Element {
    let (colour, word) = match sync {
        Sync::Offline => (LIVE, "On iPad".to_owned()),
        Sync::Synced => (INK_3, "Synced".to_owned()),
        Sync::Syncing(f) => (WARN, format!("{:.0}%", f * 100.0)),
        Sync::Cloud => (INK_3, "Cloud".to_owned()),
    };
    rsx! {
        span {
            style: "display: flex; align-items: center; gap: 6px; color: {colour}; {META} white-space: nowrap;",
            match sync {
                Sync::Offline => rsx! { HardDriveDownload { size: 15, color: colour } },
                Sync::Synced => rsx! { Check { size: 15, color: colour } },
                Sync::Syncing(_) => rsx! { RefreshCw { size: 15, color: colour } },
                Sync::Cloud => rsx! { Cloud { size: 15, color: colour } },
            }
            if !quiet { span { "{word}" } }
        }
    }
}

/// Whether a chart or the audio is ready: a word, coloured by state.
#[component]
pub fn ReadyMark(what: &'static str, ready: Ready) -> Element {
    // An empty `what` says only the state (a file's row).
    let (colour, word) = match (ready, what.is_empty()) {
        (Ready::Here, true) => (INK_2, "On iPad".to_owned()),
        (Ready::Cloud, true) => (INK_3, "Cloud".to_owned()),
        (Ready::Missing, true) => (WARN, "Missing".to_owned()),
        (Ready::Here, false) => (INK_2, what.to_owned()),
        (Ready::Cloud, false) => (INK_3, format!("{what} in cloud")),
        (Ready::Missing, false) => (WARN, format!("No {}", what.to_lowercase())),
    };
    rsx! {
        span {
            style: "display: flex; align-items: center; gap: 5px; color: {colour}; {META} white-space: nowrap;",
            match ready {
                Ready::Here => rsx! { Check { size: 14, color: colour } },
                Ready::Cloud => rsx! { CloudDownload { size: 14, color: colour } },
                Ready::Missing => rsx! { span { style: "width: 6px; height: 6px; border-radius: 3px; background: {colour};" } },
            }
            "{word}"
        }
    }
}

/// A small rounded fact: a key, a tempo, a stage.
#[component]
pub fn Chip(text: String, #[props(default)] strong: bool) -> Element {
    let (border, colour) = if strong {
        (PICK, PICK)
    } else {
        (RULE_STRONG, INK_2)
    };
    rsx! {
        span {
            style: "height: 24px; padding: 0 8px; border-radius: 6px; border: 1px solid {border}; color: {colour}; \
                    display: flex; align-items: center; {META} font-weight: 650; white-space: nowrap; \
                    font-variant-numeric: tabular-nums;",
            "{text}"
        }
    }
}

/// A song's colour as a short bar at the start of a row.
#[component]
pub fn Swatch(colour: String, #[props(default = 28)] height: u32) -> Element {
    rsx! {
        span { style: "width: 4px; height: {height}px; border-radius: 2px; background: {colour}; flex-shrink: 0;" }
    }
}

/// Bytes, said the way a person reads them.
#[must_use]
pub fn size(bytes: u64) -> String {
    const MB: f64 = 1_000_000.0;
    let b = bytes as f64;
    if b >= 1000.0 * MB {
        format!("{:.1} GB", b / (1000.0 * MB))
    } else if b >= MB {
        format!("{:.0} MB", b / MB)
    } else {
        format!("{:.0} KB", (b / 1000.0).max(1.0))
    }
}
