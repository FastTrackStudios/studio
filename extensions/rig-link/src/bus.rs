//! What a process's rig panes share past the engine's own state.
//!
//! The routing grid's selection (the FX row shows it, Edit edits it), the
//! browser's focus (a selection or a part's "Patch…" asks the browser to
//! open on a kind) and the part being built. Signal's one-screen layout
//! keeps them in one component's context; panes are apart, so they are
//! here, and each pane mirrors them into its own context
//! (`RigPane`). Actions set them too (`STUDIO_RIG_GRID_SELECT`,
//! `STUDIO_RIG_BROWSER_FOCUS`).
//!
//! Process-wide: a development copy, a process of its own, has its own.

use std::sync::{LazyLock, Mutex, PoisonError};

use guitar::ui::panels::{Focus, Selected};
use tokio::sync::watch;

/// What the panes share.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shared {
    /// The routing grid's selection.
    pub selection: Option<Selected>,
    /// Where the browser was last asked to open.
    pub focus: Option<Focus>,
    /// The part picked to build into.
    pub part: Option<usize>,
}

static STATE: Mutex<Option<Shared>> = Mutex::new(None);
static CHANGED: LazyLock<watch::Sender<u64>> = LazyLock::new(|| watch::channel(0).0);

/// What the panes share now.
#[must_use]
pub fn get() -> Shared {
    STATE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .unwrap_or_default()
}

/// Changes what the panes share; they hear of it only when it differs.
pub fn put(change: impl FnOnce(&mut Shared)) {
    let moved = {
        let mut state = STATE.lock().unwrap_or_else(PoisonError::into_inner);
        let was = state.clone().unwrap_or_default();
        let mut now = was.clone();
        change(&mut now);
        let moved = now != was;
        *state = Some(now);
        moved
    };
    if moved {
        CHANGED.send_modify(|revision| *revision = revision.wrapping_add(1));
    }
}

/// Wakes when what the panes share changes.
#[must_use]
pub fn subscribe() -> watch::Receiver<u64> {
    CHANGED.subscribe()
}

#[cfg(test)]
mod tests {
    use super::{Selected, get, put, subscribe};

    #[test]
    fn a_change_wakes_panes_and_the_same_value_does_not() {
        let mut changes = subscribe();
        changes.mark_unchanged();
        put(|b| b.selection = Some(Selected::Block("amp".to_owned())));
        assert!(changes.has_changed().unwrap_or(false));
        assert_eq!(get().selection, Some(Selected::Block("amp".to_owned())));
        changes.mark_unchanged();
        put(|b| b.selection = Some(Selected::Block("amp".to_owned())));
        assert!(!changes.has_changed().unwrap_or(true));
    }
}
