//! Pitch detection, note segmentation, correction, and ARA-style incremental
//! resynthesis behind swappable backend traits (`PitchDetector`, `NoteSegmenter`,
//! `PitchSynthesizer`).

pub mod analyze;
pub mod backends;
pub mod correct;
pub mod detect;
pub mod dna;
pub mod model;
pub mod note;
#[cfg(feature = "render")]
pub mod render;
pub mod tracker;

pub use analyze::{AnalyzeConfig, TuneAnalysis, analyze};
pub use backends::{
    BackendInfo, EditableNote, NoteSegmenter, PitchAnalysisDoc, PitchDetector, PitchRegistry,
    PitchSynthesizer,
};
pub use correct::{CorrectConfig, NoteCorrection, Scale, correct_notes};
pub use detect::{PitchFrame, YinConfig, YinDetector, hz_to_midi, midi_to_hz};
pub use dna::{DnaConfig, DnaEngine, NoteSpan, SeparatedNote};
pub use model::{NoteBlob, PitchDoc, WarpMarker, decompose};
pub use note::{Note, NoteConfig, segment_notes};
pub use tracker::{TrackConfig, TrackedNote, spans_from_frames, track_notes};
