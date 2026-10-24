//! VCA group definitions.
//!
//! A VCA track carries no audio — it is a fader that controls other faders.
//! Routing one into a bus is meaningless: there is nothing to sum, and the
//! send would be silent while making the track look organized.
//!
//! So VCAs are classified deliberately and then deliberately *not* routed. That
//! distinction matters: a track that reaches no bus because nothing recognised
//! it is work for a human, while a VCA that reaches no bus is finished. Without
//! this group, `BAND RECORD VCA` was reported as unrouted in all eleven album
//! projects, and would have been swept into `UNSORTED`.

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Top-level VCA group for control-only tracks.
pub struct Vca;

impl From<Vca> for Group<ItemMetadata> {
    fn from(_val: Vca) -> Self {
        Self::builder("VCA")
            .prefix("VCA")
            .patterns(vec!["vca"])
            .build()
    }
}
