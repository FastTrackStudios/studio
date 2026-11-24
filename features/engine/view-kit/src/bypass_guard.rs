//! The guard that nothing in a window bypasses the Action List
//! (`docs/design/actions-everywhere.md`), for every crate that draws one.
//!
//! A control that changes the song or the window's state runs a named
//! action (`window::press`, `mcp::run`), so `watch`, a guide and the CLI
//! see it. [`problems`] scans a crate's sources for what a bypass is made
//! of — an edit pushed straight at the engine, a fold or a lane opened by
//! hand, the transport moved behind its actions, the dock or an option
//! flipped — and holds each file to the count its crate allows. Each view
//! crate runs it over its own `src/` with its own [`Allowance`]s, so the
//! guard goes with the crate wherever it lives. The allowances are debt:
//! routing a handler lowers a count; a new bypass fails.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What a bypass is written as, with whitespace taken out.
pub const PATTERNS: &[&str] = &[
    "lanes::set_expanded(",
    "folds::toggle(",
    "folds::set_tops(",
    "transport_here(",
    "request_dock(",
    "edits.borrow_mut().push(",
    "edits.borrow_mut().extend(",
    "Effect::Send(",
    "mode_phases::choose(",
    "applier.send(",
    "from_mixer.borrow_mut().push(",
    "set_racks_shown(",
    "play_return::set(",
    "options::LOCKING.toggle(",
    "options::LOCKING.set(",
    "options::GROUPING.toggle(",
    "options::SNAP.toggle(",
    "options::GRID.toggle(",
    "options::RIPPLE.toggle(",
    "options::RAZOR.toggle(",
    "options::AUTO_CROSSFADE.toggle(",
    "closeups.ask(",
    "current.set(None)",
    "rails.scene=",
    "self.fold(&",
    "collab::leave(",
    "collab::host_in_background(",
    "collab::join_in_background(",
    "collab::set_shared_transport(",
    "collab::set_sync_locked(",
    "projects::open_next(",
];

/// One file's allowance: `pattern` may be written `count` times in `file`
/// (its path under `src/`), for the reason `why`.
#[derive(Clone, Copy, Debug)]
pub struct Allowance {
    pub file: &'static str,
    pub pattern: &'static str,
    pub count: usize,
    pub why: &'static str,
}

/// Every way `src`'s files break `allowed`: a pattern written more often
/// than allowed, an allowance its file no longer needs, or an allowance
/// for a pattern the scan does not look for.
#[must_use]
pub fn problems(src: &Path, allowed: &[Allowance]) -> Vec<String> {
    let found = counts(src);
    let may: BTreeMap<(&str, &str), usize> = allowed
        .iter()
        .map(|a| ((a.file, a.pattern), a.count))
        .collect();
    let unscanned = allowed
        .iter()
        .filter(|a| !PATTERNS.contains(&a.pattern))
        .map(|a| format!("{}: {} ({}) is not scanned for", a.file, a.pattern, a.why));
    let over = found.iter().filter_map(|((file, pattern), n)| {
        let may = may.get(&(file.as_str(), *pattern)).copied().unwrap_or(0);
        (*n > may).then(|| {
            format!(
                "{file}: `{pattern}` {n}× (allowed {may}) — run the control's action (window::press) instead"
            )
        })
    });
    let under = allowed.iter().filter_map(|a| {
        let n = found
            .get(&(a.file.to_owned(), a.pattern))
            .copied()
            .unwrap_or(0);
        (n < a.count).then(|| {
            format!(
                "{}: `{}` is down to {n} from {} — lower its allowance",
                a.file, a.pattern, a.count
            )
        })
    });
    unscanned.chain(over).chain(under).collect()
}

/// How often each pattern is written in each file under `src`.
fn counts(src: &Path) -> BTreeMap<(String, &'static str), usize> {
    let mut files = Vec::new();
    sources(src, &mut files);
    files
        .iter()
        .filter_map(|file| {
            Some((
                relative(src, file),
                squeezed(&std::fs::read_to_string(file).ok()?),
            ))
        })
        .flat_map(|(rel, text)| {
            PATTERNS
                .iter()
                .map(move |pattern| ((rel.clone(), *pattern), text.matches(pattern).count()))
                .collect::<Vec<_>>()
        })
        .filter(|(_, n)| *n > 0)
        .collect()
}

/// `file`'s path under `src`, as an allowance names it.
fn relative(src: &Path, file: &Path) -> String {
    file.strip_prefix(src)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Every source file of a crate's window code, but its tests and binaries.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if name != "bin" && name != "tests" {
                sources(&path, out);
            }
        } else if name.ends_with(".rs")
            && !matches!(
                name.as_str(),
                "tests.rs" | "test_song.rs" | "bypass_guard.rs"
            )
        {
            out.push(path);
        }
    }
}

/// `source` with comments out and no whitespace: what the patterns match.
fn squeezed(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .flat_map(str::chars)
        .filter(|c| !c.is_whitespace())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squeezing_drops_comments_and_whitespace() {
        assert_eq!(squeezed("a ( b ) // c (\n  d"), "a(b)d");
    }

    #[test]
    fn an_unscanned_allowance_is_a_problem() {
        let allowed = [Allowance {
            file: "x.rs",
            pattern: "nothing_scans_this(",
            count: 1,
            why: "a test",
        }];
        let found = problems(Path::new("/nonexistent"), &allowed);
        assert!(found.iter().any(|p| p.contains("is not scanned for")));
        assert!(found.iter().any(|p| p.contains("lower its allowance")));
    }
}
