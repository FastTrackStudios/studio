//! A song open in this process's engine: which engine, which project.
//!
//! Code that works on a song takes a `&Song` and nothing ambient: it can be
//! handed any song open in the engine — the one on screen, one a set
//! streams in behind it — and it reads and writes that one. Only the edge
//! (an action's handler, the window) asks which song is current:
//!
//! ```ignore
//! list.register(&PLACE, Effect::Mutating, |a: PlaceArg| {
//!     let song = Song::current()?;          // the one ambient read
//!     crate::hit_place::place(&song, &moves, joins)
//! });
//!
//! song.undo_step("Place a drum hit", |song| song.apply(&ops))?;
//! let lanes = song.read(|p| read_lanes(p, &mics));
//! ```

use daw::service::ProjectContext;
use daw::standalone::Standalone;
use daw::standalone::item_ops::ItemOp;
use daw::standalone::sync::ProjectState;
use daw_proto::id::ProjectId;

/// A song open in this process's engine.
#[derive(Clone)]
pub struct Song {
    daw: &'static Standalone,
    id: ProjectId,
}

/// Why there is no song to work on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoSong {
    /// This process runs no engine of its own (a page, a remote window).
    NoEngine,
    /// The engine has nothing open.
    NothingOpen,
    /// The engine has no project by that id open.
    NotOpen(ProjectId),
}

impl std::fmt::Display for NoSong {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoEngine => f.write_str("no song open in this window's engine"),
            Self::NothingOpen => f.write_str("no song open"),
            Self::NotOpen(id) => write!(f, "no song {id} open"),
        }
    }
}

impl std::error::Error for NoSong {}

impl From<NoSong> for String {
    fn from(why: NoSong) -> Self {
        why.to_string()
    }
}

impl Song {
    /// The song on screen: the one ambient read, for the edge of the app.
    ///
    /// # Errors
    /// With no engine in this process, or nothing open in it.
    pub(crate) fn current() -> Result<Self, NoSong> {
        let daw = crate::open::local_engine().ok_or(NoSong::NoEngine)?;
        let id = crate::open::current_song().ok_or(NoSong::NothingOpen)?;
        Ok(Self {
            daw,
            id: ProjectId::from(id),
        })
    }

    /// Song `id` in this process's engine, current or not.
    ///
    /// # Errors
    /// With no engine in this process, or no such song open in it.
    pub(crate) fn open(id: ProjectId) -> Result<Self, NoSong> {
        let daw = crate::open::local_engine().ok_or(NoSong::NoEngine)?;
        if daw.read_project(id.as_str(), |_| ()).is_none() {
            return Err(NoSong::NotOpen(id));
        }
        Ok(Self { daw, id })
    }

    /// The song named by a call, else the one on screen.
    ///
    /// # Errors
    /// As [`Self::open`] or [`Self::current`].
    pub fn named_or_current(id: Option<&str>) -> Result<Self, NoSong> {
        match id {
            Some(id) => Self::open(ProjectId::from(id)),
            None => Self::current(),
        }
    }

    /// Its id.
    #[must_use]
    pub fn id(&self) -> &ProjectId {
        &self.id
    }

    /// The engine it is open in.
    #[must_use]
    pub fn daw(&self) -> &'static Standalone {
        self.daw
    }

    /// It, as the engine's service calls name it.
    #[must_use]
    pub fn context(&self) -> ProjectContext {
        ProjectContext::from(&self.id)
    }

    /// Look at its state, under one lock: `None` once it has closed.
    pub fn read<R>(&self, look: impl FnOnce(&ProjectState) -> R) -> Option<R> {
        self.daw.read_project(self.id.as_str(), look)
    }

    /// Apply item edits, in order, under one lock.
    ///
    /// # Errors
    /// When an op names an item the song does not have (those before it
    /// stand), or the song has closed.
    pub fn apply(&self, ops: &[ItemOp]) -> daw_proto::DawResult<()> {
        self.daw.apply_items(&self.id, ops)
    }

    /// Run `edit` as one undo step named `label`: everything it does is
    /// undone together, and the step closes however `edit` returns.
    pub fn undo_step<R>(&self, label: &str, edit: impl FnOnce(&Self) -> R) -> R {
        use daw::service::Projects as _;
        self.daw.begin_undo_block(self.context(), label);
        let done = edit(self);
        self.daw.end_undo_block(self.context(), label, None);
        done
    }
}
