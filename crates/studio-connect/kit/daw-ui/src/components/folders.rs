//! Folder visibility shared by track panels, the arrange canvas, and mixer.
//! REAPER stores nesting as a delta applied *after* each track.
use dioxus::prelude::*;

/// The folder state is plain data: the views' contract's (`daw_view::folders`).
pub use daw_view::folders::FolderState;

/// A window may provide this signal to synchronize its panels. A standalone
/// panel owns its own state when no window context is available.
pub fn use_folder_state() -> Signal<FolderState> {
    let local = use_signal(FolderState::default);
    try_consume_context::<Signal<FolderState>>().unwrap_or(local)
}

#[component]
pub fn FolderButton(name: String, collapsed: bool, ontoggle: EventHandler<()>) -> Element {
    let action = if collapsed { "Expand" } else { "Collapse" };
    rsx! {
        button {
            r#type: "button",
            title: "{action} {name}",
            "data-testid": "folder-{name}",
            aria_label: "{action} {name}",
            aria_expanded: "{!collapsed}",
            style: "width:20px;height:20px;padding:0;border:0;background:transparent;color:inherit;cursor:pointer;",
            onclick: move |event| { event.stop_propagation(); ontoggle.call(()); },
            {if collapsed { "▸" } else { "▾" }}
        }
    }
}
