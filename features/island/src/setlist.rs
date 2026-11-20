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

use std::collections::BTreeMap;

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
.sl { font-family: var(--font, system-ui, -apple-system, sans-serif); color: var(--foreground, #f4f4f5);
      padding: 4px 20px 28px; box-sizing: border-box; width: 100%; }
.sl * { box-sizing: border-box; }
.sl ol { list-style: none; margin: 0; padding: 0; }
.sl-wash { height: 400px; margin: -4px -20px -396px; }
.sl-hero { position: relative; display: flex; gap: 16px; align-items: stretch; margin: 6px 0 4px; }
.sl-cover { width: 140px; height: 140px; flex: none; border-radius: 14px; overflow: hidden; position: relative;
            box-shadow: inset 0 0 0 1px rgba(255,255,255,.08); }
.sl-cover img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
.sl-htext { flex: 1; min-width: 0; display: flex; flex-direction: column; justify-content: space-between; gap: 8px; }
.sl-cal { width: 40px; height: 40px; border-radius: 9px; display: flex; flex-direction: column; align-items: center;
          justify-content: center; background: rgba(255,255,255,.04); box-shadow: inset 0 0 0 1px rgba(255,255,255,.09); }
.sl-cal b { font-size: 9px; font-weight: 800; letter-spacing: .05em; color: rgba(248,113,113,.85); text-transform: uppercase; }
.sl-cal span { font-size: 16px; font-weight: 700; line-height: 1.05; }
.sl-title { font-size: 30px; line-height: 1.12; font-weight: 750; letter-spacing: -.02em; margin: 0; }
.sl-sub { font-size: 14px; color: var(--muted-foreground, #8b8b94); margin-top: 2px; }
.sl-count { font-size: 12.5px; color: #80808a; margin-top: 3px; }
.sl-shead { position: relative; display: flex; align-items: center; justify-content: space-between; margin: 22px 2px 8px; }
.sl-section { font-size: 13px; font-weight: 650; margin: 0; color: var(--muted-foreground, #8b8b94); }
.sl-n { font-size: 12.5px; color: #80808a; }
.sl-card { position: relative; background: var(--card, #111114); border: 1px solid #1c1c21; border-radius: 16px; overflow: hidden; }
.sl-card li + li::before { content: ""; display: block; height: 1px; margin-left: 72px; background: #1f1f24; }
.sl-row { display: flex; align-items: center; gap: 14px; width: 100%; min-height: 68px; padding: 10px 14px;
          border: 0; margin: 0; background: transparent; color: inherit; font: inherit; text-align: left;
          position: relative; cursor: pointer; }
.sl-row:active { background: rgba(255,255,255,.06); }
.sl-row:focus-visible { outline: 2px solid var(--accent, #f0c24b); outline-offset: -2px; }
.sl-row[aria-disabled="true"] { cursor: default; }
.sl-row[aria-disabled="true"] .sl-tile, .sl-row[aria-disabled="true"] .sl-text { opacity: .55; }
.sl-tile { width: 44px; height: 44px; flex: none; border-radius: 8px; position: relative; overflow: hidden;
           display: flex; align-items: center; justify-content: center; color: #fff;
           box-shadow: inset 0 0 0 1px rgba(255,255,255,.08); }
.sl-art { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
.sl-eq { position: absolute; inset: 0; display: flex; gap: 3px; align-items: flex-end; justify-content: center;
         padding-bottom: 12px; background: rgba(0,0,0,.45); }
.sl-eq > span { width: 3px; background: #f0c24b; border-radius: 1px; }
.sl-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.sl-name, .sl-by { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.sl-name { font-size: 16px; font-weight: 620; }
.sl-by { font-size: 13px; color: var(--muted-foreground, #8b8b94); }
.sl-key { flex: none; height: 26px; min-width: 38px; padding: 0 8px; border-radius: 7px; display: flex;
          align-items: center; justify-content: center; gap: 3px; font-size: 13.5px; font-weight: 700; }
.sl-key small { font-size: 9.5px; font-weight: 600; color: #8b8b94; }
.sl-nokey { flex: none; height: 26px; padding: 0 8px; border-radius: 7px; display: flex; align-items: center;
            font-size: 11px; font-weight: 600; color: #8b8b94; box-shadow: inset 0 0 0 1px #3a3a42; }
.sl-row[data-current] .sl-name { color: #f0c24b; }
.sl-bar { position: absolute; left: 72px; right: 14px; bottom: 6px; height: 2px; border-radius: 1px;
          background: rgba(255,255,255,.1); overflow: hidden; }
.sl-bar > span { display: block; height: 100%; background: #f0c24b; }
.sl-where { display: flex; align-items: center; gap: 7px; height: 40px; padding: 0 12px 0 10px; border-radius: 10px;
            background: rgba(255,255,255,.025); box-shadow: inset 0 0 0 1px rgba(255,255,255,.06); color: #7a7a83; min-width: 0; }
.sl-where div { display: flex; flex-direction: column; line-height: 1.15; min-width: 0; }
.sl-where b { font-size: 13px; font-weight: 600; color: #9a9aa3; white-space: nowrap; }
.sl-where small { font-size: 11px; color: #85858e; white-space: nowrap; }
.sl-where em { font-style: normal; color: rgba(240,194,75,.75); font-weight: 600; }
.sl-facts { display: flex; gap: 8px; align-items: center; }
.sl-leads { display: flex; gap: 12px; font-size: 12px; color: #8b8b94; margin-top: 2px; }
.sl-leads span { display: flex; align-items: center; gap: 4px; white-space: nowrap; }
.sl-end { flex: none; display: flex; flex-direction: column; align-items: flex-end; gap: 8px; }
.sl-arr { display: flex; align-items: center; gap: 4px; font-size: 12px; font-weight: 600; color: #e0a63a; white-space: nowrap; }
.sl-empty { padding: 16px 0; color: var(--muted-foreground, #8b8b94); }
"#;

/// The setlist view, over the app's props.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[component]
pub fn SetlistView(
    page: Signal<serde_json::Value>,
    note: Signal<serde_json::Value>,
    ask: crate::views::Ask,
) -> Element {
    use std::cell::RefCell;
    use std::rc::Rc;

    let now_ms = js_sys::Date::now;
    // The list apart from where it is: rebuilt only when the list changes,
    // not at each tick of the playhead.
    let list = use_memo(move || queue::setlist_of_note(&note.read()));
    // Each song's cover, as the app handed it (a `data:` URL by its path).
    let art = use_memo(move || queue::images_of(&queue::files_of(&note.read())));
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
        Rows { setlist, now: now(), playable: playable(), art: art(), on_press,
            // A row's song, opened: its chart and lyrics. The app opens it
            // where the viewer may (a note the link lists) and refuses
            // otherwise; the view only asks.
            on_open: move |item: Item| {
                if let Some(note) = queue::note_of(&item.id) {
                    ask.call(("NOTE_OPEN".to_owned(), serde_json::json!({ "path": note })));
                }
            },
        }
    }
}

/// The list: its title, a desk's header, each set a list of its own.
#[component]
pub fn Rows(
    setlist: Setlist,
    now: Option<Now>,
    playable: Vec<String>,
    /// Covers by path, `data:` URLs.
    #[props(default)]
    art: BTreeMap<String, String>,
    on_press: EventHandler<Item>,
    /// A row's "open" pressed: to its song's note.
    #[props(default)]
    on_open: EventHandler<Item>,
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
    let (name, sub) = split_title(&title);
    let songs = setlist.items().count();
    let secs: f64 = setlist.items().filter_map(|i| i.length).sum();
    let count = if secs > 0.0 {
        format!("{} · {}", plural(songs), queue::clock(secs))
    } else {
        plural(songs)
    };
    let cover = setlist.art.as_ref().and_then(|a| art.get(a)).cloned();
    let wash = title_hue(&title);
    let cal = cal_of(&setlist.date);
    rsx! {
        // The setlist as the prototype's set page: its cover, its day,
        // its name, then each set on a card under its name and count.
        div { class: "sl", "data-testid": "studio-setlist", role: "region", "aria-label": "{title}",
            div { class: "sl-wash", "aria-hidden": "true",
                style: "background: linear-gradient(180deg, hsl({wash} 45% 15%) 0px, transparent 400px);" }
            div { class: "sl-hero",
                div { class: "sl-cover", style: "{cover_wash(wash)}",
                    if let Some(src) = &cover { img { src: "{src}", alt: "" } }
                }
                div { class: "sl-htext",
                    div { class: "sl-facts",
                        if let Some((mon, day)) = cal {
                            div { class: "sl-cal", "aria-hidden": "true", b { "{mon}" } span { "{day}" } }
                        }
                        if !setlist.room.is_empty() {
                            div { class: "sl-where",
                                svg { view_box: "0 0 24 24", width: "14", height: "14", fill: "none", stroke: "currentColor",
                                    stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "M20 10c0 6-8 12-8 12s-8-6-8-12a8 8 0 0 1 16 0Z" }
                                    circle { cx: "12", cy: "10", r: "3" }
                                }
                                div {
                                    b { "{setlist.room}" }
                                    small {
                                        if !setlist.call.is_empty() { em { "Call {setlist.call}" } }
                                        if !setlist.call.is_empty() && !setlist.starts.is_empty() { " · " }
                                        if !setlist.starts.is_empty() { "Starts {setlist.starts}" }
                                    }
                                }
                            }
                        }
                    }
                    div {
                        h1 { class: "sl-title", "{name}" }
                        if !sub.is_empty() { div { class: "sl-sub", "{sub}" } }
                        div { class: "sl-count", "{count}" }
                    }
                }
            }
            for (n, section) in setlist.sections.iter().enumerate() {
                div { key: "s{n}",
                    div { class: "sl-shead",
                        h2 { class: "sl-section", id: "sl-set-{n}", {section.name.clone().unwrap_or_else(|| "Songs".to_owned())} }
                        span { class: "sl-n", "{plural(section.items.len())}" }
                    }
                    ol { class: "sl-card", "aria-labelledby": "sl-set-{n}",
                        for item in section.items.iter() {
                            li { key: "{item.id}",
                                Row { item: item.clone(), now: now.clone(), playable: playable.contains(&item.id), cover: item.art.as_ref().and_then(|a| art.get(a)).cloned(), on_press, on_open }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// `Merry & Bright - CBU Christmas 2026`: its name, and what follows the
/// dash under it.
fn split_title(title: &str) -> (String, String) {
    match title.split_once(" - ") {
        Some((a, b)) => (a.trim().to_owned(), b.trim().to_owned()),
        None => (title.to_owned(), String::new()),
    }
}

fn plural(n: usize) -> String {
    if n == 1 {
        "1 song".to_owned()
    } else {
        format!("{n} songs")
    }
}

/// A hue from a title, for a cover that has no image.
fn title_hue(title: &str) -> u32 {
    title
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)))
        % 360
}

/// The abstract cover of a hue (the launcher's `Cover`).
fn cover_wash(h: u32) -> String {
    let b = (h + 300) % 360;
    format!(
        "background: radial-gradient(circle at 65% 62%, hsl({h} 70% 62%) 0%, hsl({b} 75% 50%) 26%, transparent 55%), \
         linear-gradient(160deg, hsl({h} 45% 30%), hsl({h} 40% 9%));"
    )
}

/// `2026-12-12` as its month's three letters and its day.
fn cal_of(date: &str) -> Option<(&'static str, u32)> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut parts = date.split('-');
    let _year = parts.next()?;
    let month: usize = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.get(..2)?.parse().ok()?;
    Some((MONTHS.get(month.checked_sub(1)?)?, day))
}

/// A key's hue: a step round the circle of fifths a twelfth of the wheel
/// (the launcher's `key_hue`).
fn key_hue(key: &str) -> u32 {
    const FIFTHS: [&str; 12] = [
        "C", "G", "D", "A", "E", "B", "F#", "Db", "Ab", "Eb", "Bb", "F",
    ];
    let root = key.split(['→', '*', 'm', ' ']).next().unwrap_or(key).trim();
    let root = match root {
        "Gb" => "F#",
        "C#" => "Db",
        "G#" => "Ab",
        "D#" => "Eb",
        "A#" => "Bb",
        k => k,
    };
    FIFTHS
        .iter()
        .position(|k| *k == root)
        .and_then(|i| u32::try_from(i).ok())
        .map_or(0, |i| (i * 30 + 20) % 360)
}

#[component]
fn Row(
    item: Item,
    now: Option<Now>,
    playable: bool,
    cover: Option<String>,
    on_press: EventHandler<Item>,
    on_open: EventHandler<Item>,
) -> Element {
    let current = now.as_ref().filter(|n| n.id == item.id);
    let playing = current.is_some_and(|n| n.playing);
    let progress = current.map(|n| n.progress() * 100.0);
    let by = match (item.artist.is_empty(), item.recording) {
        (_, true) => item.artist.clone(),
        (true, false) => "No recording".to_owned(),
        (false, false) => format!("{} · No recording", item.artist),
    };
    let label = queue::row_label(&item, current.is_some(), playing, playable);
    let (r, g, b) = queue::tile_rgb(item.hue);
    let pressed = item.clone();
    let opened = item.clone();
    let has_note = queue::note_of(&item.id).is_some();
    let _ = (on_open, has_note, opened);
    let kh = key_hue(&item.key);
    let shown_key = item.key.trim_end_matches('*').to_owned();
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
            span { class: "sl-tile", style: "background: rgb({r} {g} {b});", "aria-hidden": "true",
                if let Some(src) = &cover {
                    img { class: "sl-art", src: "{src}", alt: "", "data-testid": "row-art" }
                } else {
                    svg { view_box: "0 0 24 24", width: "20", height: "20", fill: "none", stroke: "currentColor",
                        stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                        path { d: "M9 18V5l12-2v13" }
                        circle { cx: "6", cy: "18", r: "3" }
                        circle { cx: "18", cy: "16", r: "3" }
                    }
                }
                if playing {
                    span { class: "sl-eq",
                        span { style: "height: 30%;" } span { style: "height: 50%;" } span { style: "height: 22%;" }
                    }
                }
            }
            span { class: "sl-text", "aria-hidden": "true",
                span { class: "sl-name", "{item.title}" }
                span { class: "sl-by", "{by}" }
                if item.vocal.is_some() || item.guitar.is_some() {
                    span { class: "sl-leads",
                        if let Some(v) = &item.vocal {
                            span {
                                svg { view_box: "0 0 24 24", width: "12", height: "12", fill: "none", stroke: "currentColor",
                                    stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" }
                                    path { d: "M19 10v2a7 7 0 0 1-14 0v-2M12 19v3" }
                                }
                                "{v}"
                            }
                        }
                        if let Some(g) = &item.guitar {
                            span { style: "color: #e0806a;",
                                svg { view_box: "0 0 24 24", width: "12", height: "12", fill: "none", stroke: "currentColor",
                                    stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "m11.9 12.1 4.514-4.514" }
                                    path { d: "M20.1 2.3a1 1 0 0 0-1.4 0l-1.114 1.114A2 2 0 0 0 17 4.828v1.344a2 2 0 0 1-.586 1.414A2 2 0 0 1 17.828 7h1.344a2 2 0 0 0 1.414-.586L21.7 5.3a1 1 0 0 0 0-1.4z" }
                                    path { d: "m6 16 2 2" }
                                    path { d: "M8.2 9.9C8.7 8.8 9.8 8 11 8c2.8 0 5 2.2 5 5 0 1.2-.8 2.3-1.9 2.8l-.9.4A2 2 0 0 0 12 18a4 4 0 0 1-4 4c-3.3 0-6-2.7-6-6a4 4 0 0 1 4-4 2 2 0 0 0 1.8-1.2z" }
                                }
                                "{g}"
                            }
                        }
                    }
                }
            }
            span { class: "sl-end", "aria-hidden": "true",
                if item.key.is_empty() {
                    span { class: "sl-nokey", "Set key" }
                } else {
                    span { class: "sl-key",
                        style: "background: hsl({kh} 40% 15%); color: hsl({kh} 70% 75%);",
                        "{shown_key}"
                        if let Some(m) = &item.moved { small { "{m}" } }
                    }
                }
                if let Some(a) = &item.arrangement {
                    span { class: "sl-arr",
                        svg { view_box: "0 0 24 24", width: "13", height: "13", fill: "none", stroke: "currentColor",
                            stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                            circle { cx: "6", cy: "3", r: "0" }
                            path { d: "M6 3v12" } circle { cx: "18", cy: "6", r: "3" } circle { cx: "6", cy: "18", r: "3" }
                            path { d: "M18 9a9 9 0 0 1-9 9" }
                        }
                        "{a}"
                    }
                }
            }
            if let Some(p) = progress {
                span { class: "sl-bar", "aria-hidden": "true", span { style: "width: {p:.2}%;" } }
            }
        }
    }
}

#[cfg(test)]
mod tests;
