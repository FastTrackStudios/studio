//! Background vocals (BGVs) group definition

use crate::item_metadata::prelude::*;
use crate::item_metadata::ItemMetadataField;

/// Background vocals group (BGVs)
///
/// Sorting priority: Performer → Section → Arrangement (harmonies) → Layers → Channels
pub struct BackgroundVocals;

/// Define harmony arrangements for BGVs
/// These are the Arrangement metadata field values specific to BGVs
fn build_harmony_arrangement() -> ItemMetadataGroup {
    ItemMetadataGroup::builder("Arrangement")
        .patterns([
            // Voice Parts
            "Soprano",
            "soprano",
            "sop",
            "s",
            "Alto",
            "alto",
            "a",
            "Tenor",
            "tenor",
            "t",
            "Baritone",
            "baritone",
            "bar",
            "bari",
            "b",
            "Bass",
            "bass",
            "low",
            // Harmony Descriptors
            "High",
            "high",
            "high harmony",
            "high harm",
            "upper",
            "Highest",
            "highest",
            "Low",
            "low",
            "low harmony",
            "low harm",
            "lower",
            "Lowest",
            "lowest",
            "Mid",
            "mid",
            "middle",
            "mid harmony",
            "mid harm",
            "Drone",
            "drone",
            "drone harmony",
            "sustained",
            // Additional Common Harmonies
            "Harmony 1",
            "harmony 1",
            "harm 1",
            "h1",
            "harmony1",
            "harm1",
            "Harmony 2",
            "harmony 2",
            "harm 2",
            "h2",
            "harmony2",
            "harm2",
            "Harmony 3",
            "harmony 3",
            "harm 3",
            "h3",
            "harmony3",
            "harm3",
            "Oohs",
            "ooh",
            "oohs",
            "ooh harmony",
            "Aahs",
            "aah",
            "aahs",
            "aah harmony",
            "Ad Libs",
            "ad lib",
            "adlib",
            "ad libs",
            "adlibs",
        ])
        .build()
}

impl From<BackgroundVocals> for ItemMetadataGroup {
    fn from(_val: BackgroundVocals) -> Self {
        let harmony_arrangement = build_harmony_arrangement();

        // Configure BGVs with field priority: Performer → Section → Arrangement → Layers → Channels
        // The order of these calls determines the priority order
        // Layers uses "Main" as default value so items without a layer are grouped alongside items with layers
        // Note: BGVs does NOT use requires_parent_match because "bgv", "background", etc.
        // are already specific enough patterns that uniquely identify background vocals
        Self::builder("BGVs")
            .prefix("BGV")
            .patterns([
                // Standard abbreviations
                "bgv",
                "bgvs",
                "bgvox",
                "bg",
                "bv",
                "bvs",
                "background",
                "backing",
                "harmony",
                // NOTE: "choir" intentionally omitted — choir stems route to the
                // dedicated Choir subgroup under Vocals, not into BGVs.
                // Ad-libs are typically supporting vocal elements
                "ad lib",
                "adlib",
                "ad-lib",
                // Harmony position descriptors - these indicate BGV content
                "high harmony",
                "low harmony",
                "mid harmony",
                "upper harmony",
                "lower harmony",
                "middle harmony",
                // "Hey Hey" is a common vocal hook/response pattern in pop/rock
                "hey hey",
            ])
            // Choir stems are handled by the dedicated Choir subgroup, not BGVs.
            .exclude(["choir", "chorale", "ensemble"])
            .performer(Self::builder("Performer").build()) // Priority 1: Performer (uses global patterns)
            .section(Self::builder("Section").build()) // Priority 2: Section (uses global patterns)
            .arrangement(harmony_arrangement) // Priority 3: Arrangement (harmony-specific patterns)
            .layers(Self::builder("Layers").build()) // Priority 4: Layers (uses global patterns)
            .field_default_value(ItemMetadataField::Layers, "Main") // Default layer name for items without a layer
            .channel(
                Self::builder("Channel")
                    .patterns(["L", "C", "R", "Left", "Center", "Right"])
                    .build(),
            ) // Priority 5: Channel (order: L, C, R)
            .build()
    }
}

// region: --- Tests

// endregion: --- Tests
