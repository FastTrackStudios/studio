//! Property panel — inspector for the selected node's properties.

use crate::prelude::*;
use crate::signals::*;

/// Property inspector panel for the currently selected node.
#[component]
pub fn PropertyPanel() -> Element {
    let selection = FRAME_SELECTION.read();
    let doc = FRAME_DOCUMENT.read();

    let selected_node = selection.first().and_then(|id| doc.get_node(*id));

    match selected_node {
        Some(node) => {
            rsx! {
                div {
                    class: "frame-property-panel",
                    style: "padding: 12px; font-size: 13px;",

                    // Name
                    div {
                        style: "font-weight: 600; margin-bottom: 8px;",
                        "{node.name}"
                    }

                    // Position
                    div {
                        style: "margin-bottom: 4px;",
                        "X: {node.absolute_position.x:.1}  Y: {node.absolute_position.y:.1}"
                    }

                    // Size
                    div {
                        style: "margin-bottom: 4px;",
                        "W: {node.size.width:.1}  H: {node.size.height:.1}"
                    }

                    // Opacity
                    {
                        let opacity_pct = node.opacity * 100.0;
                        rsx! {
                            div {
                                style: "margin-bottom: 4px;",
                                "Opacity: {opacity_pct:.0}%"
                            }
                        }
                    }
                }
            }
        }
        None => {
            rsx! {
                div {
                    class: "frame-property-panel",
                    style: "padding: 12px; color: #888; font-size: 13px;",
                    "No selection"
                }
            }
        }
    }
}
