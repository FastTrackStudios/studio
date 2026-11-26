//! Audio-thread → UI data feeds: the shapes are [`crate::feed`]'s
//! (re-exported here); this is the editor's view of them.

use crate::prelude::*;
use fts_audio_ui::prelude::*;

pub use crate::feed::{PeakMeter, WAVE_HISTORY_LEN, WaveRing};

/// Render a [`PeakMeter`] pair plus a gain-reduction meter — the metering
/// column almost every dynamics editor wants down its right-hand side.
#[component]
pub fn IoGrMeters(input_db: f32, output_db: f32, gain_reduction_db: f32) -> Element {
    rsx! {
        LevelMeterDb { level_db: input_db, label: "IN".to_string(), height: 160.0 }
        GrMeter { gain_reduction_db, height: 160.0 }
        LevelMeterDb { level_db: output_db, label: "OUT".to_string(), height: 160.0 }
    }
}
