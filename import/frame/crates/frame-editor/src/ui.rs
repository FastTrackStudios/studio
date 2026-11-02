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
use frame_doc::doc::NodeId;

/// The UI-thread-owned controller supplied by the embedding host.
pub type EditorContext = Rc<RefCell<LiveEditor>>;

/// Edit a role's selected node. Successful changes reach the
/// shared ThemeHost on the controller's worker; diagnostics stay in the view.
#[component]
pub fn FrameThemeEditor() -> Element {
    let editor = use_context::<EditorContext>();
    let mut revision = use_signal(|| 0_u64);
    let mut status = use_signal(|| String::new());
    let mut fill = use_signal(String::new);
    let mut target = use_signal(String::new);
    let mut role = use_signal(|| None::<NodeId>);
    let mut selected = use_signal(|| None::<NodeId>);
    let polling = editor.clone();
    use_future(move || {
        let editor = polling.clone();
        async move {
            loop {
                while let Some(event) = editor.borrow().try_event() {
                    match event.result {
                        Ok(()) => status.set("Preview updated (unsaved)".to_owned()),
                        Err(CompileError::Superseded) => {},
                        Err(CompileError::HostChanged) => status.set("The theme changed outside this editor. Reopen the editor to continue previewing.".to_owned()),
                        Err(e) => status.set(format!("Preview failed: {e:?}")),
                    }
                }
                if let Some(result) = editor.borrow_mut().try_save_event() {
                    match result {
                        Ok(()) => status.set("Role saved".to_owned()),
                        Err(e) => status.set(format!("Save failed: {e:?}")),
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
    let save_editor = editor.clone();
    let (roles, nodes, root) = {
        let editor = editor.borrow();
        if let Ok(doc) = editor.session().doc() {
            let roles: Vec<_> = doc
                .pages
                .iter()
                .filter_map(|id| doc.node(*id).map(|n| (*id, n.name.clone())))
                .collect();
            let page = role()
                .filter(|id| doc.pages.contains(id))
                .or_else(|| doc.pages.first().copied());
            let nodes: Vec<_> = page
                .into_iter()
                .flat_map(|p| doc.subtree(p).into_iter().skip(1))
                .filter_map(|id| doc.node(id).map(|n| (id, n.name.clone())))
                .collect();
            let chosen = selected()
                .filter(|id| nodes.iter().any(|(node, _)| node == id))
                .or_else(|| nodes.first().map(|(id, _)| *id));
            let root = chosen.and_then(|id| {
                doc.node(id)
                    .map(|n| (id, n.name.clone(), n.width, n.height))
            });
            (roles, nodes, root)
        } else {
            (Vec::new(), Vec::new(), None)
        }
    };
    let role_name = role()
        .and_then(|id| {
            roles
                .iter()
                .find(|(page, _)| *page == id)
                .map(|(_, name)| name.clone())
        })
        .or_else(|| roles.first().map(|(_, name)| name.clone()));
    let files = editor.borrow().layer_files();
    rsx! {
        div {
            h2 { "Theme properties" }
            p { "Role" }
            div { style: "display: flex; flex-wrap: wrap; gap: 4px;",
                for (id, name) in roles {
                    button { key: "role-{id}", onclick: move |_| { role.set(Some(id)); selected.set(None); }, "{name}" }
                }
            }
            p { "Layer" }
            div { style: "display: flex; flex-wrap: wrap; gap: 4px; max-height: 180px; overflow-y: auto;",
                for (id, name) in nodes {
                    button { key: "node-{id}", onclick: move |_| selected.set(Some(id)), "{name}" }
                }
            }
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
                        "Set fill (replaces fill binding)"
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
                            match color_editor.borrow_mut().set_fill(id, &value) {
                                Ok(_) => { revision += 1; status.set("Updating preview…".to_owned()); },
                                Err(e) => status.set(format!("Edit failed: {e}")),
                            }
                        },
                        "Apply fill"
                    }
                }
                p { "Save selected role to a source file (rewrites its markup and removes comments)" }
                p { "Avoid editing the target file in another application while saving; simultaneous writes can overwrite changes." }
                div { style: "display: flex; flex-wrap: wrap; gap: 4px;",
                    for file in files {
                        {
                            let path = file.display().to_string();
                            rsx! { button { key: "{path}", onclick: move |_| target.set(path.clone()), "{file.display()}" } }
                        }
                    }
                }
                label { "Target file" input { r#type: "text", value: "{target}", oninput: move |e| target.set(e.value()) } }
                if let Some(role_name) = role_name {
                    button {
                        onclick: move |_| {
                            match save_editor.borrow_mut().request_save(role_name.clone(), target().into()) {
                                Ok(()) => status.set("Saving…".to_owned()),
                                Err(e) => status.set(format!("Save failed: {e:?}")),
                            }
                        },
                        "Save role"
                    }
                }
            } else {
                p { "This document has no role to edit." }
            }
            p { role: "status", "{status}" }
        }
    }
}
