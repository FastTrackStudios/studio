//! The MIDI editor as a Studio view: the expression editor (`expression`:
//! the piano roll, drum lanes and curves over one document, drawn with
//! daw-view-kit's skin and text) and the Editor panel that follows the
//! arrangement (`editor_view`). It sits on the editor crate (the document,
//! its painter and gestures) and on daw-view-kit; the kit never names it
//! (daw-view-kit/docs/extraction-plan.md, step 5). It goes to the
//! midi-editor repo with the views.

// The kit, under the paths this code was written against (`crate::skin`,
// `crate::text`, …).
#[allow(unused_imports)]
use daw_view_kit::*;

pub mod editor_view;
pub mod expression;

/// Make this editor the app's: the arrangement's edit target (a plain
/// click on an item opens it here), its key sequences (the panes stand
/// aside while one is pending) and the `editor.*` tokens of the themes the
/// kit exports. Once per process, at start.
pub fn install() {
    editor_view::install();
    // A sequence half typed (`g e`) keeps its keys from the panes.
    daw_view_kit::keys::install_pending(editor::paint::keys::is_pending);
    daw_view_kit::skin::export::install_editor_tokens(expression::theme_tokens);
}
