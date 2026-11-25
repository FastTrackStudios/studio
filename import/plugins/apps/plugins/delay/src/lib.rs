//! FTS Delay: Signal's `delay` block family as its own CLAP/VST3 (signal#26).
//!
//! The same family Signal compiles in: it loads in any DAW, inside Signal
//! as the block's development override, and nulls against the compiled-in
//! block. Its parameters are `family_clap`'s control slots, named by the
//! loaded algorithm.

family_clap::family_clap!(
    FtsDelay,
    name: "FTS Delay",
    family: "delay",
    clap_id: "com.fasttrackstudio.delay",
    vst3_id: b"FtsDelayPlugn001",
    description: "FTS Delay: Signal's delay block, every style its own algorithm (Tape, BBD, Clean, Lo-Fi, ...)."
);
