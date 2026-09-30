//! The rig's unit faces for signal: one face per pedal, amp and
//! visualizer, and `faces.json` saying which face draws which block.
//!
//! Signal decides what is loaded — its presets, modules and overrides put
//! a pedal preset in a drive slot or a NAM capture in an amp. This set
//! only answers "what does that look like": signal matches a drive slot's
//! pedal preset name, or an amp's capture file name, against each entry's
//! `match` words (case-insensitive, first entry wins) and draws the face,
//! or its own plain card with the name when nothing matches.
//!
//! Binding, by convention: a face param `<ns>/<name>` is the block's param
//! `<name>` (`drive`, `threshold`, `b3_freq`…), `<ns>/level` is the block's
//! output level in dB, `<ns>/on` its bypass (on = not bypassed),
//! `<ns>/option` its capture choice, and `<ns>/tone` the drive block's own
//! EQ (to come). Values cross in the params' own units.

use serde_json::json;
use surface_kit::Place;
use surface_proto::{Range, Unit};

use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::rig_blues::drives;
use crate::suite_studio_blues::amps::{self, AmpModel};

/// The plugin id (and directory) the set is written under.
pub const PLUGIN: &str = "rig-faces";

/// An amp slot's face size (a row of the Control view holds two, or one
/// beside the EQ).
pub const AMP: (f64, f64) = (792.0, 392.0);
/// The gate picture's.
pub const GATE: (f64, f64) = (250.0, 392.0);

fn ns(model: AmpModel) -> &'static str {
    match model {
        AmpModel::SteelStringSinger => "dumble_sss",
        AmpModel::DumbleOds => "dumble_ods",
        AmpModel::Vibroverb => "vibroverb",
        AmpModel::TwoRock => "tworock",
        AmpModel::DeluxeReverb => "deluxe",
        AmpModel::TwinReverb => "twin",
    }
}

/// The words in a capture's file name that pick each amp's face, most
/// specific first.
fn amp_match(model: AmpModel) -> &'static [&'static str] {
    match model {
        AmpModel::SteelStringSinger => &["steel string singer", "dumble sss"],
        AmpModel::DumbleOds => &["dumble", "ods"],
        AmpModel::Vibroverb => &["vibroverb"],
        AmpModel::TwoRock => &["two-rock", "two rock", "tworock", "trck"],
        AmpModel::DeluxeReverb => &["deluxe reverb", "deluxe"],
        AmpModel::TwinReverb => &["twin reverb", "twin"],
    }
}

fn amp_face(model: AmpModel) -> Face {
    let f = amps::amp_compact(model, ns(model), AMP.0, AMP.1);
    let file: &'static str = Box::leak(format!("2{}-amp-{}", AmpModel::EVERY.iter().position(|m| *m == model).unwrap_or(0), ns(model).replace('_', "-")).into_boxed_str());
    Face { file, plugin: PLUGIN, ..f }
}

/// The gate's picture on the rig's Gate block.
fn gate() -> Face {
    let scene = canvas("Gate", |b, root| {
        ui::fit_root(b, root, GATE.0, GATE.1);
        let p = Place::new("Gate").size(GATE.0).options(&["ns=gate", &format!("h={}", GATE.1)]);
        ui::put_at(b, root, "viz.gate", &p, 0.0, 0.0);
    });
    let params = vec![
        ui::param("gate/threshold", "Threshold", Range::linear(-90.0, 0.0, Unit::Db), -50.0).featured(),
        ui::toggle("gate/on", "Gate", true).featured(),
    ];
    Face { file: "51-gate", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The Amp EQ's full graph (the rig's 24 bands).
fn eq() -> Face {
    let scene = canvas("Amp EQ", |b, root| {
        ui::fit_root(b, root, AMP.0, AMP.1);
        let p = Place::new("Amp EQ").size(AMP.0).options(&["ns=eq", "bands=24", "f_max=24000", "no_range", &format!("h={}", AMP.1)]);
        ui::put_at(b, root, "viz.eq_graph", &p, 0.0, 0.0);
    });
    let mut params = surface_kit::viz::catalog::eq_params("eq", 24);
    params.extend(surface_kit::viz::catalog::eq_view_params("eq"));
    Face { file: "52-amp-eq", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

/// The drives: (face, namespace, match words).
fn drive_entries() -> Vec<(Face, &'static str, &'static [&'static str])> {
    let d = |f: Face| Face { plugin: PLUGIN, ..f };
    vec![
        (d(drives::king_of_tone()), "kingoftone", &["king of tone"][..]),
        (d(drives::morning_glory()), "morningglory", &["morning glory"][..]),
        (d(drives::blade()), "katana", &["katana"][..]),
        (d(drives::pegasus()), "klon", &["klon", "centaur"][..]),
        (d(drives::british_blues()), "bluesbreaker", &["bluesbreaker", "blues breaker"][..]),
        (d(drives::tube_drive()), "tubescreamer", &["tube screamer", "ts10", "ts-10", "ts9", "ts808", "screamer"][..]),
    ]
}

/// Every face in the set.
#[must_use]
pub fn faces() -> Vec<Face> {
    let mut v: Vec<Face> = drive_entries().into_iter().map(|(f, _, _)| f).collect();
    v.extend(AmpModel::EVERY.iter().map(|m| amp_face(*m)));
    v.push(gate());
    v.push(eq());
    v
}

/// `faces.json`: which face draws which block.
#[must_use]
pub fn manifest() -> String {
    let drives: Vec<_> = drive_entries().into_iter().map(|(f, ns, m)| json!({ "match": m, "face": f.file, "ns": ns })).collect();
    let amps: Vec<_> = AmpModel::EVERY
        .iter()
        .map(|m| json!({ "match": amp_match(*m), "face": amp_face(*m).file, "ns": ns(*m), "name": m.short() }))
        .collect();
    let doc = json!({
        "drives": drives,
        "amps": amps,
        "gate": { "face": "51-gate", "ns": "gate" },
        "eq": { "face": "52-amp-eq", "ns": "eq" },
    });
    serde_json::to_string_pretty(&doc).expect("json")
}
