//! The budgets (`budgets.toml` beside this file): each step's time, and
//! the checks on results, for each song. A step slower than its budget
//! plus the margin fails; one well under it prints a hint to tighten the
//! budget, so budgets only ever shrink.

use std::collections::BTreeMap;

/// One song's budgets and expectations, with the file's margins.
#[derive(Clone, Debug, Default)]
pub struct Budgets {
    /// Fraction over a budget a step may run before it fails.
    pub margin: f64,
    /// …but never less than this many ms over (a 70 ms budget is noise).
    pub min_margin_ms: f64,
    /// A step under this fraction of its budget asks for a tighter one.
    pub hint_below: f64,
    /// Step name → ms.
    pub budget_ms: BTreeMap<String, f64>,
    /// Check name → value (absent: not stored yet).
    pub expect: BTreeMap<String, f64>,
}

/// How a step stood against its budget.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// No budget for it.
    Unbudgeted,
    /// Within the budget plus its margin.
    Within { budget_ms: f64 },
    /// Under `hint_below` of its budget: the hint is a budget to set.
    Tighten { budget_ms: f64, suggest_ms: f64 },
    /// Over the budget plus its margin.
    Over { budget_ms: f64, limit_ms: f64 },
}

impl Verdict {
    #[must_use]
    pub fn failed(&self) -> bool {
        matches!(self, Self::Over { .. })
    }

    #[must_use]
    pub fn word(&self) -> &'static str {
        match self {
            Self::Unbudgeted => "unbudgeted",
            Self::Within { .. } => "within",
            Self::Tighten { .. } => "tighten",
            Self::Over { .. } => "over",
        }
    }
}

impl Budgets {
    /// Song `song`'s section of the file `text`.
    ///
    /// # Errors
    ///
    /// The file is not TOML, or has no section for `song`.
    pub fn parse(text: &str, song: &str) -> Result<Self, String> {
        let file: toml::Table = text.parse().map_err(|e| format!("budgets: {e}"))?;
        let number = |v: &toml::Value| v.as_float().or_else(|| v.as_integer().map(|i| i as f64));
        let top = |key: &str, default: f64| file.get(key).and_then(number).unwrap_or(default);
        let section = file
            .get(song)
            .and_then(toml::Value::as_table)
            .ok_or_else(|| format!("budgets: no [{song}]"))?;
        let table = |key: &str| -> BTreeMap<String, f64> {
            section
                .get(key)
                .and_then(toml::Value::as_table)
                .map(|t| {
                    t.iter()
                        .filter_map(|(k, v)| number(v).map(|n| (k.clone(), n)))
                        .collect()
                })
                .unwrap_or_default()
        };
        // A song's own margins win over the file's.
        let own = |key: &str, default: f64| {
            section
                .get(key)
                .and_then(number)
                .unwrap_or_else(|| top(key, default))
        };
        Ok(Self {
            margin: own("margin", 0.25),
            min_margin_ms: own("min_margin_ms", 50.0),
            hint_below: own("hint_below", 0.8),
            budget_ms: table("budget_ms"),
            expect: table("expect"),
        })
    }

    /// How `ms` for step `step` stands.
    #[must_use]
    pub fn judge(&self, step: &str, ms: f64) -> Verdict {
        let Some(&budget_ms) = self.budget_ms.get(step) else {
            return Verdict::Unbudgeted;
        };
        let limit_ms = budget_ms + (budget_ms * self.margin).max(self.min_margin_ms);
        if ms > limit_ms {
            Verdict::Over {
                budget_ms,
                limit_ms,
            }
        } else if ms < budget_ms * self.hint_below {
            // A tenth of headroom over what was measured, rounded to
            // something a person would write.
            Verdict::Tighten {
                budget_ms,
                suggest_ms: tidy(ms * 1.1),
            }
        } else {
            Verdict::Within { budget_ms }
        }
    }
}

/// `ms` rounded to two significant figures.
#[must_use]
pub fn tidy(ms: f64) -> f64 {
    if ms <= 0.0 {
        return 0.0;
    }
    let step = 10f64.powf(ms.log10().floor() - 1.0);
    (ms / step).ceil() * step
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"
margin = 0.2
min_margin_ms = 30
hint_below = 0.8
[song.budget_ms]
open = 28600
first_audio = 70
[song.expect]
render_lufs = -14.5
"#;

    #[test]
    fn a_songs_own_margin_wins() {
        let file = format!("{FILE}\n[loose]\nhint_below = 0\n[loose.budget_ms]\nopen = 1000\n");
        let b = Budgets::parse(&file, "loose").unwrap();
        assert_eq!(b.margin, 0.2, "the file's, where the song has none");
        assert_eq!(b.judge("open", 1.0), Verdict::Within { budget_ms: 1000.0 });
    }

    #[test]
    fn the_songs_section_is_read() {
        let b = Budgets::parse(FILE, "song").unwrap();
        assert_eq!(b.budget_ms["open"], 28_600.0);
        assert_eq!(b.expect["render_lufs"], -14.5);
        assert!(Budgets::parse(FILE, "other").is_err());
    }

    #[test]
    fn a_step_over_its_budget_and_margin_fails_and_one_inside_passes() {
        let b = Budgets::parse(FILE, "song").unwrap();
        // 28.6 s + 20 %: 34.32 s.
        assert!(b.judge("open", 34_400.0).failed());
        assert_eq!(
            b.judge("open", 30_000.0),
            Verdict::Within {
                budget_ms: 28_600.0
            }
        );
        // A short budget gets the floor: 70 + 30 ms.
        assert!(!b.judge("first_audio", 95.0).failed());
        assert!(b.judge("first_audio", 101.0).failed());
        assert_eq!(b.judge("render", 1.0), Verdict::Unbudgeted);
    }

    #[test]
    fn a_step_well_under_its_budget_suggests_a_tighter_one() {
        let b = Budgets::parse(FILE, "song").unwrap();
        assert_eq!(
            b.judge("open", 15_700.0),
            Verdict::Tighten {
                budget_ms: 28_600.0,
                suggest_ms: 18_000.0
            }
        );
        assert_eq!(tidy(52.3), 53.0);
    }
}
