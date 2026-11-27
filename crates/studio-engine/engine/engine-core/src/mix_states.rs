//! The plugin states a song's mixes keep, and the sweep that drops the
//! ones no mix names any more.
//!
//! Mix snapshots and variants (`engine-mix`) keep each plugin state once,
//! in the song's `fts.mix` ext section under `state:<hash>`, and name it
//! in the mix as `@<hash>`. A snapshot replaced, or a variant group
//! deleted, leaves the states it alone named behind; [`sweep`] (run as the
//! song is saved, `crate::project_file::save`) drops them.

use std::collections::HashSet;

use daw_proto::{ExtState, ProjectContext};

/// The ext section a song's mixes live in.
pub const SECTION: &str = "fts.mix";

/// The prefix of a kept plugin state's key.
const STATE: &str = "state:";

/// How many hex digits a state's hash has (128 bits).
pub const HASH_LEN: usize = 32;

/// A plugin state, kept once in the section by what it is: `state:<hash>`.
#[must_use]
pub fn state_key(hash: &str) -> String {
    format!("{STATE}{hash}")
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

/// Drop every plugin state in the song's mix section that no mix in it
/// names. The hashes dropped, sorted.
///
/// Anything else kept in the section (a snapshot, a variant group, the
/// head) may name a state; only `state:` keys are ever dropped.
pub fn sweep<D: ExtState + ?Sized>(daw: &D, project: &ProjectContext) -> Vec<String> {
    let keys = daw.keys_project(project.clone(), SECTION);
    let mut named = HashSet::new();
    let mut kept = Vec::new();
    for key in keys {
        match key.strip_prefix(STATE) {
            Some(hash) => kept.push(hash.to_owned()),
            None => {
                if let Some(text) = daw.get_project(project.clone(), SECTION, &key) {
                    named_in(&text, &mut named);
                }
            }
        }
    }
    let mut dropped: Vec<String> = kept
        .into_iter()
        .filter(|hash| !named.contains(hash))
        .filter(|hash| {
            daw.delete_project(project.clone(), SECTION, &state_key(hash))
                .is_ok()
        })
        .collect();
    dropped.sort();
    dropped
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::named_in;

    #[test]
    fn a_mix_names_its_states_by_hash() {
        let a = "0123456789abcdef0123456789abcdef";
        let b = "fedcba9876543210fedcba9876543210";
        let text = format!(
            r#"{{"fx":[{{"state":"@{a}"}},{{"state":"@{b}"}},{{"state":"@{a}0"}},{{"state":"@short"}},{{"name":"x@y"}}]}}"#
        );
        let mut named = HashSet::new();
        named_in(&text, &mut named);
        assert_eq!(named, HashSet::from([a.to_owned(), b.to_owned()]));
    }
}
