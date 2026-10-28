//! A window attaching to the engine: its actions join the engine's Action
//! List, enabled, while it is attached, and stay listed but refuse to run
//! once it has gone. No window is opened — the attach is what a window does
//! when its arrangement comes up (`session_ui::window::attach`).

use serde_json::json;

fn listed(id: &str) -> Option<bool> {
    session_engine::actions::list()
        .infos()
        .into_iter()
        .find(|i| i.id == id)
        .map(|i| i.enabled)
}

#[test]
fn a_window_brings_its_actions_and_they_fall_disabled_when_it_goes() {
    assert_eq!(
        listed("SESSION_ZOOM"),
        None,
        "a window's action, with no window"
    );
    assert_eq!(listed("SESSION_SONGS"), Some(true), "the engine's own");
    // A window attaches: its actions join the list, enabled; it goes, and
    // they stay listed but cannot run.
    let window = session_ui::window::attach();
    assert_eq!(listed("SESSION_ZOOM"), Some(true));
    assert!(
        session_ui::window::action_ids()
            .iter()
            .any(|id| id == "SESSION_VIEW")
    );
    drop(window);
    assert_eq!(listed("SESSION_ZOOM"), Some(false));
    let refused = session_engine::actions::list().run_json(
        "SESSION_ZOOM",
        json!({"zoom": "Project"}).to_string().as_bytes(),
    );
    assert!(refused.is_err(), "{refused:?}");
}
