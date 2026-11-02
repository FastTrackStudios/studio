//! The Frame theme editor mounted in Studio's dock.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use dioxus::prelude::*;
use frame_editor::live::LiveEditor;
use frame_editor::ui::{EditorContext, FrameThemeEditor};

/// Open the currently displayed theme as an in-memory editor preview.
#[component]
pub fn ThemeEditorPanel() -> Element {
    let mut editor = use_signal(|| None::<EditorContext>);
    let mut error = use_signal(|| None::<String>);
    use_future(move || async move {
        let mut opening = LiveEditor::open_async(crate::skin::host());
        loop {
            if let Some(result) = opening.try_finish() {
                match result {
                    Ok(opened) => editor.set(Some(Rc::new(RefCell::new(opened)))),
                    Err(why) => error.set(Some(format!("{why:?}"))),
                }
                break;
            }
            architect::platform::sleep(Duration::from_millis(50)).await;
        }
    });
    rsx! {
        div { style: "display:flex; flex-direction:column; min-width:0; min-height:0; padding:12px; overflow-y:scroll;",
            p { "Preview changes are in memory until you save a role." }
            if let Some(why) = error.read().as_ref() {
                p { role: "status", "Could not open the theme editor: {why}" }
            } else if editor.read().is_some() {
                EditorReady { editor }
            } else {
                p { role: "status", "Opening theme editor…" }
            }
        }
    }
}

#[component]
fn EditorReady(editor: Signal<Option<EditorContext>>) -> Element {
    use_context_provider(|| editor.peek().as_ref().expect("editor is ready").clone());
    rsx! { FrameThemeEditor {} }
}
