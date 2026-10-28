//! Every parameter's displayed value can be typed back in.
//!
//! A host's generic parameter list is text in, text out: it shows whatever
//! `normalized_value_to_string` returns and hands whatever the user types to
//! `string_to_normalized_value`. A parameter that formats but does not parse
//! is read-only in every DAW's parameter panel and in automation-lane entry.
//!
//! This caught a real one. Percentage parameters used `v2s_f32_percentage`,
//! which renders 0.5 as "50" — and nothing else. No `.with_unit("%")`, so the
//! panel read a bare "50" with no clue what kind of 50 it was, and no
//! `.with_string_to_value`, so typing "50" back did nothing at all. Forty-one
//! parameters across the four FX were like this.

#![cfg(feature = "saturate-native")]

use nice_plug::prelude::Params;

/// Normalized points to probe. The ends matter as much as the middle: a
/// formatter that rounds to zero digits turns 0.004 into "0", and a parser
/// that cannot read its own "0" fails only there.
const PROBES: [f32; 7] = [0.0, 0.004, 0.25, 0.5, 0.75, 0.996, 1.0];

#[test]
fn every_parameter_parses_the_string_it_prints() {
    let params = dsp::saturate::ui::params::SatParams::default();
    let mut broken: Vec<String> = Vec::new();

    for (id, ptr, _group) in params.param_map() {
        // SAFETY: `params` outlives this loop, and `param_map` hands back
        // pointers into it.
        let name = unsafe { ptr.name() }.to_string();

        // Stepped parameters (choices, band shapes, profile pickers) are
        // rendered as a list by every host, not as a text field, and their
        // display is a LABEL — "Bell", "Impulse Response". Asking those to
        // parse a number back would be asking for the wrong thing. All that
        // is required of them here is that they print something.
        if unsafe { ptr.step_count() }.is_some() {
            let shown = unsafe { ptr.normalized_value_to_string(0.5, true) };
            if shown.trim().is_empty() {
                broken.push(format!("{id} ({name}): stepped parameter prints nothing"));
            }
            continue;
        }

        for probe in PROBES {
            // The host shows the value WITH its unit, so that is the string
            // the user edits and the one that has to parse.
            let shown = unsafe { ptr.normalized_value_to_string(probe, true) };
            let Some(parsed) = (unsafe { ptr.string_to_normalized_value(&shown) }) else {
                broken.push(format!("{id} ({name}): {shown:?} does not parse"));
                break;
            };
            // Compare what the user SEES, not the raw normalized value.
            // A display rounds — "1.1:1" on a skewed ratio is a couple of
            // percent off the value behind it — and that loss is fine as
            // long as it is stable. What is not fine is a string that means
            // something else when typed back, which is what this catches.
            let again = unsafe { ptr.normalized_value_to_string(parsed, true) };
            if again != shown {
                broken.push(format!(
                    "{id} ({name}): {probe} printed {shown:?}, which parsed back \
                     and printed as {again:?}"
                ));
                break;
            }
        }
    }

    assert!(
        broken.is_empty(),
        "Saturate parameters that cannot read their own display:\n  {}",
        broken.join("\n  ")
    );
}

// ── The stack layout (`fx.stack.params`) ──────────────────────────────────

use dsp::saturate::ui::params::{MAX_STAGES, SatParams};

/// Stage 1's ids before the stack existed, in declaration order, read off
/// origin/main's `SatParams`. A saved single-circuit session must land in
/// stage 1 bit-for-bit, so this list never changes.
fn pre_stack_ids() -> Vec<String> {
    let mut ids: Vec<String> = [
        "profile", "drive", "q_point", "sag", "tilt", "mix", "output", "chara", "charb",
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    for band in 1..=6 {
        for id in ["eshape", "efreq", "egain", "eq"] {
            ids.push(format!("{id}_{band}"));
        }
    }
    ids
}

/// Stage 1's full id list: the pre-stack ids, then its stack placement.
fn stage1_ids() -> Vec<String> {
    let mut ids = pre_stack_ids();
    ids.extend(["use", "on", "lane"].map(ToString::to_string));
    ids
}

fn ids(params: &SatParams) -> Vec<String> {
    params.param_map().into_iter().map(|(id, _, _)| id).collect()
}

#[test]
fn stage_one_keeps_the_pre_stack_ids() {
    let params = SatParams::default();
    let all = ids(&params);
    let stage1 = stage1_ids();
    assert_eq!(all[..stage1.len()], stage1[..], "stage 1's ids moved");
    // And nothing else in the tree is unprefixed under a pre-stack name.
    for id in pre_stack_ids() {
        assert_eq!(
            all.iter().filter(|x| **x == id).count(),
            1,
            "{id} appears more than once"
        );
    }
}

#[test]
fn stage_n_ids_are_prefixed_stage_one_ids() {
    let params = SatParams::default();
    let all = ids(&params);
    let per = stage1_ids().len();
    for n in 2..=MAX_STAGES {
        let expected: Vec<String> = stage1_ids()
            .iter()
            .map(|id| format!("s{n}_{id}"))
            .collect();
        let start = per * (n - 1);
        assert_eq!(all[start..start + per], expected[..], "stage {n}'s ids");
    }
}

#[test]
fn lane_and_stack_globals_are_present() {
    let params = SatParams::default();
    let all = ids(&params);
    let mut expected: Vec<String> = Vec::new();
    for lane in 1..=MAX_STAGES {
        for id in ["lgain", "lmute", "lsolo"] {
            expected.push(format!("{id}_{lane}"));
        }
    }
    expected.extend(["summode", "stktrim"].map(ToString::to_string));
    let start = stage1_ids().len() * MAX_STAGES;
    assert_eq!(all[start..], expected[..]);

    let unique: std::collections::BTreeSet<&String> = all.iter().collect();
    assert_eq!(unique.len(), all.len(), "duplicate parameter ids");
}

#[test]
fn persist_keys_keep_their_names() {
    let params = SatParams::default();
    params.stage(0).store_profile_id(0);
    params.stage(1).store_profile_id(0);
    *params.editor_form.write() = "responsive".to_string();
    *params.focused_stage.write() = "0".to_string();
    let keys: Vec<String> = params.serialize_fields().into_keys().collect();
    for key in ["profile_id", "editor_form", "focused_stage", "s2_profile_id"] {
        assert!(keys.iter().any(|k| k == key), "missing persist key {key}: {keys:?}");
    }
}

#[test]
fn a_default_tree_is_one_stage_on_lane_zero() {
    use dsp::stack::host::StackHost;
    let params = SatParams::default();
    assert_eq!(params.stages_in_use(), vec![0]);
    assert_eq!(params.stage(0).place.lane_index(), 0);
    assert!(params.stage(0).place.stage_on.value());
    assert_eq!(params.resolved_focused_stage(), 0);
}
