//! Architect's pane input as the Blitz events Studio's widgets take: what
//! dioxus-native hands them in `studio-desktop`'s window.

use std::str::FromStr as _;

use architect_scene::view::Button;
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
            "Mod" => {
                if cfg!(target_os = "macos") {
                    Modifiers::META
                } else {
                    Modifiers::CONTROL
                }
            }
            "Meta" => Modifiers::META,
            _ => Modifiers::empty(),
        };
    }
    out
}

/// The same, as the arrangement panel's mouse map reads them.
#[must_use]
pub fn panel_mods(mods: &[&str]) -> daw_view_kit::mousemap::Mods {
    let m = modifiers(mods);
    daw_view_kit::mousemap::Mods {
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
    daw_view_kit::panel_input::Button,
) {
    match button {
        Button::Primary => (
            MouseEventButton::Main,
            MouseEventButtons::Primary,
            daw_view_kit::panel_input::Button::Left,
        ),
        Button::Middle => (
            MouseEventButton::Auxiliary,
            MouseEventButtons::Auxiliary,
            daw_view_kit::panel_input::Button::Middle,
        ),
        Button::Secondary => (
            MouseEventButton::Secondary,
            MouseEventButtons::Secondary,
            daw_view_kit::panel_input::Button::Other,
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

/// A finger's touch as Blitz hands one to a widget.
///
/// `ViewInput::Touch` (`phase` `start`, `move`, `end` or `cancel`) at
/// `(x, y)` in the pane, as the pointer event of finger `id`, the way
/// `studio-desktop`'s window and the browser host hand a touch on (a
/// finger is told apart by its id, so two faders move at once).
/// `fingers` holds the fingers down, the first of them the primary one.
/// `None` for a phase it does not know.
#[must_use]
pub fn touch(
    fingers: &mut Vec<u64>,
    id: u64,
    (x, y): (f32, f32),
    phase: &str,
) -> Option<blitz_traits::events::UiEvent> {
    use blitz_traits::events::UiEvent;
    let event = |buttons: MouseEventButtons, primary: bool| BlitzPointerEvent {
        id: BlitzPointerId::Finger(id),
        is_primary: primary,
        ..pointer((x, y), MouseEventButton::Main, buttons, Modifiers::empty())
    };
    let primary = |fingers: &[u64]| fingers.first().is_none_or(|first| *first == id);
    Some(match phase {
        "start" => {
            if !fingers.contains(&id) {
                fingers.push(id);
            }
            UiEvent::PointerDown(event(MouseEventButtons::Primary, primary(fingers)))
        }
        "move" => UiEvent::PointerMove(event(MouseEventButtons::Primary, primary(fingers))),
        "end" | "cancel" => {
            let was = primary(fingers);
            fingers.retain(|f| *f != id);
            let e = event(MouseEventButtons::empty(), was);
            if phase == "end" {
                UiEvent::PointerUp(e)
            } else {
                UiEvent::PointerCancel(e)
            }
        }
        _ => return None,
    })
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
