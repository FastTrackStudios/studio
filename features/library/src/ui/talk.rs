//! The conversation about a thing: its open threads, and a box to say
//! something (Enter or Send). One component for every page that has one.

use dioxus::prelude::*;
use lucide_dioxus::SendHorizontal;

use super::kit::*;
use super::{edit, use_library};
use crate::model::{Anchor, Edit};

/// What has been said about `about` (and, for a setlist or a song, about
/// what is inside it), oldest first, then the box.
#[component]
pub(crate) fn Conversation(about: Anchor, #[props(default)] exact: bool) -> Element {
    let lib = use_library();
    let l = lib.read();
    let me = l.me.clone();
    let comments: Vec<_> = if exact {
        l.threads
            .iter()
            .filter(|t| !t.resolved && t.anchor == about)
            .flat_map(|t| t.comments.clone())
            .collect()
    } else {
        l.threads_on(&about)
            .into_iter()
            .flat_map(|t| t.comments.clone())
            .collect()
    };
    let mut draft = use_signal(String::new);
    let mut send = {
        let about = about.clone();
        move || {
            let text = draft.peek().trim().to_owned();
            if !text.is_empty() {
                edit(Edit::Comment {
                    about: about.clone(),
                    author: me.clone(),
                    text,
                });
                draft.set(String::new());
            }
        }
    };
    let mut send_key = send.clone();
    let empty = draft().trim().is_empty();
    let send_ink = if empty { DIM } else { PICK };
    rsx! {
        div { style: "display: flex; flex-direction: column;",
            for (i, c) in comments.iter().enumerate() {
                div { key: "{i}", style: "display: flex; gap: 10px; align-items: flex-start; padding: 12px 0 10px; border-top: 1px solid {RULE};",
                    Avatar { name: c.author.clone(), size: 28 }
                    div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                        span { style: "{META} color: {INK_2}; font-weight: 650;",
                            "{c.author} "
                            span { style: "color: {INK_3}; font-weight: 500;", "{c.when}" }
                        }
                        span { style: "font-size: 14px; line-height: 1.45; color: {INK};", "{c.text}" }
                    }
                }
            }
            if comments.is_empty() {
                span { style: "{META} color: {INK_3}; padding: 12px 0; border-top: 1px solid {RULE};", "No comments yet." }
            }
            div { style: "display: flex; align-items: center; gap: 8px; margin: 6px 0 10px; height: 44px; padding: 0 6px 0 12px; \
                          border-radius: 10px; border: 1px solid {RULE_STRONG}; background: {SHEET_2};",
                // Blitz has no `placeholder`: the hint sits under the
                // empty field instead.
                div { style: "position: relative; flex: 1; min-width: 0; height: 40px; display: flex; align-items: center;",
                    if empty {
                        span { style: "position: absolute; left: 0; top: 11px; {META} color: {INK_3}; pointer-events: none;",
                            "Say something…"
                        }
                    }
                    input {
                        r#type: "text",
                        value: "{draft}",
                        "aria-label": "Comment",
                        style: "position: relative; width: 100%; height: 40px; border: none; outline: none; background: transparent; \
                                color: {INK}; font-family: {FONT}; font-size: 14px; padding: 0;",
                        oninput: move |e| draft.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                send_key();
                            }
                            // A plain key is the field's (a `?` in a comment
                            // is a question mark); ⌘ ones go to the library.
                            if !super::command::command_held(&e) {
                                e.stop_propagation();
                            }
                        },
                    }
                }
                button {
                    class: "lp",
                    "aria-label": "Send",
                    disabled: empty,
                    style: "width: 36px; height: 36px; border: none; border-radius: 8px; background: transparent; \
                            display: flex; align-items: center; justify-content: center;",
                    onclick: move |_| send(),
                    SendHorizontal { size: 18, color: send_ink }
                }
            }
        }
    }
}
