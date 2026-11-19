//! What the launcher shows, read from the vault's notes: setlists in
//! their sets, each song with what it is ready with.
//!
//! The model is `setlists::rows`: one reading of sets and songs for the
//! launcher on a desk and the dashboard in a browser
//! (`apps/architect/island`, view `home`). Pure: rows in (a vault
//! query's answer), values out; nothing of it touches the host or the
//! disk. What differs between the two is what "open" does.

pub use setlists::rows::*;
