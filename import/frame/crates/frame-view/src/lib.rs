//! `frame.surface`, a view kind for architect's embedded views
//! (architect's `docs/content/architecture/embedded-views.md`, #224): a
//! Frame document's controls, painted by Frame, in an architect pane.
//!
//! architect defines the kinds contract and never depends on a provider;
//! this crate is the provider (architect#220's layering). An app that
//! links it declares the kind with [`register`]; an app that does not
//! (architect's lab) runs [`frame-surface`](../frame_surface/index.html),
//! the process extension whose view is of this kind ([`MANIFEST`]), and
//! declares [`spec`]'s row itself.
//!
//! This is the minimal provider #224 describes: a surface painted through
//! Frame's [`Canvas`](frame::embed::Canvas) seam into SVG ([`svg`]) and
//! shown as a Dioxus view ([`view`]), so it draws wherever a Dioxus view
//! does, a remote one included. The GPU kind (a Blitz paint source, Frame's
//! own vello scene) replaces the painter, not the kind.

pub mod surface;
pub mod svg;
pub mod view;

use architect_workspace::{Capability, KindError, KindSpec, Placement, ViewHost, ViewKinds};

/// The kind's id, what an extension's `[[views]] kind` names.
pub const KIND: &str = "frame.surface";

/// Who provides it, as the kinds registry shows.
pub const PROVIDER: &str = "frame-view";

/// `frame-surface`'s manifest: its view (of [`KIND`]) and its actions.
pub const MANIFEST: &str = include_str!("../extension.toml");

/// The kind as this crate provides it: in a desk app's window (Blitz, a
/// remote view included), as a pane or inline; it takes pointer input and
/// its controls run actions.
#[must_use]
pub fn spec() -> KindSpec {
    KindSpec::new(KIND, PROVIDER)
        .host(ViewHost::Blitz)
        .placement(Placement::Pane)
        .placement(Placement::Embed)
        .capability(Capability::Input)
        .capability(Capability::Actions)
}

/// Declares [`KIND`] in `kinds`.
///
/// # Errors
///
/// Another provider registered it first.
pub fn register(kinds: &mut ViewKinds) -> Result<(), KindError> {
    kinds.register(spec())
}
