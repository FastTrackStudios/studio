//! Presentation Components
//!
//! Pure presentation components that render UI based on props only.
//! No signal dependencies - all state is passed via props.

#[cfg(feature = "chart-editor")]
pub mod chart_text;
pub mod lyric_sync;
pub mod lyrics;
pub mod mixer;
pub mod progress;
pub mod section_progress;
pub mod sidebar_items;
pub mod song;
pub mod take_review;
pub mod transport_controls;
pub mod transport_stats;

pub use lyric_sync::*;
pub use mixer::*;
pub use progress::*;
pub use section_progress::*;
pub use sidebar_items::*;
pub use song::*;
pub use take_review::*;
pub use transport_controls::*;
pub use transport_stats::*;
