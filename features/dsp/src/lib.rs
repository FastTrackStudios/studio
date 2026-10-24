//! The FTS effects and their engines — one crate, one module (and one cargo feature) per effect.
//!
//! Each module is behind the cargo feature of the same name (`perform_ui` is
//! `perform-ui`); a module's own features are prefixed with it (`eq-ui`).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

/// align-dsp — take-to-take temporal alignment: macro offset search, multi-band frame features, banded DTW, and sparse warp anchors
#[cfg(feature = "align")]
pub mod align;
/// Measure FTS FX against reference plugins — signal generation, metrics, and A/B comparison
#[cfg(feature = "analyzer")]
pub mod analyzer;
/// The compressors (VCA, FET, opto, tube, look-ahead) and the limiter: engines, profiles, editors
#[cfg(feature = "comp")]
pub mod comp;
/// The delay: its machines, their profiles and its editor
#[cfg(feature = "delay")]
pub mod delay;
/// The EQ (bands, dynamic, spectral, transient) and its spectrum analyzer: engines, profiles, editors
#[cfg(feature = "eq")]
pub mod eq;
/// The drum gate with source modes, look-ahead, a shaped release and an HF de-bleed expander on a subtractive crossover; matched by null test against a reference drum gate.
#[cfg(feature = "gate")]
pub mod gate;
/// Drum-hit detection on close mics and triggers
#[cfg(feature = "hits")]
pub mod hits;
/// Levelling: gain riding and loudness
#[cfg(feature = "level")]
pub mod level;
/// FTS Mic: microphone modelling, and its editor
#[cfg(feature = "mic")]
pub mod mic;
/// Modulation: chorus, tremolo and wah engines, their profiles and editors
#[cfg(feature = "modulation")]
pub mod modulation;
/// Reusable multiband splitter core — LR4 crossovers with phase-matched flat sum, for making any FTS effect multiband
#[cfg(feature = "multiband")]
pub mod multiband;
/// Pitch shifting
#[cfg(feature = "pitch")]
pub mod pitch;
/// The reverb: its algorithms, their profiles and its editor
#[cfg(feature = "reverb")]
pub mod reverb;
/// Saturation: its circuits, their profiles and its editor
#[cfg(feature = "saturate")]
pub mod saturate;
/// Split a finished mix into stems for reference analysis — model registry and cascade
#[cfg(feature = "separator")]
pub mod separator;
/// The FX stack core — lanes of serial stages summed in parallel, one topology for every FTS effect kind (docs/spec/fx/stack.md)
#[cfg(feature = "stack")]
pub mod stack;
/// Time stretching
#[cfg(feature = "stretch")]
pub mod stretch;
/// Drum trigger: hit detection to MIDI, and its editor
#[cfg(feature = "trigger")]
pub mod trigger;
/// Pitch detection and correction
#[cfg(feature = "tune")]
pub mod tune;
