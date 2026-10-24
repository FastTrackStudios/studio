//! How the strip collapses as it gets shorter.
//!
//! A short mixer has to stay usable, not become a crushed copy of a tall
//! one, so REAPER drops parts of the strip at fixed heights. Those heights
//! are the target — the map's standing rule is that REAPER is the source of
//! truth while mapping — and they are already written down in the theme, so
//! this is data entry rather than design.
//!
//! # It is not one breakpoint ladder
//!
//! Five different kinds of thing, and only the first is a plain height
//! comparison:
//!
//! 1. **Container height.** The input-FX row, the record input, the pan
//!    labels, the pan section and the fader's dB readout each disappear at
//!    their own height.
//! 2. **A derived residual.** What is left after the fixed bands — the
//!    stretch section. The IO, envelope and phase buttons drop at their own
//!    values *of that*, and padding steps down in three stages.
//! 3. **A gap between two resolved siblings.** Not modelled here yet; see
//!    the note on [`Collapse::fader`].
//! 4. **A widget-type switch.** Below a threshold the fader stops being a
//!    fader and becomes a knob — in Dioxus a Rust conditional, not CSS.
//! 5. **A re-anchor.** Below the pan-section threshold the pan *section* is
//!    gone but the pan *control* is not: it re-parents into the input area.
//!    Genuine re-anchoring, not repositioning.
//!
//! The numbers themselves live in [`crate::art::collapse`] — they are
//! facts about the theme, and `fts-themer` needs the same ones to keep the
//! layout file in step. This module is what a strip *does* with them.
//!
//! # Why this is Rust and not a container query
//!
//! A container query can only ask about a container, so (2) and (3) are
//! inexpressible in CSS until the stretch section and the gaps are real
//! boxes. They are real boxes in the tree — that decomposition is most of
//! what this module is for — but the *decision* still has to read a derived
//! number, and a derived number is a Rust expression. Being Rust also makes
//! it testable by sweeping a height across a boundary, which is the only way
//! to prove a threshold fires where REAPER's does.

use crate::art::collapse::{
    BOTTOM_SECTION, FX_SECTION, INPUT_SECTION_FULL, INPUT_SECTION_MINIMAL, INPUT_SECTION_NO_FX,
    PAN_SECTION_COLLAPSED, PAN_SECTION_UNLABELLED,
};
pub use crate::art::collapse::{Bands, REAPER, REAPER_BANDS, Thresholds};

/// Where the pan control lives at a given height.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PanAnchor {
    /// Its own section, under the FX row.
    PanSection,
    /// Re-parented into the input area, because the pan section is gone.
    /// Record mode gives up its place to it.
    InputArea,
}

/// What the volume control is drawn as.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VolumeWidget {
    /// A fader and its rail.
    Fader,
    /// A knob — below the swap threshold there is no room for travel.
    Knob,
}

/// The shape of the strip at one height.
///
/// Everything the strip renders differently is decided here, once, so the
/// markup asks a question rather than doing arithmetic in six places.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Collapse {
    pub show_input_fx: bool,
    pub show_record_input: bool,
    pub show_pan_labels: bool,
    pub show_volume_label: bool,
    pub show_io: bool,
    pub show_envelope: bool,
    pub show_phase: bool,
    pub show_record_mode: bool,
    pub pan: PanAnchor,
    pub volume: VolumeWidget,
    /// Vertical padding between the stacked controls, in px.
    pub padding: f32,
    /// The pan section's own height — 6, 33 or 50, never anything
    /// between. `rtconfig` writes it as a three-way choice and the stretch
    /// section absorbs every other pixel.
    pub pan_band: f32,
    /// The input section's, the same way: 22, 42 or 54.
    pub input_band: f32,
    /// The height left for the fader and meter after the fixed bands.
    pub stretch: f32,
}

impl Collapse {
    /// Resolve the strip's shape at `height`, against REAPER's thresholds.
    pub fn at(height: f32) -> Self {
        Self::with(height, REAPER)
    }

    /// The same, against an arbitrary set — for tests that sweep a boundary
    /// and for a scale other than 1.
    pub fn with(height: f32, t: Thresholds) -> Self {
        let show_pan_labels = height >= t.pan_labels;
        let show_pan_section = height >= t.pan_section;
        let show_record_input = height >= t.record_input;
        let show_input_fx = height >= t.input_fx;

        // The stretch section is a *residual*: what the height has left
        // after the bands above and below it. Which is why the collapses
        // that key off it cannot be written as a query on the strip.
        // 6 or 33, never 50. `rtconfig` writes the section as
        //
        //     !panLabelsMode (h<hide_pan_s ? 6 : 33) : 50
        //
        // and `panLabelsMode` is a *mode* the user turns on, not a height —
        // the 50 belongs to it and to nothing else. Reading it as "tall
        // strips get 50" made the band grow by seventeen rows at exactly
        // the heights where there is most room for it to look wrong, which
        // is why a tall strip was mostly colour.
        //
        // `show_pan_labels` still decides whether the word "pan" is
        // printed; that is `hide_pan_labels`, a different line.
        let pan_band = if show_pan_section {
            PAN_SECTION_UNLABELLED
        } else {
            PAN_SECTION_COLLAPSED
        };
        let input_band = if !show_record_input {
            INPUT_SECTION_MINIMAL
        } else if !show_input_fx {
            INPUT_SECTION_NO_FX
        } else {
            INPUT_SECTION_FULL
        };
        let stretch = (height - FX_SECTION - pan_band - input_band - BOTTOM_SECTION).max(0.0);

        Self {
            show_input_fx,
            show_record_input,
            show_pan_labels,
            show_volume_label: height >= t.volume_label,
            show_io: stretch >= t.io,
            show_envelope: stretch >= t.envelope,
            show_phase: stretch >= t.phase,
            // The other half of the re-anchor: when pan moves into the
            // input area it takes record mode's place, so record mode goes.
            show_record_mode: show_pan_section,
            pan_band,
            input_band,
            pan: if show_pan_section {
                PanAnchor::PanSection
            } else {
                PanAnchor::InputArea
            },
            // Asked of the fader area, which is what REAPER asks it of.
            volume: if crate::art::collapse::fader_area(
                stretch,
                stretch >= t.io,
                stretch >= t.envelope,
                stretch >= t.phase,
            ) >= t.fader_swap
            {
                VolumeWidget::Fader
            } else {
                VolumeWidget::Knob
            },
            padding: if stretch < t.padding_steps.1 {
                2.0
            } else if stretch < t.padding_steps.0 {
                3.0
            } else {
                4.0
            },
            stretch,
        }
    }
}
