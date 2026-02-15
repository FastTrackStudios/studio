//! Toolbar — tool selection bar for the editor.

use crate::prelude::*;
use crate::signals::*;

/// Toolbar component for selecting the active editing tool.
#[component]
pub fn FrameToolbar() -> Element {
    let active_tool = FRAME_ACTIVE_TOOL.read();

    let tools = [
        (FrameTool::Select, "Select", "V"),
        (FrameTool::Frame, "Frame", "F"),
        (FrameTool::Rectangle, "Rectangle", "R"),
        (FrameTool::Ellipse, "Ellipse", "O"),
        (FrameTool::Line, "Line", "L"),
        (FrameTool::Text, "Text", "T"),
        (FrameTool::Pen, "Pen", "P"),
        (FrameTool::Hand, "Hand", "H"),
    ];

    rsx! {
        div {
            class: "frame-toolbar",
            style: "display: flex; gap: 4px; padding: 4px;",

            for (tool, label, shortcut) in tools {
                button {
                    class: if *active_tool == tool { "frame-tool-btn active" } else { "frame-tool-btn" },
                    title: "{label} ({shortcut})",
                    onclick: move |_| {
                        *FRAME_ACTIVE_TOOL.write() = tool;
                    },
                    "{label}"
                }
            }
        }
    }
}
