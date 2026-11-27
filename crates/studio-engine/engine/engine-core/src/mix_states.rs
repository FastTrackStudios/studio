//! The plugin states a song's mixes keep, and the sweep that drops the
//! ones no mix names any more.
//!
//! Mix snapshots and variants (`engine-mix`) keep each plugin state once,
//! in the song's `fts.mix` ext section under `state:<hash>`, and name it
//! in the mix as `@<hash>`. A snapshot replaced, or a variant group
//! deleted, leaves the states it alone named behind; [`sweep`] (run as the
//! song is saved, `crate::project_file::save`) drops them.

use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard, PoisonError};

use daw_proto::{ExtState, ProjectContext};

/// The ext section a song's mixes live in.
pub const SECTION: &str = "fts.mix";

/// The prefix of a kept plugin state's key.
const STATE_PREFIX: &str = "state:";

/// How many hex digits a state's hash has (128 bits).
pub const HASH_LEN: usize = 32;

/// A plugin state, kept once in the section by what it is: `state:<hash>`.
#[must_use]
pub fn state_key(hash: &str) -> String {
    format!("{STATE_PREFIX}{hash}")
}

/// What names a state in a mix, ahead of its hash. Base64 never starts
/// with it, so a state kept inline is told apart.
pub const REF: char = '@';

/// A state's hash: half of BLAKE3 ([`HASH_LEN`] hex digits, 128 bits).
#[cfg(feature = "native")]
#[must_use]
pub fn hash(state: &str) -> String {
    blake3::hash(state.as_bytes())
        .to_hex()
        .chars()
        .take(HASH_LEN)
        .collect()
}

/// Whether a mix keeps `state` inline, not by reference: what [`compact`]
/// and a mix's encoding move into the section.
#[must_use]
pub fn is_inline(state: &str) -> bool {
    !state.is_empty() && !state.starts_with(REF)
}

/// The mixes in `ext`'s [`SECTION`] with the states they keep inline moved
/// onto references, each state kept once: what an older song is tidied
/// with as it opens (`crate::project_file`). How many states it moved.
///
/// It works on the mixes' JSON as text: a state is `"state":"<base64>"`,
/// and base64 holds no quote or backslash, so the value ends at the next
/// quote.
#[cfg(feature = "native")]
pub fn compact(
    ext: &mut std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
) -> usize {
    const FIELD: &str = "\"state\":\"";
    let Some(section) = ext.get_mut(SECTION) else {
        return 0;
    };
    let mut kept = std::collections::BTreeMap::new();
    let mut moved = 0_usize;
    for (key, text) in section.iter_mut() {
        if hash_of_key(key).is_some() || !text.contains(FIELD) {
            continue;
        }
        let mut out = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(at) = rest.find(FIELD) {
            let (before, after) = rest.split_at(at.saturating_add(FIELD.len()));
            out.push_str(before);
            let end = after.find('"').unwrap_or(after.len());
            let (state, tail) = after.split_at(end);
            if is_inline(state) {
                let hash = hash(state);
                out.push(REF);
                out.push_str(&hash);
                kept.entry(state_key(&hash))
                    .or_insert_with(|| state.to_owned());
                moved = moved.saturating_add(1);
            } else {
                out.push_str(state);
            }
            rest = tail;
        }
        out.push_str(rest);
        *text = out;
    }
    for (key, state) in kept {
        section.entry(key).or_insert(state);
    }
    moved
}

/// The hash a section key keeps a plugin state under, if it keeps one.
#[must_use]
pub fn hash_of_key(key: &str) -> Option<&str> {
    key.strip_prefix(STATE_PREFIX)
}

/// Held while a mix is kept (its states stored, then the mix naming
/// them) and while [`sweep`] runs, so a sweep never falls between the
/// two and drops a state the mix is about to name.
static WRITING: Mutex<()> = Mutex::new(());

/// Hold off [`sweep`] until the guard goes: take it before storing a
/// mix's states, drop it once the mix that names them is written.
pub fn writing() -> MutexGuard<'static, ()> {
    WRITING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Every hash `text` names as a kept state: `@` and [`HASH_LEN`] hex
/// digits, wherever in a mix it is (a snapshot, a variant, a group's base).
fn named_in(text: &str, into: &mut HashSet<String>) {
    for (at, _) in text.match_indices('@') {
        let hash = &text[at + 1..];
        let Some(hash) = hash.get(..HASH_LEN) else {
            continue;
        };
        let ends = text[at + 1 + HASH_LEN..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_hexdigit());
        if ends && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            into.insert(hash.to_owned());
        }
    }
}

/// The kept hashes (`state:` keys among `keys`) that no other key's value
/// names, sorted. `value` reads a key's value.
fn unnamed(keys: &[String], value: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let mut named = HashSet::new();
    let mut kept = Vec::new();
    for key in keys {
        match hash_of_key(key) {
            Some(hash) => kept.push(hash.to_owned()),
            None => {
                if let Some(text) = value(key) {
                    named_in(&text, &mut named);
                }
            }
        }
    }
    kept.retain(|hash| !named.contains(hash));
    kept.sort();
    kept
}

/// Drop every plugin state in the song's mix section that no mix in it
/// names. The hashes dropped, sorted.
///
/// Anything else kept in the section (a snapshot, a variant, a group's
/// base, the head) may name a state; only `state:` keys are ever dropped.
///
/// # Errors
/// A state would not be deleted (those before it are gone).
pub fn sweep<D: ExtState + ?Sized>(
    daw: &D,
    project: &ProjectContext,
) -> Result<Vec<String>, String> {
    let _writing = writing();
    let keys = daw.keys_project(project.clone(), SECTION);
    let dropped = unnamed(&keys, |key| daw.get_project(project.clone(), SECTION, key));
    for hash in &dropped {
        daw.delete_project(project.clone(), SECTION, &state_key(hash))
            .map_err(|e| format!("plugin state {hash}: {e}"))?;
    }
    Ok(dropped)
}

#[cfg(test)]
mod tests;
