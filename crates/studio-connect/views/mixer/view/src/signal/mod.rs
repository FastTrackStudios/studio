//! What is inside a Signal FX, block by block, as if each were an insert.
//!
//! A track running Signal holds one CLAP; its strip shows that CLAP's
//! phases (each a bar with its bypass and make-up) and its blocks (each a
//! row with its name, kind, switch, status and key source), and a block
//! opens to its parameters and meters. The mixer reads only the data
//! contract, `signal.chain/1` ([`chain`]), from a [`SignalSource`] the
//! engine installs ([`install`]); it never links Signal's crates, so the
//! same rows can be drawn by Signal's own editor in another DAW.
//!
//! - values: `FX_SET` on each parameter's `clap_id` slot
//!   ([`SignalSource::fx_set`]);
//! - structure: `SIGNAL_RUN` (`SIGNAL_SET`, `SIGNAL_BLOCK_ADD`,
//!   `SIGNAL_BLOCK_REMOVE`, `SIGNAL_BLOCK_MOVE`; [`source::Edit`]);
//! - live: each chain is read again when its revision moves, the meters
//!   ten times a second, drawn over each strip's memoized rows.
//!
//! The mixer's own view action is `SIGNAL_INSERTS` (open or shut a
//! track's Signal FX); a block's close-up is `SESSION_CLOSEUP {track,
//! block}`.

pub mod chain;
pub mod closeup;
#[cfg(feature = "native")]
pub mod engine;
pub mod fixtures;
pub mod paint;
pub mod source;
pub mod view;

#[cfg(test)]
mod tests;

use std::sync::{Arc, RwLock};

use architect::action::{ActionList, ActionMeta, Effect};
use facet::Facet;
use daw_view::actions::meta;

pub use chain::Chain;
pub use source::{Edit, Fixture, ParamValue, SignalSource};
pub use view::{Fold, Inserts};

static SOURCE: RwLock<Option<Arc<dyn SignalSource>>> = RwLock::new(None);

/// Hand the mixer the engine's Signal source: once at start, by whatever
/// wires the mixer to the engine. Mixers built after this show Signal's
/// blocks.
pub fn install(source: Arc<dyn SignalSource>) {
    *SOURCE
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(source);
}

/// The installed source, if any.
#[must_use]
pub fn installed() -> Option<Arc<dyn SignalSource>> {
    SOURCE
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

pub static INSERTS: ActionMeta = meta(
    "SIGNAL_INSERTS",
    "signal_inserts",
    "Signal inserts",
    "Show a track's Signal FX in the mixer as its blocks or as one insert — `track` (its GUID), `open` (true: every phase and block as its own insert; false: Signal as one row; left out: the other way). The strip's Signal header does the same. A block's parameters and meters are `SESSION_CLOSEUP {track, block}`; its switches and make-up are the engine's `SIGNAL_RUN`, its values `FX_SET` on the parameter's `clap_id`.",
    "View",
    "",
);

/// What `SIGNAL_INSERTS` asks.
#[derive(Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct InsertsArg {
    pub track: String,
    #[facet(default)]
    pub open: Option<bool>,
}

/// `SIGNAL_INSERTS`'s asks.
pub static INSERTS_ASKS: std::sync::Mutex<crate::view_actions::Asks<InsertsArg>> =
    std::sync::Mutex::new(crate::view_actions::Asks::new());

fn inserts(a: InsertsArg) -> Result<String, String> {
    if a.track.trim().is_empty() {
        return Err("`track` (its GUID)".to_owned());
    }
    let said = match a.open {
        Some(true) => "opened to its blocks",
        Some(false) => "shut to one insert",
        None => "turned the other way",
    };
    crate::view_actions::post(&INSERTS_ASKS, a);
    Ok(format!(
        "the track's Signal FX {said} on the mixer's next frame"
    ))
}

/// `SIGNAL_INSERTS` on `list`.
///
/// # Errors
/// The list refuses it (a duplicate id).
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&INSERTS, Effect::Mutating, inserts)
}

/// Apply every `SIGNAL_INSERTS` asked since `seen` to `inserts`; `true`
/// when one changed something.
pub fn take_asks(inserts: &mut Inserts, seen: &mut u64) -> bool {
    if !crate::view_actions::pending(&INSERTS_ASKS, *seen) {
        return false;
    }
    let mut changed = false;
    for ask in crate::view_actions::since(&INSERTS_ASKS, seen) {
        changed |= inserts.set_fold(&ask.track, ask.open.map(Fold::of));
    }
    changed
}
