//! FTS Comp: Signal's `comp` block family as its own CLAP/VST3 (signal#26).
//!
//! The same family Signal compiles in: it loads in any DAW, inside Signal
//! as the block's development override, and nulls against the compiled-in
//! block. Its parameters are `family_clap`'s control slots, named by the
//! loaded algorithm.

family_clap::family_clap!(
    FtsComp,
    name: "FTS Comp",
    family: "comp",
    clap_id: "com.fasttrackstudio.comp",
    vst3_id: b"FtsCompPlugin001",
    description: "FTS Comp: Signal's compressor block."
);
