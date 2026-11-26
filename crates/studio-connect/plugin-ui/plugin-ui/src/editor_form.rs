//! The editor form a session persists: the param trees keep its id
//! (`editor_form`, a persisted string, in `plugin_core::fx`); the form
//! itself is the editor's ([`fts_audio_ui::EditorForm`]).

use fts_audio_ui::EditorForm;

/// A param tree that persists its editor's form by id.
pub trait EditorFormParams {
    /// The persisted id.
    fn editor_form_cell(&self) -> &parking_lot::RwLock<String>;

    /// The editor form a loaded session should open at. An unknown or missing
    /// id means Responsive, which is the size the face asks for anyway.
    fn resolved_editor_form(&self) -> EditorForm {
        EditorForm::from_id(&self.editor_form_cell().read()).unwrap_or_default()
    }

    fn store_editor_form(&self, form: EditorForm) {
        *self.editor_form_cell().write() = form.id().to_string();
    }
}
