//! The chart as text: a song's Keyflow source in the editor.
//!
//! The editor is `editor-view` (Blitz-native), the source coloured and
//! checked as Keyflow as it is typed (`editor-keyflow`: syntax colours,
//! live diagnostics), under a one-line status: a dot and what the last
//! pause did.
//!
//! The pane alone. What a pause in typing does is the host's: Session's
//! chart editor (`session_ui::chart_editor`) lays the chart over the song
//! and saves it; the setlists extension's song workspace re-engraves the
//! chart beside it and saves the `.kf` back to the song. Both draw this.

use dioxus::prelude::*;

/// The pane's colours: Session's window's (`session_ui::shell`).
const RULE: &str = "#2a2c31";
const TEXT: &str = "#e5e7eb";
const DIM: &str = "#8b9099";

/// The editor's own theme, in the window's colours.
const EDITOR_THEME: &str = "
:root {
    --background: #101114;
    --muted: #1a1c20;
    --foreground: #e5e7eb;
    --muted-foreground: #8b9099;
    --primary: #3aa0ff;
}
";

/// How the status dot reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChartTextStatus {
    /// Nothing done yet: grey.
    #[default]
    Idle,
    /// Done: green.
    Done,
    /// Refused, or done in part: amber.
    Warn,
}

impl ChartTextStatus {
    const fn color(self) -> &'static str {
        match self {
            Self::Idle => DIM,
            Self::Done => "#4ac26b",
            Self::Warn => "#e3b341",
        }
    }
}

/// Keyflow's decorations of `state`: what the pane colours a chart with
/// when its host adds none of its own.
#[must_use]
pub fn keyflow_decorations(state: &editor_state::EditorState) -> Vec<editor_state::DecoratedRange> {
    keyflow_editor_lang::lang::keyflow_decorations(state)
}

/// The chart's text pane: `state` in the editor, `message` beside a dot
/// coloured by `status`. `decorations` replaces Keyflow's own (a host adds
/// others' carets to them); `on_press` is told of a press in the pane
/// (Session: its keys stop being shortcuts while it is typed in).
#[component]
pub fn ChartText(
    state: Signal<editor_state::EditorState>,
    message: String,
    #[props(default)] status: ChartTextStatus,
    #[props(default)] decorations: Option<editor_view::DecorationSource>,
    #[props(default)] on_press: Option<EventHandler<()>>,
) -> Element {
    let css = keyflow_editor_lang::lang::highlight_css(
        &keyflow_editor_lang::lang::HighlightTheme::default_dark(),
    );
    let decorations =
        decorations.unwrap_or_else(|| editor_view::DecorationSource::new(keyflow_decorations));
    let dot = status.color();
    rsx! {
        style { dangerous_inner_html: editor_view::EDITOR_CSS }
        style { dangerous_inner_html: "{css}" }
        style { dangerous_inner_html: EDITOR_THEME }
        div {
            style: "position:absolute; top:0; left:0; right:0; bottom:0; display:flex; \
                    flex-direction:column; background:#101114;",
            onmousedown: move |event| {
                if let Some(press) = on_press {
                    press.call(());
                }
                event.stop_propagation();
            },
            div {
                style: "flex:none; display:flex; align-items:center; gap:8px; height:26px; \
                        padding:0 10px; border-bottom:1px solid {RULE}; font-size:11px;",
                span { style: "flex:none; width:7px; height:7px; border-radius:4px; background:{dot};" }
                span {
                    style: "color:{DIM}; overflow:hidden; white-space:nowrap;",
                    "{message}"
                }
            }
            div {
                style: "flex:1; min-height:0; overflow:scroll; padding:8px 12px; color:{TEXT}; \
                        font-family: ui-monospace, Menlo, monospace; font-size:13px;",
                editor_view::Editor { state, decorations: Some(decorations) }
            }
        }
    }
}
