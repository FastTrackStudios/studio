//! Studio's arrangement, a view only: the recorded scene (`arrangement`),
//! painted by one thing, [`widget::ArrangementWidget`], mounted by
//! [`panel::use_arrangement_panel`] and [`panel::PanelChrome`] wherever the
//! app shows it; its editing (`arrange_edit`, `automation_edit`, `swipe`,
//! `rename`), the ruler, the automation lanes, the takes and comps, and the
//! Organize tools. It draws over daw-view-kit's base (the track panel, the
//! theme, the controls, the dock's links) and hands the window its panels
//! with [`arrangement_host::install`].

// The kit, under the paths this code was written against (`crate::tcp`,
// `crate::skin`, …).
#[allow(unused_imports)]
use daw_view_kit::*;

pub mod arrange_camera;
pub mod arrange_edit;
pub mod arrangement;
pub mod arrangement_host;
pub mod automation_draw;
pub mod automation_edit;
#[cfg(feature = "native")]
pub mod automation_menu;
pub mod comp_predict;
pub mod cursor;
pub mod folder_item;
pub mod ghost_paint;
pub mod hit;

pub mod lane_stack;
pub mod marks_strip;
pub mod notice;

pub mod panel;
pub mod poi_paint;
pub mod rename;
pub mod ruler;
pub mod swipe;
pub mod take_window;
pub mod tcp_live;
pub mod theme;
pub mod theme_tokens;
pub mod widget;
