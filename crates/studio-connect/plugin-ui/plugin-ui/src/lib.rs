//! Effect plugin UIs, parameter trees, visualizations, and stack host adapters
//! for FastTrackStudio.
//!
//! Pure DSP engines live in [`dsp`] (`FastTrackStudios/dsp`); this crate layers
//! Dioxus views, Vello/WGPU visualizers, `nice-plug` parameter trees, and stack
//! host adapters on top while preserving the familiar module layout.

#[cfg(any(
    all(feature = "comp-native", feature = "comp-ui"),
    all(feature = "delay-native", feature = "delay-ui"),
    all(feature = "modulation-native", feature = "modulation-ui"),
    all(feature = "reverb-native", feature = "reverb-ui"),
    all(feature = "saturate-native", feature = "saturate-ui"),
))]
pub mod editor_form;

#[cfg(feature = "multiband")]
pub use ::dsp::multiband;

#[cfg(feature = "level")]
pub use ::dsp::level;

#[cfg(feature = "align")]
pub use ::dsp::align;

#[cfg(feature = "analyzer")]
pub use ::dsp::analyzer;

#[cfg(feature = "gate")]
pub use ::dsp::gate;

#[cfg(feature = "pitch")]
pub use ::dsp::pitch;

#[cfg(feature = "comp")]
pub mod comp {
    pub use ::dsp::comp::*;
    #[cfg(feature = "comp-ui")]
    pub mod ui;
}

#[cfg(feature = "delay")]
pub mod delay {
    pub use ::dsp::delay::*;
    #[cfg(feature = "delay-ui")]
    pub mod ui;
}

#[cfg(feature = "eq")]
pub mod eq {
    #[cfg(feature = "eq-profiles")]
    pub use ::dsp::eq::profiles;
    pub use ::dsp::eq::{
        Ballistics, BandConfig, BandId, BiquadCoefficients, Character, Complex, CutSlope,
        DetectorSource, Dynamics, DynamicsMode, EqConfig, EqProcessor, Error, Filter,
        FilterProcessor, FilterType, Listen, Output, Pan, Placement, PreparedEq, PreparedFilter,
        ProcessSpec, Steepness, StereoResponse, Stream, Threshold, Transient, design, dynamics,
        engine, hardware, host, math, model, pultec_color, runtime,
    };

    #[cfg(feature = "eq-analyzer")]
    #[allow(
        clippy::allow_attributes,
        clippy::allow_attributes_without_reason,
        clippy::arithmetic_side_effects,
        clippy::as_conversions,
        clippy::assigning_clones,
        clippy::cast_lossless,
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::manual_slice_fill,
        clippy::missing_const_for_fn,
        clippy::non_std_lazy_statics,
        clippy::struct_excessive_bools,
        clippy::suboptimal_flops,
        reason = "preserved from fts-fx eq::analyzer module"
    )]
    pub mod analyzer {
        pub use ::dsp::eq::analyzer::*;
        #[cfg(feature = "eq-analyzer-ui")]
        pub mod ui;
    }

    #[cfg(feature = "eq-ui")]
    pub mod ui;
}

#[cfg(feature = "mic")]
pub mod mic {
    pub use ::dsp::mic::*;
    #[cfg(feature = "mic-ui")]
    pub mod ui;
}

#[cfg(feature = "modulation")]
pub mod modulation {
    pub use ::dsp::modulation::*;
    #[cfg(feature = "modulation-ui")]
    pub mod ui;
}

#[cfg(feature = "reverb")]
pub mod reverb {
    pub use ::dsp::reverb::*;
    #[cfg(feature = "reverb-ui")]
    pub mod ui;
}

#[cfg(feature = "saturate")]
pub mod saturate {
    pub use ::dsp::saturate::*;
    #[cfg(feature = "saturate-ui")]
    pub mod ui;
}

#[cfg(feature = "stack")]
pub mod stack {
    pub use ::dsp::stack::*;
    #[cfg(feature = "stack-host")]
    pub mod host;
}

#[cfg(feature = "trigger")]
#[allow(
    clippy::allow_attributes,
    clippy::allow_attributes_without_reason,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::option_if_let_else,
    clippy::similar_names,
    reason = "preserved from fts-fx trigger module"
)]
pub mod trigger {
    pub use ::dsp::trigger::*;
    #[cfg(feature = "trigger-ui")]
    pub mod ui;
}
