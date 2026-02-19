//! Canvas — zoom/pan wrapper for the Frame design surface.

use crate::prelude::*;
use crate::render::FrameNodeRenderer;
use crate::signals::*;
use frame_proto::NodeId;

/// The main canvas component that wraps the design surface with zoom/pan.
#[component]
pub fn FrameCanvas() -> Element {
    let zoom = FRAME_ZOOM.read();
    let pan = FRAME_PAN_OFFSET.read();
    let active_page = FRAME_ACTIVE_PAGE.read();

    let page_id: Option<NodeId> = *active_page;

    let transform = format!(
        "transform: translate({}px, {}px) scale({}); transform-origin: 0 0;",
        pan.x, pan.y, *zoom,
    );

    rsx! {
        div {
            class: "frame-canvas",
            style: "position: relative; overflow: hidden; width: 100%; height: 100%;",

            div {
                class: "frame-canvas-content",
                style: "{transform}",

                if let Some(page_id) = page_id {
                    FrameNodeRenderer { node_id: page_id }
                }
            }
        }
    }
}
