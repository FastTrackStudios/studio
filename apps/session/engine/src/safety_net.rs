//! A named way back before the drums are rewritten across the song.
//!
//! An action that rewrites the kit over the whole song or a big stretch of
//! it (`DRUMS_REQUANTIZE`, `QUANTIZE`, `DRUMS_REVERT`, `DRUMS_EDIT_LANES`,
//! `DRUMS_TRIGGERS_FROM_MICS`, `COMP_BEST`, `EDIT_REVERT`) first forks a
//! checkpoint of the song as it is — `Before DRUMS_REQUANTIZE 2026-10-31
//! 14:05` — the way `SESSION_CHECKPOINT` does: a branch of the song's
//! project file, listed with its alternatives (`SESSION_VERSIONS`) and
//! opened as one. Undo is in memory and 200 steps deep; the checkpoint
//! outlives both.
//!
//! A branch is one row (its parent and the frontier it forked at: a few
//! bytes) and shares the song's history up to there, so it is cheap; and a
//! compaction keeps every operation, so it still finds its fork point
//! after (`session-format`'s `a_checkpoint_shares_the_song_and_outlives_its_compaction`).
//! When one cannot be made — the song not open from its project file, the
//! file refusing the write — the action is refused, never run unprotected.
//!
//! One checkpoint per action as the user ran it: an action run by another
//! (`DRUMS_REQUANTIZE` runs `QUANTIZE`) runs under its caller's.

use std::cell::RefCell;

/// A stretch at least this long (seconds, summed over its spans) is a big
/// one: a section of a song and more. A few bars quantized or put back are
/// what undo is for.
pub const BIG_STRETCH: f64 = 30.0;

/// How far an action reaches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reach {
    /// Every track of the kit, the whole song.
    Song,
    /// Spans of the song adding up to this many seconds.
    Seconds(f64),
    /// Nothing changes (a dry run).
    Nothing,
}

impl Reach {
    /// The reach of `spans` (`from`, `to` in seconds): their lengths
    /// summed.
    #[must_use]
    pub fn of_spans(spans: impl IntoIterator<Item = (f64, f64)>) -> Self {
        Self::Seconds(spans.into_iter().map(|(from, to)| (to - from).abs()).sum())
    }

    /// Whether it is far enough to fork a checkpoint before.
    #[must_use]
    pub fn is_big(self) -> bool {
        match self {
            Self::Song => true,
            Self::Seconds(s) => s >= BIG_STRETCH,
            Self::Nothing => false,
        }
    }
}

thread_local! {
    /// The checkpoint the action running on this thread forked: what an
    /// action it runs reports instead of forking another.
    static UNDER: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The checkpoint an action runs under, held for as long as it runs.
#[derive(Debug)]
#[must_use = "hold it while the action runs: dropped, the next action forks its own"]
pub struct Net {
    name: Option<String>,
    /// Whether this one forked it (and so lets go of it when dropped).
    own: bool,
}

impl Net {
    /// The checkpoint's name; `None` when the action reaches too little
    /// to need one.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        self.name.clone()
    }
}

impl Drop for Net {
    fn drop(&mut self) {
        if self.own {
            UNDER.with(|under| under.borrow_mut().take());
        }
    }
}

/// Fork a checkpoint of the open song before `action` rewrites the drums
/// over `reach` — or, when it reaches too little, nothing; under an action
/// that already forked one, that one. Hold the [`Net`] while the action
/// runs and say its [`Net::name`] in the action's result.
///
/// # Errors
///
/// When the checkpoint cannot be made, saying why: the action must then
/// not run.
pub fn before(action: &str, reach: Reach) -> Result<Net, String> {
    guarded(reach, || {
        let (daw, project) = crate::actions::song()?;
        let daw::service::ProjectContext::Project(guid) = project else {
            return Err("no song open".to_owned());
        };
        fork_before(daw, &guid, action, &stamp())
    })
    .map_err(|why| format!("{action} was not run: no checkpoint could be made before it — {why}"))
}

/// Now, local time, to the minute: `2026-10-31 14:05`.
fn stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

/// The checkpoint for `reach`, `fork` making it when it is big and no
/// action on this thread has one already.
fn guarded(reach: Reach, fork: impl FnOnce() -> Result<String, String>) -> Result<Net, String> {
    if let Some(name) = UNDER.with(|under| under.borrow().clone()) {
        return Ok(Net {
            name: Some(name),
            own: false,
        });
    }
    if !reach.is_big() {
        return Ok(Net {
            name: None,
            own: false,
        });
    }
    let name = fork()?;
    UNDER.with(|under| *under.borrow_mut() = Some(name.clone()));
    Ok(Net {
        name: Some(name),
        own: true,
    })
}

/// Fork `Before <action> <stamp>` from the song open as `guid`, as it is
/// now, the song saved on the way. Says the name it got.
///
/// # Errors
///
/// The song is not open from its project file, or the file refused.
pub fn fork_before(
    daw: &daw::standalone::Standalone,
    guid: &str,
    action: &str,
    stamp: &str,
) -> Result<String, String> {
    let began = web_time::Instant::now();
    let path = crate::project_file::path_of(guid).ok_or(
        "the song is not open from its project file (a .session): open or save it as one first",
    )?;
    let (file, _) = crate::project_file::save_target(&path);
    let taken: Vec<String> = crate::project_file::branches(&file)
        .unwrap_or_default()
        .into_iter()
        .map(|b| b.name)
        .collect();
    let name = name_for(action, stamp, &taken);
    crate::project_file::fork(daw, guid, &path, &name)?;
    // The song was saved on the way: nothing unsaved now.
    crate::projects::mark_saved(daw, guid);
    architect_telemetry::wide::set("checkpoint.name", name.as_str());
    architect_telemetry::wide::set("checkpoint.ms", began.elapsed().as_secs_f64() * 1_000.0);
    Ok(name)
}

/// `Before <action> <stamp>`, or with ` 2`, ` 3`, … after it when a branch
/// is already called that: a fork over a branch of the same name replaces
/// it, and two rewrites in one minute must leave two ways back.
#[must_use]
pub fn name_for(action: &str, stamp: &str, taken: &[String]) -> String {
    let base = format!("Before {action} {stamp}");
    std::iter::once(base.clone())
        .chain((2_u32..).map(|n| format!("{base} {n}")))
        .find(|n| !taken.iter().any(|t| t == n))
        .unwrap_or(base)
}

#[cfg(test)]
mod tests;
