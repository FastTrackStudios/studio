//! Studio's mixer, a view only: the strips (`mcp`, `strip_overlay`), the
//! docked and alone mixer panels (`mixer_panel`), the Tone racks (`tone`),
//! the FX view (`fx_view`), close-ups (`closeup`), the plugin faces and
//! macros, and the mixer's view actions (`mixer_actions`). It draws over
//! daw-view-kit's base (the controls, the theme, the dock's links) and hands
//! the window its views with [`mixer_host::install`]. Moving to
//! FastTrackStudios/mixer-view.

// daw-view-kit's base, under the paths this code was written against
// (`crate::tcp`, `crate::skin`, …).
#[allow(unused_imports)]
use daw_view_kit::*;

pub mod app;
pub mod balance;
pub mod closeup;
pub mod faces;
pub mod fx_view;
pub mod live;
pub mod macros;
pub mod mcp;
pub mod mixer_actions;
pub mod mixer_host;
pub mod mixer_panel;
pub mod plugin_faces;
pub mod rails;
pub mod rig;
pub mod signal;
pub mod simulate;
pub mod strip_overlay;
pub mod theme;
pub mod tone;
#[cfg(feature = "native")]
pub mod tone_bind;
pub mod tone_read;
