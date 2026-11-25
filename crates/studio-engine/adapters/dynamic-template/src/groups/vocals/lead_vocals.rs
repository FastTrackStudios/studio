//! Lead vocals group definition

use crate::item_metadata::prelude::*;
use crate::item_metadata::ItemMetadataField;

/// Lead vocals group
///
/// Sorting priority: Performer → Section → Layers → Channels
pub struct LeadVocals;

impl From<LeadVocals> for ItemMetadataGroup {
    fn from(_val: LeadVocals) -> Self {
        // Configure lead vocals with field priority: Performer → Section → Layers → Channels
        // The order of these calls determines the priority order
        // Layers uses "Main" as default value so items without a layer are grouped alongside items with layers
        // Note: No prefix for Lead Vocals - parent Vocals has "V" prefix which is sufficient
        Self::builder("Lead")
            // Patterns for lead vocals - includes generic vocal patterns since Lead is the
            // default destination for vocal tracks that don't match BGVs patterns
            // NOTE: "lead" and "solo" are intentionally excluded - they're ambiguous without
            // vocal context (could be guitar lead/solo). Use requires_parent_match() to ensure
            // parent Vocals group matches first, providing the vocal context.
            .patterns([
                "main", "ld", "ldv", "lv", "vld", "voc", "vox", "vocal", "voca", "voice", "scream",
                "screams", "screamer",
            ])
            // Only match if parent (Vocals) also matches - prevents "JohnyLead" from matching
            // just because it contains "Lead" without any vocal-related patterns
            .requires_parent_match()
            .performer(Self::builder("Performer").build()) // Priority 1: Performer (uses global patterns)
            .section(Self::builder("Section").build()) // Priority 2: Section (uses global patterns)
            // Layers includes quad (quadruple tracking), stereo, mono, etc.
            .layers(
                Self::builder("Layers")
                    .patterns(["quad", "stereo", "mono", "double", "triple"])
                    .build(),
            ) // Priority 3: Layers
            .field_default_value(ItemMetadataField::Layers, "Main") // Default layer name for items without a layer
            .channel(
                Self::builder("Channel")
                    .patterns(["L", "C", "R", "Left", "Center", "Right"])
                    .build(),
            ) // Priority 4: Channel (order: L, C, R)
            .build()
    }
}

// region: --- Tests

// endregion: --- Tests
