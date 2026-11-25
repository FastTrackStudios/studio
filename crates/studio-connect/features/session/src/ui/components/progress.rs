//! The song progress bar: `transport_ui`'s (one source for Session's views
//! and Studio's share-page island, studio#445), with Session's queued
//! navigation (`QueuedTarget`) said in its terms.

use crate::proto::QueuedTarget;
use crate::ui::prelude::*;

pub use transport_ui::progress::{
    CommentMarker, LoopIndicatorData, ProgressSection, SegmentedProgressBar, TempoCard,
    TempoMarker, TempoMarkerData, TimeSignatureCard,
};

/// Measure indicator definition for section progress bar (Session's: it
/// carries a DAW position).
#[derive(Clone, Debug, PartialEq)]
pub struct MeasureIndicator {
    pub position_percent: f64,
    pub measure_number: i32,
    pub time_signature: Option<(u8, u8)>,
    pub musical_position: daw_proto::MusicalPosition,
}

/// [`transport_ui::SongProgressBar`], its queued section or comment taken
/// from Session's [`QueuedTarget`].
#[component]
pub fn SongProgressBar(
    /// Current progress percentage (0-100)
    progress: f64,
    sections: Vec<ProgressSection>,
    #[props(default)] on_section_click: Option<Callback<usize>>,
    #[props(default)] on_comment_click: Option<Callback<f64>>,
    #[props(default)] tempo_markers: Vec<TempoMarkerData>,
    #[props(default)] comment_markers: Vec<CommentMarker>,
    #[props(default)] song_key: Option<String>,
    #[props(default)] loop_indicator: Option<LoopIndicatorData>,
    #[props(default)] queued_target: Option<QueuedTarget>,
    /// The bar's height as CSS (see [`SegmentedProgressBar`]).
    #[props(default)]
    height: Option<String>,
    /// Whether the sections are named (see [`SegmentedProgressBar`]).
    #[props(default = true)]
    labels: bool,
    /// Square-cornered (see [`SegmentedProgressBar`]).
    #[props(default)]
    flat: bool,
) -> Element {
    let (queued_section, queued_comment) = match &queued_target {
        Some(QueuedTarget::Section { section_index, .. }) => (Some(*section_index), None),
        Some(QueuedTarget::Comment {
            position_seconds, ..
        }) => (None, Some(*position_seconds)),
        _ => (None, None),
    };
    rsx! {
        transport_ui::SongProgressBar {
            progress,
            sections,
            on_section_click,
            on_comment_click,
            tempo_markers,
            comment_markers,
            song_key,
            loop_indicator,
            queued_section,
            queued_comment,
            height,
            labels,
            flat,
        }
    }
}
