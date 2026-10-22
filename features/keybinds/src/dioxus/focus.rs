//! Text-field focus, shared by every keyboard handler in a window.

use dioxus::prelude::{GlobalSignal, Signal};

/// While non-zero, a root keyboard handler should skip action
/// processing: a text field has focus. Increment on focus, decrement on
/// blur, so nested fields don't clobber each other.
pub static TEXT_INPUT_FOCUS_COUNT: GlobalSignal<u32> = Signal::global(|| 0);
