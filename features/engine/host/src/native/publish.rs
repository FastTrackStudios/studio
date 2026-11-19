//! Publish to Task, on the welcome screen: pick songs on this machine — a
//! song folder, an album's folder, a Live set — and put them into the
//! org's library, ready to open anywhere.
//!
//! The card is a remote control: it starts `SESSION_INGEST` on the engine
//! (in this process, or the one this window is linked to — the same
//! `session_engine::ingest` `studio ingest` runs) and shows
//! `SESSION_INGEST_STATUS` until it is done.

use std::path::PathBuf;
use std::time::Duration;

use dioxus::prelude::*;
use serde_json::{Value, json};

const SHEET: &str = "#0e0e11";
const RULE: &str = "#222228";
const RULE_STRONG: &str = "#2b2b31";
const INK: &str = "#e4e4e7";
const INK_3: &str = "#80808a";
const PRIMARY: &str = "#2563eb";
const WARN: &str = "#f59e0b";
const FONT: &str =
    "-apple-system, 'SF Pro Text', system-ui, 'Segoe UI', Roboto, 'Helvetica Neue', sans-serif";

/// What a project's form can be from here.
const FORMS: [(&str, &str); 3] = [("", "No form"), ("album", "Album"), ("concert", "Concert")];

/// How the card stands.
#[derive(Clone, Debug, PartialEq)]
enum Stage {
    /// Picking what to publish.
    Picking,
    /// The engine is at it: its last status.
    Running(Value),
    /// Done: the engine's report.
    Done(Value),
    Failed(String),
}

/// The card. `on_join` opens a live link (the published set's, when one
/// was minted) in the DAW; `on_typing` says when its field has the keys.
#[component]
pub fn Publish(on_join: EventHandler<String>, on_typing: EventHandler<bool>) -> Element {
    let mut inputs = use_signal(Vec::<PathBuf>::new);
    let mut project = use_signal(String::new);
    let mut form = use_signal(|| "");
    let mut link = use_signal(|| true);
    let mut stage = use_signal(|| Stage::Picking);
    let org = super::handoff::welcome_org();

    let mut start = move |org: String| {
        let args = json!({
            "inputs": inputs().iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "org": org,
            "project": Some(project().trim().to_owned()).filter(|p| !p.is_empty()),
            "form": Some(form()).filter(|f| !f.is_empty() && !project().trim().is_empty()),
            "link": link(),
        });
        match run("SESSION_INGEST", &args) {
            Ok(_) => stage.set(Stage::Running(Value::Null)),
            Err(why) => stage.set(Stage::Failed(why)),
        }
    };
    // While the engine is at it, ask how it goes twice a second.
    use_future(move || async move {
        loop {
            futures_timer::Delay::new(Duration::from_millis(500)).await;
            if !matches!(*stage.peek(), Stage::Running(_)) {
                continue;
            }
            let Ok(row) = run("SESSION_INGEST_STATUS", &Value::Null) else {
                continue;
            };
            stage.set(match row["state"].as_str() {
                Some("done") => Stage::Done(row["report"].clone()),
                Some("failed") => {
                    Stage::Failed(row["why"].as_str().unwrap_or("it failed").to_owned())
                }
                _ => Stage::Running(row["status"].clone()),
            });
        }
    });

    let picked = inputs();
    let can_start = !picked.is_empty() && org.is_some();
    let opacity = if can_start { "1" } else { "0.45" };
    let body = match stage() {
        Stage::Picking => rsx! {
            div { style: "display: flex; gap: 8px; flex-wrap: wrap;",
                Small {
                    label: "Choose a folder…".to_owned(),
                    on_press: move |()| {
                        if let Some(dir) = pick_folder() {
                            inputs.write().push(dir);
                        }
                    },
                }
                Small {
                    label: "Choose a file…".to_owned(),
                    on_press: move |()| {
                        if let Some(file) = pick_file() {
                            inputs.write().push(file);
                        }
                    },
                }
                if !picked.is_empty() {
                    Small { label: "Clear".to_owned(), on_press: move |()| inputs.write().clear() }
                }
            }
            for path in picked.iter() {
                span {
                    key: "{path.display()}",
                    style: "font-size: 13px; color: {INK}; overflow: hidden; white-space: nowrap;",
                    "{path.display()}"
                }
            }
            if !picked.is_empty() {
                Field {
                    label: "Project (optional)".to_owned(),
                    hint: "An album or event, e.g. JT's Senior Recital".to_owned(),
                    value: project(),
                    on_input: move |v| project.set(v),
                    on_typing: move |on| on_typing.call(on),
                }
                div { style: "display: flex; gap: 6px; align-items: center; flex-wrap: wrap;",
                    for (value, name) in FORMS {
                        Chip {
                            key: "{value}",
                            label: name.to_owned(),
                            on: form() == value,
                            on_press: move |()| form.set(value),
                        }
                    }
                    Chip {
                        label: "Live link".to_owned(),
                        on: link(),
                        on_press: move |()| link.set(!link()),
                    }
                }
                button {
                    class: "lp",
                    disabled: !can_start,
                    style: "height: 44px; padding: 0 20px; border: none; border-radius: 8px; background: {PRIMARY}; \
                            color: #ffffff; font-family: {FONT}; font-size: 15px; font-weight: 750; cursor: pointer; \
                            opacity: {opacity};",
                    onclick: {
                        let org = org.clone();
                        move |_| {
                            if let Some(org) = org.clone() {
                                start(org);
                            }
                        }
                    },
                    {match &org {
                        Some(org) => format!("Publish to {org}"),
                        None => "Sign in to publish".to_owned(),
                    }}
                }
            }
        },
        Stage::Running(status) => rsx! {
            span { style: "font-size: 14px; color: {INK};", "{running_line(&status)}" }
        },
        Stage::Done(report) => {
            let url = report["link"]["url"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let open = url.clone();
            let again = move |()| {
                inputs.write().clear();
                stage.set(Stage::Picking);
            };
            rsx! {
                span { style: "font-size: 14px; color: {INK}; line-height: 1.4;", "{done_line(&report)}" }
                for why in report["failures"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                    span { key: "{why}", style: "font-size: 12px; color: {WARN};", "{why}" }
                }
                if !url.is_empty() {
                    span { style: "font-size: 12px; color: {INK_3}; overflow: hidden; white-space: nowrap;", "{url}" }
                }
                div { style: "display: flex; gap: 8px;",
                    if !url.is_empty() {
                        Small { label: "Open it".to_owned(), on_press: move |()| on_join.call(open.clone()) }
                    }
                    Small { label: "Publish more".to_owned(), on_press: again }
                }
            }
        }
        Stage::Failed(why) => rsx! {
            span { style: "font-size: 13px; color: {WARN}; line-height: 1.4;", "{why}" }
            Small { label: "Back".to_owned(), on_press: move |()| stage.set(Stage::Picking) }
        },
    };
    rsx! {
        div {
            style: "min-width: 0; display: flex; flex-direction: column; gap: 10px; padding: 16px 18px; \
                    border: 1px solid {RULE}; border-radius: 12px; background: {SHEET}; box-sizing: border-box; \
                    font-family: {FONT}; color: {INK};",
            span { style: "font-size: 16px; font-weight: 750;", "Publish to Task" }
            span { style: "font-size: 13px; color: {INK_3}; line-height: 1.4;",
                "Songs, an album or a Live set from this machine, into your org's library — ready to open anywhere."
            }
            {body}
        }
    }
}

/// The folder dialog: a song's folder, or an album's.
#[cfg(not(target_os = "ios"))]
fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("A song's folder, or an album's")
        .pick_folder()
}

/// The file dialog: a Live set, a `.session` or an `.RPP`.
#[cfg(not(target_os = "ios"))]
fn pick_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("A Live set, a .session or an .RPP")
        .add_filter("Live set or song", &["als", "session", "RPP", "rpp"])
        .pick_file()
}

/// A phone has no dialogs: what it publishes comes from a desktop.
#[cfg(target_os = "ios")]
fn pick_folder() -> Option<PathBuf> {
    None
}

#[cfg(target_os = "ios")]
fn pick_file() -> Option<PathBuf> {
    None
}

/// Run `id` on the engine with `args`, its answer as JSON.
fn run(id: &str, args: &Value) -> Result<Value, String> {
    let bytes = session_engine::actions::run(id, args.to_string().as_bytes())?;
    serde_json::from_slice(&bytes)
        .or_else(|_| Ok(Value::String(String::from_utf8_lossy(&bytes).into_owned())))
}

/// What the engine is doing, in a line.
fn running_line(status: &Value) -> String {
    let n = |k: &str| status[k].as_u64().unwrap_or(0);
    let song = status["song"].as_str().unwrap_or("");
    match status["phase"].as_str().unwrap_or("starting") {
        "uploading" => format!(
            "Uploading {} of {} files — {:.0} of {:.0} MB",
            n("files_done"),
            n("files"),
            n("bytes_sent") as f64 / 1e6,
            n("bytes") as f64 / 1e6
        ),
        "preparing" => format!(
            "Preparing {song} ({} of {})",
            n("songs_done") + 1,
            n("songs")
        ),
        phase if !song.is_empty() => format!("{} {song}…", capitalised(phase)),
        phase => format!("{}…", capitalised(phase)),
    }
}

/// What the engine did, in a line.
fn done_line(report: &Value) -> String {
    let songs = report["songs"].as_array().map_or(0, Vec::len);
    let sent = report["files_sent"].as_u64().unwrap_or(0);
    let mb = report["bytes_sent"].as_u64().unwrap_or(0) as f64 / 1e6;
    let set = report["setlist"]["title"]
        .as_str()
        .map(|t| format!(", the setlist {t}"))
        .unwrap_or_default();
    format!("Published {songs} songs{set}: {sent} files sent ({mb:.0} MB), the rest already there.")
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// A small button.
#[component]
fn Small(label: String, on_press: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: "lp",
            style: "height: 36px; padding: 0 14px; border: 1px solid {RULE_STRONG}; border-radius: 8px; \
                    background: transparent; color: {INK}; font-family: {FONT}; font-size: 14px; \
                    font-weight: 650; cursor: pointer;",
            onclick: move |_| on_press.call(()),
            "{label}"
        }
    }
}

/// A toggle, lit when `on`.
#[component]
fn Chip(label: String, on: bool, on_press: EventHandler<()>) -> Element {
    let (bg, edge) = if on {
        ("rgba(37, 99, 235, 0.18)", "#1e3a8a")
    } else {
        ("transparent", RULE_STRONG)
    };
    rsx! {
        button {
            class: "lp",
            style: "height: 32px; padding: 0 12px; border: 1px solid {edge}; border-radius: 16px; \
                    background: {bg}; color: {INK}; font-family: {FONT}; font-size: 13px; cursor: pointer;",
            onclick: move |_| on_press.call(()),
            "{label}"
        }
    }
}

/// A text field with its hint under it while it is empty (Blitz draws no
/// `placeholder`).
#[component]
fn Field(
    label: String,
    hint: String,
    value: String,
    on_input: EventHandler<String>,
    on_typing: EventHandler<bool>,
) -> Element {
    let empty = value.is_empty();
    rsx! {
        div { style: "position: relative; height: 44px;",
            if empty {
                span {
                    style: "position: absolute; top: 0; left: 12px; height: 44px; display: flex; align-items: center; \
                            font-size: 14px; color: {INK_3}; pointer-events: none; white-space: nowrap; overflow: hidden;",
                    "{hint}"
                }
            }
            input {
                r#type: "text",
                "aria-label": "{label}",
                value: "{value}",
                style: "position: absolute; top: 0; left: 0; width: 100%; height: 44px; padding: 0 12px; \
                        border: 1px solid {RULE_STRONG}; border-radius: 8px; background: transparent; color: {INK}; \
                        font-family: {FONT}; font-size: 14px; box-sizing: border-box;",
                oninput: move |e| on_input.call(e.value()),
                onfocus: move |_| on_typing.call(true),
                onblur: move |_| on_typing.call(false),
                onkeydown: move |e| {
                    // The field's keys are its own, not the shortcuts'.
                    if !e.modifiers().contains(Modifiers::META) && !e.modifiers().contains(Modifiers::CONTROL) {
                        e.stop_propagation();
                    }
                },
            }
        }
    }
}
