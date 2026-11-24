//! Who this person is in a set — what a window shows; the set's actions
//! are the engine's (`session_engine::studio_engine::set_actions`).

/// Who this person is in a set, when they are not named: the account's
/// name, else the machine user's.
#[must_use]
pub fn person_name() -> String {
    std::env::var("FTS_COLLAB_NAME")
        .ok()
        .or_else(|| std::env::var("USER").ok())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "Someone".to_owned())
}
