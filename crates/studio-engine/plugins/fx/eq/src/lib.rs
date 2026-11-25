//! FTS EQ: Signal's `eq` block family as its own CLAP/VST3 (signal#26).
//!
//! The same family Signal compiles in: it loads in any DAW, inside Signal
//! as the block's development override, and nulls against the compiled-in
//! block. Its parameters are `family_clap`'s control slots, named by the
//! loaded algorithm.

family_clap::family_clap!(
    FtsEq,
    name: "FTS EQ",
    family: "eq",
    clap_id: "com.fasttrackstudio.eq",
    vst3_id: b"FtsEqPlugin00001",
    description: "The FTS EQ: Signal's EQ block. Its algorithms (the FTS EQ's own parametric face, a channel strip, console faces, community ones) run on the FTS EQ engine."
);
