//! The setlist view (web bundle only; its rows render on the host too, for
//! the tests): a setlist as a streaming app's playlist, the share page's
//! list (`setlists::share`, Architect's `player.js`) rebuilt as a view of
//! Studio's own bundle for the app's frame host, mounted as
//! [`queue::VIEW`].
//!
//! Its title (on a desk only: on a phone the app's own heading names the
//! note), then each set as a list of its own under its name, each song a
//! row: its number, its tile (its colour with its key on it and how far
//! the set moves it), its title over who it is by, its length. The song
//! playing is marked (its number becomes a bar glyph, its title takes the
//! accent) with a line under it that follows the playhead; a row pressed
//! plays its song. A song the source cannot queue (no recording, nothing
//! the host takes) is listed, quieter, and asks nothing. Two layouts by
//! the frame's width: a phone's (the artist under the title) and a desk's
//! (a header, the artist in a column of its own).
//!
//! What it reads and asks is [`crate::queue`]'s; its colours are the
//! theme the app hands the frame (`--foreground`, `--muted-foreground`,
//! `--accent`, `--border`, `--font`); nothing of a palette is its own but
//! each song's tile, coloured from its title as the share page's (darkened
//! where its key would not read: [`queue::tile_rgb`]).

use dioxus::prelude::*;

use crate::queue::{self, Item, Now, Setlist};

/// The view's sheet: the frame's `style-src 'unsafe-inline'` takes it, and
/// it is the same on every host. Colours only through the theme's
/// variables (system colours as their fallbacks, before a theme arrives);
/// `color-mix()` after a plain fallback (Safari before 16.2 has none).
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(dead_code, reason = "drawn by the web bundle only")
)]
pub const SHEET: &str = r#"
.sl { font-family: var(--font, system-ui, -apple-system, sans-serif); color: var(--foreground, CanvasText);
      padding: 4px 0 12px; box-sizing: border-box; width: 100%; }
.sl * { box-sizing: border-box; }
.sl-head { display: none; }
.sl ol { list-style: none; margin: 0; padding: 0; }
.sl-section { font-size: 11px; font-weight: 600; letter-spacing: .08em; text-transform: uppercase; margin: 0;
              color: var(--muted-foreground, GrayText); padding: 18px 0 6px; }
.sl-row { display: grid; grid-template-columns: 22px 48px minmax(0, 1fr) auto; align-items: center; column-gap: 12px;
          width: 100%; min-height: 64px; padding: 8px 0; border: 0; margin: 0; background: transparent; color: inherit;
          font: inherit; text-align: left; position: relative; cursor: pointer; border-radius: 8px; }
.sl-row:hover { background: rgba(128, 128, 128, .12);
                background: color-mix(in srgb, var(--foreground, CanvasText) 6%, transparent); }
.sl-row:focus-visible { outline: 2px solid var(--accent, Highlight); outline-offset: -2px; }
.sl-row[aria-disabled="true"] { cursor: default; }
.sl-row[aria-disabled="true"]:hover { background: transparent; }
.sl-row[aria-disabled="true"] .sl-tile, .sl-row[aria-disabled="true"] .sl-text { opacity: .55; }
.sl-num { font-size: 14px; color: var(--muted-foreground, GrayText); text-align: right; font-variant-numeric: tabular-nums; }
.sl-tile { width: 48px; height: 48px; border-radius: 6px; position: relative; overflow: hidden; display: flex;
           align-items: center; justify-content: center; color: #fff; }
.sl-key { font-size: 17px; font-weight: 700; letter-spacing: -.01em; }
.sl-key[data-size="s"] { font-size: 14px; }
.sl-key[data-size="xs"] { font-size: 12px; }
.sl-moved { position: absolute; right: 3px; bottom: 2px; font-size: 11px; font-weight: 700; }
.sl-text { display: flex; flex-direction: column; min-width: 0; gap: 2px; }
.sl-name, .sl-by, .sl-artist { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.sl-name { font-size: 15px; font-weight: 500; }
.sl-by { font-size: 13px; color: var(--muted-foreground, GrayText); }
.sl-artist { display: none; font-size: 14px; color: var(--muted-foreground, GrayText); }
.sl-len { font-size: 13px; color: var(--muted-foreground, GrayText); font-variant-numeric: tabular-nums; text-align: right; }
.sl-row[data-current] .sl-name, .sl-row[data-current] .sl-num { color: var(--accent, Highlight); }
.sl-bar { position: absolute; left: 0; right: 0; bottom: 2px; height: 2px; border-radius: 1px;
          background: rgba(128, 128, 128, .25);
          background: color-mix(in srgb, var(--foreground, CanvasText) 12%, transparent); overflow: hidden; }
.sl-bar > span { display: block; height: 100%; background: var(--accent, Highlight); }
.sl-eq { display: inline-flex; gap: 2px; align-items: flex-end; height: 14px; }
.sl-eq > span { width: 3px; background: var(--accent, Highlight); border-radius: 1px; }
.sl-empty { padding: 16px 0; color: var(--muted-foreground, GrayText); }
@media (min-width: 640px) {
  .sl-head, .sl-row { grid-template-columns: 28px 48px minmax(0, 3fr) minmax(0, 2fr) 64px; }
  .sl-head { display: grid; column-gap: 12px; font-size: 12px; color: var(--muted-foreground, GrayText);
             padding-top: 6px; padding-bottom: 6px; border-bottom: 1px solid var(--border, GrayText); margin-bottom: 4px; }
  .sl-head > span:first-child { text-align: right; }
  .sl-head > span:last-child { text-align: right; }
  .sl-by { display: none; }
  .sl-artist { display: block; }
}
"#;

/// The setlist view, over the app's props.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[component]
pub fn SetlistView(page: Signal<serde_json::Value>, ask: crate::views::Ask) -> Element {
    use std::cell::RefCell;
    use std::rc::Rc;

    let now_ms = js_sys::Date::now;
    // The list apart from where it is: rebuilt only when the list changes,
    // not at each tick of the playhead.
    let list = use_memo(move || queue::setlist_of(&page.read()));
    let now = use_memo(move || queue::now_of(&page.read()));
    let outbox = use_hook(|| Rc::new(RefCell::new(queue::Outbox::default())));
    // The ids its rows may play: written only when they change.
    let playable = use_signal(Vec::<String>::new);
    let send = move |asks: Vec<queue::Ask>| {
        for (action, args) in asks {
            ask.call((action.to_owned(), args));
        }
    };
    let sync = {
        let outbox = outbox.clone();
        move || {
            let ids = outbox.borrow().ids().to_vec();
            if *playable.peek() != ids {
                let mut shown = playable;
                shown.set(ids);
            }
        }
    };
    {
        let (outbox, sync) = (outbox.clone(), sync.clone());
        use_effect(move || {
            if let Some(s) = list() {
                outbox.borrow_mut().set_list(s, now_ms());
                sync();
            }
        });
    }
    {
        // The outbox's clock: the list once it settles, a refused `SET`
        // again.
        let (outbox, sync) = (outbox.clone(), sync.clone());
        use_future(move || {
            let (outbox, sync) = (outbox.clone(), sync.clone());
            async move {
                loop {
                    gloo_timers::future::TimeoutFuture::new(200).await;
                    let asks = outbox.borrow_mut().tick(&page.peek(), now_ms());
                    sync();
                    send(asks);
                }
            }
        });
    }
    let on_press = move |item: Item| {
        let asks = outbox.borrow_mut().press(&item, &page.peek(), now_ms());
        sync();
        send(asks);
    };
    let Some(setlist) = list() else {
        return rsx! {
            document::Style { {SHEET} }
            div { class: "sl", p { class: "sl-empty", "No setlist." } }
        };
    };
    rsx! {
        document::Style { {SHEET} }
        Rows { setlist, now: now(), playable: playable(), on_press }
    }
}

/// The list: its title, a desk's header, each set a list of its own.
#[component]
pub fn Rows(
    setlist: Setlist,
    now: Option<Now>,
    playable: Vec<String>,
    on_press: EventHandler<Item>,
) -> Element {
    if setlist.items().next().is_none() {
        // A share link's guest is handed no body (architect#648): the
        // songs are there, not shared here.
        let said = if setlist.withheld {
            "This setlist's songs are not shared here."
        } else {
            "No songs in this setlist yet."
        };
        return rsx! {
            div { class: "sl", p { class: "sl-empty", "{said}" } }
        };
    }
    let title = if setlist.title.is_empty() {
        "Setlist".to_owned()
    } else {
        setlist.title.clone()
    };
    rsx! {
        // No heading of its own: the host names the note (its tab, its
        // place in the navigator); the list keeps the name for a screen
        // reader.
        div { class: "sl", "data-testid": "studio-setlist", role: "region", "aria-label": "{title}",
            div { class: "sl-head", "aria-hidden": "true",
                span { "#" } span {} span { "Title" } span { "Artist" } span { "Length" }
            }
            for (n, section) in setlist.sections.iter().enumerate() {
                if let Some(name) = &section.name {
                    div { key: "s{n}",
                        h2 { class: "sl-section", id: "sl-set-{n}", "{name}" }
                        ol { "aria-labelledby": "sl-set-{n}",
                            for item in section.items.iter() {
                                li { key: "{item.id}",
                                    Row { item: item.clone(), now: now.clone(), playable: playable.contains(&item.id), on_press }
                                }
                            }
                        }
                    }
                } else {
                    ol { key: "s{n}", "aria-label": "{title}",
                        for item in section.items.iter() {
                            li { key: "{item.id}",
                                Row { item: item.clone(), now: now.clone(), playable: playable.contains(&item.id), on_press }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Row(item: Item, now: Option<Now>, playable: bool, on_press: EventHandler<Item>) -> Element {
    let current = now.as_ref().filter(|n| n.id == item.id);
    let playing = current.is_some_and(|n| n.playing);
    let progress = current.map(|n| n.progress() * 100.0);
    let by = match (item.artist.is_empty(), item.recording) {
        (_, true) => item.artist.clone(),
        (true, false) => "No recording".to_owned(),
        (false, false) => format!("{} · No recording", item.artist),
    };
    let length = item.length.map(queue::clock).unwrap_or_default();
    let label = queue::row_label(&item, current.is_some(), playing, playable);
    let (r, g, b) = queue::tile_rgb(item.hue);
    let size = match item.key.chars().count() {
        0..=3 => "",
        4..=5 => "s",
        _ => "xs",
    };
    let pressed = item.clone();
    rsx! {
        button {
            r#type: "button",
            class: "sl-row",
            "data-current": if current.is_some() { "" },
            "data-playing": if playing { "" },
            "aria-disabled": if playable { "false" } else { "true" },
            "aria-current": if current.is_some() { "true" },
            "aria-label": "{label}",
            onclick: move |_| {
                if playable {
                    on_press.call(pressed.clone());
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
            span { class: "sl-tile", style: "background: rgb({r} {g} {b});", "aria-hidden": "true",
                if item.key.is_empty() {
                    svg { view_box: "0 0 24 24", width: "22", height: "22", fill: "none", stroke: "currentColor",
                        stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                        path { d: "M9 18V5l12-2v13" }
                        circle { cx: "6", cy: "18", r: "3" }
                        circle { cx: "18", cy: "16", r: "3" }
                    }
                } else {
                    span { class: "sl-key", "data-size": "{size}", "{item.key}" }
                    if let Some(m) = &item.moved {
                        span { class: "sl-moved", "{m}" }
                    }
                }
            }
            span { class: "sl-text", "aria-hidden": "true",
                span { class: "sl-name", "{item.title}" }
                span { class: "sl-by", "{by}" }
            }
            span { class: "sl-artist", "aria-hidden": "true", "{by}" }
            span { class: "sl-len", "aria-hidden": "true", "{length}" }
            if let Some(p) = progress {
                span { class: "sl-bar", "aria-hidden": "true", span { style: "width: {p:.2}%;" } }
            }
        }
    }
}

#[cfg(test)]
mod tests;
