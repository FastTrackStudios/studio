//! FTS Reverb: Signal's `reverb` block family as its own CLAP/VST3 (signal#26).
//!
//! The same family Signal compiles in: it loads in any DAW, inside Signal
//! as the block's development override, and nulls against the compiled-in
//! block. Its parameters are `family_clap`'s control slots, named by the
//! loaded algorithm.

family_clap::family_clap!(
    FtsReverb,
    name: "FTS Reverb",
    family: "reverb",
    clap_id: "com.fasttrackstudio.reverb",
    vst3_id: b"FtsReverbPlug001",
    description: "FTS Reverb: Signal's reverb block, every space its own algorithm (Plate, Hall, Room, Spring, Shimmer, ...)."
);
