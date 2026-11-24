//! The studio as a panel: the painted arrangement, its ruler, its track
//! panel and its scrollbars, in whatever rectangle it is given.
//!
//! Lifted out of `bin/blitz_shot.rs`, where it was one whole window, so the
//! Session app can dock it beside the other panels (`docs/app-on-blitz.md`).
//! What changed on the way:
//!
//! - **Sized by its container**, not by the window. The window may hold
//!   other panels; this one fills the tile it is put in and reads its own
//!   rectangle back from the layout.
//! - **Input only over itself.** Blitz does not send wheel events to the
//!   DOM, so the wheel, the middle-drag hand and the zoom spring are read
//!   at the winit level — and with other panels in the window, only while
//!   the pointer is inside this panel's rectangle.
//! - **No side rails and no modes.** The modes belong to the app's top bar
//!   now, visible whichever view is up.
//!
//! [`StudioSession`] is what the panel stands on: the opened, prepared
//! session and the reads the widget is built from. The app opens it once
//! and provides it as context; every Arrangement panel reads that.

use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Pixels a second at zoom 1 — the studio's base scale.
pub const PPS: f64 = 40.0;

pub use crate::song_model::*;

use crate::panel_input::BAR;

/// The arrangement's queue of edits for the engine, shared with what sits on
/// the panel beside the widget (the main toolbar). Equal by identity.
#[derive(Clone)]
pub struct Edits(pub Rc<RefCell<Vec<crate::engine::Edit>>>);

impl PartialEq for Edits {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A window position as winit gives it (device pixels from the window's
/// corner) in CSS pixels from the page's: the space the panels' rects
/// (`get_client_rect`) and Blitz's own events are in. The safe area comes
/// off as Blitz takes it off its own — except on a Mac, where Blitz draws
/// under the title bar and takes nothing off.
///
/// Without it a 2x screen put the pointer twice as far along as the panel
/// thought it was, and an iPad's status bar further down still.
#[cfg(feature = "native")]
#[must_use]
pub fn css_point(window: &dyn winit::window::Window, x: f64, y: f64) -> (f64, f64) {
    let scale = window.scale_factor().max(f64::EPSILON);
    #[cfg(not(target_os = "macos"))]
    let (left, top) = {
        let insets = window.safe_area();
        (f64::from(insets.left), f64::from(insets.top))
    };
    #[cfg(target_os = "macos")]
    let (left, top) = (0.0, 0.0);
    ((x - left) / scale, (y - top) / scale)
}

/// A wheel's pixel delta, which winit gives in device pixels, in CSS
/// pixels.
#[cfg(feature = "native")]
#[must_use]
pub fn css_delta(window: &dyn winit::window::Window, dx: f64, dy: f64) -> (f64, f64) {
    let scale = window.scale_factor().max(f64::EPSILON);
    (dx / scale, dy / scale)
}

/// One scrollbar: a track, and a thumb saying where in the session the
/// view is and how much of it is on screen. Dragged: the thumb IS the view.
#[component]
pub fn ScrollBar(
    across: bool,
    at: f64,
    travel: f64,
    window: f64,
    left: f64,
    top: f64,
    /// Pinned this far above the foot of the panel instead of at `top`:
    /// laid out by the page, so it moves with the panel's height (a dock
    /// opened or hidden below it) on the very frame the panel does,
    /// rather than when the panel's rectangle is next measured.
    #[props(default)]
    bottom: Option<f64>,
    length: f64,
    /// How thick, in CSS pixels: the lanes' [`BAR`] as drawn, which touch
    /// mode's zoom makes a finger's width.
    #[props(default = BAR)]
    thick: f64,
    colors: daw_ui::studio::lanes::Colors,
    on_move: EventHandler<f64>,
) -> Element {
    let whole = (travel + window).max(1.0);
    // Never smaller than a thumb you can hit, however long the session.
    let thumb = (window / whole * length).max(24.0);
    let along = (at / travel.max(1.0)) * (length - thumb);
    let mut held = use_signal(|| Option::<f64>::None);
    let edge = bottom.map_or_else(|| format!("top:{top}px;"), |b| format!("bottom:{b}px;"));
    let (size, place) = if across {
        (
            format!("left:{left}px; {edge} width:{length}px; height:{thick}px;"),
            format!(
                "left:{along:.1}px; top:1px; width:{thumb:.1}px; height:{}px;",
                thick - 2.0
            ),
        )
    } else {
        (
            format!("left:{left}px; top:{top}px; width:{thick}px; height:{length}px;"),
            format!(
                "left:1px; top:{along:.1}px; width:{}px; height:{thumb:.1}px;",
                thick - 2.0
            ),
        )
    };
    rsx! {
        div {
            style: "position:absolute; {size} background:{colors.tcp_column};",
            // Pointer events rather than mouse ones: Blitz gives a finger
            // pointer and touch events, never mouse ones.
            onpointerdown: move |event| {
                let at = event.data().element_coordinates();
                held.set(Some(if across { at.x } else { at.y }));
            },
            onpointerup: move |_| held.set(None),
            onpointerleave: move |_| held.set(None),
            onpointermove: move |event| {
                if held().is_none() {
                    return;
                }
                let at = event.data().element_coordinates();
                let along = if across { at.x } else { at.y };
                let room = (length - thumb).max(1.0);
                let fraction = ((along - thumb / 2.0) / room).clamp(0.0, 1.0);
                on_move.call(fraction * travel.max(0.0));
            },
            div {
                style: "position:absolute; {place} background:{colors.text_dim}; border-radius:2px;",
            }
        }
    }
}
