//! The setlist view (web bundle only): a setlist as a streaming app's
//! playlist, the share page's list (`setlists::share`, Architect's
//! `player.js`) rebuilt as a view of Studio's own bundle for the app's
//! frame host.
//!
//! Its title, then each set's name as a quiet label and each song a row:
//! its number, its tile (its art, or its colour with its key on it and how
//! far the set moves it), its title over who it is by, its length. The
//! song playing is marked (its number becomes a bar glyph, its title takes
//! the accent) with a line under it that follows the playhead; a row
//! pressed plays its song. A song with no recording is listed, quieter,
//! and asks nothing. Two layouts by the frame's width: a phone's (the
//! artist under the title) and a desk's (a header, the artist in a column
//! of its own).
//!
//! What it reads and asks is [`crate::queue`]'s; its colours are the
//! theme the app hands the frame (`--foreground`, `--muted-foreground`,
//! `--background`, `--accent`, `--border`, `--font`); nothing of a palette
//! is its own but each song's tile, coloured from its title as the share
//! page's.

use dioxus::prelude::*;
use serde_json::Value;

use crate::queue::{self, Item, Now, Setlist};
use crate::views::Ask;

/// The view's sheet: the frame's `style-src 'unsafe-inline'` takes it, and
/// it is the same on every host. Colours only through the theme's
/// variables (system colours as their fallbacks, before a theme arrives).
const SHEET: &str = r#"
.sl { font-family: var(--font, system-ui, -apple-system, sans-serif); color: var(--foreground, CanvasText);
      padding: 4px 0 12px; box-sizing: border-box; width: 100%; }
.sl * { box-sizing: border-box; }
.sl-title { font-size: 20px; font-weight: 700; margin: 4px 16px 10px; line-height: 1.2; }
.sl-head { display: none; }
.sl ol { list-style: none; margin: 0; padding: 0; }
.sl-section { font-size: 11px; font-weight: 600; letter-spacing: .08em; text-transform: uppercase;
              color: var(--muted-foreground, GrayText); padding: 18px 16px 6px; }
.sl-row { display: grid; grid-template-columns: 22px 48px minmax(0, 1fr) auto; align-items: center; column-gap: 12px;
          width: 100%; min-height: 64px; padding: 8px 16px; border: 0; margin: 0; background: transparent; color: inherit;
          font: inherit; text-align: left; position: relative; cursor: pointer; border-radius: 8px; }
.sl-row:hover { background: color-mix(in srgb, var(--foreground, CanvasText) 6%, transparent); }
.sl-row:focus-visible { outline: 2px solid var(--accent, Highlight); outline-offset: -2px; }
.sl-row[aria-disabled="true"] { cursor: default; }
.sl-row[aria-disabled="true"]:hover { background: transparent; }
.sl-row[aria-disabled="true"] .sl-tile, .sl-row[aria-disabled="true"] .sl-text { opacity: .55; }
.sl-num { font-size: 14px; color: var(--muted-foreground, GrayText); text-align: right; font-variant-numeric: tabular-nums; }
.sl-tile { width: 48px; height: 48px; border-radius: 6px; position: relative; overflow: hidden; display: flex;
           align-items: center; justify-content: center; color: #fff; }
.sl-tile img { width: 100%; height: 100%; object-fit: cover; display: block; }
.sl-key { font-size: 17px; font-weight: 700; letter-spacing: -.01em; }
.sl-key[data-size="s"] { font-size: 14px; }
.sl-key[data-size="xs"] { font-size: 11px; }
.sl-moved { position: absolute; right: 3px; bottom: 2px; font-size: 9px; font-weight: 700; opacity: .9; }
.sl-text { display: flex; flex-direction: column; min-width: 0; gap: 2px; }
.sl-name, .sl-by, .sl-artist { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.sl-name { font-size: 15px; font-weight: 500; }
.sl-by { font-size: 13px; color: var(--muted-foreground, GrayText); }
.sl-artist { display: none; font-size: 14px; color: var(--muted-foreground, GrayText); }
.sl-len { font-size: 13px; color: var(--muted-foreground, GrayText); font-variant-numeric: tabular-nums; text-align: right; }
.sl-row[data-current] .sl-name, .sl-row[data-current] .sl-num { color: var(--accent, Highlight); }
.sl-bar { position: absolute; left: 16px; right: 16px; bottom: 2px; height: 2px; border-radius: 1px;
          background: color-mix(in srgb, var(--foreground, CanvasText) 12%, transparent); overflow: hidden; }
.sl-bar > span { display: block; height: 100%; background: var(--accent, Highlight); }
.sl-eq { display: inline-flex; gap: 2px; align-items: flex-end; height: 14px; }
.sl-eq > span { width: 3px; background: var(--accent, Highlight); border-radius: 1px; }
.sl-empty { padding: 16px; color: var(--muted-foreground, GrayText); }
@media (min-width: 640px) {
  .sl-title { font-size: 26px; margin: 8px 24px 14px; }
  .sl-head, .sl-row { grid-template-columns: 28px 48px minmax(0, 3fr) minmax(0, 2fr) 64px; padding-left: 24px; padding-right: 24px; }
  .sl-head { display: grid; column-gap: 12px; font-size: 12px; color: var(--muted-foreground, GrayText);
             padding-top: 6px; padding-bottom: 6px; border-bottom: 1px solid var(--border, GrayText); margin-bottom: 4px; }
  .sl-head > span:first-child { text-align: right; }
  .sl-head > span:last-child { text-align: right; }
  .sl-section { padding-left: 24px; }
  .sl-by { display: none; }
  .sl-artist { display: block; }
  .sl-bar { left: 24px; right: 24px; }
}
"#;

/// The setlist view, over the app's props.
#[component]
pub fn SetlistView(page: Signal<Value>, ask: Ask) -> Element {
    // The list apart from where it is: rebuilt only when the list changes,
    // not at each tick of the playhead.
    let list = use_memo(move || queue::setlist_of(&page.read()));
    let now = use_memo(move || queue::now_of(&page.read()));
    // The list as the extension's source, put in the app's playhead when
    // the list is drawn or changes.
    use_effect(move || {
        if let Some(s) = list() {
            ask.call((queue::PLAYHEAD_SET.to_owned(), s.source()));
        }
    });
    let Some(setlist) = list() else {
        return rsx! {
            document::Style { {SHEET} }
            div { class: "sl", p { class: "sl-empty", "No setlist." } }
        };
    };
    let now = now();
    rsx! {
        document::Style { {SHEET} }
        div { class: "sl", "data-testid": "studio-setlist",
            if !setlist.title.is_empty() {
                h1 { class: "sl-title", "{setlist.title}" }
            }
            div { class: "sl-head", "aria-hidden": "true",
                span { "#" } span {} span { "Title" } span { "Artist" } span { "Length" }
            }
            Rows { setlist, now, ask }
        }
    }
}

#[component]
fn Rows(setlist: Setlist, now: Option<Now>, ask: Ask) -> Element {
    if setlist.items().next().is_none() {
        return rsx! { p { class: "sl-empty", "No songs in this setlist yet." } };
    }
    rsx! {
        ol { "aria-label": "{setlist.title}",
            for (n, section) in setlist.sections.iter().enumerate() {
                if let Some(name) = &section.name {
                    li { key: "s{n}", class: "sl-section", role: "presentation", "{name}" }
                }
                for item in section.items.iter() {
                    li { key: "{item.id}",
                        Row { item: item.clone(), now: now.clone(), ask }
                    }
                }
            }
        }
    }
}

#[component]
fn Row(item: Item, now: Option<Now>, ask: Ask) -> Element {
    let current = now.as_ref().filter(|n| n.id == item.id);
    let playing = current.is_some_and(|n| n.playing);
    let progress = current.map(|n| n.progress() * 100.0);
    let pressed = queue::press(&item, now.as_ref());
    let enabled = pressed.is_some();
    let by = match (item.artist.is_empty(), item.recording) {
        (_, true) => item.artist.clone(),
        (true, false) => "No recording".to_owned(),
        (false, false) => format!("{} · No recording", item.artist),
    };
    let length = item.length.map(queue::clock).unwrap_or_default();
    let label = format!(
        "{}{}{}",
        item.title,
        if item.artist.is_empty() {
            String::new()
        } else {
            format!(", {}", item.artist)
        },
        match (playing, current.is_some(), item.recording) {
            (true, _, _) => ", playing",
            (false, true, _) => ", paused",
            (_, _, false) => ", no recording",
            _ => "",
        }
    );
    rsx! {
        button {
            r#type: "button",
            class: "sl-row",
            "data-current": if current.is_some() { "" },
            "data-playing": if playing { "" },
            "aria-disabled": if enabled { "false" } else { "true" },
            "aria-current": if current.is_some() { "true" },
            "aria-label": "{label}",
            onclick: move |_| {
                if let Some((action, args)) = pressed.clone() {
                    ask.call((action.to_owned(), args));
                }
            },
            span { class: "sl-num", "aria-hidden": "true",
                if playing {
                    span { class: "sl-eq",
                        span { style: "height: 60%;" } span { style: "height: 100%;" } span { style: "height: 40%;" }
                    }
                } else {
                    "{item.number}"
                }
            }
            Tile { item: item.clone() }
            span { class: "sl-text",
                span { class: "sl-name", "{item.title}" }
                span { class: "sl-by", "{by}" }
            }
            span { class: "sl-artist", "{by}" }
            span { class: "sl-len", "{length}" }
            if let Some(p) = progress {
                span { class: "sl-bar", "aria-hidden": "true", span { style: "width: {p:.2}%;" } }
            }
        }
    }
}

/// A song's tile: its art, else its colour with its key (and how far the
/// set moves it), else its colour and a music note.
#[component]
fn Tile(item: Item) -> Element {
    let hue = item.hue;
    let bg = format!("background: hsl({hue} 42% 40%);");
    if let Some(art) = &item.art {
        return rsx! { span { class: "sl-tile", style: "{bg}", img { src: "{art}", alt: "" } } };
    }
    if item.key.is_empty() {
        return rsx! {
            span { class: "sl-tile", style: "{bg}", "aria-hidden": "true",
                svg { view_box: "0 0 24 24", width: "22", height: "22", fill: "none", stroke: "currentColor",
                    stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "M9 18V5l12-2v13" }
                    circle { cx: "6", cy: "18", r: "3" }
                    circle { cx: "18", cy: "16", r: "3" }
                }
            }
        };
    }
    let size = match item.key.chars().count() {
        0..=3 => "",
        4..=5 => "s",
        _ => "xs",
    };
    rsx! {
        span { class: "sl-tile", style: "{bg}", role: "img", "aria-label": "{item.key_said}",
            span { class: "sl-key", "data-size": "{size}", "aria-hidden": "true", "{item.key}" }
            if let Some(m) = &item.moved {
                span { class: "sl-moved", "aria-hidden": "true", "{m}" }
            }
        }
    }
}
