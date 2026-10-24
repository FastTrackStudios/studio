//! Acoustic guitar group definition

use super::mandolin::Mandolin;
use crate::item_metadata::{ItemMetadata, ItemMetadataField};
use monarchy::{FieldValueDescriptor, Group};

/// Acoustic guitar group
pub struct AcousticGuitar;

/// The sources of one acoustic channel, in the order a part grows them.
///
/// `flow.guitars.acoustics`: "a DI and two mics — a pickup DI, a
/// neck-side condenser, a body-side condenser". The same `MultiMic`
/// dimension as the electric's seven, with the acoustic's own vocabulary,
/// which is the whole reason acoustics need no separate code path.
fn build_multi_mic_descriptors() -> Vec<FieldValueDescriptor> {
    vec![
        FieldValueDescriptor::builder("DI").patterns(["di"]).build(),
        FieldValueDescriptor::builder("Neck")
            .patterns(["neck", "nk"])
            .build(),
        FieldValueDescriptor::builder("Body")
            .patterns(["body", "bdy"])
            .build(),
    ]
}

impl From<AcousticGuitar> for Group<ItemMetadata> {
    fn from(_val: AcousticGuitar) -> Self {
        Self::builder("Acoustic")
            .prefix("AG")
            .patterns(vec![
                // Generic acoustic patterns
                "acoustic",
                "ag", // Common abbreviation for Acoustic Guitar
                // The house convention pairs a bare GTR with A or E for
                // acoustic or electric. Electric owns the bare "gtr" pattern,
                // so without these every "GTR A" lands on the electric bus.
                "gtr a",
                "gtr-a",
                "gtr_a",
                "guitar a",
                "aco", // Another common abbreviation
                "acc",
                "nylon",
                // A steel-string acoustic (the pedal steel excludes these).
                "steel string",
                "steel-string",
                "steelstring",
                "classical",
                "fingerpick",
                // Gibson acoustic models
                "J160",
                "J-160",
                "J45",
                "J-45",
                "J200",
                "J-200",
                "Hummingbird",
                // Martin acoustic models
                "D28",
                "D-28",
                "D18",
                "D-18",
                "OM28",
                "OM-28",
                "HD28",
                "HD-28",
                // Taylor acoustic models
                "Taylor",
                "814ce",
                "714ce",
                "214ce",
                // Other acoustic brands/models
                "Framus",
                "Guild",
                "Takamine",
                "Ovation",
                "Seagull",
                "Yamaha FG",
                "Epiphone",
            ])
            .exclude([
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
            // Only the MultiMic vocabulary is declared here, not the
            // whole dimension stack. The acoustic's sources are its own —
            // a pickup DI and two condensers (`flow.guitars.acoustics`) —
            // and nothing else in the template says what they are.
            //
            // Its arrangement and layer names stay global, deliberately.
            // Declaring either on this group replaces the global list for
            // every acoustic folder in every session, and two things
            // break when it does: an acoustic folder stops grouping by
            // player (the global Performer list no longer applies), and
            // "Strum" is claimed here before Mandolin can claim it. So
            // the acoustic's own Nashville arrangement and its Harmony
            // layer are not in the vocabulary yet — see the note on #70.
            // Growing an acoustic works either way: Strum grows a
            // Fingerpick beside it and Main grows a DBL, both reading
            // back as the dimension they were created as.
            .field_value_descriptors(ItemMetadataField::MultiMic, build_multi_mic_descriptors())
            .group(Mandolin)
            .build()
    }
}
