//! Nothing in the arrangement bypasses the Action List
//! (`docs/design/actions-everywhere.md`; the scan is
//! [`daw_view_kit::bypass_guard`]).

use std::path::Path;

use daw_view_kit::bypass_guard::{Allowance, problems};

/// What the arrangement may still write, and why.
const ALLOWED: &[Allowance] = &[
    // A gesture commits as its named action (`commit_edits` →
    // `ARRANGE_*` / `ITEMS_SELECT`, `turn_released` → `TRACK_SET`), pushed
    // as `Edit::Run` so it runs in order with the edits around it. What
    // is left is no gesture of the hand's:
    Allowance {
        file: "widget.rs",
        pattern: "edits.borrow_mut().push(",
        count: 7,
        why: "the commit lane itself (Edit::Run); an action's own work (visibility, \
              three Done barriers an engine action waits on); the track knob's live \
              turn and its touch (TRACK_SET on release)",
    },
    Allowance {
        file: "widget.rs",
        pattern: "edits.borrow_mut().extend(",
        count: 4,
        why: "an action's own work (a key's through the list, the engine's asked); \
              commit_edits' edits no action names (none today, warned); an \
              automation drag's live edits (lane_moved) and the close of its undo \
              step after its commit (lane_ended)",
    },
    Allowance {
        file: "widget.rs",
        pattern: "folds::toggle(",
        count: 1,
        why: "after SESSION_FOLD: this process's folds, which an engine in another \
              process cannot tell the window",
    },
    // Debt: still to route.
    Allowance {
        file: "arrangement_host.rs",
        pattern: "applier.send(",
        count: 1,
        why: "the arrangement's applier",
    },
];

#[test]
fn no_control_bypasses_the_action_list() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let problems = problems(&src, ALLOWED);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
