//! Folder visibility shared by track panels, the arrange canvas, and mixer.
//! REAPER stores nesting as a delta applied *after* each track.
use std::collections::HashSet;

use daw_proto::Track;
use dioxus::prelude::*;

#[derive(Clone, Default, PartialEq)]
pub struct FolderState {
    collapsed: HashSet<String>,
}

impl FolderState {
    pub fn is_collapsed(&self, guid: &str) -> bool {
        self.collapsed.contains(guid)
    }

    pub fn toggle(&mut self, guid: &str) {
        if !self.collapsed.remove(guid) {
            self.collapsed.insert(guid.to_owned());
        }
    }

    /// Preserve project order and original track numbers. Hidden descendants
    /// still contribute their closing deltas; nested collapse choices survive
    /// closing and reopening their parent.
    pub fn visible(&self, tracks: &[Track]) -> (Vec<Track>, Vec<u32>) {
        let mut visible = Vec::new();
        let mut depths = Vec::new();
        let mut depth = 0_u32;
        let mut hidden_below = None;
        for track in tracks {
            if hidden_below.is_some_and(|parent| depth <= parent) {
                hidden_below = None;
            }
            if hidden_below.is_none() {
                if track.visible_in_tcp {
                    visible.push(track.clone());
                    depths.push(depth);
                    if track.folder_depth > 0 && self.is_collapsed(&track.guid) {
                        hidden_below = Some(depth);
                    }
                } else if track.folder_depth > 0 {
                    // Hidden from the track panel, and a folder: hiding it
                    // hides what it holds, so unhiding the one track (the
                    // MIX BUS) brings the whole tree back. The mixer does
                    // not read this — it shows them all.
                    hidden_below = Some(depth);
                }
            }
            depth = depth.saturating_add_signed(track.folder_depth);
        }
        (visible, depths)
    }
}

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
