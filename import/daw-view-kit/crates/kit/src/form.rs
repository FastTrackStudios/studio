//! The window's shape, as the host measured it: which layout the app takes
//! ([`Form`]) and where a phone on its side keeps clear ([`Sides`]). The
//! host provides both as signals; a view reads them with [`use_form`] and
//! [`use_sides`]. Classifying a screen into a form is the app's
//! (studio-panels' `compact::form_of`).

use dioxus::prelude::*;

/// The window's shape, and so which layout the app takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Form {
    /// Room for the wide layout: the top bar and the docked views.
    Wide,
    /// A phone held upright, or a window that narrow.
    Portrait,
    /// A phone on its side, or a window that short.
    Landscape,
}

impl Form {
    /// Whether this is the small-screen layout.
    #[must_use]
    pub const fn compact(self) -> bool {
        !matches!(self, Self::Wide)
    }
}

/// The window's form, as the host measured it (wide when it did not say).
#[must_use]
pub fn use_form() -> Form {
    try_use_context::<Signal<Form>>().map_or(Form::Wide, |form| form())
}

/// Where a phone on its side has its camera housing, and how far the controls
/// beside it keep clear: `right`, the inspector's strip moving in from that
/// edge; `left`, the views' rail moving to the right edge instead (no gap
/// kept — the view runs under the housing) — and how far the top rows keep
/// clear of the screen's rounded corners (`corner`). Nothing upright, or with no host saying. In
/// logical pixels; the host provides it as a `Signal<Sides>`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Sides {
    pub left: f64,
    pub right: f64,
    pub corner: f64,
}

/// The sides, as the host measured them (none when it did not say).
#[must_use]
pub fn use_sides() -> Sides {
    try_use_context::<Signal<Sides>>().map_or_else(Sides::default, |sides| sides())
}
