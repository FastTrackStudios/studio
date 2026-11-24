//! Wheel gestures, resolved from the shared input configuration.
//!
//! The editor used to hardcode a `match` on modifiers, and it disagreed
//! with both the REAPER config and the arrange view about what the same
//! gesture meant — Shift did nothing here, Alt scrolled sideways instead
//! of zooming, and plain wheel zoomed instead of scrolling. Three
//! schemes for one set of gestures.
//!
//! So the mapping is not written here at all. It comes from
//! `input`'s keymap, under the [`SURFACE`] key, and REAPER binds the same
//! gesture names to its own command ids. Changing what Shift+wheel does
//! is a config edit, in one file, for every surface at once.
//!
//! The editor is its own *surface* rather than reusing the arrange
//! view's because it does not fully exist in the DAW: it has gestures the
//! arrange view has no meaning for (nudging notes off-grid). A surface
//! may add to the shared scheme; it must not redefine it, which is what
//! `input`'s own tests pin.

use architect_input::{InputContext, KeymapConfig, ScrollBindingTable, ScrollEvent};

use crate::tools::Mods;

/// The editor's key in the keymap's `scroll` table.
pub const SURFACE: &str = "editor";

/// The resolved bindings, built once.
///
/// A `OnceLock` rather than a `use_signal`, because the table is process
/// state, not component state: every canvas in every panel resolves the
/// same gestures, and rebuilding it per mount would parse the keymap on
/// every remount.
static TABLE: std::sync::OnceLock<ScrollBindingTable> = std::sync::OnceLock::new();

/// Load the shipped defaults, overlaid with the user's keymap where
/// there is one.
///
/// A broken user keymap falls back to the defaults rather than leaving
/// the editor with no scroll at all — a config typo should cost you your
/// customisation, not the ability to scroll.
fn build() -> ScrollBindingTable {
    let mut config = match keybinds::load_default_config() {
        Ok(c) => c,
        // The defaults are `include_str!`d and tested, so this is
        // unreachable short of a bad edit to the crate itself.
        Err(_) => KeymapConfig::default(),
    };

    // User overrides are a filesystem read, which a browser build has no
    // business attempting.
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(Some(user)) = keybinds::load_user_config() {
        config = KeymapConfig::merge(config, user);
    }

    architect_input::table_for_surface(&config, SURFACE).unwrap_or_default()
}

fn table() -> &'static ScrollBindingTable {
    TABLE.get_or_init(build)
}

/// The user's own wheel bindings for this surface, and only those: what
/// overrides the shared wheel (`timeline::gestures::wheel`) on the roll.
static USER: std::sync::OnceLock<Option<ScrollBindingTable>> = std::sync::OnceLock::new();

fn user_table() -> Option<&'static ScrollBindingTable> {
    USER.get_or_init(|| {
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(Some(user)) = keybinds::load_user_config() {
            return architect_input::table_for_surface(&user, SURFACE).ok();
        }
        None
    })
    .as_ref()
}

/// What the user's own keymap binds this gesture to on the roll, if
/// anything: the override of the shared wheel.
pub fn override_for(dx: f64, dy: f64, mods: Mods) -> Option<String> {
    let event = ScrollEvent {
        delta_x: dx,
        delta_y: dy,
        modifiers: architect_input::Modifiers {
            ctrl: mods.ctrl,
            alt: mods.alt,
            shift: mods.shift,
            ..Default::default()
        },
    };
    user_table()?.match_event(&event, &InputContext::new())
}

/// One wheel notch, in pixels.
///
/// The conversion a touchpad needs. A mouse reports scroll in *lines* —
/// winit gives about 1.0 per notch — and every constant downstream is
/// tuned for that. A touchpad reports *pixels*, tens of them per event,
/// and the two were being passed on interchangeably: `strip_units()`
/// discards which unit arrived and hands over the bare number.
///
/// The result was not a subtle difference in feel. Zoom is
/// `e^(travel / 3)`, so a forty-pixel two-finger gesture asked for a
/// six-hundred-thousand-fold zoom, and a two-finger scroll panned five
/// thousand pixels an event. Touchpad input was unusable rather than
/// merely wrong.
pub const PIXELS_PER_NOTCH: f64 = 20.0;

/// What the shared configuration says this gesture does here.
///
/// `None` when nothing is bound, which the caller should treat as "leave
/// the view alone" rather than falling back to a guess — an unbound
/// gesture that still did something is how the schemes drifted apart.
pub fn action_for(dx: f64, dy: f64, mods: Mods) -> Option<String> {
    let event = ScrollEvent {
        delta_x: dx,
        delta_y: dy,
        modifiers: architect_input::Modifiers {
            ctrl: mods.ctrl,
            alt: mods.alt,
            shift: mods.shift,
            ..Default::default()
        },
    };
    table().match_event(&event, &InputContext::new())
}

/// A one-line summary of what the roll's wheel does: the shared wheel
/// (`timeline::gestures::wheel`), where the user's keymap does not say
/// otherwise.
pub fn hint() -> String {
    let mut parts = Vec::new();
    for (label, mods, shared) in [
        ("scroll", Mods::default(), "scrolls"),
        (
            "shift",
            Mods {
                shift: true,
                ..Mods::default()
            },
            "scrolls sideways",
        ),
    ] {
        let what = override_for(0.0, 1.0, mods).map_or(shared.to_owned(), |action| {
            match action.as_str() {
                "view.vscroll" => "scrolls",
                "view.hscroll" => "scrolls sideways",
                "view.zoom_v" => "zooms pitch",
                "view.zoom_h" => "zooms time",
                "view.zoom_both" => "zooms",
                other => other,
            }
            .to_owned()
        });
        parts.push(format!("{label} {what}"));
    }
    parts.push("z zooms time".to_owned());
    parts.push("z+shift zooms pitch".to_owned());
    parts.join(" \u{b7} ")
}
