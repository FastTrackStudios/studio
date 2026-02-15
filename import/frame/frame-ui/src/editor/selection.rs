//! Selection overlay — draws selection rectangles and resize handles
//! on top of the canvas.

use crate::prelude::*;
use crate::signals::*;

/// Overlay component that renders selection indicators on selected nodes.
#[component]
pub fn SelectionOverlay() -> Element {
    let selection = FRAME_SELECTION.read();
    let doc = FRAME_DOCUMENT.read();

    if selection.is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "frame-selection-overlay",
            style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%; pointer-events: none;",

            for &node_id in selection.iter() {
                if let Some(node) = doc.get_node(node_id) {
                    {
                        let left = node.absolute_position.x;
                        let top = node.absolute_position.y;
                        let w = node.size.width;
                        let h = node.size.height;
                        rsx! {
                            div {
                                key: "{node_id}",
                                class: "frame-selection-box",
                                style: "position: absolute; left: {left}px; top: {top}px; width: {w}px; height: {h}px; border: 2px solid #0d99ff; pointer-events: none;",
                            }
                        }
                    }
                }
            }
        }
    }
}
