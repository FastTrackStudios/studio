//! Talkback group definitions.
//!
//! Talkback is the engineer's and players' comms — a mic in the control room,
//! a mic on each player's stand — recorded alongside the music so takes can be
//! reviewed with the conversation intact. It is **not** content: a talkback mic
//! on the bass player is not bass, and summing it into the mix prints room
//! chatter over the record.
//!
//! The convention in these sessions is a `TB` prefix naming whose talkback it
//! is (`TB Bass`, `TB Drums`, `TB Engineer`), which is why this group has to
//! exist rather than letting `TB Bass` classify as bass — it matched the
//! instrument, and routed the chatter into the mix.

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Top-level talkback group for comms mics.
pub struct Talkback;

impl From<Talkback> for Group<ItemMetadata> {
    fn from(_val: Talkback) -> Self {
        Self::builder("Talkback")
            .prefix("TB")
            .patterns(vec![
                "talkback",
                "talk back",
                "talk-back",
                // The house prefix. Bare "tb" only ever means talkback here;
                // it is not an abbreviation any instrument uses.
                "tb",
                "comms",
                "intercom",
                "cue mic",
                "engineer mic",
            ])
            .build()
    }
}
