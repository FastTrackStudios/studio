//! Headphone / cue-mix group definitions.
//!
//! Every player gets their own monitor mix, so a session carries a cue bus per
//! musician alongside the mix. These are monitor paths: they must never reach
//! `MIX BUS`, or the record prints somebody's foldback.
//!
//! The convention in these sessions is `Headphone Bus` and `Cue Buss 1`, which
//! matched nothing and were reported as unrouted in ten of the eleven album
//! projects.

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Top-level headphone/cue group for per-player monitor mixes.
pub struct Headphones;

impl From<Headphones> for Group<ItemMetadata> {
    fn from(_val: Headphones) -> Self {
        Self::builder("Headphones")
            .prefix("HP")
            .patterns(vec![
                "headphone",
                "headphones",
                "hp",
                "cue",
                "cue mix",
                "cue buss",
                "cue bus",
                "foldback",
                "monitor mix",
            ])
            // "Cue" also names the guide-track cues (section callouts), which
            // belong to Guide. Guide is registered first, so a cue *track*
            // reaches it before this group; these exclusions cover the
            // compound names that would otherwise be ambiguous.
            .exclude(["dynamic cue", "section cue", "count"])
            .build()
    }
}
