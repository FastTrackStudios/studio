//! The FX plugins' parameter trees and the state they share with their
//! editors, one module per plugin (each behind its `fx-*` feature). A
//! plugin runs on these with no editor, as the engine runs the FX
//! in-process; `plugin_ui::<fx>::ui::params` re-exports each for its editor.

#[cfg(feature = "fx-comp")]
pub mod comp;
#[cfg(feature = "fx-delay")]
pub mod delay;
#[cfg(feature = "fx-eq")]
pub mod eq;
#[cfg(feature = "fx-limiter")]
pub mod limiter;
#[cfg(feature = "fx-mic")]
pub mod mic;
#[cfg(feature = "fx-modulation")]
pub mod modulation;
#[cfg(feature = "fx-reverb")]
pub mod reverb;
#[cfg(feature = "fx-saturate")]
pub mod saturate;
#[cfg(feature = "fx-stack")]
pub mod stack_host;
