//! Kick drum group definition

use crate::item_metadata::prelude::*;

/// Kick drum group
pub struct Kick;

impl From<Kick> for ItemMetadataGroup {
    fn from(_val: Kick) -> Self {
        use crate::item_metadata::ItemMetadataField;
        use monarchy::FieldValueDescriptor;

        // Define multi-mic positions using field value descriptors
        // Each value can have its own patterns and negative patterns
        let multi_mic_descriptors = vec![
            FieldValueDescriptor::builder("In").patterns(["in"]).build(),
            FieldValueDescriptor::builder("Out")
                .patterns(["out"])
                .build(),
            FieldValueDescriptor::builder("Top")
                .patterns(["top"])
                .build(),
            FieldValueDescriptor::builder("Bottom")
                .patterns(["bottom"])
                .build(),
        ];

        // Define SUM tagged collection - items matching these patterns will be grouped together
        let sum_collection = Self::builder("SUM").patterns(["In", "Out", "Trig"]).build();

        // Use field_value_descriptors for MultiMic, and keep tagged_collection for backward compatibility
        Self::builder("Kick")
            .patterns(["kick", "kik", "bd", "bassdrum", "bass_drum"])
            .field_value_descriptors(ItemMetadataField::MultiMic, multi_mic_descriptors)
            .tagged_collection(sum_collection)
            .build()
    }
}

// region: --- Tests

// endregion: --- Tests
