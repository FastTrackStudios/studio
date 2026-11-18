//! Architect's pane input as the Blitz events Studio's widgets take: what
//! dioxus-native hands them in `studio-desktop`'s window.

use std::str::FromStr as _;

use architect_workspace::Button;
use blitz_traits::events::{
    BlitzKeyEvent, BlitzPointerEvent, BlitzPointerId, Code, Key, KeyState, Location, Modifiers,
    MouseEventButton, MouseEventButtons, PointerCoords, PointerDetails,
};

/// Architect's modifier names as Blitz's (`Mod` is the platform's command
/// key: Ctrl here, ⌘ on a Mac).
#[must_use]
pub fn modifiers(mods: &[&str]) -> Modifiers {
    let mut out = Modifiers::empty();
    for m in mods {
        out |= match *m {
            "Shift" => Modifiers::SHIFT,
            "Alt" => Modifiers::ALT,
            "Ctrl" => Modifiers::CONTROL,
            "Mod" if cfg!(target_os = "macos") => Modifiers::META,
            "Mod" => Modifiers::CONTROL,
            "Meta" => Modifiers::META,
            _ => Modifiers::empty(),
        };
    }
    out
}

/// The same, as the arrangement panel's mouse map reads them.
#[must_use]
pub fn panel_mods(mods: &[&str]) -> session_ui::mousemap::Mods {
    let m = modifiers(mods);
    session_ui::mousemap::Mods {
        shift: m.contains(Modifiers::SHIFT),
        ctrl: m.contains(Modifiers::CONTROL),
        alt: m.contains(Modifiers::ALT),
    }
}

/// A button as Blitz names it, the buttons it holds, and as the
/// arrangement panel names it.
#[must_use]
pub const fn mouse(
    button: &Button,
) -> (
    MouseEventButton,
    MouseEventButtons,
    session_ui::panel::Button,
) {
    match button {
        Button::Primary => (
            MouseEventButton::Main,
            MouseEventButtons::Primary,
            session_ui::panel::Button::Left,
        ),
        Button::Middle => (
            MouseEventButton::Auxiliary,
            MouseEventButtons::Auxiliary,
            session_ui::panel::Button::Middle,
        ),
        Button::Secondary => (
            MouseEventButton::Secondary,
            MouseEventButtons::Secondary,
            session_ui::panel::Button::Other,
        ),
    }
}

/// A pointer event at `(x, y)` in the pane, as Blitz hands one to a
/// widget (coordinates relative to it).
#[must_use]
pub fn pointer(
    (x, y): (f32, f32),
    button: MouseEventButton,
    buttons: MouseEventButtons,
    mods: Modifiers,
) -> BlitzPointerEvent {
    BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: PointerCoords {
            page_x: x,
            page_y: y,
            screen_x: x,
            screen_y: y,
            client_x: x,
            client_y: y,
        },
        button,
        buttons,
        mods,
        details: PointerDetails::default(),
        element: blitz_traits::events::Point { x, y },
        active_pointers: std::sync::Arc::default(),
    }
}

/// A key as Blitz hands one to a widget: by its W3C names. A letter's code
/// is its key's (`z` is `KeyZ`), which is what Studio's keys read.
#[must_use]
pub fn key(name: &str, mods: &[&str], state: KeyState) -> BlitzKeyEvent {
    let key = Key::from_str(name).unwrap_or_else(|_| Key::Character(name.to_owned()));
    let code = match name.chars().next() {
        Some(c) if name.chars().count() == 1 && c.is_ascii_alphabetic() => {
            Code::from_str(&format!("Key{}", c.to_ascii_uppercase())).unwrap_or(Code::Unidentified)
        }
        Some(c) if name.chars().count() == 1 && c.is_ascii_digit() => {
            Code::from_str(&format!("Digit{c}")).unwrap_or(Code::Unidentified)
        }
        _ if name == " " => Code::Space,
        _ => Code::from_str(name).unwrap_or(Code::Unidentified),
    };
    BlitzKeyEvent {
        key,
        code,
        modifiers: modifiers(mods),
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state,
        text: None,
    }
}
