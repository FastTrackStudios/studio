//! The modulation's parameter tree and shared state: `plugin_core::fx::modulation`
//! (the plugin runs it with no editor), re-exported for the editor.

pub use crate::editor_form::EditorFormParams;
pub use plugin_core::fx::modulation::*;

impl EditorFormParams for ModParams {
    fn editor_form_cell(&self) -> &parking_lot::RwLock<String> {
        &self.editor_form
    }
}
