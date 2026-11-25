//! The window's form factor, the one source every view reads it from.
//!
//! A phone, a tablet or a desk ([`FormFactor`]), held upright or on its
//! side ([`Orientation`]), and how far the page keeps clear of the
//! screen's edges itself ([`Insets`]) — together a [`Screen`].
//!
//! The classification is pure ([`FormFactor::of`], [`Screen::of`]); the
//! host publishes the window's [`Screen`] once, as a `Signal<Screen>` in
//! context ([`provide_screen`]; on dioxus-native, feature `native`,
//! [`use_window_screen`] measures the window and keeps it current), and
//! the views read it with [`use_screen`].
//!
//! One set of thresholds, the DAW view's (whose small-screen layout is the
//! most exacting of the four the apps carried): a window **upright and
//! narrower than 700**, or **on its side and shorter than 500**, is a
//! phone's; anything else is a tablet's where the screen is the pointer,
//! and a desk's where it is not.

use dioxus::prelude::*;

/// What sort of screen the window is: which layout the apps take.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FormFactor {
    /// A phone, or a desk window made that small: the one-hand layouts.
    Phone,
    /// A tablet: the wide layouts, under a finger.
    Tablet,
    /// A desk: the wide layouts, under a mouse.
    Desk,
}

impl FormFactor {
    /// Upright and narrower than this is a phone's.
    pub const NARROW: f64 = 700.0;
    /// On its side and shorter than this is a phone's.
    pub const SHORT: f64 = 500.0;

    /// The form of a window `width` × `height` logical pixels, `touch`
    /// where the screen is the pointer.
    #[must_use]
    pub fn of(width: f64, height: f64, touch: bool) -> Self {
        let phone = match Orientation::of(width, height) {
            Orientation::Portrait => width < Self::NARROW,
            Orientation::Landscape => height < Self::SHORT,
        };
        if phone {
            Self::Phone
        } else if touch {
            Self::Tablet
        } else {
            Self::Desk
        }
    }
}

/// Which way the window is held.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Orientation {
    /// As tall as wide, or taller.
    Portrait,
    /// Wider than tall.
    Landscape,
}

impl Orientation {
    #[must_use]
    pub fn of(width: f64, height: f64) -> Self {
        if width <= height {
            Self::Portrait
        } else {
            Self::Landscape
        }
    }
}

/// How far the page keeps clear of each edge itself, in CSS pixels.
///
/// The part of the system's safe area (the status bar, the camera housing,
/// the home indicator) the renderer has not already kept out of the page.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Insets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Insets {
    pub const ZERO: Self = Self {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    /// As a CSS `padding` value: `top right bottom left`.
    #[must_use]
    pub fn padding(self) -> String {
        format!(
            "{}px {}px {}px {}px",
            self.top, self.right, self.bottom, self.left
        )
    }
}

/// The window as the layouts see it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Screen {
    /// The window's size in logical (CSS) pixels.
    pub width: f64,
    pub height: f64,
    pub form: FormFactor,
    pub orientation: Orientation,
    /// What the page keeps clear itself.
    pub insets: Insets,
    /// Whether the screen is the pointer.
    pub touch: bool,
}

impl Screen {
    #[must_use]
    pub fn of(width: f64, height: f64, touch: bool, insets: Insets) -> Self {
        Self {
            width,
            height,
            form: FormFactor::of(width, height, touch),
            orientation: Orientation::of(width, height),
            insets,
            touch,
        }
    }

    /// A desk window of `width` × `height`, nothing to keep clear: what a
    /// view assumes when no host published a screen.
    #[must_use]
    pub fn desk(width: f64, height: f64) -> Self {
        Self::of(width, height, false, Insets::ZERO)
    }

    #[must_use]
    pub fn phone(&self) -> bool {
        self.form == FormFactor::Phone
    }

    #[must_use]
    pub fn landscape(&self) -> bool {
        self.orientation == Orientation::Landscape
    }
}

/// Whether this device's own pointer is a finger (known at build time; a
/// browser asks `(pointer: coarse)` itself).
#[must_use]
pub const fn touch_device() -> bool {
    cfg!(any(target_os = "ios", target_os = "android"))
}

/// Publish `screen` for the views below, unless a host above already did
/// (then its signal is returned).
pub fn provide_screen(screen: impl FnOnce() -> Screen) -> Signal<Screen> {
    use_hook(|| {
        try_consume_context::<Signal<Screen>>()
            .unwrap_or_else(|| provide_context(Signal::new(screen())))
    })
}

/// The screen the host published; a 1194 × 834 desk window when it did
/// not.
#[must_use]
pub fn use_screen() -> Screen {
    try_use_context::<Signal<Screen>>().map_or_else(|| Screen::desk(1194.0, 834.0), |s| s())
}

/// What of the system's safe area `system` the page keeps clear itself.
///
/// Mirrors blitz-shell at the pinned tag
/// (`fts-pointer-2026-10`, `window.rs`'s `get_safe_area_insets`): off a
/// Mac it insets the viewport by all of winit's safe area, on every side,
/// whenever the surface is resized; on a Mac it takes nothing off, and
/// the title bar (the only safe area there) is drawn through on purpose —
/// the bars make room for the window buttons themselves. So today the
/// page has nothing left to keep clear anywhere; a renderer that left the
/// sides to the page would change only this function.
#[must_use]
pub const fn page_insets(system: Insets) -> Insets {
    let _ = system;
    Insets::ZERO
}

#[cfg(feature = "native")]
pub use native::use_window_screen;

#[cfg(feature = "native")]
mod native {
    use super::{Insets, Screen, page_insets, provide_screen, touch_device};
    use dioxus::prelude::*;
    use dioxus_native::winit::{event::WindowEvent, window::Window};

    /// The window's screen as it is now.
    fn measure(window: &dyn Window) -> Option<Screen> {
        let px = window.surface_size();
        if px.width == 0 || px.height == 0 {
            return None;
        }
        let scale = window.scale_factor().max(0.1);
        let safe = window.safe_area();
        let system = Insets {
            top: f64::from(safe.top) / scale,
            right: f64::from(safe.right) / scale,
            bottom: f64::from(safe.bottom) / scale,
            left: f64::from(safe.left) / scale,
        };
        Some(Screen::of(
            f64::from(px.width) / scale,
            f64::from(px.height) / scale,
            touch_device(),
            page_insets(system),
        ))
    }

    /// Whether `now` differs from `was` enough to lay out again.
    fn moved(was: &Screen, now: &Screen) -> bool {
        (was.width - now.width).abs() > 0.5
            || (was.height - now.height).abs() > 0.5
            || was.insets != now.insets
            || was.touch != now.touch
    }

    /// The window's [`Screen`], published once for every view below.
    ///
    /// Kept current: re-measured on a resize, a change of scale and (the
    /// safe area moves without a resize) a redraw. On iOS a window takes
    /// its scene's size only after its first frame, so there it is also
    /// read a few times a second — the one poll the apps keep.
    ///
    /// Called again below a host that already published, it hands back
    /// that host's signal and measures nothing.
    pub fn use_window_screen() -> Signal<Screen> {
        let window = dioxus_native::use_window();
        let owner = use_hook(try_consume_context::<Signal<Screen>>).is_none();
        let first = window.clone();
        let mut screen =
            provide_screen(move || measure(&*first).unwrap_or_else(|| Screen::desk(1194.0, 834.0)));
        let mut update = {
            let window = window.clone();
            move || {
                if let Some(now) = measure(&*window)
                    && moved(&screen.peek(), &now)
                {
                    screen.set(now);
                }
            }
        };
        let mut on_event = update.clone();
        dioxus_native::use_window_event(move |event, _| {
            if owner
                && matches!(
                    event,
                    WindowEvent::SurfaceResized(_)
                        | WindowEvent::ScaleFactorChanged { .. }
                        | WindowEvent::RedrawRequested
                )
            {
                on_event();
            }
        });
        use_hook(move || {
            if owner && cfg!(target_os = "ios") {
                spawn(async move {
                    loop {
                        futures_timer::Delay::new(std::time::Duration::from_millis(300)).await;
                        update();
                    }
                });
            }
        });
        screen
    }
}

#[cfg(test)]
mod tests {
    use super::{FormFactor, Insets, Orientation, Screen};

    #[test]
    fn a_phone_is_a_phone_either_way_up() {
        // iPhone 16 Pro, upright and on its side.
        assert_eq!(FormFactor::of(402.0, 874.0, true), FormFactor::Phone);
        assert_eq!(FormFactor::of(874.0, 402.0, true), FormFactor::Phone);
        // The smallest iPhone still sold, and a Plus on its side.
        assert_eq!(FormFactor::of(375.0, 667.0, true), FormFactor::Phone);
        assert_eq!(FormFactor::of(956.0, 440.0, true), FormFactor::Phone);
    }

    #[test]
    fn a_tablet_is_a_tablet_either_way_up() {
        // iPad Pro 11", and an iPad mini upright (744 wide).
        assert_eq!(FormFactor::of(1194.0, 834.0, true), FormFactor::Tablet);
        assert_eq!(FormFactor::of(834.0, 1194.0, true), FormFactor::Tablet);
        assert_eq!(FormFactor::of(744.0, 1133.0, true), FormFactor::Tablet);
    }

    #[test]
    fn a_desk_window_made_small_takes_the_phone_layout() {
        assert_eq!(FormFactor::of(1440.0, 900.0, false), FormFactor::Desk);
        assert_eq!(FormFactor::of(699.0, 900.0, false), FormFactor::Phone);
        assert_eq!(FormFactor::of(1200.0, 499.0, false), FormFactor::Phone);
    }

    #[test]
    fn the_thresholds_are_the_daw_views() {
        // Upright: 700 wide is wide enough, at any height.
        assert_eq!(FormFactor::of(700.0, 2000.0, false), FormFactor::Desk);
        // On its side: 500 tall is tall enough, even narrower than 700.
        assert_eq!(FormFactor::of(690.0, 500.0, false), FormFactor::Desk);
        assert_eq!(FormFactor::of(690.0, 499.0, false), FormFactor::Phone);
        // Square counts as upright.
        assert_eq!(FormFactor::of(650.0, 650.0, false), FormFactor::Phone);
        assert_eq!(Orientation::of(650.0, 650.0), Orientation::Portrait);
    }

    #[test]
    fn a_screen_carries_its_form_orientation_and_insets() {
        let insets = Insets {
            top: 0.0,
            right: 0.0,
            bottom: 21.0,
            left: 0.0,
        };
        let s = Screen::of(874.0, 402.0, true, insets);
        assert!(s.phone());
        assert!(s.landscape());
        assert_eq!(s.insets.padding(), "0px 0px 21px 0px");
        assert!(!Screen::desk(1194.0, 834.0).phone());
    }
}
