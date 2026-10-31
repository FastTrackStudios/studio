//! Nothing in the window bypasses the Action List
//! (`docs/design/actions-everywhere.md`).
//!
//! A control that changes the song or the window's state runs a named
//! action (`window::press`, `mcp::run`), so `watch`, a guide and the CLI
//! see it. This scans session-ui's sources for what a bypass is made of —
//! an edit pushed straight at the engine, a fold or a lane opened by hand,
//! the transport moved behind its actions (`transport_here`; a press is
//! `engine::transport`, which runs `SESSION_PLAY_STOP`, `SESSION_SEEK`, …),
//! the dock or an option flipped — and holds
//! each file to the count it is allowed. The allowances are debt: routing a
//! handler lowers its count here; a new bypass fails.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What a bypass is written as, with whitespace taken out.
const PATTERNS: &[&str] = &[
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
];

/// `(file under src/, pattern, how many, why)`: what is allowed, and why.
const ALLOWED: &[(&str, &str, usize, &str)] = &[
    // Inside an action's own handler, or the key path that runs them.
    (
        "window.rs",
        "request_dock(",
        1,
        "the docks chord, in through_list",
    ),
    (
        "view_actions.rs",
        "set_racks_shown(",
        1,
        "SESSION_MIXER_FX's handler",
    ),
    // Live previews (rule 3): what a continuous gesture sends while it
    // moves, committed through its action when it ends.
    (
        "fx_view.rs",
        "applier.send(",
        1,
        "the strip fader's live turn (TRACK_SET on release, commit_turn)",
    ),
    (
        "mixer_panel.rs",
        "applier.send(",
        1,
        "the fader's live drag (TRACK_SET on release, commit_turn)",
    ),
    (
        "mixer_panel.rs",
        "from_mixer.borrow_mut().push(",
        1,
        "the fader's live drag (TRACK_SET on release, commit_turn)",
    ),
    // widget.rs: a gesture commits as its named action (`commit_edits` →
    // `ARRANGE_*` / `ITEMS_SELECT`, `turn_released` → `TRACK_SET`), pushed
    // as `Edit::Run` so it runs in order with the edits around it. What
    // is left is no gesture of the hand's:
    (
        "widget.rs",
        "edits.borrow_mut().push(",
        7,
        "the commit lane itself (Edit::Run); an action's own work (visibility, \
         three Done barriers an engine action waits on); the track knob's live \
         turn and its touch (TRACK_SET on release)",
    ),
    (
        "widget.rs",
        "edits.borrow_mut().extend(",
        6,
        "an action's own work (a key's through the list, the engine's asked); \
         commit_edits' edits no action names (none today, warned); debt: the \
         automation lanes' press, drag, release and cancel (AUTOMATION_ITEM / \
         AUTOMATION_WRITE)",
    ),
    (
        "tone.rs",
        "applier.send(",
        1,
        "the racks' worker: a gesture's live edits, then its FX_SET / \
         FX_ENABLE / RIG_BLOCK_SET commits (Edit::Run) in order after them",
    ),
    (
        "mixer_panel.rs",
        "rails.scene=",
        1,
        "MIXER_VIEW taken up (take_view)",
    ),
    (
        "fx_view.rs",
        "self.fold(&",
        1,
        "FX_VIEW taken up (take_view)",
    ),
    // Debt: still to route (the inventory in the design doc).
    ("studio.rs", "applier.send(", 1, "the arrangement's applier"),
    (
        "mix_guide.rs",
        "applier.send(",
        1,
        "Mix mode's guide mute: TRACK_SET inside SESSION_MODE",
    ),
    (
        "mixer_panel.rs",
        "mode_phases::choose(",
        1,
        "the first sync of the shell's phase",
    ),
    ("mixer_panel.rs", "set_racks_shown(", 1, "its definition"),
    (
        "shell.rs",
        "folds::set_tops(",
        1,
        "fold every folder: SESSION_FOLD_TOPS",
    ),
    ("shell.rs", "options::LOCKING.set(", 1, "lock: SESSION_LOCK"),
    (
        "transport_bar.rs",
        "play_return::set(",
        1,
        "stop behaviour: let SESSION_STOP_BEHAVIOUR set it",
    ),
];

/// Every source file of the window, but its tests and binaries.
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
        } else if name.ends_with(".rs") && name != "tests.rs" && name != "test_song.rs" {
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

/// How often each pattern is written in each file.
fn found() -> BTreeMap<(String, &'static str), usize> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    let mut out = BTreeMap::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let rel = file
            .strip_prefix(&src)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = squeezed(&text);
        for pattern in PATTERNS {
            let n = text.matches(pattern).count();
            if n > 0 {
                out.insert((rel.clone(), *pattern), n);
            }
        }
    }
    out
}

#[test]
fn no_control_bypasses_the_action_list() {
    let found = found();
    let allowed: BTreeMap<(String, &str), usize> = ALLOWED
        .iter()
        .map(|(file, pattern, n, _)| (((*file).to_owned(), *pattern), *n))
        .collect();
    let mut problems = Vec::new();
    for ((file, pattern), n) in &found {
        let may = allowed.get(&(file.clone(), *pattern)).copied().unwrap_or(0);
        if *n > may {
            problems.push(format!(
                "{file}: `{pattern}` {n}× (allowed {may}) — run the control's action (window::press) instead"
            ));
        }
    }
    for ((file, pattern), may) in &allowed {
        let n = found.get(&(file.clone(), *pattern)).copied().unwrap_or(0);
        if n < *may {
            problems.push(format!(
                "{file}: `{pattern}` is down to {n} from {may} — lower its allowance"
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_pattern_is_found_where_it_is_allowed() {
    // The scan itself works: an allowance names a pattern it looks for.
    for (file, pattern, _, why) in ALLOWED {
        assert!(
            PATTERNS.contains(pattern),
            "{file}: {pattern} ({why}) is not scanned for"
        );
    }
}
