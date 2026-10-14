//! Contextual zoom and scroll over the Editor's notes: MeMagic, by
//! Ilias-Timon Poulakis (MIT), in [`timeline::zoom::contextual`] (with
//! its attribution), fed this document's notes as spans.

use timeline::zoom::contextual::{self as ctx, Span};
pub use timeline::zoom::contextual::{
    Anchor, Config, Horizontal, Modes, Region, Scope, Vertical, pad_to_min_rows, slide_into,
    snap_to_measures,
};

use crate::camera::Content;
use crate::doc::ExpressionDoc;

/// The document's notes, as contextual zoom measures them.
#[must_use]
pub fn spans(doc: &ExpressionDoc) -> Vec<Span> {
    doc.notes
        .iter()
        .map(|n| Span {
            start: n.start,
            end: n.end,
            row: f64::from(n.row),
        })
        .collect()
}

/// [`ctx::smart_span`] over the document's notes.
#[must_use]
pub fn smart_span(doc: &ExpressionDoc, anchor_t: f64, cfg: &Config) -> Option<f64> {
    ctx::smart_span(&spans(doc), anchor_t, cfg)
}

/// [`ctx::pitch_range`] over the document's notes.
#[must_use]
pub fn pitch_range(doc: &ExpressionDoc, from: f64, to: f64) -> Option<(f64, f64)> {
    ctx::pitch_range(&spans(doc), from, to)
}

/// [`ctx::horizontal_span`] over the document's notes.
#[must_use]
pub fn horizontal_span(
    doc: &ExpressionDoc,
    content: Content,
    mode: Horizontal,
    anchor: Anchor,
    units_per_bar: f64,
    current: (f64, f64),
    cfg: &Config,
) -> Option<(f64, f64)> {
    ctx::horizontal_span(
        &spans(doc),
        (content.t_start, content.t_end),
        mode,
        anchor,
        units_per_bar,
        current,
        cfg,
    )
}

/// [`ctx::vertical_range`] over the document's notes.
#[must_use]
pub fn vertical_range(
    doc: &ExpressionDoc,
    content: Content,
    mode: Vertical,
    anchor: Anchor,
    view: (f64, f64),
    cfg: &Config,
) -> Option<(f64, f64)> {
    ctx::vertical_range(
        &spans(doc),
        (content.t_start, content.t_end),
        mode,
        anchor,
        view,
        cfg,
    )
}
