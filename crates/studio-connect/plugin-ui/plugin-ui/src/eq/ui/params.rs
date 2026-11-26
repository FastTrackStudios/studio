//! The EQ's parameter tree and shared state: `plugin_core::fx::eq` (the
//! plugin runs it with no editor), re-exported for the editor, plus the
//! editor's own handle on a stage ([`EqStageRef`]).

use std::sync::Arc;

pub use plugin_core::fx::eq::*;

// The range table itself lives in the portable graph model
// (`eq_graph_model::DB_RANGE_STEPS`) so wasm builds share it; re-exported
// here because the param layer is its natural address.
pub use crate::eq::ui::eq_graph_model::{
    DB_RANGE_STEPS, DEFAULT_DB_RANGE, db_range_for_index, db_range_index_containing,
};

/// One stage of the tree, as the editor holds it: cheap to clone, `'static`,
/// and a `Deref` to the stage — so a surface that used to take the whole
/// tree takes this instead and its `params.bands[i]` reads the stage.
///
/// The index is fixed at construction. Components rebuild it every render
/// from the focus signal, so a closure built this render edits the stage
/// that was focused when it was built — the one on screen.
#[derive(Clone)]
pub struct EqStageRef {
    root: Arc<FtsEqParams>,
    index: usize,
}

impl EqStageRef {
    /// Stage `index` of `root` (clamped into the pool).
    #[must_use]
    pub fn new(root: Arc<FtsEqParams>, index: usize) -> Self {
        Self {
            root,
            index: index.min(MAX_STAGES - 1),
        }
    }

    /// The stage the editor is focused on (`fx.stack.focus`) — stage 0 where
    /// no focus provider is mounted. Reads a context, so call it from a
    /// component body like any hook.
    #[must_use]
    pub fn focused(root: Arc<FtsEqParams>) -> Self {
        let stage = crate::stack::host::use_visible_stage(root.as_ref());
        Self::new(root, stage)
    }

    /// The whole tree — for the editor-level state (form, focus, stack).
    #[must_use]
    pub const fn root(&self) -> &Arc<FtsEqParams> {
        &self.root
    }

    /// Which stage this is, 0-based.
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }
}

impl std::ops::Deref for EqStageRef {
    type Target = EqStageParams;
    fn deref(&self) -> &EqStageParams {
        self.root.stage(self.index)
    }
}
