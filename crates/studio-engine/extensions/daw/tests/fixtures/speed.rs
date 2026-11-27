//! Session open and room join, timed and held to `speed.toml` beside this
//! file (studio#717): each time is a check, so a time over its budget
//! fails the test, and a row of the record's `times`, which CI reports.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use crate::budgets::{Budgets, Verdict};
use crate::check::Checks;

/// `section`'s budgets (`open`, `join`) from `speed.toml`.
///
/// # Panics
///
/// The file is missing or has no such section: a test that times nothing
/// would pass without saying so.
#[must_use]
pub fn budgets(section: &str) -> Budgets {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/speed.toml");
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    Budgets::parse(&text, section).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

/// The times of one run, held to one section's budgets.
#[derive(Debug)]
pub struct Times {
    section: &'static str,
    budgets: Budgets,
    rows: Vec<Value>,
}

impl Times {
    #[must_use]
    pub fn new(section: &'static str) -> Self {
        Self {
            section,
            budgets: budgets(section),
            rows: Vec::new(),
        }
    }

    /// `took` for `what`, judged by budget `step`: a check named `check`
    /// (failed when over, or when `step` has no budget), and a row of
    /// [`Times::json`].
    pub fn judge(
        &mut self,
        checks: &mut Checks,
        check: impl Into<String>,
        step: &str,
        what: &str,
        took: Duration,
    ) {
        let ms = took.as_secs_f64() * 1000.0;
        let verdict = self.budgets.judge(step, ms);
        let (ok, said) = match &verdict {
            Verdict::Unbudgeted => (
                false,
                format!(
                    "{what}: {ms:.0} ms, and no budget for {step:?} in speed.toml [{}.budget_ms]",
                    self.section
                ),
            ),
            Verdict::Over {
                budget_ms,
                limit_ms,
            } => (
                false,
                format!(
                    "{what}: {ms:.0} ms, over its budget of {budget_ms} ms (limit {limit_ms:.0} ms)"
                ),
            ),
            Verdict::Within { budget_ms } | Verdict::Tighten { budget_ms, .. } => {
                (true, format!("{what}: {ms:.0} ms, budget {budget_ms} ms"))
            }
        };
        checks.add(check, ok, said);
        self.rows.push(json!({
            "measure": self.section,
            "step": step,
            "what": what,
            "ms": ms.round(),
            "budget_ms": self.budgets.budget_ms.get(step),
            "verdict": verdict.word(),
        }));
    }

    /// The record's `times`: one row per time judged.
    #[must_use]
    pub fn json(&self) -> Value {
        Value::Array(self.rows.clone())
    }
}
