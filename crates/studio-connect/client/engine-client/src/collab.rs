//! The live session as a window shows it: its status (`Status`), read off
//! the engine's link (`crate::view_client::live`) or this process's own
//! (`studio_engine::collab`, which runs it).

/// What the collaboration is doing, for the toolbar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub hosting: bool,
    /// What to hand someone so they can join.
    pub ticket: String,
    /// How many others are here.
    pub peers: usize,
    pub shared_transport: bool,
    /// The set being shared (`Worship Set`, or a lone song's name).
    pub set: String,
    /// This person, as the others see them: name and colour.
    pub name: String,
    pub color: u32,
    /// When a playground set (the public demo) starts over, on this
    /// machine's clock; `None` for a set that keeps what is done in it.
    pub resets_at: Option<architect::platform::Instant>,
    /// This device is locked ([`sync_locked`]): nothing goes out or comes
    /// in until it is unlocked.
    pub locked: bool,
    /// Songs with something held by the lock, to merge when it lifts.
    pub held: usize,
}

/// Whether an environment switch is on: set, and not to nothing.
#[must_use]
pub fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}
