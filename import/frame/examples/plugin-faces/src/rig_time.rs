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
    v.extend(reverb::phone_faces(true).into_iter().map(|(_, f)| f));
    v
}

/// `faces.json`'s `time` entry: each algorithm's face, by its name.
#[must_use]
pub fn manifest() -> serde_json::Value {
    let size = [kit::W, kit::H];
    let delays: Vec<_> = kit::DELAY_ALGOS
        .iter()
        .zip(delay::faces(false).into_iter().zip(delay::faces(true)))
        .map(|(name, (f, d))| json!({ "match": [name.to_lowercase()], "face": f.file, "dark": d.file, "ns": kit::DLY, "size": size }))
        .collect();
    let reverbs: Vec<_> = kit::VERB_ALGOS
        .iter()
        .zip(reverb::faces(false).into_iter().zip(reverb::faces(true)))
        .enumerate()
        .map(|(i, (name, (f, d)))| {
            let mut e = json!({ "match": [name.to_lowercase()], "face": f.file, "dark": d.file, "ns": kit::VERB, "size": size });
            // A face with the nameplate prints its algorithm and preset
            // itself: the host fills the preset and answers its presses.
            if f.params.iter().any(|p| p.addr.to_string().ends_with("/preset_browse")) {
                e["nameplate"] = json!(true);
            }
            // Its versions for other sizes (`tiers`: phone, ipad, desktop,
            // fullscreen, strip), where it has them — the app draws the one
            // for the room it is given, the base face otherwise.
            let light = reverb::phone_faces(false).into_iter().find(|(at, _)| *at == i);
            let dark = reverb::phone_faces(true).into_iter().find(|(at, _)| *at == i);
            if let (Some((_, pl)), Some((_, pd))) = (light, dark) {
                e["tiers"] = json!({ "phone": { "face": pl.file, "dark": pd.file, "size": [kit::Tier::Phone.lane_w(), kit::H] } });
            }
            e
        })
        .collect();
    json!({ "delay": delays, "reverb": reverbs })
}
