//! `xtask-workflows` — every .github/workflows/*.yml is generated from this
//! crate (the way Zed's `cargo xtask workflows` does it).
//!
//! ```text
//! cargo run -p xtask-workflows -- generate   # `just workflows`: write them
//! cargo run -p xtask-workflows -- check      # `just workflows-check`: fail on drift
//! ```
//!
//! Each workflow is one module; the WHY of every job and step lives there as
//! comments next to the code that builds it (the generated YAML carries
//! none). `check` regenerates in memory and compares with the files on
//! disk, so a hand edit to a workflow, or a source change without a
//! regenerate, fails the lint job in checks.yml. Composite actions under
//! .github/actions stay hand-written.

// The doc comments are the workflows' prose (moved here from the YAML):
// env var names, file paths and runner labels read better as written.
#![allow(clippy::doc_markdown)]

mod cache_devshell;
mod checks;
mod deploy;
mod model;
mod pr;
mod release;
mod steps;
mod studio_ios;

use anyhow::{Context, Result, bail};
use model::Workflow;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Every generated file starts with this, so `generate` can delete a
/// generated workflow that no longer has a source and `check` can report it.
const PREAMBLE: &str = "# Generated from xtask::workflows::";

/// A workflow's builder.
type Build = fn() -> Result<Workflow>;

/// (file stem, builder); the module is the stem with `_` for `-`.
const WORKFLOWS: &[(&str, Build)] = &[
    ("checks", checks::workflow),
    ("pr", pr::workflow),
    ("cache-devshell", cache_devshell::workflow),
    ("deploy", deploy::workflow),
    ("release", release::workflow),
    ("studio-ios", studio_ios::workflow),
];

fn workflows_dir() -> PathBuf {
    // xtask/workflows -> the repo root.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .unwrap_or(manifest)
        .join(".github/workflows")
}

/// Render every workflow: (file name, contents).
fn render_all() -> Result<Vec<(String, String)>> {
    WORKFLOWS
        .iter()
        .map(|&(stem, build)| {
            let workflow = build().with_context(|| format!("building {stem}"))?;
            let yaml = serde_yaml::to_string(&workflow)
                .with_context(|| format!("serialising {stem}"))?;
            let module = stem.replace('-', "_");
            let mut out = String::new();
            writeln!(out, "{PREAMBLE}{module} (xtask/workflows/src/{module}.rs)")?;
            writeln!(
                out,
                "# Rebuild with `just workflows`; do not edit by hand (`just workflows-check` fails CI on drift)."
            )?;
            writeln!(
                out,
                "# The reasons for every job and step are comments in that Rust source."
            )?;
            out.push_str(&yaml);
            Ok((format!("{stem}.yml"), out))
        })
        .collect()
}

/// Generated files on disk that no workflow produces any more.
fn stale_files(dir: &Path, rendered: &[(String, String)]) -> Result<Vec<PathBuf>> {
    let mut stale = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if rendered.iter().any(|(n, _)| n == name) {
            continue;
        }
        if std::fs::read_to_string(&path).is_ok_and(|c| c.starts_with(PREAMBLE)) {
            stale.push(path);
        }
    }
    Ok(stale)
}

fn generate() -> Result<()> {
    let dir = workflows_dir();
    let rendered = render_all()?;
    for path in stale_files(&dir, &rendered)? {
        std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        println!("removed {}", path.display());
    }
    for (name, contents) in &rendered {
        let path = dir.join(name);
        std::fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// The first line where two texts differ (1-based), with both sides.
fn first_difference(want: &str, have: &str) -> String {
    let mut want_lines = want.lines();
    let mut have_lines = have.lines();
    let mut n: usize = 1;
    loop {
        match (want_lines.next(), have_lines.next()) {
            (Some(w), Some(h)) if w == h => n = n.saturating_add(1),
            (w, h) => {
                return format!(
                    "line {n}:\n  generated: {}\n  on disk:   {}",
                    w.unwrap_or("<end of file>"),
                    h.unwrap_or("<end of file>")
                );
            }
        }
    }
}

/// Ok(true) when every file on disk is what `generate` would write.
fn check() -> Result<bool> {
    let dir = workflows_dir();
    let rendered = render_all()?;
    let mut clean = true;
    for (name, want) in &rendered {
        let path = dir.join(name);
        match std::fs::read_to_string(&path) {
            Ok(have) if &have == want => {}
            Ok(have) => {
                clean = false;
                eprintln!(
                    "{} is stale or hand-edited; {}",
                    path.display(),
                    first_difference(want, &have)
                );
            }
            Err(_) => {
                clean = false;
                eprintln!("{} is missing", path.display());
            }
        }
    }
    for path in stale_files(&dir, &rendered)? {
        clean = false;
        eprintln!("{} is generated but has no source any more", path.display());
    }
    if !clean {
        eprintln!(
            "\nThe workflows are generated from xtask/workflows: change the Rust source, \
             run `just workflows`, and commit both."
        );
    }
    Ok(clean)
}

fn run() -> Result<bool> {
    match std::env::args().nth(1).as_deref() {
        Some("generate") => generate().map(|()| true),
        Some("check") => check(),
        other => bail!(
            "usage: xtask-workflows <generate|check> (got {})",
            other.unwrap_or("nothing")
        ),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
