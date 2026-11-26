//! Which tracks a track panel shows: REAPER stores nesting as a delta
//! applied *after* each track, and a shut folder hides what it holds.
//! Plain data, shared by the track panels, the arrange canvas and the
//! mixer (`daw_ui::components::folders` re-exports it beside its hook),
//! and by the engine's read-back (`song_model`).

use std::collections::HashSet;

use daw_proto::Track;

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
