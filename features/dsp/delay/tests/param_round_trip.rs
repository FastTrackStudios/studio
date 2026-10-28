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

#![cfg(feature = "delay-native")]

use nice_plug::prelude::Params;

/// Normalized points to probe. The ends matter as much as the middle: a
/// formatter that rounds to zero digits turns 0.004 into "0", and a parser
/// that cannot read its own "0" fails only there.
const PROBES: [f32; 7] = [0.0, 0.004, 0.25, 0.5, 0.75, 0.996, 1.0];

#[test]
fn every_parameter_parses_the_string_it_prints() {
    let params = dsp::delay::ui::params::DelayParams::default();
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
        "Delay parameters that cannot read their own display:\n  {}",
        broken.join("\n  ")
    );
}

// ── Stack ids (`fx.stack.params`) ────────────────────────────────────────

use dsp::delay::ui::params::{DelayParams, MAX_STAGES};

/// Every param id the pre-stack delay had, from origin/main's params. Stage 1
/// must keep exactly these so an old session loads bit-for-bit.
const PRE_STACK_IDS: &[&str] = &[
    "profile",
    "time_l",
    "time_r",
    "time_sync",
    "div_l",
    "div_r",
    "link",
    "feedback",
    "tone",
    "drive",
    "wow",
    "flutter",
    "duck",
    "mix",
    "chara",
    "charb",
];

/// The placement ids every stage gains (`StagePlacement`).
const PLACEMENT_IDS: &[&str] = &["use", "on", "lane"];

fn ids(params: &DelayParams) -> Vec<String> {
    params.param_map().into_iter().map(|(id, _, _)| id).collect()
}

/// Stage 1's ids, in order: the pre-stack set, then the placement.
fn stage1_ids() -> Vec<String> {
    PRE_STACK_IDS
        .iter()
        .chain(PLACEMENT_IDS)
        .map(ToString::to_string)
        .collect()
}

#[test]
fn stage_one_keeps_the_pre_stack_ids() {
    let all = ids(&DelayParams::default());
    let stage1 = stage1_ids();
    assert_eq!(
        all[..stage1.len()],
        stage1[..],
        "stage 1 must lead with the pre-stack ids, in order"
    );
}

#[test]
fn stage_n_ids_are_prefixed_stage_one_ids() {
    let all = ids(&DelayParams::default());
    let per_stage = stage1_ids();
    for n in 2..=MAX_STAGES {
        let start = (n - 1) * per_stage.len();
        let expected: Vec<String> = per_stage.iter().map(|id| format!("s{n}_{id}")).collect();
        assert_eq!(all[start..start + per_stage.len()], expected[..], "stage {n}");
    }
}

#[test]
fn lane_and_stack_globals_are_present() {
    let all = ids(&DelayParams::default());
    let mut expected: Vec<String> = Vec::new();
    for kind in ["lgain", "lmute", "lsolo"] {
        for l in 1..=MAX_STAGES {
            expected.push(format!("{kind}_{l}"));
        }
    }
    expected.push("summode".into());
    expected.push("stktrim".into());
    for id in &expected {
        assert!(all.contains(id), "missing {id}");
    }
    // Nothing besides the stages, lanes and globals.
    assert_eq!(
        all.len(),
        MAX_STAGES * stage1_ids().len() + 3 * MAX_STAGES + 2,
        "unexpected param ids: {all:?}"
    );
    let mut sorted = all.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), all.len(), "duplicate param ids");
}

#[test]
fn stage_one_keeps_its_persist_keys() {
    let params = DelayParams::default();
    let keys: Vec<String> = params.serialize_fields().into_keys().collect();
    // Pre-stack keys, unchanged.
    for key in ["profile_id", "editor_form"] {
        assert!(keys.iter().any(|k| k == key), "missing {key}: {keys:?}");
    }
    // Each further stage's profile id is prefixed like its params.
    for n in 2..=MAX_STAGES {
        let key = format!("s{n}_profile_id");
        assert!(keys.contains(&key), "missing {key}: {keys:?}");
    }
    assert!(keys.iter().any(|k| k == "focused_stage"));
}

/// A stage restores its own profile id from a session, independently of
/// stage 1's — and stage 1 restores from the pre-stack key.
#[test]
fn stage_profile_ids_round_trip() {
    let a = DelayParams::default();
    a.stage(0).store_profile_id(2);
    a.stage(3).store_profile_id(5);
    let saved = a.serialize_fields();

    let b = DelayParams::default();
    b.deserialize_fields(&saved);
    assert_eq!(b.stage(0).resolved_profile_index(), 2);
    assert_eq!(b.stage(3).resolved_profile_index(), 5);
    assert_eq!(*b.stage(0).profile_id.read(), *a.stage(0).profile_id.read());
}
