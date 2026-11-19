//! The song's history: every step it was edited by and every mark set on
//! it, kept in its project file, listed and gone back to.
//!
//! The song is a Loro doc, and a Loro snapshot keeps every operation: any
//! version it ever was is still in it, by its frontier. What the doc does
//! not keep is a way to name them: undo is in memory and 200 steps deep,
//! and the file's labelled updates are folded away by a compaction. So the
//! project file keeps a `history` table (`session_format::store::Moment`):
//! each step's name, the action that made it, when, and the doc's frontier
//! after it — and the marks set before a rewrite (`safety_net`), a name on
//! the version as it was, no branch. `SESSION_HISTORY` lists them;
//! `SESSION_HISTORY_RESTORE` brings the song back to one as a new step
//! (the doc forked at its frontier, read, and written over the song as it
//! is), undone like any other — the history itself is never rewritten.
//!
//! Where the doc's history is not enough (edits it never saw, a live set
//! joined, files written beside it): `docs/design/song-history.md`.

use std::cell::RefCell;

use architect::action::{ActionList, ActionMeta, Effect};
use facet::Facet;
use session_format::store::{MomentKind, MomentRef};

thread_local! {
    /// The actions running on this thread, the innermost last: what a step
    /// finished here was made by.
    static RUNNING: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The action running on this thread, the innermost, if any: read where a
/// step finishes, so the history says what made it.
#[must_use]
pub fn running_action() -> Option<String> {
    RUNNING.with(|r| r.borrow().last().cloned())
}

/// An action's id, kept on its span.
struct ActionId(String);

/// Reads the Action List's span (`action`, its `action.id`) as actions
/// run, so [`running_action`] can say which one a step was made by. Added
/// to the app's tracing subscriber.
pub struct ActionLayer;

/// Finds `action.id` among a span's fields.
#[derive(Default)]
struct FindId(Option<String>);

impl tracing::field::Visit for FindId {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "action.id" {
            self.0 = Some(value.to_owned());
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "action.id" && self.0.is_none() {
            self.0 = Some(format!("{value:?}").trim_matches('"').to_owned());
        }
    }
}

impl<S> tracing_subscriber::Layer<S> for ActionLayer
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if attrs.metadata().name() != "action" {
            return;
        }
        let mut found = FindId::default();
        attrs.record(&mut found);
        if let (Some(action), Some(span)) = (found.0, ctx.span(id)) {
            span.extensions_mut().insert(ActionId(action));
        }
    }

    fn on_enter(&self, id: &tracing::span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        if let Some(span) = ctx.span(id)
            && let Some(ActionId(action)) = span.extensions().get::<ActionId>()
        {
            RUNNING.with(|r| r.borrow_mut().push(action.clone()));
        }
    }

    fn on_exit(&self, id: &tracing::span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        if let Some(span) = ctx.span(id)
            && let Some(ActionId(action)) = span.extensions().get::<ActionId>()
        {
            RUNNING.with(|r| {
                let mut running = r.borrow_mut();
                if running.last() == Some(action) {
                    running.pop();
                }
            });
        }
    }
}

static HISTORY: ActionMeta = crate::actions::meta(
    "SESSION_HISTORY",
    "session_history",
    "The song's history",
    "Every step the open song was edited by and every mark set on it (`Before DRUMS_REQUANTIZE <date time>`, set before a rewrite), the newest first, kept in its project file through every save and compaction: each its `step` (the number SESSION_HISTORY_RESTORE takes), `kind` (step or mark), `label`, the `action` that made it (when one did), `when` (local time) and whether it can still be gone back to (`restorable`: false for a version from before the song joined a live set). Paged by `offset`/`limit` (50). Steps recorded before the history was kept are not listed. Read-only.",
    "File",
    "",
);

/// Which page.
#[derive(Facet, Default)]
struct HistoryArg {
    offset: Option<u32>,
    limit: Option<u32>,
}

/// The song's history, a page of it.
#[derive(Facet)]
struct HistoryOut {
    moments: Vec<MomentOut>,
    total: u32,
}

/// One step or mark.
#[derive(Facet, Debug, PartialEq)]
struct MomentOut {
    step: i64,
    kind: String,
    label: String,
    action: Option<String>,
    when: String,
    restorable: bool,
}

static RESTORE: ActionMeta = crate::actions::meta(
    "SESSION_HISTORY_RESTORE",
    "session_history_restore",
    "Go back in the song's history",
    "Bring the open song back to how it was after a step of its history (`step`, as SESSION_HISTORY numbers them) or at a mark (`mark`, its name — what a guarded rewrite says in its `checkpoint`), as one new step: the song as it was then written over the song as it is, so SESSION_UNDO takes it back, and nothing of the history is rewritten — every step after it is still there to go back to. Works after the song is reopened. Files the song plays are not part of its history: one an edit wrote is never overwritten, so an older version's still plays. Says what it went back to.",
    "File",
    "",
);

/// Which step or mark.
#[derive(Facet, Default)]
struct RestoreArg {
    step: Option<i64>,
    mark: Option<String>,
}

/// `seconds` since the epoch, local time: `2026-10-31 14:05:09`.
fn local(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0).map_or_else(String::new, |at| {
        at.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    })
}

/// The open song's engine guid, open from its project file.
fn open_song() -> Result<String, String> {
    let (_, project) = crate::actions::song()?;
    let daw::service::ProjectContext::Project(guid) = project else {
        return Err("no song open".to_owned());
    };
    Ok(guid)
}

/// `SESSION_HISTORY` and `SESSION_HISTORY_RESTORE`.
///
/// # Errors
/// When either is already registered.
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&HISTORY, Effect::ReadOnly, |a: HistoryArg| {
        let guid = open_song()?;
        let (moments, total) = crate::project_file::history(
            &guid,
            (a.offset.unwrap_or(0), a.limit.unwrap_or(50).min(500)),
        )?;
        Ok::<_, String>(HistoryOut {
            moments: moments
                .into_iter()
                .map(|(m, restorable)| MomentOut {
                    step: m.seq,
                    kind: match m.kind {
                        MomentKind::Step => "step",
                        MomentKind::Mark => "mark",
                    }
                    .to_owned(),
                    label: m.label,
                    action: m.action,
                    when: local(m.at),
                    restorable,
                })
                .collect(),
            total,
        })
    })?;
    list.register(&RESTORE, Effect::Mutating, |a: RestoreArg| {
        let which = match (a.step, a.mark) {
            (Some(seq), None) => MomentRef::Seq(seq),
            (None, Some(name)) => MomentRef::Mark(name),
            _ => return Err("say one of `step` or `mark`".to_owned()),
        };
        let guid = open_song()?;
        let moment = crate::project_file::restore(&guid, which)?;
        let (daw, project) = crate::actions::song()?;
        crate::actions::samples_follow(daw, &project, crate::hits_midi::Mode::Follow);
        crate::changes::announce(crate::changes::Change::Marks);
        crate::song_model::request_resync();
        crate::actions::list().changed("items", "history");
        Ok(format!(
            "went back to {} ({}), as a step of its own: SESSION_UNDO takes it back",
            moment.label,
            local(moment.at)
        ))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests;
