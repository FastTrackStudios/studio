//! Studio's views as one linked extension (`studio.transport`).
//!
//! Each view is the island's component (`studio_island::view::View`),
//! given what a frame would have been given: the playhead source's state
//! (`page`, re-read on every change of the deck and ten times a second
//! while it plays, as the web app's frame host does), the note it is
//! drawn beside (`use_embed`'s `props.note`, or the home's `{home}`),
//! and a way to ask the playhead, as this extension.

use std::sync::{Mutex, PoisonError};

use architect_extensions::linked::{Builder, Extension, Handle};
use dioxus::prelude::*;
use serde_json::{Value, json};

/// The extension's id: the playhead source its views set and read.
const ID: &str = "studio.transport";

/// The host, as this extension: what opens a note a view asks for.
static HANDLE: Mutex<Option<Handle>> = Mutex::new(None);

/// Studio's views, linked into the web client.
pub struct StudioViews;

impl Extension for StudioViews {
    fn manifest(&self) -> &'static str {
        crate::MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        *HANDLE.lock().unwrap_or_else(PoisonError::into_inner) = Some(ext.handle());
        ext.view("setlist.playlist", setlist)
            .view("song", song)
            .view("lyrics", lyrics)
            .view("arrangement", arrangement)
            .view("home", home)
            .view("mixer", mixer)
            .view("lighting", lighting)
            .view("progress", progress)
            .view("transport", transport);
    }
}

fn setlist() -> Element {
    rsx! { Linked { view: "setlist.playlist" } }
}
fn song() -> Element {
    rsx! { Linked { view: "song" } }
}
fn lyrics() -> Element {
    rsx! { Linked { view: "lyrics" } }
}
fn arrangement() -> Element {
    rsx! { Linked { view: "arrangement" } }
}
fn home() -> Element {
    rsx! { Linked { view: "home" } }
}
fn mixer() -> Element {
    rsx! { Linked { view: "mixer" } }
}
/// The song's lighting (the setlist's More menu): Ignition's room, drawn
/// in the page by Ignition's web renderer (its `show` view, linked in).
fn lighting() -> Element {
    rsx! { ignition_pane::ext::ShowView {} }
}
fn progress() -> Element {
    rsx! { Linked { view: "progress" } }
}
fn transport() -> Element {
    rsx! { Linked { view: "transport" } }
}

/// This extension's source on the deck, flat, with `note`: what a frame
/// is handed (Architect's `apps/web/src/app/embed.rs` `props_of`). Before
/// the views set a source, an empty one's.
fn state(note: &Value) -> Value {
    let now = architect_playhead::now_ms();
    let mut s = architect_playhead::with(|d| d.state(ID, now)).unwrap_or_else(|| {
        json!({
            "source": ID, "name": "", "active": false, "playing": false,
            "time": 0, "at": now, "rate": 1, "duration": 0, "index": null,
            "tracks": 0, "clock": "silent", "item": null, "queue": [],
            "title": null, "subtitle": null, "artist": null, "sections": [],
        })
    });
    if let Some(o) = s.as_object_mut() {
        o.insert("note".to_owned(), note.clone());
    }
    s
}

/// Opens the note at `args.path` through the host, as this extension.
fn open_note(args: Value) {
    let Some(path) = args.get("path").and_then(Value::as_str).map(str::to_owned) else {
        tracing::warn!("studio-web: NOTE_OPEN without a path");
        return;
    };
    let Some(handle) = HANDLE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
    else {
        tracing::warn!("studio-web: NOTE_OPEN before the extension was built");
        return;
    };
    spawn(async move {
        let request = architect_extensions::hostcall::HostRequest::Open { path };
        if let Err(e) = handle.host(request).await {
            tracing::warn!(error = %e, "studio-web: the host did not open the note");
        }
    });
}

/// What a view is drawn beside, from its embed's props: the note
/// (`props.note`), or for the home the whole `{home}` value.
fn note_of(props: &Value) -> Value {
    if props.get("home").is_some() {
        props.clone()
    } else {
        props.get("note").cloned().unwrap_or(Value::Null)
    }
}

#[component]
fn Linked(view: &'static str) -> Element {
    let props = architect_extensions::linked::use_embed().map_or(Value::Null, |e| e.props);
    let given = note_of(&props);
    let mut note = use_signal(|| given.clone());
    use_effect(use_reactive((&given,), move |(given,)| {
        if *note.peek() != given {
            note.set(given);
        }
    }));
    let mut page = use_signal(|| state(&note.peek()));
    use_future(move || async move {
        let mut seen = u64::MAX;
        loop {
            gloo_timers::future::TimeoutFuture::new(100).await;
            let rev = architect_playhead::revision();
            let playing = page.peek()["playing"] == json!(true);
            if playing || rev != seen {
                seen = rev;
                page.set(state(&note.peek()));
            }
        }
    });
    let ask = Callback::new(|(action, args): (String, Value)| {
        // A row or a card opens its note: the host opens it where this
        // person may (a member's workspace; for a link's guest only a note
        // the link lists), as it does for every linked view.
        if action == "NOTE_OPEN" {
            open_note(args);
            return;
        }
        let caller = architect_playhead::Caller::Extension(ID.to_owned());
        if let Err(e) = architect_playhead::ask(&caller, &action, &args) {
            tracing::warn!(view.ask = %action, error = %e, "studio-web: a view's ask was refused");
        }
    });
    rsx! { studio_island::view::View { view: view.to_owned(), page, note, ask } }
}
