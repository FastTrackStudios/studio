//! Nothing in the app panels bypasses the Action List
//! (`docs/design/actions-everywhere.md`; the scan is
//! [`daw_view_kit::bypass_guard`]).

use std::path::Path;

use daw_view_kit::bypass_guard::{Allowance, problems};

/// What the panels may still write, and why. All debt: still to route
/// (the inventory in the design doc).
const ALLOWED: &[Allowance] = &[
    Allowance {
        file: "mix_guide.rs",
        pattern: "applier.send(",
        count: 1,
        why: "Mix mode's guide mute: TRACK_SET inside SESSION_MODE",
    },
    Allowance {
        file: "shell.rs",
        pattern: "folds::set_tops(",
        count: 1,
        why: "fold every folder: SESSION_FOLD_TOPS",
    },
    Allowance {
        file: "transport_bar.rs",
        pattern: "play_return::set(",
        count: 1,
        why: "stop behaviour: let SESSION_STOP_BEHAVIOUR set it",
    },
];

#[test]
fn no_control_bypasses_the_action_list() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let problems = problems(&src, ALLOWED);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
