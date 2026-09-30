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
    let mut v = delay::faces();
    v.extend(reverb::faces());
    v
}

/// `faces.json`'s `time` entry: each algorithm's face, by its name.
#[must_use]
pub fn manifest() -> serde_json::Value {
    let size = [kit::W, kit::H];
    let delays: Vec<_> = kit::DELAY_ALGOS
        .iter()
        .zip(delay::faces())
        .map(|(name, f)| json!({ "match": [name.to_lowercase()], "face": f.file, "ns": kit::DLY, "size": size }))
        .collect();
    let reverbs: Vec<_> = kit::VERB_ALGOS
        .iter()
        .zip(reverb::faces())
        .map(|(name, f)| json!({ "match": [name.to_lowercase()], "face": f.file, "ns": kit::VERB, "size": size }))
        .collect();
    json!({ "delay": delays, "reverb": reverbs })
}
