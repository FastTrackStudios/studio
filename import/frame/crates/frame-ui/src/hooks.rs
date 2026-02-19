//! Hooks for common Frame UI operations.

use crate::prelude::*;
use crate::signals::*;
use frame_proto::*;

/// Hook that provides selection actions.
pub fn use_frame_selection() -> FrameSelectionActions {
    FrameSelectionActions
}

/// Actions for manipulating the current selection.
pub struct FrameSelectionActions;

impl FrameSelectionActions {
    /// Select a single node, replacing the current selection.
    pub fn select(&self, node_id: NodeId) {
        FRAME_SELECTION.write().clear();
        FRAME_SELECTION.write().push(node_id);
    }

    /// Toggle a node in the selection (for Cmd/Ctrl-click multi-select).
    pub fn toggle(&self, node_id: NodeId) {
        let mut sel = FRAME_SELECTION.write();
        if let Some(pos) = sel.iter().position(|id| *id == node_id) {
            sel.remove(pos);
        } else {
            sel.push(node_id);
        }
    }

    /// Clear the selection.
    pub fn clear(&self) {
        FRAME_SELECTION.write().clear();
    }

    /// Check if a node is selected.
    pub fn is_selected(&self, node_id: NodeId) -> bool {
        FRAME_SELECTION.read().contains(&node_id)
    }
}

/// Hook that provides document mutation actions with undo support.
pub fn use_frame_actions() -> FrameDocumentActions {
    FrameDocumentActions
}

/// Actions for mutating the Frame document.
pub struct FrameDocumentActions;

impl FrameDocumentActions {
    /// Insert a new node as a child of the given parent.
    pub fn insert_node(&self, node: FrameNode, parent_id: NodeId) {
        let mut doc = FRAME_DOCUMENT.write();
        doc.insert_node(node, parent_id);
    }

    /// Remove a node from the document.
    pub fn remove_node(&self, node_id: NodeId) {
        let mut doc = FRAME_DOCUMENT.write();
        doc.remove_node(node_id);
    }

    /// Set the active page.
    pub fn set_active_page(&self, page_id: NodeId) {
        *FRAME_ACTIVE_PAGE.write() = Some(page_id);
    }

    /// Set the zoom level.
    pub fn set_zoom(&self, zoom: f64) {
        *FRAME_ZOOM.write() = zoom;
    }

    /// Set the pan offset.
    pub fn set_pan(&self, offset: Point) {
        *FRAME_PAN_OFFSET.write() = offset;
    }

    /// Set the active tool.
    pub fn set_tool(&self, tool: FrameTool) {
        *FRAME_ACTIVE_TOOL.write() = tool;
    }
}
