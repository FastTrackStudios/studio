//! Drum kit group definitions

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

mod cymbals;
mod kick;
mod room;
mod snare;
mod tom;

pub use cymbals::Cymbals;
pub use kick::Kick;
pub use room::Room;
pub use snare::Snare;
pub use tom::Tom;

/// Drum kit container group that includes all drum subgroups
pub struct DrumKit;

impl From<DrumKit> for Group<ItemMetadata> {
    fn from(_val: DrumKit) -> Self {
        Group::builder("Drum Kit")
            .patterns(vec!["d_", "drum", "kit", "drums", "brush", "brushes"])
            .exclude(vec![
                "electronic",
                "elec",
                "e-kit",
                "ekit",
                "808",
                "909",
                "drum machine",
                "machine",
                "midi drum",
                "vst drum",
            ])
            .block_prefix("D") // Avoid "D Drum Kit" redundancy
            .group(Kick)
            .group(Snare)
            .group(Tom)
            .group(Cymbals)
            .group(Room)
            .build()
    }
}
