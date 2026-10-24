//! The per-device "lock / don't sync" switch, as a session's task keeps
//! it.
//!
//! Live by default: an edit here goes into the song's doc at once, and an
//! edit from anyone else (a member, an agent) is made in this engine as
//! it arrives. Locked, this device stops doing either: what arrives is
//! held in the doc — still imported, never made in the engine — and what
//! is edited here stays in the engine, not yet in the doc. Unlocking
//! merges them, song by song ([`crate::sync::Bridge::merge_held`]): this
//! device's edits are recorded first, as one step, then the engine is made
//! to match whatever the others did meanwhile. Concurrent edits merge as
//! any two peers' do (field by field in the doc), so an edit here and one
//! from someone else on different things both survive.
//!
//! [`Hold`] only says which songs have something held. It is engine- and
//! transport-free, so the switch's rules are tested on their own.

use std::collections::BTreeSet;

/// Which of a session's songs (by index) have something held while this
/// device is locked.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Hold {
    locked: bool,
    /// Songs edited here since the lock.
    local: BTreeSet<usize>,
    /// Songs others edited since the lock.
    remote: BTreeSet<usize>,
}

/// What a hold kept back, handed over as it is lifted: the songs to merge.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Held {
    /// Every song with something held, from either side, in order.
    pub songs: BTreeSet<usize>,
    /// How many had edits from here.
    pub local: usize,
    /// How many had edits from the others.
    pub remote: usize,
}

impl Held {
    /// Whether nothing was held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.songs.is_empty()
    }
}

impl Hold {
    /// A hold, locked or live from the start (a device remembers which).
    #[must_use]
    pub fn new(locked: bool) -> Self {
        Self {
            locked,
            ..Self::default()
        }
    }

    /// Whether this device is locked.
    #[must_use]
    pub const fn locked(&self) -> bool {
        self.locked
    }

    /// Lock or unlock. Unlocking hands back what was held, to merge;
    /// locking (or switching to what it already is) hands back nothing.
    pub fn set_locked(&mut self, locked: bool) -> Held {
        let was = std::mem::replace(&mut self.locked, locked);
        if locked || !was {
            return Held::default();
        }
        let local = std::mem::take(&mut self.local);
        let remote = std::mem::take(&mut self.remote);
        Held {
            local: local.len(),
            remote: remote.len(),
            songs: local.union(&remote).copied().collect(),
        }
    }

    /// Song `song` changed in this engine. Whether to record it in the doc
    /// now — `false` while locked: it is held instead.
    pub fn local(&mut self, song: usize) -> bool {
        if self.locked {
            self.local.insert(song);
        }
        !self.locked
    }

    /// Song `song`'s doc changed remotely. Whether to make the engine match
    /// now — `false` while locked: it is held instead.
    pub fn remote(&mut self, song: usize) -> bool {
        if self.locked {
            self.remote.insert(song);
        }
        !self.locked
    }

    /// How many songs have something held (from either side).
    #[must_use]
    pub fn held(&self) -> usize {
        self.local.union(&self.remote).count()
    }

    /// How many songs have edits from the others waiting.
    #[must_use]
    pub fn held_remote(&self) -> usize {
        self.remote.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{Held, Hold};

    #[test]
    fn live_applies_both_ways_and_holds_nothing() {
        let mut hold = Hold::new(false);
        assert!(hold.local(0));
        assert!(hold.remote(1));
        assert_eq!(hold.held(), 0);
        assert!(hold.set_locked(false).is_empty());
    }

    #[test]
    fn locked_holds_both_ways_and_unlocking_hands_them_back_once() {
        let mut hold = Hold::new(false);
        assert!(hold.set_locked(true).is_empty(), "locking merges nothing");
        assert!(!hold.local(0), "an edit here is kept here");
        assert!(!hold.remote(0), "the others' edit to the same song waits");
        assert!(!hold.remote(2));
        assert!(!hold.local(0));
        assert_eq!(hold.held(), 2);
        assert_eq!(hold.held_remote(), 2);

        let held = hold.set_locked(false);
        assert_eq!(
            held,
            Held {
                songs: [0, 2].into_iter().collect(),
                local: 1,
                remote: 2,
            }
        );
        assert_eq!(hold.held(), 0);
        assert!(hold.set_locked(false).is_empty(), "handed back once");
        assert!(hold.local(0) && hold.remote(0), "live again");
    }

    #[test]
    fn a_device_that_starts_locked_holds_from_the_first_change() {
        let mut hold = Hold::new(true);
        assert!(hold.locked());
        assert!(!hold.remote(3));
        assert!(
            hold.set_locked(true).is_empty(),
            "locking again keeps the hold"
        );
        assert_eq!(hold.held(), 1);
        assert_eq!(
            hold.set_locked(false).songs.into_iter().collect::<Vec<_>>(),
            [3]
        );
    }
}
