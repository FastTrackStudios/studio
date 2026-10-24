//! Bass-related group definitions

use crate::item_metadata::prelude::*;

pub mod bass_guitar;
pub mod synth_bass;
pub mod upright_bass;

pub use bass_guitar::BassGuitar;
pub use synth_bass::SynthBass;
pub use upright_bass::UprightBass;

/// Top-level bass group containing all bass types
pub struct Bass;

impl From<Bass> for ItemMetadataGroup {
    fn from(_val: Bass) -> Self {
        Self::builder("Bass")
            .prefix("Bass")
            .patterns(["bass"])
            // Negative patterns to avoid matching bass drums
            .exclude([
                "bassdrum",
                "bass_drum",
                "bd",
                "kick",
                "tb",
                "talkback",
                "vca",
                "hp",
                "headphone",
            ])
            .group(BassGuitar)
            .group(SynthBass)
            .group(UprightBass)
            .build()
    }
}

// region: --- Tests

// endregion: --- Tests
