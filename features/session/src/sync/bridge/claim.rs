//! One bridge per project in a process.
//!
//! A bridge applies its doc onto its engine project and writes the project
//! back into its doc, through a map of which doc FX and item is which
//! engine one ([`super::EngineIds`]) that only it knows. Two bridges on one
//! project rewrite each other: each takes what the other made for new and
//! makes it again, and a song's FX chains doubled every round (#316). So
//! a bridge holds a [`Claim`] on its project for as long as it lives, and
//! a second is refused.

use std::collections::HashSet;
use std::sync::{Mutex, PoisonError};

/// A project, as the claims know it: the engine it is in (its
/// [`daw_standalone::sync::Standalone::engine_id`], or 0 for an engine
/// reached only through its facade) and its guid there.
type Key = (usize, String);

static CLAIMED: Mutex<Option<HashSet<Key>>> = Mutex::new(None);

/// A second bridge was asked for on a project that has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlreadyBridged {
    /// The project's guid.
    pub project: String,
}

impl std::fmt::Display for AlreadyBridged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "project {} already has a bridge in this process: a second would rewrite it against the first",
            self.project
        )
    }
}

impl std::error::Error for AlreadyBridged {}

/// This bridge's hold on its project, let go when it drops.
#[derive(Debug)]
pub struct Claim {
    key: Key,
}

impl Claim {
    /// Hold project `guid` of engine `engine`.
    ///
    /// # Errors
    /// Another bridge holds it.
    pub fn take(engine: usize, guid: &str) -> Result<Self, AlreadyBridged> {
        let key = (engine, guid.to_owned());
        let mut claimed = CLAIMED.lock().unwrap_or_else(PoisonError::into_inner);
        if !claimed.get_or_insert_with(HashSet::new).insert(key.clone()) {
            drop(claimed);
            tracing::error!(
                project.guid = %guid,
                bridge.engine = engine,
                "a second bridge on one project was refused"
            );
            return Err(AlreadyBridged {
                project: guid.to_owned(),
            });
        }
        Ok(Self { key })
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        let mut claimed = CLAIMED.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(set) = claimed.as_mut() {
            set.remove(&self.key);
        }
    }
}

/// Which engine `direct` is, for a claim: 0 when the bridge reaches it
/// only through its facade.
pub fn engine_of(direct: Option<&dyn super::Direct>) -> usize {
    direct
        .and_then(super::Direct::standalone)
        .map_or(0, daw_standalone::sync::Standalone::engine_id)
}
