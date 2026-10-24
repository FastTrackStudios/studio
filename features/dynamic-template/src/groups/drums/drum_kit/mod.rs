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
        Self::builder("Drum Kit")
            .patterns(vec!["d_", "drum", "kit", "drums", "brush", "brushes"])
            .exclude(vec![
                // A talkback mic is named for whoever it belongs to
                // ("TB Drums"), so without this it classifies as the kit it is
                // listening to. The exclusion has to sit here as well as on the
                // Drums container: the parser matches this group on its own
                // patterns, so blocking only the parent leaves it reachable.
                "tb",
                "vca",
                "hp",
                "headphone",
                "talkback",
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

// region: --- Tests

// endregion: --- Tests
