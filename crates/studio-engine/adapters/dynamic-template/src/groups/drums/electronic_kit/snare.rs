//! Electronic snare drum group definition

use crate::item_metadata::prelude::*;
use crate::item_metadata::ItemMetadataField;
use monarchy::FieldValueDescriptor;

/// Electronic snare drum group
pub struct Snare;

impl From<Snare> for ItemMetadataGroup {
    fn from(_val: Snare) -> Self {
        // Define multi-mic positions as a Group
        let multi_mic = Self::builder("MultiMic")
            .patterns(["Top", "Bottom", "Side", "OH"])
            .build();

        // Define variant descriptors for drum machine types
        // These are also defined on Electronic Kit parent, but having them here
        // allows isolated testing and ensures they're available at this level
        let variant_descriptors = vec![
            FieldValueDescriptor::builder("808")
                .patterns(["808", "tr-808", "tr808"])
                .build(),
            FieldValueDescriptor::builder("909")
                .patterns(["909", "tr-909", "tr909"])
                .build(),
            FieldValueDescriptor::builder("707")
                .patterns(["707", "tr-707", "tr707"])
                .build(),
        ];

        // Use the convenience method - extension trait is in scope via prelude
        // Require parent match so this only matches when "Electronic Kit" also matches
        Self::builder("Snare")
            .patterns(["snare", "snr", "sn"])
            .multi_mic(multi_mic)
            .field_value_descriptors(ItemMetadataField::Variant, variant_descriptors)
            .requires_parent_match()
            .build()
    }
}

// region: --- Tests

// endregion: --- Tests
