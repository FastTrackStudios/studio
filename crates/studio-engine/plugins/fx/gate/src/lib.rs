//! FTS Gate: Signal's `gate` block family as its own CLAP/VST3 (signal#26).
//!
//! The same family Signal compiles in: it loads in any DAW, inside Signal
//! as the block's development override, and nulls against the compiled-in
//! block. Its parameters are `family_clap`'s control slots, named by the
//! loaded algorithm.

family_clap::family_clap!(
    FtsGate,
    name: "FTS Gate",
    family: "gate",
    clap_id: "com.fasttrackstudio.gate",
    vst3_id: b"FtsGatePlugin001",
    description: "FTS Gate: Signal's gate block."
);
