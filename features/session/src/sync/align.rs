//! What differs between two models, and what is only named differently.
//!
//! An engine mints its own guid for every FX and MIDI item it makes, so
//! the same song read from two engines — or from one engine by a bridge
//! that did not build it — names those under ids the doc does not know.
//! A diff would call each one removed and another added, and the engine
//! would make it all again. [`renames`] finds them instead: an FX in the
//! same slot of the same chain, or an item on the same track at the same
//! place, equal in everything but its id. Renaming them
//! ([`Renames::apply`]) leaves only what really differs.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::sync::model::{EnvelopeState, FxState, ItemState, SessionModel};

/// FX and items of one model that are another's under other ids: each
/// pair is (the id it has, the id the other gives it).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Renames {
    pub fx: Vec<(String, String)>,
    pub items: Vec<(String, String)>,
}

impl Renames {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.fx.is_empty() && self.items.is_empty()
    }

    /// `model` with every pair renamed: each FX's guid and the envelopes
    /// that name it (`fx:<guid>:<param>`), each item's key.
    pub fn apply(&self, model: &mut SessionModel) {
        let fx: HashMap<&str, &str> = self
            .fx
            .iter()
            .map(|(from, to)| (from.as_str(), to.as_str()))
            .collect();
        if !fx.is_empty() {
            for track in &mut model.tracks {
                rename_chain(&mut track.fx, &fx);
                rename_chain(&mut track.input_fx, &fx);
                rename_envelopes(&mut track.envelopes, &fx);
            }
            rename_chain(&mut model.project.master.fx, &fx);
            rename_chain(&mut model.project.monitoring_fx, &fx);
            rename_envelopes(&mut model.project.master.envelopes, &fx);
        }
        for (from, to) in &self.items {
            if let Some(item) = model.items.remove(from) {
                model.items.insert(to.clone(), item);
            }
        }
    }
}

/// What of `shown` is `target`'s under other ids (see the module docs).
/// An FX pairs with the one in its slot when the two are equal but for
/// their guid and the target's guid is not one `shown` already uses; an
/// item pairs with an equal one of the target's on its track at its
/// position that `shown` does not have, each used once.
#[must_use]
pub fn renames(shown: &SessionModel, target: &SessionModel) -> Renames {
    Renames {
        fx: fx_renames(shown, target),
        items: item_renames(&shown.items, &target.items),
    }
}

/// Every FX chain of `model`, by where it is: each track's two, the
/// master's, the monitoring chain.
fn chains(model: &SessionModel) -> Vec<(String, &[FxState])> {
    let mut chains = Vec::with_capacity(model.tracks.len() * 2 + 2);
    for track in &model.tracks {
        chains.push((format!("{}/fx", track.guid), track.fx.as_slice()));
        chains.push((format!("{}/input", track.guid), track.input_fx.as_slice()));
    }
    chains.push(("master".to_owned(), model.project.master.fx.as_slice()));
    chains.push((
        "monitoring".to_owned(),
        model.project.monitoring_fx.as_slice(),
    ));
    chains
}

fn fx_renames(shown: &SessionModel, target: &SessionModel) -> Vec<(String, String)> {
    let shown_chains = chains(shown);
    let taken: HashSet<&str> = shown_chains
        .iter()
        .flat_map(|(_, chain)| chain.iter().map(|fx| fx.guid.as_str()))
        .collect();
    let target_chains: HashMap<String, &[FxState]> = chains(target).into_iter().collect();
    let mut pairs = Vec::new();
    for (place, chain) in &shown_chains {
        let Some(theirs) = target_chains.get(place) else {
            continue;
        };
        for (mine, theirs) in chain.iter().zip(theirs.iter()) {
            if mine.guid != theirs.guid
                && !taken.contains(theirs.guid.as_str())
                && same_but_guid(mine, theirs)
            {
                pairs.push((mine.guid.clone(), theirs.guid.clone()));
            }
        }
    }
    pairs
}

fn same_but_guid(a: &FxState, b: &FxState) -> bool {
    FxState {
        guid: b.guid.clone(),
        ..a.clone()
    } == *b
}

fn item_renames(
    shown: &BTreeMap<String, ItemState>,
    target: &BTreeMap<String, ItemState>,
) -> Vec<(String, String)> {
    // The target's items `shown` has no id for, by track and position.
    let mut unmatched: HashMap<(&str, u64), Vec<&str>> = HashMap::new();
    for (id, item) in target {
        if !shown.contains_key(id) {
            unmatched
                .entry((item.track.as_str(), item.position.to_bits()))
                .or_default()
                .push(id.as_str());
        }
    }
    if unmatched.is_empty() {
        return Vec::new();
    }
    let mut pairs = Vec::new();
    for (id, item) in shown {
        if target.contains_key(id) {
            continue;
        }
        let Some(candidates) = unmatched.get_mut(&(item.track.as_str(), item.position.to_bits()))
        else {
            continue;
        };
        if let Some(at) = candidates
            .iter()
            .position(|theirs| target.get(*theirs) == Some(item))
        {
            let theirs = candidates.remove(at);
            pairs.push((id.clone(), theirs.to_owned()));
        }
    }
    pairs
}

fn rename_chain(chain: &mut [FxState], renames: &HashMap<&str, &str>) {
    for fx in chain {
        if let Some(to) = renames.get(fx.guid.as_str()) {
            (*to).clone_into(&mut fx.guid);
        }
    }
}

/// Envelope keys naming a renamed FX (`fx:<guid>:<param>`), renamed.
fn rename_envelopes(
    envelopes: &mut BTreeMap<String, EnvelopeState>,
    renames: &HashMap<&str, &str>,
) {
    let renamed: Vec<(String, String)> = envelopes
        .keys()
        .filter_map(|key| {
            let (fx, param) = key.strip_prefix("fx:")?.rsplit_once(':')?;
            let to = renames.get(fx)?;
            Some((key.clone(), format!("fx:{to}:{param}")))
        })
        .collect();
    for (from, to) in renamed {
        if let Some(envelope) = envelopes.remove(&from) {
            envelopes.insert(to, envelope);
        }
    }
}

#[cfg(test)]
mod tests;
