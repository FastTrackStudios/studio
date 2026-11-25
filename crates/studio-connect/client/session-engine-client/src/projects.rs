//! A song's versions, as a window lists them. Saving, checkpoints and
//! reading them off disk are the engine's (`session_engine::projects`).

/// One of the song's versions: the song's own, or a checkpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub name: String,
    /// When it was saved ("5 min ago"); empty for the song's own.
    pub when: String,
    pub path: std::path::PathBuf,
    /// Whether it is the one open now.
    pub open: bool,
}
