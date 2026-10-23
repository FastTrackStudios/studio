//! A small typed model of a GitHub Actions workflow — just the keys studio's
//! workflows use — serialised with serde_yaml.
//!
//! Not the `gh-workflow` crate: studio's workflows lean on a handful of keys
//! (`environment`, matrix `include`, step `continue-on-error`, job-level
//! `concurrency`, list-valued `runs-on`) and on exact key order for a readable
//! diff, which a dozen serde structs say more plainly than a builder DSL.
//! Free-form values (`on:`, `permissions:`, `strategy:`, a step's `with:`)
//! are [`Value`]s; [`yaml`] parses a YAML snippet into one, so triggers read
//! in the source the way they read in the generated file.

use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::{Serialize, Serializer};
use serde_yaml::Value;

/// Parse a YAML snippet into a [`Value`] (triggers, permissions, matrices).
pub fn yaml(snippet: &str) -> Result<Value> {
    serde_yaml::from_str(snippet).with_context(|| format!("bad YAML snippet:\n{snippet}"))
}

#[derive(Serialize)]
pub struct Workflow {
    pub name: String,
    pub on: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<Concurrency>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<String, String>,
    pub jobs: IndexMap<String, Job>,
}

impl Workflow {
    pub fn new(name: &str, on: Value) -> Self {
        Self {
            name: name.to_owned(),
            on,
            permissions: None,
            concurrency: None,
            env: IndexMap::new(),
            jobs: IndexMap::new(),
        }
    }

    pub fn permissions(mut self, permissions: Value) -> Self {
        self.permissions = Some(permissions);
        self
    }

    pub fn concurrency(mut self, group: &str, cancel_in_progress: bool) -> Self {
        self.concurrency = Some(Concurrency::new(group, cancel_in_progress));
        self
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_owned(), value.to_owned());
        self
    }

    pub fn job(mut self, id: &str, job: Job) -> Self {
        self.jobs.insert(id.to_owned(), job);
        self
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Concurrency {
    pub group: String,
    pub cancel_in_progress: bool,
}

impl Concurrency {
    pub fn new(group: &str, cancel_in_progress: bool) -> Self {
        Self {
            group: group.to_owned(),
            cancel_in_progress,
        }
    }
}

/// `runs-on`: one label, or a list of labels for a self-hosted runner.
pub enum RunsOn {
    Label(&'static str),
    Labels(&'static [&'static str]),
    /// A label built at generation time (a WarpBuild label with options).
    Owned(String),
}

impl Serialize for RunsOn {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Label(label) => s.serialize_str(label),
            Self::Labels(labels) => labels.serialize(s),
            Self::Owned(label) => s.serialize_str(label),
        }
    }
}

/// The GitHub-hosted image every hosted job uses.
pub const UBUNTU: RunsOn = RunsOn::Label("ubuntu-latest");

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Job {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", serialize_with = "one_or_many")]
    pub needs: Vec<String>,
    #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
    pub cond: Option<String>,
    pub runs_on: RunsOn,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_minutes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continue_on_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<Concurrency>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<Value>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub outputs: IndexMap<String, String>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<String, String>,
    pub steps: Vec<Step>,
}

/// `needs: plan` for one, `needs: [a, b]` for several, as a human writes it.
fn one_or_many<S: Serializer>(needs: &[String], s: S) -> Result<S::Ok, S::Error> {
    match needs {
        [one] => s.serialize_str(one),
        many => many.serialize(s),
    }
}

impl Job {
    pub fn new(runs_on: RunsOn) -> Self {
        Self {
            name: None,
            needs: Vec::new(),
            cond: None,
            runs_on,
            timeout_minutes: None,
            continue_on_error: None,
            permissions: None,
            concurrency: None,
            strategy: None,
            environment: None,
            outputs: IndexMap::new(),
            env: IndexMap::new(),
            steps: Vec::new(),
        }
    }

    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_owned());
        self
    }

    pub fn needs(mut self, needs: &[&str]) -> Self {
        self.needs = needs.iter().map(|&n| n.to_owned()).collect();
        self
    }

    pub fn cond(mut self, cond: &str) -> Self {
        self.cond = Some(cond.to_owned());
        self
    }

    pub const fn timeout(mut self, minutes: u32) -> Self {
        self.timeout_minutes = Some(minutes);
        self
    }

    /// The job's failure never fails the workflow (a trial).
    pub const fn continue_on_error(mut self) -> Self {
        self.continue_on_error = Some(true);
        self
    }

    pub fn permissions(mut self, permissions: Value) -> Self {
        self.permissions = Some(permissions);
        self
    }

    pub fn concurrency(mut self, group: &str, cancel_in_progress: bool) -> Self {
        self.concurrency = Some(Concurrency::new(group, cancel_in_progress));
        self
    }

    pub fn strategy(mut self, strategy: Value) -> Self {
        self.strategy = Some(strategy);
        self
    }

    pub fn environment(mut self, environment: Value) -> Self {
        self.environment = Some(environment);
        self
    }

    pub fn output(mut self, key: &str, value: &str) -> Self {
        self.outputs.insert(key.to_owned(), value.to_owned());
        self
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_owned(), value.to_owned());
        self
    }

    pub fn step(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }

    pub fn steps(mut self, steps: impl IntoIterator<Item = Step>) -> Self {
        self.steps.extend(steps);
        self
    }
}

#[derive(Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct Step {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
    pub cond: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continue_on_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uses: Option<String>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub with: IndexMap<String, Value>,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
}

impl Step {
    /// A `run:` step. The script is used verbatim (a `\`-continued Rust
    /// string or a raw literal at column 0 both read well).
    pub fn run(name: &str, script: &str) -> Self {
        Self {
            name: Some(name.to_owned()),
            run: Some(script.to_owned()),
            ..Self::default()
        }
    }

    /// A `uses:` step with a name.
    pub fn uses(name: &str, action: &str) -> Self {
        Self {
            name: Some(name.to_owned()),
            uses: Some(action.to_owned()),
            ..Self::default()
        }
    }

    /// A `uses:` step without a name (GitHub shows the action).
    pub fn action(action: &str) -> Self {
        Self {
            uses: Some(action.to_owned()),
            ..Self::default()
        }
    }

    pub fn id(mut self, id: &str) -> Self {
        self.id = Some(id.to_owned());
        self
    }

    pub fn cond(mut self, cond: &str) -> Self {
        self.cond = Some(cond.to_owned());
        self
    }

    pub const fn continue_on_error(mut self) -> Self {
        self.continue_on_error = Some(true);
        self
    }

    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.with.insert(key.to_owned(), value.into());
        self
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_owned(), value.to_owned());
        self
    }
}
