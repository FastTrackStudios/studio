//! Nothing in the mixer bypasses the Action List
//! (`docs/design/actions-everywhere.md`; the scan is
//! [`daw_view_kit::bypass_guard`]).

use std::path::Path;

use daw_view_kit::bypass_guard::{Allowance, problems};

/// What the mixer may still write, and why.
const ALLOWED: &[Allowance] = &[
    Allowance {
        file: "mixer_actions.rs",
        pattern: "set_racks_shown(",
        count: 1,
        why: "SESSION_MIXER_FX's handler",
    },
    // Live previews (rule 3): what a continuous gesture sends while it
    // moves, committed through its action when it ends.
    Allowance {
        file: "fx_view.rs",
        pattern: "applier.send(",
        count: 1,
        why: "the strip fader's live turn (TRACK_SET on release, commit_turn)",
    },
    Allowance {
        file: "mixer_panel.rs",
        pattern: "applier.send(",
        count: 1,
        why: "the fader's live drag (TRACK_SET on release, commit_turn)",
    },
    Allowance {
        file: "mixer_panel.rs",
        pattern: "from_mixer.borrow_mut().push(",
        count: 1,
        why: "the fader's live drag (TRACK_SET on release, commit_turn)",
    },
    Allowance {
        file: "tone.rs",
        pattern: "applier.send(",
        count: 1,
        why: "the racks' worker: a gesture's live edits, then its FX_SET / \
              FX_ENABLE / RIG_BLOCK_SET commits (Edit::Run) in order after them",
    },
    Allowance {
        file: "mixer_panel.rs",
        pattern: "rails.scene=",
        count: 1,
        why: "MIXER_VIEW taken up (take_view)",
    },
    Allowance {
        file: "fx_view.rs",
        pattern: "self.fold(&",
        count: 1,
        why: "FX_VIEW taken up (take_view)",
    },
    Allowance {
        file: "mixer_panel.rs",
        pattern: "folds::toggle(",
        count: 1,
        why: "after SESSION_FOLD: this process's folds (see the arrangement's)",
    },
    // Debt: still to route.
    Allowance {
        file: "mixer_panel.rs",
        pattern: "mode_phases::choose(",
        count: 1,
        why: "the first sync of the shell's phase",
    },
];

#[test]
fn no_control_bypasses_the_action_list() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let problems = problems(&src, ALLOWED);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
