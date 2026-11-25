//! FTS Saturate: Signal's `drive` block family as its own CLAP/VST3 (signal#26).
//!
//! The same family Signal compiles in: it loads in any DAW, inside Signal
//! as the block's development override, and nulls against the compiled-in
//! block. Its parameters are `family_clap`'s control slots, named by the
//! loaded algorithm.

family_clap::family_clap!(
    FtsSaturate,
    name: "FTS Saturate",
    family: "drive",
    clap_id: "com.fasttrackstudio.saturate",
    vst3_id: b"FtsSaturatePlg01",
    description: "FTS Saturate: Signal's drive block."
);
