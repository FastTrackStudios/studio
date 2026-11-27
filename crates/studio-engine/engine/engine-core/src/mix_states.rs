//! A mix's plugin states, kept once by what they are.
//!
//! A mix snapshot or variant (`engine_mix::mix_snapshot`, kept in the
//! song's ext state, section [`SECTION`]) names each FX's state by its
//! hash (`@<hash>`), the state kept once in the section under
//! [`state_key`]. One song's 54 snapshots held 280 MB of plugin state, 8 MB
//! of it distinct. The format is here, below both its users: the mix
//! actions that write it and the open that moves an older song onto it
//! ([`compact`]).

use std::collections::BTreeMap;

/// The ext section the mixes and their states are kept in.
pub const SECTION: &str = "fts.mix";

/// What names a state in a mix, ahead of its hash. Base64 never starts
/// with it, so a state kept inline is told apart.
pub const REF: char = '@';

/// A state's hash: half of BLAKE3 (128 bits), hex.
#[must_use]
pub fn hash(state: &str) -> String {
    blake3::hash(state.as_bytes())
        .to_hex()
        .chars()
        .take(32)
        .collect()
}

/// Where a state is kept in [`SECTION`].
#[must_use]
pub fn state_key(hash: &str) -> String {
    format!("state:{hash}")
}

/// The mixes in `ext`'s [`SECTION`] with their states inline moved onto
/// references, each state kept once. How many states it moved.
///
/// It works on the mixes' JSON as text: a state is `"state":"<base64>"`,
/// and base64 holds no quote or backslash, so the value ends at the next
/// quote.
pub fn compact(ext: &mut BTreeMap<String, BTreeMap<String, String>>) -> usize {
    const FIELD: &str = "\"state\":\"";
    let Some(section) = ext.get_mut(SECTION) else {
        return 0;
    };
    let mut kept: BTreeMap<String, String> = BTreeMap::new();
    let mut moved = 0_usize;
    for (key, text) in section.iter_mut() {
        if key.starts_with("state:") || !text.contains(FIELD) {
            continue;
        }
        let mut out = String::with_capacity(text.len().min(64 * 1024));
        let mut rest = text.as_str();
        while let Some(at) = rest.find(FIELD) {
            let (before, after) = rest.split_at(at.saturating_add(FIELD.len()));
            out.push_str(before);
            let end = after.find('"').unwrap_or(after.len());
            let (state, tail) = after.split_at(end);
            if state.is_empty() || state.starts_with(REF) {
                out.push_str(state);
            } else {
                let h = hash(state);
                out.push(REF);
                out.push_str(&h);
                kept.entry(state_key(&h))
                    .or_insert_with(|| state.to_owned());
                moved = moved.saturating_add(1);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_states_become_references_kept_once() {
        let state = "QUJD".repeat(100);
        let mix = format!(
            r#"{{"name":"A","fx":[{{"guid":"a","state":"{state}"}},{{"guid":"b","state":"{state}"}},{{"guid":"c","state":""}}]}}"#
        );
        let mut ext = BTreeMap::from([(
            SECTION.to_owned(),
            BTreeMap::from([
                ("snapshot:A".to_owned(), mix.clone()),
                ("snapshot:B".to_owned(), mix),
                ("head".to_owned(), "A".to_owned()),
            ]),
        )]);
        assert_eq!(compact(&mut ext), 4);
        let section = &ext[SECTION];
        let h = hash(&state);
        assert_eq!(section[&state_key(&h)], state);
        assert_eq!(
            section["snapshot:A"],
            format!(
                r#"{{"name":"A","fx":[{{"guid":"a","state":"@{h}"}},{{"guid":"b","state":"@{h}"}},{{"guid":"c","state":""}}]}}"#
            )
        );
        assert_eq!(section["head"], "A");
        assert_eq!(compact(&mut ext), 0, "once is enough");
    }
}
