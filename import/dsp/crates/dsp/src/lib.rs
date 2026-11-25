//! Pure Rust audio DSP engines for FastTrackStudio — zero UI or plugin-shell dependencies.
//!
//! Each effect family lives behind a Cargo feature of the same name so downstream
//! plugins, headless engines, and third-party developers can compose filters,
//! dynamics, spectral processing, saturation, reverb, delay, and metering freely.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "align")]
pub mod align;
#[cfg(feature = "analyzer")]
pub mod analyzer;
#[cfg(feature = "comp")]
pub mod comp;
#[cfg(feature = "delay")]
pub mod delay;
#[cfg(feature = "eq")]
pub mod eq;
#[cfg(feature = "gate")]
pub mod gate;
#[cfg(feature = "level")]
pub mod level;
#[cfg(feature = "mic")]
pub mod mic;
#[cfg(feature = "modulation")]
pub mod modulation;
#[cfg(feature = "multiband")]
pub mod multiband;
#[cfg(feature = "pitch")]
pub mod pitch;
#[cfg(feature = "reverb")]
pub mod reverb;
#[cfg(feature = "saturate")]
pub mod saturate;
#[cfg(feature = "stack")]
pub mod stack;
#[cfg(feature = "transient")]
pub mod transient;
#[cfg(feature = "trigger")]
pub mod trigger;
