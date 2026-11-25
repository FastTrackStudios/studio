//! The studio's shared pieces: the project as one immutable snapshot
//! ([`project`]), the handles that compare it by pointer ([`ProjectRef`],
//! [`RowsRef`]), how a shut folder shows what it holds ([`folded`]) — the
//! views' contract's, `daw_view::song` — and the colours and panel geometry the DOM around the painted arrangement
//! agrees with ([`lanes::Colors`], [`panel::TINT_W`]).
//!
//! The studio window this module was — a DOM arrangement, rails, ruler,
//! transport and track panel for a WRY WebView — is retired. The
//! arrangement is painted by ONE thing, `arrangement_view::widget::
//! ArrangementWidget`, which the Session app mounts; see that crate.
//!
//! # Nothing is copied that could be shared
//!
//! Dioxus hands a component its props by value on every render, so
//! project data travels behind [`ProjectRef`]/[`RowsRef`], which compare
//! by pointer — a plain `Arc<Project>` would deep-compare the whole
//! session every time dioxus checked whether to memoise a child.

pub mod lanes;
pub mod panel;

/// The song model the views draw is the views' contract's (`daw_view::song`),
/// here under the paths the studio code was written against.
pub use daw_view::song::{Project, ProjectRef, RowsRef, folded, project};
