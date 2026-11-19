//! Where the time went: `PERF_RECENT`, the latest timed units of work —
//! every action run, the window's part of an edit — read back from the
//! recorder the app installs (`architect_telemetry::timing::Recorder`).
//!
//! An action marks its stages (`edit.read_ms`, `edit.slip_ms`, …) on its
//! own span; the window marks its frame's share (`window.settle_ms`,
//! `window.replan_ms`). A model asks the running app what the last edit
//! cost, stage by stage, on the song as it is.

use architect::action::{ActionList, ActionMeta, Effect};
use facet::Facet;

static RECENT: ActionMeta = crate::actions::meta(
    "PERF_RECENT",
    "perf_recent",
    "Where the time went",
    "The latest timed units of work, newest first: each action run (`action`, with `action.id` and `action.ok`) and the window's part of an edit (`window.slip`), with its fields, its stages in ms (`edit.read_ms`, `edit.slip_ms`, `edit.land_ms`, `window.settle_ms`, `window.replan_ms`, …) and its total. `limit` (20), `name` (a span name: `action`, `window.slip`), `action` (an action id: QUANTIZE). Read-only.",
    "Data",
    "",
);

/// Which to say.
#[derive(Facet)]
struct RecentArg {
    limit: Option<u32>,
    name: Option<String>,
    action: Option<String>,
}

/// One field of a unit of work.
#[derive(Facet)]
struct Field {
    key: String,
    value: String,
}

/// One stage of it, ms.
#[derive(Facet)]
struct Stage {
    key: String,
    ms: f64,
}

/// One unit of work.
#[derive(Facet)]
struct Timed {
    name: String,
    fields: Vec<Field>,
    stages: Vec<Stage>,
    total_ms: f64,
    /// How long ago it finished, seconds.
    ago_s: f64,
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&RECENT, Effect::ReadOnly, |a: RecentArg| {
        let limit = usize::try_from(a.limit.unwrap_or(20)).unwrap_or(20);
        let now = web_time::SystemTime::now();
        Ok(architect_telemetry::timing::recent(usize::MAX)
            .into_iter()
            .filter(|t| a.name.as_deref().is_none_or(|n| t.name == n))
            .filter(|t| {
                a.action.as_deref().is_none_or(|id| {
                    t.field("action.id")
                        .is_some_and(|f| f.eq_ignore_ascii_case(id))
                })
            })
            .take(limit)
            .map(|t| Timed {
                name: t.name.to_owned(),
                ago_s: now.duration_since(t.at).map_or(0.0, |d| d.as_secs_f64()),
                fields: t
                    .fields
                    .into_iter()
                    .map(|(key, value)| Field {
                        key: key.to_owned(),
                        value,
                    })
                    .collect(),
                stages: t
                    .stages
                    .into_iter()
                    .map(|(key, ms)| Stage {
                        key: key.to_owned(),
                        ms,
                    })
                    .collect(),
                total_ms: t.total_ms,
            })
            .collect::<Vec<Timed>>())
    })?;
    Ok(())
}
