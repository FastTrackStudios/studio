//! An embeddable first properties panel for a live theme document.
//!
//! The host provides `Rc<RefCell<LiveEditor>>` as Dioxus context, constructed
//! from the document it owns and the same ThemeHost used by its playing view.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::live::LiveEditor;
use crate::theme::CompileError;
use dioxus::prelude::*;

/// The UI-thread-owned controller supplied by the embedding host.
pub type EditorContext = Rc<RefCell<LiveEditor>>;

/// Edit the size of the first role's root. Successful changes reach the
/// shared ThemeHost on the controller's worker; diagnostics stay in the view.
#[component]
pub fn FrameThemeEditor() -> Element {
    let editor = use_context::<EditorContext>();
    let mut revision = use_signal(|| 0_u64);
    let mut status = use_signal(|| String::new());
    let mut fill = use_signal(String::new);
    let polling = editor.clone();
    use_future(move || {
        let editor = polling.clone();
        async move {
            loop {
                while let Some(event) = editor.borrow().try_event() {
                    match event.result {
                        Ok(()) => status.set("Preview updated".to_owned()),
                        Err(CompileError::Superseded) => {},
                        Err(CompileError::HostChanged) => status.set("The theme changed outside this editor. Reopen the editor to continue previewing.".to_owned()),
                        Err(e) => status.set(format!("Preview failed: {e:?}")),
                    }
                }
                let changed = "The theme changed outside this editor. Reopen the editor to continue previewing.";
                if editor.borrow().host_changed() && status.peek().as_str() != changed {
                    status.set(changed.to_owned());
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    });
    let _ = revision();
    let color_editor = editor.clone();
    let root = {
        let editor = editor.borrow();
        editor.session().doc().ok().and_then(|doc| {
            let page = doc.pages.first().and_then(|id| doc.node(*id))?;
            let id = *page.children.first()?;
            let node = doc.node(id)?;
            Some((id, node.name.clone(), node.width, node.height))
        })
    };
    rsx! {
        div {
            h2 { "Theme properties" }
            if let Some((id, name, width, height)) = root {
                p { "{name}: {width:.0} × {height:.0}" }
                button {
                    onclick: move |_| {
                        let args = serde_json::json!({"ids": [id], "width": width + 10.0, "height": height});
                        match editor.borrow_mut().execute("transform.resize", &args, None) {
                            Ok(_) => { revision += 1; status.set("Updating preview…".to_owned()); },
                            Err(e) => status.set(format!("Edit failed: {e}")),
                        }
                    },
                    "Widen by 10 px"
                }
                div {
                    label {
                        "Fill color"
                        input {
                            r#type: "text",
                            placeholder: "#rrggbb",
                            style: "width: 110px; margin: 0 8px; padding: 4px;",
                            value: "{fill}",
                            oninput: move |e| fill.set(e.value()),
                        }
                    }
                    button {
                        onclick: move |_| {
                            let value = fill().trim().to_owned();
                            let args = serde_json::json!({"id": id, "props": {"fill": value}});
                            match color_editor.borrow_mut().execute("node.set", &args, None) {
                                Ok(_) => { revision += 1; status.set("Updating preview…".to_owned()); },
                                Err(e) => status.set(format!("Edit failed: {e}")),
                            }
                        },
                        "Apply fill"
                    }
                }
            } else {
                p { "This document has no role to edit." }
            }
            p { role: "status", "{status}" }
        }
    }
}
