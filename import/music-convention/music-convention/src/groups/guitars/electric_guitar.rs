//! Electric guitar group definition

use crate::item_metadata::prelude::*;
use monarchy::{FieldGroupingStrategy, FieldValueDescriptor};

/// Electric guitar group
pub struct ElectricGuitar;

impl From<ElectricGuitar> for ItemMetadataGroup {
    fn from(_val: ElectricGuitar) -> Self {
        use crate::item_metadata::ItemMetadataField;

        // Define guitar-specific arrangement patterns (playing styles / tones)
        let guitar_arrangement = ItemMetadataGroup::builder("Arrangement")
            .patterns([
                "Clean",
                "Crunch",
                "Drive",
                "Lead",
                "Pick",
                "Chug",
                "Rhythm",
                "Slide",
                "Solo",
                "Outro",
                "Phaser",
                "Pitch",
                "Wah",
                "Distortion",
                "Overdrive",
            ])
            .build();

        // Define guitar model/variant descriptors
        let variant_descriptors = vec![
            FieldValueDescriptor::builder("Strat")
                .patterns(["strat", "stratocaster"])
                .build(),
            FieldValueDescriptor::builder("Tele")
                .patterns(["tele", "telecaster"])
                .build(),
            FieldValueDescriptor::builder("Les Paul")
                .patterns(["les paul", "lespaul", "lp"])
                .build(),
            FieldValueDescriptor::builder("SG").patterns(["sg"]).build(),
            FieldValueDescriptor::builder("Gretsch")
                .patterns(["gretsch"])
                .build(),
            FieldValueDescriptor::builder("Jazzmaster")
                .patterns(["jazzmaster", "jm"])
                .build(),
            FieldValueDescriptor::builder("Jaguar")
                .patterns(["jaguar", "jag"])
                .build(),
            FieldValueDescriptor::builder("335")
                .patterns(["335", "es-335", "es335"])
                .build(),
            FieldValueDescriptor::builder("Rickenbacker")
                .patterns(["rickenbacker", "ric", "ricky"])
                .build(),
            FieldValueDescriptor::builder("PRS")
                .patterns(["prs", "paul reed smith"])
                .build(),
        ];

        // Define multi-mic descriptors for guitar (Amp, DI, Amplitube)
        let multi_mic_descriptors = vec![
            FieldValueDescriptor::builder("Amp")
                .patterns(["amp", "amplitube"])
                .build(),
            FieldValueDescriptor::builder("DI").patterns(["di"]).build(),
        ];

        // Configure electric guitar with field priority: Performer → Arrangement → Layers → Channel → MultiMic
        // The order of these calls determines the priority order
        // MultiMic uses MainOnContainer strategy so base tracks go on folder, multi-mic versions become children
        // Layers uses "Main" as default value so items without a layer are grouped alongside items with layers
        ItemMetadataGroup::builder("Electric")
            .prefix("E")
            .patterns([
                "electric",
                "elec", // Common abbreviation for Electric (as in "Elec Gui")
                "guitar",
                "gtr", // Very common 3-letter abbreviation used by engineers
                "eg",  // Common abbreviation for Electric Guitar
                "lead guitar",
                "lead_guitar",
                "leadguitar",
                // Common electric guitar models (also matched as variants)
                "strat",
                "stratocaster",
                "tele",
                "telecaster",
                "les paul",
                "lespaul",
                "sg",
                "gretsch",
                "jazzmaster",
                "jaguar",
                "firebird",
                "explorer",
                "flying v",
            ])
            // Prevent matching bass or piano items that start with "Elec"
            .exclude(["bass", "piano", "pno", "keys", "keyboard"])
            .performer(ItemMetadataGroup::builder("Performer").build()) // Priority 1: Performer (uses global patterns)
            .field_value_descriptors(ItemMetadataField::Variant, variant_descriptors) // Priority 2: Variant/model
            .arrangement(guitar_arrangement) // Priority 3: Arrangement
            .layers(ItemMetadataGroup::builder("Layers").build()) // Priority 4: Layers (uses global patterns)
            .field_default_value(ItemMetadataField::Layers, "Main") // Default layer name for items without a layer
            .channel(
                ItemMetadataGroup::builder("Channel")
                    .patterns(["L", "C", "R", "Left", "Center", "Right"])
                    .build(),
            ) // Priority 5: Channel (order: L, C, R)
            // Note: We use field_value_descriptors for MultiMic, so we don't need the nested .multi_mic() group
            // The field_value_descriptors handle the MultiMic value extraction and matching
            .field_value_descriptors(ItemMetadataField::MultiMic, multi_mic_descriptors)
            .field_strategy(
                ItemMetadataField::MultiMic,
                FieldGroupingStrategy::MainOnContainer,
            )
            .build()
    }
}
