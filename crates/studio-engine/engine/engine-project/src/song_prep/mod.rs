//! Songs on this machine, made ready to travel: what a path holds
//! ([`inputs`] — a Live set split into songs, a song folder, `.session`
//! or `.RPP`, a folder of song folders) and each song prepared
//! ([`prepare`] — the project file folded whole, the proxies and peaks of
//! what it plays, its reference), each step kept when it is already there
//! and current. What `link-prepare` runs before a song is shared.

pub mod inputs;
pub mod prepare;

#[cfg(test)]
mod tests;
