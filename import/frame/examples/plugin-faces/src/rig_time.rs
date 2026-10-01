//! The rig's time effects: a face for every delay machine and every reverb
//! algorithm of signal's Delay and Reverb blocks, each its own unit — the
//! machine that algorithm is (a tape echo, a bucket-brigade box, a steel
//! plate, a hall processor…), with a live picture of what it is doing and
//! its own controls.
//!
//! Each face fills one lane of signal's Time module: [`kit::W`] ×
//! [`kit::H`], two stacked in the Delay panel (DLY 1, DLY 2) and two in the
//! Reverb panel. A block's `style` (delay) or `algorithm` (reverb) picks the
//! face; switching it on the face's selector swaps the face.
//!
//! Binding: every delay face's params are `dly/<block param>`, every
//! reverb's `verb/<block param>` (see [`kit::delay_params`] /
//! [`kit::reverb_params`]); `<ns>/on` is the block's bypass.

pub mod delay;
pub mod kit;
pub mod reverb;

use serde_json::json;

use crate::face::Face;

/// Every time face.
#[must_use]
pub fn faces() -> Vec<Face> {
    let mut v = delay::faces(false);
    v.extend(delay::faces(true));
    v.extend(reverb::faces(false));
    v.extend(reverb::faces(true));
    v.extend(reverb::phone_faces(false).into_iter().map(|(_, f)| f));
    v.extend(delay::phone_faces(false).into_iter().map(|(_, f)| f));
    v.extend(delay::phone_faces(true).into_iter().map(|(_, f)| f));
    for t in [kit::Tier::Desktop, kit::Tier::Phone] {
        for dark in [false, true] {
            v.extend(reverb::back_faces(dark, t).into_iter().map(|(_, f)| f));
            v.extend(delay::back_faces(dark, t).into_iter().map(|(_, f)| f));
        }
    }
    v.extend(reverb::phone_faces(true).into_iter().map(|(_, f)| f));
    v
}

/// `faces.json`'s `time` entry: each algorithm's face, by its name.
#[must_use]
/// A Time face's manifest entry: `e`, and what `f` carries — the
/// nameplate (the host fills the preset and answers its presses), the Time
/// knob (time, sync, time_text, the host maps), and its versions for other
/// rooms (`tiers`: the app draws the one for the room it is given).
fn entry(mut e: serde_json::Value, f: &Face, phone: Option<(usize, Face)>, phone_dark: Option<(usize, Face)>, backs: impl Fn(bool, kit::Tier) -> Option<Face>) -> serde_json::Value {
    if f.params.iter().any(|p| p.addr.to_string().ends_with("/preset_browse")) {
        e["nameplate"] = json!(true);
    }
    if f.params.iter().any(|p| p.addr.to_string().ends_with("/time_text")) {
        e["time_knob"] = json!(true);
    }
    // Its back (the advanced controls), where it has one: the host turns
    // the face over to it on the flip arrow.
    if let (Some(bl), Some(bd)) = (backs(false, kit::Tier::Desktop), backs(true, kit::Tier::Desktop)) {
        e["back"] = json!(bl.file);
        e["back_dark"] = json!(bd.file);
    }
    if let (Some((_, pl)), Some((_, pd))) = (phone, phone_dark) {
        let mut t = json!({ "face": pl.file, "dark": pd.file, "size": [kit::Tier::Phone.lane_w(), kit::H] });
        if let (Some(bl), Some(bd)) = (backs(false, kit::Tier::Phone), backs(true, kit::Tier::Phone)) {
            t["back"] = json!(bl.file);
            t["back_dark"] = json!(bd.file);
        }
        e["tiers"] = json!({ "phone": t });
    }
    e
}

pub fn manifest() -> serde_json::Value {
    let size = [kit::W, kit::H];
    let delays: Vec<_> = kit::DELAY_ALGOS
        .iter()
        .zip(delay::faces(false).into_iter().zip(delay::faces(true)))
        .enumerate()
        .map(|(i, (name, (f, d)))| entry(json!({ "match": [name.to_lowercase()], "face": f.file, "dark": d.file, "ns": kit::DLY, "size": size }), &f, delay::phone_faces(false).into_iter().find(|(at, _)| *at == i), delay::phone_faces(true).into_iter().find(|(at, _)| *at == i), |dark, t| delay::back_faces(dark, t).into_iter().find(|(at, _)| *at == i).map(|(_, f)| f)))
        .collect();
    let reverbs: Vec<_> = kit::VERB_ALGOS
        .iter()
        .zip(reverb::faces(false).into_iter().zip(reverb::faces(true)))
        .enumerate()
        .map(|(i, (name, (f, d)))| entry(json!({ "match": [name.to_lowercase()], "face": f.file, "dark": d.file, "ns": kit::VERB, "size": size }), &f, reverb::phone_faces(false).into_iter().find(|(at, _)| *at == i), reverb::phone_faces(true).into_iter().find(|(at, _)| *at == i), |dark, t| reverb::back_faces(dark, t).into_iter().find(|(at, _)| *at == i).map(|(_, f)| f)))
        .collect();
    json!({ "delay": delays, "reverb": reverbs })
}
