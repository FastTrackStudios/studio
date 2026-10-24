//! Layout history — undo/redo based on compact layout diffs.

use crate::layout::DockLayout;
use crate::LayoutDiff;

// region: --- LayoutHistory

/// Undo/redo history for dock layout changes.
///
/// Records layout states on mutation and allows stepping backward (undo)
/// and forward (redo) through the history. The redo stack is cleared
/// whenever a new mutation is recorded, matching standard undo semantics.
///
/// # Example
///
/// ```
/// use crate::history::LayoutHistory;
/// use crate::layout::DockLayout;
/// use crate::panel::PanelId;
///
/// let mut layout = DockLayout::single(PanelId::Performance);
/// let mut history = LayoutHistory::new();
///
/// let before = layout.clone();
/// // ... mutate layout ...
/// history.push(&before, &layout);
///
/// // Now we can undo:
/// history.undo(&mut layout);
/// ```
#[derive(Debug, Clone)]
pub struct LayoutHistory {
    undo_stack: Vec<LayoutDiff>,
    redo_stack: Vec<LayoutDiff>,
    max_depth: usize,
}

impl LayoutHistory {
    /// Default maximum undo depth.
    pub const DEFAULT_MAX_DEPTH: usize = 50;

    /// Create a new empty history with the default max depth (50).
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth: Self::DEFAULT_MAX_DEPTH,
        }
    }

    /// Create a new empty history with a custom max depth.
    pub fn with_max_depth(max_depth: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth: max_depth.max(1),
        }
    }

    /// Record a layout change. Pushes the before/after state onto the undo
    /// stack and clears the redo stack (standard undo semantics).
    pub fn push(&mut self, before: &DockLayout, after: &DockLayout) {
        let diff = DockLayout::diff(before, after);
        if diff.is_empty() {
            return;
        }

        self.redo_stack.clear();
        self.undo_stack.push(diff);

        // Trim oldest entries if we exceed max depth
        if self.undo_stack.len() > self.max_depth {
            let excess = self.undo_stack.len() - self.max_depth;
            self.undo_stack.drain(..excess);
        }
    }

    /// Undo the most recent change. Restores `current` to the before-state
    /// and moves the entry to the redo stack. Returns `true` if an undo
    /// was performed, `false` if the undo stack is empty.
    pub fn undo(&mut self, current: &mut DockLayout) -> bool {
        let Some(diff) = self.undo_stack.pop() else {
            return false;
        };

        current.apply(&diff.invert());
        self.redo_stack.push(diff);
        true
    }

    /// Redo the most recently undone change. Restores `current` to the
    /// after-state and moves the entry back to the undo stack. Returns
    /// `true` if a redo was performed, `false` if the redo stack is empty.
    pub fn redo(&mut self, current: &mut DockLayout) -> bool {
        let Some(diff) = self.redo_stack.pop() else {
            return false;
        };

        current.apply(&diff);
        self.undo_stack.push(diff);
        true
    }

    /// Whether there are any changes to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Whether there are any undone changes to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Number of entries on the undo stack.
    pub fn undo_depth(&self) -> usize {
        self.undo_stack.len()
    }

    /// Number of entries on the redo stack.
    pub fn redo_depth(&self) -> usize {
        self.redo_stack.len()
    }

    /// Clear all history (both undo and redo).
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

impl Default for LayoutHistory {
    fn default() -> Self {
        Self::new()
    }
}

// endregion: --- LayoutHistory

// region: --- Tests

// endregion: --- Tests
