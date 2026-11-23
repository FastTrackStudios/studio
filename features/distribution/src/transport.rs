//! Studio's set views as one linked extension (`studio.transport`): a
//! setlist's tabs (Setlist, Chart, Lyrics, More: Arrangement, Mixer,
//! Lighting) and what each opens, in every app of Studio's.
//!
//! The setlist is the island's view (`studio_island`), given what a frame
//! would have been given: the playhead source's state (`page`, re-read on
//! every change of the deck and ten times a second while it plays), the
//! note it is drawn beside (`use_embed`'s `props.note`, or the home's
//! `{home}`), and a way to ask the playhead, as this extension. With
//! `web` the song, lyrics, home and transport are the island's too; on a
//! device they are the native extensions' views (Keyflow's chart, Studio's
//! lyrics and launcher). The arrangement, the mixer and the lighting are
//! always the extensions' (`studio.arrangement`, `studio.mixer`,
//! `ignition`), on the engine the app runs.

use std::sync::{Mutex, PoisonError};

use architect_extensions::linked::{Builder, Extension, Handle};
use dioxus::prelude::*;
use serde_json::{Value, json};

/// The extension's id: the playhead source its views set and read.
const ID: &str = "studio.transport";

/// The host, as this extension: what opens a note a view asks for.
static HANDLE: Mutex<Option<Handle>> = Mutex::new(None);

/// The extension's manifest: its views, its claims (a setlist note, a
/// song note), what they read and a setlist's favourites.
pub const MANIFEST: &str = include_str!("../transport.toml");

/// Studio's set views, linked into every app.
pub struct StudioViews;

impl Extension for StudioViews {
    fn manifest(&self) -> &'static str {
        MANIFEST
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
/// The song's chart: the island's (following the playhead) in a browser,
/// Keyflow's chart on a device.
fn song() -> Element {
    if cfg!(feature = "web") {
        rsx! { Linked { view: "song" } }
    } else {
        host_view("keyflow", "chart")
    }
}
/// The song's lyrics: the island's in a browser, Studio's on a device.
fn lyrics() -> Element {
    if cfg!(feature = "web") {
        rsx! { Linked { view: "lyrics" } }
    } else {
        host_view("studio.lyrics", "lyrics")
    }
}
/// The song's arrangement: Studio's arrangement on the engine in the
/// page (`studio.arrangement`), which a share link's page opens its set
/// in and a member's launcher opens a setlist in.
fn arrangement() -> Element {
    host_view("studio.arrangement", "arrangement")
}
fn home() -> Element {
    if cfg!(feature = "web") {
        rsx! { Linked { view: "home" } }
    } else {
        host_view("studio.launcher", "launcher")
    }
}
/// The song's mixer: Studio's mixer on the engine in the page
/// (`studio.mixer`).
fn mixer() -> Element {
    host_view("studio.mixer", "mixer")
}
/// The song's lighting (the setlist's More menu): Ignition's room, drawn
/// in the page by Ignition's web renderer: its `show` view, as the host
/// has it (deferred in a browser: Ignition's chunk is fetched when this
/// first opens).
fn lighting() -> Element {
    host_view("ignition", "show")
}

/// View `view` of linked extension `extension`, as the host has it
/// (deferred: its chunk fetched when this first draws).
fn host_view(extension: &str, view: &str) -> Element {
    let root = try_use_context::<architect_extensions::interface::ExtensionUi>()
        .and_then(|ui| ui.host().linked_view(extension, view));
    root.map_or_else(|| rsx! {}, |root| rsx! { LinkedView { root: Root(root) } })
}

/// A view the host links, as a component of its own.
#[derive(Clone, Copy)]
struct Root(fn() -> Element);

impl PartialEq for Root {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::fn_addr_eq(self.0, other.0)
    }
}

#[component]
fn LinkedView(root: Root) -> Element {
    (root.0)()
}
/// The song's progress and transport: the island's in a browser; on a
/// device the Now Playing bar is the app's own.
fn progress() -> Element {
    if cfg!(feature = "web") {
        rsx! { Linked { view: "progress" } }
    } else {
        rsx! {}
    }
}
fn transport() -> Element {
    if cfg!(feature = "web") {
        rsx! { Linked { view: "transport" } }
    } else {
        rsx! {}
    }
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
        tracing::warn!("studio: NOTE_OPEN without a path");
        return;
    };
    let Some(handle) = HANDLE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
    else {
        tracing::warn!("studio: NOTE_OPEN before the extension was built");
        return;
    };
    spawn(async move {
        let request = architect_extensions::hostcall::HostRequest::Open { path };
        if let Err(e) = handle.host(request).await {
            tracing::warn!(error = %e, "studio: the host did not open the note");
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
            futures_timer::Delay::new(std::time::Duration::from_millis(100)).await;
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
            tracing::warn!(view.ask = %action, error = %e, "studio: a view's ask was refused");
        }
    });
    #[cfg(feature = "web")]
    return rsx! { studio_island::view::View { view: view.to_owned(), page, note, ask } };
    // On a device the island draws the setlist (Blitz); its other views
    // are the native extensions' (above).
    #[cfg(not(feature = "web"))]
    rsx! { studio_island::setlist::SetlistView { page, note, ask } }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_manifest_reads_and_claims_setlists_and_songs() {
        let manifest: toml::Value = toml::from_str(super::MANIFEST).expect("the manifest is TOML");
        assert_eq!(manifest["id"].as_str(), Some("studio.transport"));
        let views: Vec<&str> = manifest["views"]
            .as_array()
            .expect("views")
            .iter()
            .filter_map(|v| v["id"].as_str())
            .collect();
        for view in ["setlist.playlist", "song", "lyrics", "arrangement", "home"] {
            assert!(views.contains(&view), "the manifest declares {view}");
        }
        let claims: Vec<String> = manifest["embeds"]
            .as_array()
            .expect("embeds")
            .iter()
            .flat_map(|e| {
                e["claims"]["note_type"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .filter_map(|t| t.as_str().map(str::to_owned))
            .collect();
        assert_eq!(claims, ["setlist", "song"]);
    }
}
