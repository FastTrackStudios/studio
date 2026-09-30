//! The rig's unit faces for signal: one face per pedal, amp and
//! visualizer, and `faces.json` saying which face draws which block.
//!
//! Signal decides what is loaded — its presets, modules and overrides put
//! a pedal preset in a drive slot or a NAM capture in an amp. This set
//! only answers "what does that look like": signal matches a drive slot's
//! pedal preset name, or an amp's capture file name, against each entry's
//! `match` words (case-insensitive, first entry wins; `a+b` needs every
//! word) and draws the face,
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

use crate::face::{Face, canvas, face_size};
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
        AmpModel::Ac30 => "ac30",
    }
}

/// The words in a capture's file name that pick each amp's face, most
/// specific first.
fn amp_match(model: AmpModel) -> &'static [&'static str] {
    match model {
        AmpModel::SteelStringSinger => &["steel string singer", "dumble sss", "dumble steel"],
        AmpModel::DumbleOds => &["dumble", "ods"],
        AmpModel::Vibroverb => &["vibroverb"],
        AmpModel::TwoRock => &["two-rock", "two rock", "tworock", "trck"],
        AmpModel::DeluxeReverb => &["deluxe reverb", "deluxe", "drri", "fender clean"],
        AmpModel::TwinReverb => &["twin reverb", "twin"],
        AmpModel::Ac30 => &["ac30", "ac-30", "ac 30", "vox", "vx "],
    }
}

fn amp_face(model: AmpModel) -> Face {
    let f = amps::amp_compact(model, ns(model), AMP.0, AMP.1);
    let file: &'static str = Box::leak(format!("2{}-amp-{}", AmpModel::EVERY.iter().position(|m| *m == model).unwrap_or(0), ns(model).replace('_', "-")).into_boxed_str());
    Face { file, plugin: PLUGIN, ..f }
}

/// The John Mayer trio as one capture (the '64 Vibroverb, the Dumble
/// Steel String Singer #002 and the Two-Rock prototype blended): the three
/// side by side, their GAIN and LEVEL all the one capture's.
fn trio() -> Face {
    const W: f64 = 1180.0;
    let (aw, gap) = ((W - 2.0 * 14.0) / 3.0, 14.0);
    let models = [AmpModel::Vibroverb, AmpModel::SteelStringSinger, AmpModel::TwoRock];
    let faces: Vec<Face> = models.iter().map(|m| amps::amp_compact(*m, "trio", aw, AMP.1)).collect();
    let scene = canvas("John Mayer trio", |b, root| {
        ui::fit_root(b, root, W, AMP.1);
        for (i, f) in faces.iter().enumerate() {
            crate::face::embed(b, root, f, (i as f64 * (aw + gap), 0.0, aw, AMP.1));
        }
    });
    let params = faces.into_iter().next().map(|f| f.params).unwrap_or_default();
    Face { file: "27-amp-jm-trio", plugin: PLUGIN, scene, params, streams: Vec::new() }
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

/// The reverb algorithms of the rig's reverb block, in its DSP's order.
const VERB_ALGOS: [&str; 15] = [
    "Room", "Hall", "Plate", "Spring", "Cloud", "Bloom", "Shimmer", "Chorale", "Magneto", "NonLinear", "Swell", "Reflections", "Velvet", "FreeVerb", "Convolution",
];

/// A spring reverb block's picture, tall and narrow for a panel beside its
/// unit: the open tank stood on end, its springs ringing as the reverb's
/// decay, damping and modulation set, and the algorithm the block is
/// actually running named over it.
fn spring_viz() -> Face {
    const W: f64 = 230.0;
    const H: f64 = 392.0;
    let a = crate::reverb::spring::Springs {
        dwell: "spring/decay",
        damping: "spring/damping",
        tension: "spring/tension",
        drip: "spring/modulation",
        on: "spring/on",
        input: "spring/in",
        output: "spring/out",
    };
    let scene = canvas("Spring picture", |b, root| {
        ui::fit_root(b, root, W, H);
        // The algorithm, over the tank.
        let head = b.add(root, surface_kit::draw::text("algorithm", "SPRING", 0.0, 10.0, W, 12.0, "#c9cdd2", document_proto::TextAlign::Center));
        let c = surface_proto::Control::new(surface_proto::ControlKind::Readout)
            .slot(surface_proto::SlotRole::Value, head.0)
            .bound(ui::bind("spring/algorithm"));
        b.controls.insert(head, c);
        // The tank, drawn lying down and stood on end.
        let (len, depth) = (H - 44.0, W - 24.0);
        let mut tank = surface_kit::draw::frame("Tank", W / 2.0 - len / 2.0, 34.0 + (H - 34.0) / 2.0 - depth / 2.0 - 4.0, len, depth);
        tank.clips_content = true;
        tank.corner_radii = document_proto::CornerRadii::uniform(6.0);
        let tank = b.add(root, surface_kit::draw::rotated(tank, 90.0));
        crate::reverb::spring::tank_art_with(b, tank, &a, len, depth, 2, "");
    });
    let params = vec![
        ui::choice("spring/algorithm", "Algorithm", &VERB_ALGOS, 3),
        ui::param("spring/decay", "Decay", Range::linear(0.0, 1.0, Unit::None), 0.35),
        ui::param("spring/damping", "Damping", Range::linear(0.0, 1.0, Unit::None), 0.3),
        ui::param("spring/tension", "Tension", Range::linear(0.0, 1.0, Unit::None), 0.5),
        ui::param("spring/modulation", "Modulation", Range::linear(0.0, 1.0, Unit::None), 0.2),
        ui::toggle("spring/on", "Reverb", true),
    ];
    Face { file: "53-spring-viz", plugin: PLUGIN, scene, params, streams: Vec::new() }
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

/// The Gravity Tank's bindings: the suite's spring tank on the rig's two
/// blocks, named as the plugin names it.
fn tank_bind() -> crate::suite_studio_blues::tank::Tank {
    crate::suite_studio_blues::tank::Tank {
        title: "GRAVITY TANK",
        file: "19-gravity-tank-full",
        plugin: PLUGIN,
        trem_on: "trem/on",
        intensity: "trem/depth",
        speed: "trem/rate",
        division: "trem/division",
        sync: "trem/sync",
        mix: "spring/level",
        tone: "spring/tone",
        decay: "spring/decay",
        verb_on: "spring/on",
        lfo: "trem/lfo",
        rig: true,
    }
}

/// The pre effects: the Aqua-Puss on the Pre Delay; the Gravity Tank on
/// the Pre Verb, its tremolo half on the Pre Motion.
fn pre_entries() -> Vec<Face> {
    let d = |f: Face| Face { plugin: PLUGIN, ..f };
    // The rig row's tank: the suite's spring tank, compact, the knobs as big
    // as a row allows.
    // The Gravity Tank as two units, each loadable on its own: its
    // harmonic tremolo on a motion block, its spring on a reverb block.
    use crate::suite_studio_blues::tank::{TankHalf, tank_unit};
    let _ = TankHalf::Spring;
    let trem = tank_unit(&tank_bind(), TankHalf::Tremolo, "18-gravity-tank-trem", 360.0, 300.0);
    // The spring: frame's spring reverb, trimmed to a rig row.
    let spring = crate::reverb::spring::rig_face("19-spring-reverb", PLUGIN, 560.0, 330.0);
    vec![d(drives::aqua_puss()), trem, spring]
}

fn d_(f: Face) -> Face {
    Face { plugin: PLUGIN, ..f }
}

/// Every face in the set.
#[must_use]
pub fn faces() -> Vec<Face> {
    let mut v: Vec<Face> = drive_entries().into_iter().map(|(f, _, _)| f).collect();
    v.extend(pre_entries());
    v.push(trio());
    v.extend(AmpModel::EVERY.iter().map(|m| amp_face(*m)));
    v.push(gate());
    v.push(eq());
    v.push(spring_viz());
    v.extend([d_(drives::cry_baby()), d_(drives::q_tron()), d_(drives::filter_viz()), d_(drives::pitch_voices())]);
    v
}

/// `faces.json`: which face draws which block.
#[must_use]
pub fn manifest() -> String {
    let size = |f: &Face| {
        let (w, h) = face_size(f);
        json!([w.round(), h.round()])
    };
    let drives: Vec<_> = drive_entries().into_iter().map(|(f, ns, m)| json!({ "match": m, "face": f.file, "ns": ns, "size": size(&f) })).collect();
    // The trio first: its capture's name mentions all three amps
    // (`a+b` = every word).
    let t = trio();
    let mut amps = vec![json!({ "match": ["vibroverb+dumble+two-rock"], "face": t.file, "ns": "trio", "name": "JOHN MAYER TRIO", "size": size(&t) })];
    amps.extend(AmpModel::EVERY.iter().map(|m| {
        let f = amp_face(*m);
        json!({ "match": amp_match(*m), "face": f.file, "ns": ns(*m), "name": m.short(), "size": size(&f) })
    }));
    let pre = pre_entries();
    let pre = vec![
        json!({ "match": ["aqua"], "face": pre[0].file, "ns": "aquapuss", "size": size(&pre[0]) }),
        // The Gravity Tank's two halves, each by its block preset's name.
        json!({ "match": ["gravity tank trem", "harmonic trem"], "face": pre[1].file, "ns": "trem", "size": size(&pre[1]) }),
        // Any spring on a reverb block ("Gravity Tank Spring", "Fender
        // Spring"): frame's spring reverb.
        json!({ "match": ["spring"], "face": pre[2].file, "ns": "spring", "size": size(&pre[2]) }),
    ];
    let doc = json!({
        "drives": drives,
        "pre": pre,
        "amps": amps,
        "gate": { "face": "51-gate", "ns": "gate", "size": size(&gate()) },
        "eq": { "face": "52-amp-eq", "ns": "eq", "size": size(&eq()) },
        // The input stage's units, by block type, and the filter's picture.
        "input": [
            { "match": ["wah"], "face": "30-wah-cry-baby", "ns": "wah", "size": size(&drives::cry_baby()) },
            { "match": ["filter"], "face": "31-filter-q-tron", "ns": "qtron", "size": size(&drives::q_tron()) },
            { "match": ["pitch"], "face": "33-pitch-voices", "ns": "pitch", "size": size(&drives::pitch_voices()) },
        ],
        "filter_picture": { "face": "32-filter-viz", "ns": "qtron", "size": size(&drives::filter_viz()) },
        // Pictures of a block, beside its unit: a reverb's springs.
        "pictures": [ { "match": ["reverb"], "face": "53-spring-viz", "ns": "spring", "size": size(&spring_viz()) } ],
    });
    serde_json::to_string_pretty(&doc).expect("json")
}
