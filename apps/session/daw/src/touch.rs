//! Touch mode: controls sized for a finger, and gestures made for one.
//!
//! A mouse can hit a 21-pixel mute button and drag a fader from anywhere
//! on its groove without anyone noticing. A finger covers the button and
//! its neighbours, and a finger landing on a groove is far more often the
//! start of a scroll than a decision to move the level. So touch mode is
//! not the same controls made bigger: it is bigger controls, a fader that
//! only moves when its cap is taken, and empty space that scrolls.
//!
//! On by default where the screen is the pointer (iOS, Android, a browser
//! whose primary pointer is coarse), and a switch everywhere, since a
//! touchscreen laptop is both.

use dioxus::prelude::*;

/// How much bigger everything is in touch mode. REAPER's mute is 21 by
/// 20; at this it is 37 by 35, the far side of the smallest target a
/// fingertip hits without looking.
pub const ZOOM: f64 = 1.75;

/// How much bigger the arrangement is drawn in touch mode: its track
/// panel's 21-pixel buttons to 28, its ruler's 15-pixel lanes to 20, its
/// nine-point names to twelve. Less than the mixer's [`ZOOM`]: the
/// arrangement is mostly lanes, and every pixel of chrome it gains is one
/// the music loses.
pub const ARRANGE_ZOOM: f64 = 1.35;

/// How much bigger to draw the arrangement, as touch mode is, in a panel
/// `width` CSS pixels wide (0 while it is not yet measured).
///
/// A phone held upright is some four hundred wide, and the full zoom would
/// give its track panel more of that than its lanes; there the zoom eases
/// down, reaching the full [`ARRANGE_ZOOM`] at a tablet's width.
#[must_use]
pub fn arrange_zoom(touch: bool, width: f64) -> f64 {
    if !touch {
        return 1.0;
    }
    if width <= 0.0 {
        return ARRANGE_ZOOM;
    }
    let narrow = 1.1;
    let t = ((width - 400.0) / (700.0 - 400.0)).clamp(0.0, 1.0);
    narrow + (ARRANGE_ZOOM - narrow) * t
}

/// How much bigger to draw the mixer's strips, as touch mode is, in a
/// panel `width` CSS pixels wide: whatever puts [`STRIPS_ACROSS`] default
/// strips across it — ten on an 11-inch iPad on its side, at about 1.36 —
/// but never below [`MIXER_ZOOM_MIN`], where a finger stops hitting what
/// it aims at (so an upright tablet or a phone scrolls instead), nor
/// above [`ZOOM`].
#[must_use]
pub fn mixer_zoom(touch: bool, width: f64) -> f64 {
    if !touch {
        return 1.0;
    }
    let across = STRIPS_ACROSS * (crate::mcp::STRIP_W + crate::mcp::STRIP_GAP);
    (width / across).clamp(MIXER_ZOOM_MIN, ZOOM)
}

/// How many default strips a touchscreen's mixer aims to show across.
pub const STRIPS_ACROSS: f64 = 10.0;
/// The least a touchscreen's strips are zoomed: its buttons still a
/// fingertip.
pub const MIXER_ZOOM_MIN: f64 = 1.35;

pub use timeline::scroll::{Fling, PINCH_SPAN, Speed, Step, pinch_step};

/// A way to ask for another frame, for a widget still moving with nothing
/// to prompt a paint (a fling): the window's `request_redraw` under
/// Blitz, which paints a widget only after an event; nothing on a page,
/// which paints every frame anyway. Read from context, so call it where a
/// context can be read (a component, or a hook's closure) — not in a
/// widget's constructor, which tests call with no runtime.
#[must_use]
pub fn redraw_hook() -> Option<std::rc::Rc<dyn Fn()>> {
    #[cfg(feature = "native")]
    {
        let window = try_consume_context::<std::sync::Arc<dyn winit::window::Window>>()?;
        Some(std::rc::Rc::new(move || window.request_redraw()))
    }
    #[cfg(not(feature = "native"))]
    None
}

/// How far a finger may wander, in CSS pixels, and still be a tap. Past
/// it the press is a scroll.
pub const SLOP: f64 = 10.0;

/// Whether touch mode is on: a context the shell provides.
#[derive(Clone, Copy, PartialEq)]
pub struct Touch(pub Signal<bool>);

impl Touch {
    /// Touch mode as the device suggests: on where the screen is the
    /// pointer.
    #[must_use]
    pub fn detect() -> Self {
        Self(Signal::new(detected()))
    }
}

/// Whether touch mode is on here. Off with no [`Touch`] provided.
#[must_use]
pub fn use_touch() -> bool {
    try_use_context::<Touch>().is_some_and(|touch| (touch.0)())
}

/// Whether the device's own pointer is a finger.
#[must_use]
pub fn detected() -> bool {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        return true;
    }
    coarse_pointer()
}

#[cfg(feature = "web")]
fn coarse_pointer() -> bool {
    web_sys::window()
        .and_then(|window| window.match_media("(pointer: coarse)").ok().flatten())
        .is_some_and(|query| query.matches())
}

#[cfg(not(feature = "web"))]
const fn coarse_pointer() -> bool {
    false
}

/// The fingers on a view, for one that only pans and pinches (a chart, a
/// page): one finger drags it, two pinch and drag it together. Fed the
/// view's pointer events; says what each move did.
#[derive(Clone, Debug, Default)]
pub struct Fingers {
    down: Vec<(blitz_traits::events::BlitzPointerId, (f64, f64))>,
}

impl Fingers {
    /// A finger down at `at`. The third and later are not followed.
    pub fn down(&mut self, id: blitz_traits::events::BlitzPointerId, at: (f64, f64)) {
        if self.down.len() < 2 && !self.down.iter().any(|(each, _)| *each == id) {
            self.down.push((id, at));
        }
    }

    /// A finger moved to `at`: what the view should do, if it is one of
    /// the fingers followed — one finger, a drag (spread 1.0); two, a pinch.
    pub fn moved(
        &mut self,
        id: blitz_traits::events::BlitzPointerId,
        at: (f64, f64),
    ) -> Option<Step> {
        let i = self.down.iter().position(|(each, _)| *each == id)?;
        let was = self.down[i].1;
        self.down[i].1 = at;
        Some(match self.down.as_slice() {
            [(_, other0), (_, other1)] => {
                // The other finger stayed where it was.
                let other = if i == 0 { *other1 } else { *other0 };
                pinch_step(was, other, at, other)
            }
            _ => Step {
                sx: 1.0,
                sy: 1.0,
                mid: at,
                dx: at.0 - was.0,
                dy: at.1 - was.1,
            },
        })
    }

    /// A finger lifted (or cancelled). The other, if any, carries on as a
    /// drag from where it is.
    pub fn up(&mut self, id: blitz_traits::events::BlitzPointerId) {
        self.down.retain(|(each, _)| *each != id);
    }

    /// Whether any finger is down.
    #[must_use]
    pub fn any(&self) -> bool {
        !self.down.is_empty()
    }
}
