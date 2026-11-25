//! What a process's keys panes share: the engine or layer zoomed into and
//! the node the browser is pointed at.
//!
//! In Signal's one keys screen both are signals in `KeysRigRemote`'s
//! context; panes are apart, so they are here, and each pane mirrors them
//! into its own context. `STUDIO_KEYS_OPEN` sets the zoom too.
//!
//! Process-wide: a development copy, a process of its own, has its own.

use std::sync::{LazyLock, Mutex, PoisonError};

use keys_ui::{Selection, Zoom};
use serde_json::{Value as Json, json};
use tokio::sync::watch;

/// What the keys panes share.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shared {
    pub zoom: Zoom,
    pub selection: Selection,
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

/// The zoom as an action answers it.
#[must_use]
pub fn zoom_json(zoom: &Zoom) -> Json {
    match zoom {
        Zoom::Mixer => json!({ "view": "mixer" }),
        Zoom::Engine(engine) => json!({ "view": "engine", "engine": engine }),
        Zoom::Layer(layer) => json!({ "view": "layer", "layer": layer }),
    }
}

/// The selection as an action answers it.
#[must_use]
pub fn selection_json(selection: &Selection) -> Json {
    match selection {
        Selection::None => Json::Null,
        other => Json::String(format!("{other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{Zoom, get, put, subscribe, zoom_json};
    use serde_json::json;

    #[test]
    fn opening_a_layer_wakes_the_panes_once() {
        let mut changes = subscribe();
        changes.mark_unchanged();
        put(|s| s.zoom = Zoom::Layer("Felt Piano".to_owned()));
        assert!(changes.has_changed().unwrap_or(false));
        assert_eq!(
            zoom_json(&get().zoom),
            json!({ "view": "layer", "layer": "Felt Piano" })
        );
        changes.mark_unchanged();
        put(|s| s.zoom = Zoom::Layer("Felt Piano".to_owned()));
        assert!(!changes.has_changed().unwrap_or(true));
    }
}
