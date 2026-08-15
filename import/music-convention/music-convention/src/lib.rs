//! Music production naming conventions, as data.
//!
//! This crate is the DAW-free half of FastTrackStudio's dynamic template: the
//! classification tree (instrument groups and their patterns), the metadata
//! model that monarchy sorts by, and the pure string transforms that read and
//! strip production metadata out of track/file names — tempo, song names, Pro
//! Tools playlist markers, equipment names.
//!
//! Nothing here knows about a DAW. Turning the resulting [`monarchy::Structure`]
//! into REAPER tracks is the job of the `dynamic-template` crate that wraps this
//! one.

use monarchy::Config;

// region: --- Modules

pub mod colors;
pub mod equipment;
pub mod error;
pub mod groups;
pub mod icons;
pub mod item_metadata;
pub mod layouts;
pub mod metadata_patterns;
pub mod protools;
pub mod song_name;
pub mod tempo;
pub mod track_schema;
pub mod visibility_rules;

pub use error::{Error, Result};
pub use groups::{
    Bass, Choir, Drums, Guide, Guitars, Harmonica, Horns, Keys, Orchestra, Percussion, Reference,
    StemSplit, Strings, Synths, Tracks, Vocals, SFX,
};
pub use item_metadata::ItemMetadata;

// Re-export monarchy types needed for direct classification
pub use groups::stem_split::is_stem_split_set;
pub use monarchy::{monarchy_sort, Structure};
pub use protools::{extract_protools_metadata, strip_protools_markers, ProToolsMetadata};
pub use song_name::{
    detect_song_names, detect_song_names_with_config, strip_song_names, SongNameConfig,
};
pub use tempo::{extract_tempo, strip_tempo};

// endregion: --- Modules

/// Type alias for our standard Config with ItemMetadata
pub type MusicConventionConfig = Config<ItemMetadata>;

/// Creates a default configuration for the music convention classifier
pub fn default_config() -> MusicConventionConfig {
    Config::builder()
        // Add metadata field patterns first (metadata-only group)
        .group(metadata_patterns::default_metadata_field_patterns())
        // Then add regular groups
        .group(Drums)
        .group(Percussion)
        .group(Bass)
        .group(Guitars)
        .group(Keys)
        .group(Synths)
        .group(Horns)
        .group(Harmonica)
        .group(Strings)
        .group(Vocals)
        // Top-level Choir catches standalone voice-part choir sessions
        // (soprano/alto/tenor, "chorus", ensembles). Literal "choir"/"chorale"
        // stems are handled by Vocals' Choir subgroup instead (routed to the
        // vocal bus) — the top-level group deliberately does NOT match those two
        // bare words, so a "Choir" stem lands under Vocals with no duplication.
        .group(Choir)
        .group(Orchestra)
        .group(SFX)
        // Backing/playback tracks (loops, sequences) — a live-rig top-level group.
        .group(Tracks)
        // Utility tracks at the bottom
        .group(Guide)
        .group(Reference)
        .group(StemSplit)
        .build()
}

// region: --- Tests

#[cfg(test)]
mod tests {
    use super::*;
    use monarchy::StructureAssertions;

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    #[test]
    fn test_acc_guitar_structure() -> Result<()> {
        // -- Setup & Fixtures
        let inputs = vec!["Acc Guitar"];
        let config = default_config();

        // -- Exec
        let result = monarchy_sort(inputs, &config)?;

        // -- Check
        // Guitars is not transparent, so it keeps its name even with a single item
        // The structure should be: Guitars: [item]
        let guitars = result
            .find_child("Guitars")
            .expect("Should have Guitars folder");
        assert_eq!(guitars.items.len(), 1, "Guitars should have 1 item");
        assert_eq!(guitars.items[0].original, "Acc Guitar");

        Ok(())
    }

    #[test]
    fn test_kick_and_drumkit() -> Result<()> {
        // -- Setup & Fixtures
        let inputs = vec!["kick_in.wav", "kick_out.wav", "snare.wav"];
        let config = default_config();

        // -- Exec
        let result = monarchy_sort(inputs, &config)?;

        // -- Check
        // The structure should be:
        // Drums
        //   Kick (folder with In/Out children)
        //     In: [kick_in.wav]
        //     Out: [kick_out.wav]
        //   Snare: [snare.wav]
        result
            .assert()
            .has_total_items(3)
            .has_groups(1)
            .group("Drums")
            .has_groups(2) // Kick folder and Snare track
            .group("Kick")
            .has_groups(2) // In and Out sub-groups
            .done()
            .group("Drums")
            .group("Snare")
            .contains_exactly(&["snare.wav"])
            .done();

        Ok(())
    }

    #[test]
    fn test_bass_types() -> Result<()> {
        // -- Setup & Fixtures
        // Note: "808_bass" will match Electronic Kit's "808" pattern before Bass Synth,
        // so we use "sub_bass" instead to properly test synth bass categorization
        let inputs = vec![
            "bass_guitar_di.wav",
            "synth_bass_sub.wav",
            "upright_pizz.wav",
            "electric_bass_amp.wav",
            "sub_bass.wav", // Changed from "808_bass.wav" to avoid matching Electronic Kit
            "acoustic_bass.wav",
        ];
        let config = default_config();

        // -- Exec
        let result = monarchy_sort(inputs, &config)?;

        // -- Check
        // Structure should be:
        // Bass
        //   Guitar
        //     DI:  [bass_guitar_di.wav]      ← bass guitar now splits into
        //     Amp: [electric_bass_amp.wav]     its DI / Amp multi-mic captures
        //   Synth: [synth_bass_sub.wav, sub_bass.wav]
        //   Upright Bass: [upright_pizz.wav, acoustic_bass.wav]
        // Note: Group names are "Guitar", "Synth", "Upright Bass" (not prefixed with "Bass")
        result
            .assert()
            .has_total_items(6)
            .has_groups(1)
            .group("Bass")
            .has_groups(3)
            .group("Guitar") // The group name is "Guitar", display name is "Bass Guitar"
            .has_groups(2)
            .group("DI")
            .contains_exactly(&["bass_guitar_di.wav"])
            .done()
            .group("Bass")
            .group("Guitar")
            .group("Amp")
            .contains_exactly(&["electric_bass_amp.wav"])
            .done()
            .group("Bass")
            .group("Synth") // The group name is "Synth", display name is "Bass Synth"
            .contains_exactly(&["synth_bass_sub.wav", "sub_bass.wav"])
            .done()
            .group("Bass")
            .group("Upright Bass")
            .contains_exactly(&["upright_pizz.wav", "acoustic_bass.wav"])
            .done();

        Ok(())
    }
}

// endregion: --- Tests
