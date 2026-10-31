//! A named way back before the drums are rewritten across the song.
//!
//! The way back is a mark in the song's history (`crate::history`): the
//! doc's frontier and a name, a few bytes in its project file, no branch.
//! Loro keeps every version the song ever was; the mark names the one
//! before the rewrite, and `SESSION_HISTORY_RESTORE` brings it back as a
//! step of its own. (Until 2026-10-31 this forked a branch — what
//! `SESSION_CHECKPOINT` makes on purpose — and those older automatic forks
//! are what `SESSION_CHECKPOINTS_PRUNE` cleans up.)
//!
//! An action that rewrites the drums (`DRUMS_REQUANTIZE`, `QUANTIZE`,
//! `HITS_DETECT`, `COMP_MAIN`, … — the table in
//! `docs/design/song-history.md`) first marks the song as it is —
//! `Before DRUMS_REQUANTIZE 2026-10-31 14:05` — and says the mark in its
//! result's `checkpoint`. `SESSION_HISTORY` lists it, and
//! `SESSION_HISTORY_RESTORE` brings the song back to it, after a reopen
//! too. Undo is in memory and 200 steps deep; the mark outlives both.
//!
//! A mark is one row: a name and the frontier, a few bytes. A compaction
//! keeps every operation, so the version it names is still there after.
//! When one cannot be made — the song not open from its project file, the
//! file refusing the write — the action is refused, never run unprotected.
//!
//! One checkpoint per action as the user ran it: an action run by another
//! (`DRUMS_REQUANTIZE` runs `QUANTIZE`) runs under its caller's.
//!
//! Every rewrite is protected, whatever its size — except what a key or a
//! drag runs over a selection (Q, Alt+Q, Backspace, T, Ctrl+T over a
//! selection, a hit dragged): a mark first records what the engine holds
//! that no step has yet — a read of the whole song, a step's cost — and a
//! keypress must never wait on that. Undo covers those ([`on_a_key`]).
//!
//! The automatic forks made before marks pile up in older songs:
//! `SESSION_CHECKPOINTS_PRUNE` lets go of them (`Before <ACTION> <date
//! time>` branches), never a version a user named.

use std::cell::RefCell;

/// A stretch at least this long (seconds, summed over its spans) is worth
/// a checkpoint: any at all — every rewrite is protected, whatever its
/// size (what a key runs over a selection excepted: [`on_a_key`]).
pub const BIG_STRETCH: f64 = 0.0;

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

    /// The whole song, unless `dry` (a dry run): nothing.
    #[must_use]
    pub const fn song_unless(dry: bool) -> Self {
        if dry { Self::Nothing } else { Self::Song }
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
    /// What the action running on this thread runs under: the checkpoint
    /// it forked (`Some(Some(name))`), or none because a key ran it
    /// (`Some(None)`) — what an action it runs reports instead of forking
    /// another.
    static UNDER: RefCell<Option<Option<String>>> = const { RefCell::new(None) };
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

/// Run what follows as a key or a drag over a selection runs it: no
/// checkpoint for it, nor for any action it runs (`EDIT_QUANTIZE` runs
/// `QUANTIZE`) — a keypress never waits on a save. Hold the [`Net`] while
/// the action runs.
pub fn on_a_key() -> Net {
    if let Some(name) = UNDER.with(|under| under.borrow().clone()) {
        return Net { name, own: false };
    }
    UNDER.with(|under| *under.borrow_mut() = Some(None));
    Net {
        name: None,
        own: true,
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
        let (_, project) = crate::actions::song()?;
        let daw::service::ProjectContext::Project(guid) = project else {
            return Err("no song open".to_owned());
        };
        mark_before(&guid, action, &stamp())
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
        return Ok(Net { name, own: false });
    }
    if !reach.is_big() {
        return Ok(Net {
            name: None,
            own: false,
        });
    }
    let name = fork()?;
    UNDER.with(|under| *under.borrow_mut() = Some(Some(name.clone())));
    Ok(Net {
        name: Some(name),
        own: true,
    })
}

/// Mark `Before <action> <stamp>` on the song open as `guid`, as it is
/// now — what the engine holds unrecorded recorded first — in its history.
/// Says the name it got.
///
/// No save comes first and none is needed: every step is appended to the
/// song's file as it is recorded, and the mark records any the engine
/// holds that no step has yet.
///
/// # Errors
///
/// The song is not open from its project file, or the file refused.
pub fn mark_before(guid: &str, action: &str, stamp: &str) -> Result<String, String> {
    let began = web_time::Instant::now();
    let name = crate::project_file::mark(guid, action, |taken| name_for(action, stamp, taken))?;
    architect_telemetry::wide::set("checkpoint.name", name.clone());
    architect_telemetry::wide::set("checkpoint.ms", began.elapsed().as_secs_f64() * 1_000.0);
    Ok(name)
}

/// `Before <action> <stamp>`, or with ` 2`, ` 3`, … after it when a mark
/// is already called that: two rewrites in one minute leave two ways back,
/// each gone back to by its own name.
#[must_use]
pub fn name_for(action: &str, stamp: &str, taken: &[String]) -> String {
    let base = format!("Before {action} {stamp}");
    std::iter::once(base.clone())
        .chain((2_u32..).map(|n| format!("{base} {n}")))
        .find(|n| !taken.iter().any(|t| t == n))
        .unwrap_or(base)
}

/// Whether `name` is a checkpoint made before a rewrite
/// (`Before <ACTION_ID> YYYY-MM-DD HH:MM`, maybe with ` 2`, ` 3`, … after
/// it) — never one a user named, nor a live set's `Before joining …`.
#[must_use]
pub fn is_automatic(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("Before ") else {
        return false;
    };
    let Some((action, rest)) = rest.split_once(' ') else {
        return false;
    };
    let id = !action.is_empty()
        && action
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && action.starts_with(|c: char| c.is_ascii_uppercase());
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    let mut words = rest.split(' ');
    let date = words.next().is_some_and(|d| {
        let parts: Vec<&str> = d.split('-').collect();
        parts.len() == 3 && parts.iter().all(|p| digits(p))
    });
    let time = words.next().is_some_and(|t| {
        t.split_once(':')
            .is_some_and(|(h, m)| digits(h) && digits(m))
    });
    let tail = words.next().is_none_or(digits) && words.next().is_none();
    id && date && time && tail
}

/// A branch of the song's file, as pruning weighs it.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub name: String,
    /// When it was made, seconds since the epoch.
    pub at: i64,
    /// Another branch was forked from it (and loads through it).
    pub parent: bool,
    /// It is the version open now.
    pub open: bool,
}

/// What pruning lets go of, and what it leaves and why.
#[derive(Clone, Debug, Default, PartialEq, facet::Facet)]
pub struct Pruned {
    /// The checkpoints let go of (or that would be, for a dry run).
    pub removed: Vec<String>,
    /// Automatic checkpoints kept.
    pub kept: u32,
    /// Ones it would have let go of but did not: `name: why`.
    pub left: Vec<String>,
    pub applied: bool,
}

/// Which of `branches` to let go of: the automatic checkpoints
/// ([`is_automatic`]) past the newest `keep`, or made more than
/// `older_than_days` before `now` (seconds since the epoch) — never one
/// another branch was forked from, nor the one open.
#[must_use]
pub fn prune_plan(
    branches: &[Candidate],
    (keep, older_than_days): (Option<u32>, Option<f64>),
    now: i64,
) -> Pruned {
    let mut automatic: Vec<&Candidate> =
        branches.iter().filter(|b| is_automatic(&b.name)).collect();
    automatic.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.name.cmp(&a.name)));
    let mut out = Pruned::default();
    for (n, b) in automatic.into_iter().enumerate() {
        let past_keep = keep.is_some_and(|k| n >= k as usize);
        #[allow(clippy::cast_precision_loss)]
        let too_old = older_than_days.is_some_and(|days| (now - b.at) as f64 > days * 86_400.0);
        if !(past_keep || too_old) {
            out.kept = out.kept.saturating_add(1);
        } else if b.open {
            out.left.push(format!("{}: it is the version open", b.name));
            out.kept = out.kept.saturating_add(1);
        } else if b.parent {
            out.left.push(format!(
                "{}: another version was forked from it and opens through it",
                b.name
            ));
            out.kept = out.kept.saturating_add(1);
        } else {
            out.removed.push(b.name.clone());
        }
    }
    out
}

static PRUNE: architect::action::ActionMeta = crate::actions::meta(
    "SESSION_CHECKPOINTS_PRUNE",
    "session_checkpoints_prune",
    "Prune the automatic checkpoints",
    "Let go of the open song's older automatic checkpoints — the `Before <ACTION> <date time>` versions a drum rewrite forks first — keeping the newest `keep` and/or none made more than `older_than_days` ago (at least one of the two). A checkpoint a user named (SESSION_CHECKPOINT), a live set's `Before joining …`, the version open and one another version was forked from are never let go. `dry_run` says what it would let go. Not undoable: what a checkpoint held, past what the song still has, is gone.",
    "File",
    "",
);

/// How many to keep, how old to let go, whether only to say.
#[derive(facet::Facet)]
struct PruneArg {
    keep: Option<u32>,
    older_than_days: Option<f64>,
    #[facet(default)]
    dry_run: bool,
}

/// `SESSION_CHECKPOINTS_PRUNE`.
///
/// # Errors
/// When the action is already registered.
pub fn register(
    list: &architect::action::ActionList,
) -> Result<(), architect::action::ActionError> {
    list.register(
        &PRUNE,
        architect::action::Effect::Destructive,
        |a: PruneArg| {
            if a.keep.is_none() && a.older_than_days.is_none() {
                return Err("say `keep` (how many to keep) or `older_than_days`".to_owned());
            }
            let (_, project) = crate::actions::song()?;
            let daw::service::ProjectContext::Project(guid) = project else {
                return Err("no song open".to_owned());
            };
            let path = crate::project_file::path_of(&guid)
                .ok_or("the song is not open from its project file (a .session)")?;
            let (file, open_branch) = crate::project_file::save_target(&path);
            let branches = crate::project_file::branches(&file)?;
            let candidates: Vec<Candidate> = branches
                .iter()
                .map(|b| Candidate {
                    name: b.name.clone(),
                    at: b.at,
                    parent: branches
                        .iter()
                        .any(|o| o.parent.as_deref() == Some(b.name.as_str())),
                    open: b.name == open_branch,
                })
                .collect();
            let now = web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
            let mut plan = prune_plan(&candidates, (a.keep, a.older_than_days), now);
            if !a.dry_run && !plan.removed.is_empty() {
                crate::project_file::delete_branches(&guid, &plan.removed)?;
                plan.applied = true;
            }
            architect_telemetry::wide::set(
                "checkpoint.pruned",
                i64::try_from(plan.removed.len()).unwrap_or(i64::MAX),
            );
            Ok(plan)
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
