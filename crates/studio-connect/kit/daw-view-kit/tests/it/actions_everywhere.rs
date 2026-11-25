//! Nothing in the kit bypasses the Action List
//! (`docs/design/actions-everywhere.md`; the scan is
//! [`daw_view_kit::bypass_guard`], and each view crate runs it over its own
//! sources).

use std::path::Path;

use daw_view_kit::bypass_guard::{Allowance, problems};

/// What the kit's own files may still write, and why.
const ALLOWED: &[Allowance] = &[
    Allowance {
        file: "window.rs",
        pattern: "request_dock(",
        count: 1,
        why: "the docks chord, in through_list",
    },
    Allowance {
        file: "app.rs",
        pattern: "request_dock(",
        count: 2,
        why: "its definition, and if_docked asking the app's dock",
    },
    Allowance {
        file: "dock.rs",
        pattern: "set_racks_shown(",
        count: 1,
        why: "its definition (SESSION_MIXER_FX's handler calls it)",
    },
    // Debt: still to route (the inventory in the design doc).
    Allowance {
        file: "chrome.rs",
        pattern: "options::LOCKING.set(",
        count: 1,
        why: "lock: SESSION_LOCK",
    },
];

#[test]
fn no_control_bypasses_the_action_list() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let problems = problems(&src, ALLOWED);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
