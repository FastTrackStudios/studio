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

#![cfg(feature = "eq-native")]

use nice_plug::prelude::Params;

/// Normalized points to probe. The ends matter as much as the middle: a
/// formatter that rounds to zero digits turns 0.004 into "0", and a parser
/// that cannot read its own "0" fails only there.
const PROBES: [f32; 7] = [0.0, 0.004, 0.25, 0.5, 0.75, 0.996, 1.0];

#[test]
fn every_parameter_parses_the_string_it_prints() {
    let params = dsp::eq::ui::params::FtsEqParams::default();
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
        "EQ parameters that cannot read their own display:\n  {}",
        broken.join("\n  ")
    );
}

// ── The stack's param layout (`fx.stack.params`) ──────────────────────────
//
// The EQ became a stack of up to four complete EQs. Stage 1 is nested bare,
// so a session saved by the single-EQ plugin has to land on it bit-for-bit:
// the same ids, in the same order (VST3 persists by index), and the same
// persist keys. These lists are the pre-stack plugin's, copied from
// `origin/main` before the split — not derived from the code under test.

/// The pre-stack top-level ids, in declaration order. The band block sits
/// between `ssl_trim` and `auto_range`.
const OLD_TOP_IDS_BEFORE_BANDS: &[&str] = &[
    "model",
    "output_gain",
    "delta",
    "db_range",
    "gain_scale",
    "auto_gain",
    "character",
    "output_pan",
    "output_pan_mode",
    "tune_peak_q_comp",
    "neve_eq_in",
    "neve_phase",
    "neve_trim",
    "neve_drive",
    "neve_hpf",
    "neve_low_freq",
    "neve_low_gain",
    "neve_mid_freq",
    "neve_mid_gain",
    "neve_high_gain",
    "pultec_eq_in",
    "pultec_low_freq",
    "pultec_low_boost",
    "pultec_low_atten",
    "pultec_high_boost_freq",
    "pultec_high_boost",
    "pultec_bandwidth",
    "pultec_high_atten_freq",
    "pultec_high_atten",
    "pultec_drive",
    "pultec_trim",
    "api_eq_in",
    "api_low_freq",
    "api_low_gain",
    "api_mid_freq",
    "api_mid_gain",
    "api_high_freq",
    "api_high_gain",
    "api_drive",
    "api_trim",
    "ssl_eq_in",
    "ssl_hpf",
    "ssl_lpf",
    "ssl_lf_freq",
    "ssl_lf_gain",
    "ssl_lmf_freq",
    "ssl_lmf_gain",
    "ssl_hmf_freq",
    "ssl_hmf_gain",
    "ssl_hf_freq",
    "ssl_hf_gain",
    "ssl_drive",
    "ssl_trim",
];
const OLD_TOP_IDS_AFTER_BANDS: &[&str] = &["auto_range"];

/// The pre-stack per-band ids, suffixed `_1`…`_24`.
const OLD_BAND_IDS: &[&str] = &[
    "on",
    "type",
    "freq",
    "gain",
    "q",
    "slope",
    "focus",
    "dynrange",
    "dynthr",
    "dynatk",
    "dynrel",
    "dynauto",
    "spectral",
    "place",
    "specdens",
    "spectilt",
    "scfilt",
    "sclo",
    "schi",
    "dynrelmode",
    "dynkey",
];
const OLD_BANDS: usize = 24;

/// The stack placement each stage appends (`fx.stack.params`).
const PLACEMENT_IDS: &[&str] = &["use", "on", "lane"];

/// Stage 1's ids in param-map order: the old plugin's, then the placement.
fn stage1_ids() -> Vec<String> {
    let mut ids: Vec<String> = OLD_TOP_IDS_BEFORE_BANDS
        .iter()
        .map(ToString::to_string)
        .collect();
    for band in 1..=OLD_BANDS {
        ids.extend(OLD_BAND_IDS.iter().map(|id| format!("{id}_{band}")));
    }
    ids.extend(OLD_TOP_IDS_AFTER_BANDS.iter().map(ToString::to_string));
    ids.extend(PLACEMENT_IDS.iter().map(ToString::to_string));
    ids
}

fn all_ids() -> Vec<String> {
    let params = dsp::eq::ui::params::FtsEqParams::default();
    params
        .param_map()
        .into_iter()
        .map(|(id, _, _)| id)
        .collect()
}

#[test]
fn stage_one_keeps_the_pre_stack_ids_in_order() {
    let ids = all_ids();
    let want = stage1_ids();
    assert_eq!(
        &ids[..want.len()],
        &want[..],
        "stage 1 must lead the param map with exactly the pre-stack ids"
    );
}

#[test]
fn stage_n_is_stage_one_prefixed() {
    use dsp::eq::ui::params::MAX_STAGES;
    let ids = all_ids();
    let per_stage = stage1_ids();
    for n in 2..=MAX_STAGES {
        let start = (n - 1) * per_stage.len();
        let want: Vec<String> = per_stage.iter().map(|id| format!("s{n}_{id}")).collect();
        assert_eq!(&ids[start..start + want.len()], &want[..], "stage {n}");
    }
}

#[test]
fn lane_and_stack_globals_follow_the_stages() {
    use dsp::eq::ui::params::MAX_STAGES;
    let ids = all_ids();
    let tail = &ids[MAX_STAGES * stage1_ids().len()..];
    let mut want: Vec<String> = Vec::new();
    for lane in 1..=MAX_STAGES {
        want.extend(["lgain", "lmute", "lsolo"].map(|id| format!("{id}_{lane}")));
    }
    want.push("summode".to_string());
    want.push("stktrim".to_string());
    assert_eq!(tail, &want[..]);

    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "duplicate param ids");
}

#[test]
fn stage_one_keeps_the_pre_stack_persist_keys() {
    use dsp::eq::ui::params::MAX_STAGES;
    let params = dsp::eq::ui::params::FtsEqParams::default();
    let keys: Vec<String> = params.serialize_fields().into_keys().collect();
    let mut want = vec!["model_id".to_string(), "editor_form".to_string()];
    for band in 1..=OLD_BANDS {
        want.push(format!("name_{band}"));
        want.push(format!("notes_{band}"));
    }
    for key in &want {
        assert!(keys.contains(key), "persist key {key} missing");
    }
    for n in 2..=MAX_STAGES {
        assert!(
            keys.contains(&format!("s{n}_model_id")),
            "stage {n} model_id"
        );
        assert!(
            keys.contains(&format!("s{n}_name_1")),
            "stage {n} band names"
        );
    }
    assert!(keys.contains(&"focused_stage".to_string()));
}

#[test]
fn default_stack_is_one_stage_on_lane_zero() {
    use dsp::eq::ui::params::MAX_STAGES;
    use dsp::stack::host::StackHost;
    let params = dsp::eq::ui::params::FtsEqParams::default();
    assert_eq!(params.stages_in_use(), vec![0]);
    assert_eq!(params.placement(0).lane_index(), 0);
    assert!(params.placement(0).stage_on.value());
    for i in 1..MAX_STAGES {
        assert!(
            !params.placement(i).in_use.value(),
            "stage {} in use",
            i + 1
        );
    }
    assert_eq!(params.resolved_focused_stage(), 0);
}
