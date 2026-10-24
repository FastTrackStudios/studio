//! Electric guitar group definition

use crate::item_metadata::prelude::*;
use monarchy::{FieldGroupingStrategy, FieldValueDescriptor};

/// Electric guitar group
pub struct ElectricGuitar;

fn build_guitar_arrangement() -> ItemMetadataGroup {
    ItemMetadataGroup::builder("Arrangement")
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
        .build()
}

fn build_variant_descriptors() -> Vec<FieldValueDescriptor> {
    vec![
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
    ]
}

/// The sources of one electric channel (`flow.guitars.dimensions`): up to
/// seven for a single take of a single channel — the DI, the pedalboard,
/// two amps, and the 57 and 121 on each amp.
///
/// This list is *mixer* order, which is what organizing a folder sorts
/// by, so Amp stays at the head where it has always been. The order a
/// part **grows** through these is a different question, and
/// `track_schema::growth_values_for_dimension` answers it.
fn build_multi_mic_descriptors() -> Vec<FieldValueDescriptor> {
    vec![
        FieldValueDescriptor::builder("Amp")
            .patterns(["amp", "amplitube"])
            .build(),
        FieldValueDescriptor::builder("DI").patterns(["di"]).build(),
        FieldValueDescriptor::builder("Pedalboard")
            .patterns(["pedalboard", "pedal board", "pedals", "pedalbrd"])
            .build(),
        FieldValueDescriptor::builder("Amp 1")
            .patterns(["amp 1", "amp1", "amp-1"])
            .build(),
        FieldValueDescriptor::builder("Amp 2")
            .patterns(["amp 2", "amp2", "amp-2"])
            .build(),
        // The two mics on a cab, by model. Not "57" and "R121", which is
        // what an engineer says out loud: a bare "57" also matches the
        // mic named in a capture's file name ("EG2 (57)"), where it is
        // equipment metadata rather than a source track of its own, and
        // matching it reshaped a real multitrack fixture; and "R121"
        // reads as the R channel, because "R" is a channel and the
        // classifier checks channels first.
        FieldValueDescriptor::builder("SM57")
            .patterns(["sm57", "sm-57"])
            .build(),
        FieldValueDescriptor::builder("Royer")
            .patterns(["royer", "r121", "r-121", "121"])
            .build(),
    ]
}

impl From<ElectricGuitar> for ItemMetadataGroup {
    fn from(_val: ElectricGuitar) -> Self {
        use crate::item_metadata::ItemMetadataField;

        let guitar_arrangement = build_guitar_arrangement();
        let variant_descriptors = build_variant_descriptors();
        let multi_mic_descriptors = build_multi_mic_descriptors();

        // Configure electric guitar with field priority: Performer → Arrangement → Layers → Channel → MultiMic
        // The order of these calls determines the priority order
        // MultiMic uses MainOnContainer strategy so base tracks go on folder, multi-mic versions become children
        // Layers uses "Main" as default value so items without a layer are grouped alongside items with layers
        Self::builder("Electric")
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
            .exclude([
                // "GTR A" is the acoustic in the house convention; the bare
                // "gtr" pattern below would otherwise claim it for electric.
                "gtr a",
                "gtr-a",
                "gtr_a",
                "guitar a",
                // A talkback mic is named for whoever it belongs to
                // ("TB Guitar"), so without this it classifies as the
                // instrument it is listening to. Excluding on the Guitars
                // container is not enough — the parser matches this group on
                // its own patterns, independently of its parent.
                "tb",
                "vca",
                "hp",
                "headphone",
                "talkback",
            ])
            // Prevent matching bass or piano items that start with "Elec"
            .exclude(["bass", "piano", "pno", "keys", "keyboard"])
            .performer(Self::builder("Performer").build()) // Priority 1: Performer (uses global patterns)
            .field_value_descriptors(ItemMetadataField::Variant, variant_descriptors) // Priority 2: Variant/model
            .arrangement(guitar_arrangement) // Priority 3: Arrangement
            .layers(Self::builder("Layers").build()) // Priority 4: Layers (global patterns)
            .field_default_value(ItemMetadataField::Layers, "Main") // Default layer name for items without a layer
            .channel(
                Self::builder("Channel")
                    .patterns(["L", "C", "R", "Left", "Center", "Right"])
                    .build(),
            ) // Priority 5: Channel (order: L, C, R)
            // L/R is the Channel dimension for guitars, not a Layer: a
            // Channel shares a chain, a Layer has its own
            // (`flow.guitars.dimensions`).
            // Note: We use field_value_descriptors for MultiMic, so we don't need the nested .multi_mic() group
            // The field_value_descriptors handle the MultiMic value extraction and matching
            .field_value_descriptors(ItemMetadataField::MultiMic, multi_mic_descriptors)
            .field_strategy(
                ItemMetadataField::MultiMic,
                FieldGroupingStrategy::MainOnContainer,
            )
            // The DI feed nests under its sibling amp track (muted,
            // folder-collapsed) instead of sitting beside it as an equal.
            .nest_secondary_mics(true)
            .build()
    }
}

// region: --- Tests

// endregion: --- Tests
