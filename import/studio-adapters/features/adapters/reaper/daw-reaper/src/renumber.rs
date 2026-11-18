//! Telling a renumbering apart from a deletion.
//!
//! Markers and regions are one list in REAPER wearing one set of
//! numbers, and "Renumber markers/regions in timeline order" reassigns
//! them all at once. Diffed by number, that reads as the whole list
//! being deleted and a set of strangers arriving in its place — so a
//! client loses its selection, and a client mid-drag moves whatever now
//! wears the number it was holding.
//!
//! Pulled out of the two pollers so the rule has somewhere to be
//! tested. Driving REAPER's own renumber action from a test turned out
//! to be a bad way to ask: the action never returned and the run sat
//! through its whole timeout. The rule is arithmetic on two maps, and
//! it can be asked directly.

use std::collections::HashMap;

/// Pair numbers that vanished with numbers that appeared.
///
/// A pair is a removal and an addition that `same` considers identical
/// — everything a user can see, the number excepted. Two markers alike
/// in name, position, colour and lane are indistinguishable to a user
/// as well, so choosing the wrong one of those cannot produce a wrong
/// answer.
///
/// Returns old number → new number. Anything unpaired is a real
/// addition or a real removal and is reported as one.
pub fn pair<T>(
    previous: &HashMap<u32, T>,
    fresh: &HashMap<u32, T>,
    same: impl Fn(&T, &T) -> bool,
) -> HashMap<u32, u32> {
    let gone: Vec<u32> = previous
        .keys()
        .copied()
        .filter(|id| !fresh.contains_key(id))
        .collect();
    let mut arrived: Vec<u32> = fresh
        .keys()
        .copied()
        .filter(|id| !previous.contains_key(id))
        .collect();
    // Both lists are what changed in one 33 ms tick, so they are short
    // even when the whole project was renumbered — REAPER does that in
    // one action, and one action lands in one tick.
    arrived.sort_unstable();

    let mut paired = HashMap::new();
    for old_id in gone {
        let Some(old) = previous.get(&old_id) else {
            continue;
        };
        let found = arrived
            .iter()
            .position(|new_id| fresh.get(new_id).is_some_and(|new| same(old, new)));
        if let Some(at) = found {
            paired.insert(old_id, arrived.remove(at));
        }
    }
    paired
}
