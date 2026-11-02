//! Frame's editor on the figcraft engine (`Session`, the command registry, pointer tools).
//!
//! The engine is the fork's slim build (`default-features = false`): no PDF export, no wasm
//! plugins, no font downloads, no VectorDrawable. See `NOTICE` and the fork's `FORK.md`.

pub use figcraft_engine as engine;
pub use frame_doc as doc;

pub mod theme;
