//! The reverb's parameter tree and shared state: `plugin_core::fx::reverb`
//! (the plugin runs it with no editor), re-exported for the editor.

pub use crate::editor_form::EditorFormParams;
pub use plugin_core::fx::reverb::*;

impl EditorFormParams for ReverbParams {
    fn editor_form_cell(&self) -> &parking_lot::RwLock<String> {
        &self.editor_form
    }
}
