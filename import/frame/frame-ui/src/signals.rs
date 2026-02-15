//! Global signals for Frame document state.
//!
//! Follows the dock-dioxus GlobalSignal pattern.

use crate::prelude::*;
use frame_proto::*;

/// The current Frame document.
pub static FRAME_DOCUMENT: GlobalSignal<FrameDocument> =
    Signal::global(|| FrameDocument::empty("Untitled"));

/// Currently active page ID.
pub static FRAME_ACTIVE_PAGE: GlobalSignal<Option<NodeId>> = Signal::global(|| None);

/// Currently selected node IDs (supports multi-select).
pub static FRAME_SELECTION: GlobalSignal<Vec<NodeId>> = Signal::global(Vec::new);

/// Hovered node ID (for hover highlight in editor).
pub static FRAME_HOVERED_NODE: GlobalSignal<Option<NodeId>> = Signal::global(|| None);

/// Canvas zoom level (1.0 = 100%).
pub static FRAME_ZOOM: GlobalSignal<f64> = Signal::global(|| 1.0);

/// Canvas scroll/pan offset.
pub static FRAME_PAN_OFFSET: GlobalSignal<Point> = Signal::global(Point::default);

/// Current editing tool.
pub static FRAME_ACTIVE_TOOL: GlobalSignal<FrameTool> = Signal::global(|| FrameTool::Select);

/// Active editing tool mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameTool {
    Select,
    Frame,
    Rectangle,
    Ellipse,
    Line,
    Text,
    Pen,
    Hand,
}
