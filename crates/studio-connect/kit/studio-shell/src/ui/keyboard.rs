//! A text field's hold on the keyboard, so global hotkeys stand aside
//! while it is typed into (`utils::keyboard`). The hook is the rig frame's
//! (`signal_rig_shell::keyboard`), so a rig and the studio shell share it.

pub use signal_rig_shell::keyboard::{TextFocus, use_text_focus};

/// Holds the keyboard switch (`utils::keyboard::TEXT_FIELD`) whenever the
/// renderer has a text input focused, in any window of the process. Call
/// once, before the app launches.
#[cfg(feature = "native")]
pub fn follow_text_fields() {
    dioxus_native::on_text_focus_change(|on| utils::keyboard::TEXT_FIELD.set(on));
}
