//! FTS Gain Block: Signal's `gain` block as its own CLAP (signal#26).

block_clap::block_clap!(Gain, name: "FTS Gain Block", block: "gain", id: "com.fasttrackstudio.block.gain", description: "Signal's gain block, the same processor Signal compiles in");
