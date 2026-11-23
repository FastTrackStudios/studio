//! The session: a set open for playing, taking the whole window. The
//! app's other mode beside the launcher (Home, Setlists, the set pages and
//! the sidebar), as the old app switched from its launcher into Session's
//! frame (`features/engine/ui/src/shell.rs`).
//!
//! Its own layout (`Route::SessionPage`, under the root beside the
//! launcher's `Shell`): an `AppShell` of its own with no sidebars, whose top
//! bar's middle is the setlist bar ([`SetlistBar`]: the set's progress,
//! the song before and after, a tab per song with its own progress) and
//! whose accessory is the bottom bar ([`BottomBar`]: back to sets, the
//! transport, the views, the mixer's own window). The middle is the view
//! on show, the arrangement first. Playback is the root's, so the set
//! plays on across a switch back to the launcher and in again.

use dioxus::prelude::*;

use app_shell::ui::{AppShell, BarSpan, TopBar, use_top_bar_span};

use crate::Route;
use crate::data::Song;
use crate::icons::{I, Icon};
use crate::state::{AppState, Place, use_app};

/// The session's views, as the old bottom bar had them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Overview,
    Chart,
    Lyrics,
    Editor,
    Arrangement,
    Mixer,
    Fx,
    Setup,
}

impl View {
    pub const ALL: [View; 8] = [
        View::Overview,
        View::Chart,
        View::Lyrics,
        View::Editor,
        View::Arrangement,
        View::Mixer,
        View::Fx,
        View::Setup,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            View::Overview => "overview",
            View::Chart => "chart",
            View::Lyrics => "lyrics",
            View::Editor => "editor",
            View::Arrangement => "arrangement",
            View::Mixer => "mixer",
            View::Fx => "fx",
            View::Setup => "setup",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            View::Overview => "Overview",
            View::Chart => "Chart",
            View::Lyrics => "Lyrics",
            View::Editor => "Editor",
            View::Arrangement => "Arrangement",
            View::Mixer => "Mixer",
            View::Fx => "FX",
            View::Setup => "Setup",
        }
    }

    /// A view by its slug (`daw` is the arrangement, as the old app named it).
    pub fn parse(s: &str) -> Option<View> {
        let s = s.trim().to_ascii_lowercase();
        if s == "daw" {
            return Some(View::Arrangement);
        }
        View::ALL.into_iter().find(|v| v.slug() == s)
    }
}

/// Where a view of the session is.
pub fn route(view: View) -> Route {
    Route::SessionPage {
        view: view.slug().into(),
    }
}

/// Into the session: song `i` of a set's songs on show (playing or not),
/// "back to sets" returning to `from`.
#[cfg_attr(not(feature = "drive"), allow(dead_code))]
pub fn enter(
    app: AppState,
    list: Vec<(Song, Place)>,
    name: String,
    i: usize,
    play: bool,
    from: Option<Route>,
    view: View,
) {
    #[cfg(feature = "drive")]
    let _span = {
        let mut span = crate::trace::Span::root("studio.session.open");
        span.set("studio.set", name.clone());
        span.set("studio.set.songs", list.len() as u64);
        if let Some((s, _)) = list.get(i) {
            span.set("studio.song", s.title);
        }
        span.set("studio.session.view", view.slug());
        span
    };
    app.select_places(list, name, i, play);
    if let Some(f) = from {
        let mut back = app.session_from;
        back.set(Some(f));
    }
    navigator().push(route(view));
}

/// Back to the launcher, where the session was opened from. The set plays on.
pub fn leave(app: AppState) {
    let to = app
        .session_from
        .peek()
        .clone()
        .unwrap_or(Route::Setlists {});
    navigator().push(to);
}

/// Song `j` of the set, from its start, playing as the set was.
pub fn go_to(app: AppState, j: usize) {
    let mut np = app.np;
    np.with_mut(|n| {
        if let Some(n) = n
            && j < n.list.len()
            && j != n.i
        {
            n.i = j;
            n.pos = 0.0;
            n.dur = 0.0;
            n.timeline = Default::default();
        }
    });
}

/// The session's frame: its own shell, no sidebars.
#[component]
pub fn SessionShell() -> Element {
    let app = use_app();
    let route = use_route::<Route>();
    let view = match &route {
        Route::SessionPage { view } => View::parse(view).unwrap_or(View::Arrangement),
        _ => View::Arrangement,
    };
    let title = (app.np)()
        .map(|n| n.from.clone())
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| "Session".into());
    #[cfg(feature = "native")]
    use_set_lengths();
    rsx! {
        div { class: "window",
            AppShell {
                theme: use_hook(crate::shell::theme),
                title: title.clone(),
                top_bar: rsx! {
                    TopBar { title: title.clone() }
                },
                accessory: rsx! {
                    BottomBar { view }
                },
                div { class: "content session", "data-form": "desktop",
                    SetlistBarHost {}
                    main { class: "sessstage", Outlet::<Route> {} }
                }
            }
        }
    }
}

/// Each song's length, read from its file off the UI thread when the set
/// changes (the set's progress needs them all).
#[cfg(feature = "native")]
fn use_set_lengths() {
    let app = use_app();
    let folders: Vec<(String, u32)> = (app.np)()
        .map(|n| {
            n.list
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let folder = n
                        .places
                        .get(i)
                        .and_then(Option::as_ref)
                        .map(|p| p.folder.clone())
                        .unwrap_or_default();
                    (folder, s.tempo.unwrap_or(120))
                })
                .collect()
        })
        .unwrap_or_default();
    use_effect(use_reactive!(|folders| {
        let mut lens = app.set_lens;
        spawn(async move {
            let (tx, rx) = futures_channel::oneshot::channel();
            std::thread::spawn(move || {
                let out: Vec<f64> = folders
                    .iter()
                    .map(|(f, bpm)| {
                        if f.is_empty() {
                            return 0.0;
                        }
                        crate::arrangement::of_folder(f, f64::from(*bpm))
                            .map_or(0.0, |(_, a)| a.len)
                    })
                    .collect();
                let _ = tx.send(out);
            });
            if let Ok(out) = rx.await {
                lens.set(out);
            }
        });
    }));
}

/// Puts the setlist bar in the top bar's middle (title, center and
/// trailing items: everything but the window's own controls).
#[component]
fn SetlistBarHost() -> Element {
    use_top_bar_span("studio/setlist", BarSpan::Middle, move || {
        rsx! {
            SetlistBar {}
        }
    });
    rsx! {}
}

/// Song `j`'s length: read from its file, else the list's, else four minutes.
fn len_of(lens: &[f64], songs: &[Song], j: usize) -> f64 {
    lens.get(j)
        .copied()
        .filter(|l| *l > 0.0)
        .or_else(|| songs.get(j).and_then(|s| s.len).map(f64::from))
        .unwrap_or(240.0)
}

/// How far through the set (0 to 1), song `i` at `now` seconds.
pub fn set_progress(lens: &[f64], songs: &[Song], i: usize, now: f64) -> f64 {
    let total: f64 = (0..songs.len()).map(|j| len_of(lens, songs, j)).sum();
    if total <= 0.0 {
        return 0.0;
    }
    let before: f64 = (0..i.min(songs.len()))
        .map(|j| len_of(lens, songs, j))
        .sum();
    ((before + now) / total).clamp(0.0, 1.0)
}

/// The set, along the top: its progress, the song before, a tab per song
/// with its own progress, the song after, and the engine's state.
#[component]
fn SetlistBar() -> Element {
    let app = use_app();
    let Some(n) = (app.np)() else {
        return rsx! {};
    };
    let lens = (app.set_lens)();
    let (now, _) = n.secs();
    let whole = set_progress(&lens, &n.list, n.i, now) * 100.0;
    let count = n.list.len();
    let engine = (app.engine)();
    let (dot, word) = match &engine {
        crate::state::EngineStatus::Ready(_) if (app.engine_drives)() => ("ok", "Engine"),
        crate::state::EngineStatus::Ready(_) => ("idle", "Engine"),
        crate::state::EngineStatus::Linking => ("busy", "Starting"),
        crate::state::EngineStatus::Opening(_) => ("busy", "Loading"),
        crate::state::EngineStatus::Failed(_) => ("bad", "Timer"),
        crate::state::EngineStatus::Off => ("idle", "No engine"),
    };
    rsx! {
        div { class: "sbar",
            div { class: "sbset", title: "The set: {whole:.0}%",
                div { class: "sbsetfill", style: "height:{whole}%;" }
            }
            div { class: "sbname",
                b { "{n.from}" }
                span { "{n.i + 1} of {count}" }
            }
            button {
                class: "sbstep",
                aria_label: "The song before",
                disabled: n.i == 0,
                onclick: move |_| go_to(app, n.i.saturating_sub(1)),
                Icon { name: I::ChevLeft, size: 16, w: 2.2 }
            }
            div { class: "sbtabs",
                for (j , s) in n.list.iter().enumerate() {
                    {
                        let p = match j.cmp(&n.i) {
                            std::cmp::Ordering::Less => 100.0,
                            std::cmp::Ordering::Equal => n.pos * 100.0,
                            std::cmp::Ordering::Greater => 0.0,
                        };
                        rsx! {
                            button {
                                key: "{j}",
                                class: if j == n.i { "sbtab on" } else if j < n.i { "sbtab done" } else { "sbtab" },
                                title: "{s.title}",
                                onclick: move |_| go_to(app, j),
                                span { class: "sbtabt", "{s.title}" }
                                span { class: "sbtabp",
                                    i { style: "width:{p}%;" }
                                }
                            }
                        }
                    }
                }
            }
            button {
                class: "sbstep",
                aria_label: "The song after",
                disabled: n.i + 1 >= count,
                onclick: move |_| go_to(app, n.i + 1),
                Icon { name: I::ChevRight, size: 16, w: 2.2 }
            }
            span { class: "sbbadge {dot}", title: "{engine.line()}",
                i {}
                "{word}"
            }
        }
    }
}

/// The session's foot: back to the sets, the transport, the views and the
/// mixer's own window.
#[component]
fn BottomBar(view: View) -> Element {
    let app = use_app();
    let mut np = app.np;
    let n = np();
    let playing = n.as_ref().is_some_and(|n| n.playing);
    let (now, len) = n.as_ref().map_or((0.0, 0.0), |n| n.secs());
    let section = n
        .as_ref()
        .and_then(|n| n.timeline.at(now).map(|s| s.name.clone()));
    let title = n
        .as_ref()
        .and_then(|n| n.song().map(|s| s.title.to_string()))
        .unwrap_or_default();
    #[cfg(feature = "native")]
    let windows = try_use_context::<app_shell::ui::Windows>();
    rsx! {
        div { class: "sbot",
            button {
                class: "sbback",
                onclick: move |_| leave(app),
                Icon { name: I::ChevLeft, size: 15, w: 2.2 }
                "Sets"
            }
            div { class: "sbtrans",
                button {
                    class: "sbtp",
                    aria_label: "Previous song",
                    onclick: move |_| {
                        let i = np.peek().as_ref().map_or(0, |n| n.i);
                        go_to(app, i.saturating_sub(1));
                    },
                    Icon { name: I::SkipBack, size: 16, w: 2.0 }
                }
                button {
                    class: "sbtp big",
                    aria_label: if playing { "Pause" } else { "Play" },
                    onclick: move |_| {
                        np.with_mut(|n| {
                            if let Some(n) = n {
                                n.playing = !n.playing;
                            }
                        })
                    },
                    Icon { name: if playing { I::Pause } else { I::Play }, size: 18, w: 2.0 }
                }
                button {
                    class: "sbtp",
                    aria_label: "Next song",
                    onclick: move |_| {
                        let i = np.peek().as_ref().map_or(0, |n| n.i);
                        go_to(app, i + 1);
                    },
                    Icon { name: I::Skip, size: 16, w: 2.0 }
                }
                span { class: "sbclock",
                    b { "{title}" }
                    " {crate::songs::clock(now)} / {crate::songs::clock(len)}"
                    if let Some(s) = &section {
                        " · {s}"
                    }
                }
            }
            div { class: "sbviews",
                for v in View::ALL {
                    button {
                        key: "{v.slug()}",
                        class: if v == view { "sbview on" } else { "sbview" },
                        onclick: move |_| {
                            navigator().replace(route(v));
                        },
                        "{v.name()}"
                    }
                }
            }
            {
                #[cfg(feature = "native")]
                {
                    rsx! {
                        button {
                            class: "sbview pop",
                            title: "The mixer in a window of its own",
                            onclick: move |_| {
                                if let Err(e) = open_mixer_window(windows.clone()) {
                                    app.say(e);
                                }
                            },
                            Icon { name: I::Layers, size: 14 }
                            "Mixer window"
                        }
                    }
                }
                #[cfg(not(feature = "native"))]
                {
                    rsx! {}
                }
            }
        }
    }
}

/// A view of the session, by its route.
#[component]
pub fn SessionPage(view: String) -> Element {
    let v = View::parse(&view).unwrap_or(View::Arrangement);
    let body = match v {
        View::Arrangement => rsx! {
            crate::pages::arrangement::ArrangementView {}
        },
        View::Chart => rsx! {
            crate::pages::drive_set::ChartView {}
        },
        View::Mixer => rsx! {
            MixerView {}
        },
        View::Overview => rsx! {
            Overview {}
        },
        other => rsx! {
            div { class: "sessempty",
                b { "{other.name()}" }
                p { class: "pline",
                    "Not in this Studio yet. The arrangement, chart, overview and mixer are; the rest of the session's views come over from the old app next."
                }
            }
        },
    };
    rsx! {
        div { class: "sessview", {body} }
    }
}

/// The set at a glance: every song, its length, where the set is.
#[component]
fn Overview() -> Element {
    let app = use_app();
    let Some(n) = (app.np)() else {
        return rsx! {
            p { class: "pline", "No set is open." }
        };
    };
    let lens = (app.set_lens)();
    let total: f64 = (0..n.list.len()).map(|j| len_of(&lens, &n.list, j)).sum();
    rsx! {
        div { class: "sessover",
            h2 { "{n.from}" }
            p { class: "pline",
                "{n.list.len()} songs · {crate::songs::clock(total)} · {(app.engine)().line()}"
            }
            for (j , s) in n.list.iter().enumerate() {
                button {
                    key: "{j}",
                    class: if j == n.i { "sessrow on" } else { "sessrow" },
                    onclick: move |_| go_to(app, j),
                    span { class: "sessnum", "{j + 1}" }
                    span { class: "sesst", "{s.title}" }
                    span { class: "sesslen", "{crate::songs::clock(len_of(&lens, &n.list, j))}" }
                }
            }
        }
    }
}

// ── the mixer ──────────────────────────────────────────────────────────

/// The id of the mixer's own window.
#[cfg(feature = "native")]
pub const MIXER_WINDOW: &str = "studio-mixer";

/// Opens the mixer in a window of its own (natively).
#[cfg(feature = "native")]
pub fn open_mixer_window(windows: Option<app_shell::ui::Windows>) -> Result<(), String> {
    let windows = windows.ok_or("this build has no other windows")?;
    let mut span = crate::trace::Span::root("studio.mixer.window");
    span.set("studio.window", MIXER_WINDOW);
    windows.0.open(
        &app_shell::WindowId::new(MIXER_WINDOW),
        "Mixer",
        Some(app_shell::Rect {
            x: 160.0,
            y: 140.0,
            w: 1280.0,
            h: 620.0,
        }),
    );
    Ok(())
}

/// The mixer in its own window: its own shell (the window's frame), the
/// app's sheets, the strips. A document of its own, so nothing of the main
/// window's state: it reads and drives the engine itself.
#[cfg(feature = "native")]
#[component]
pub fn MixerWindow(id: app_shell::WindowId) -> Element {
    rsx! {
        document::Stylesheet { href: crate::STYLE }
        document::Stylesheet { href: crate::APP_STYLE }
        div { class: "window",
            AppShell {
                window: id,
                theme: use_hook(crate::shell::theme),
                title: "Mixer".to_string(),
                top_bar: rsx! {
                    TopBar { title: "Mixer".to_string() }
                },
                div { class: "mixwin", MixerView {} }
            }
        }
    }
}

/// A track of the engine's song, as a strip.
#[cfg(feature = "native")]
#[derive(Clone, Debug, PartialEq)]
pub struct Strip {
    pub guid: String,
    pub name: String,
    pub path: String,
    pub depth: usize,
    pub folder: bool,
    pub volume_db: f64,
    pub pan: f64,
    pub muted: bool,
    pub soloed: bool,
}

#[cfg(feature = "native")]
/// The strips worth showing of `SESSION_TRACKS`' answer: the top-level
/// folders and every track with something on it.
pub fn strips(tracks: &serde_json::Value) -> Vec<Strip> {
    use serde_json::Value;
    tracks
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(|t| {
                    let path = t.get("path")?.as_str()?.to_string();
                    let depth = path.matches('/').count();
                    let folder = t.get("folder").and_then(Value::as_bool).unwrap_or(false);
                    let items = t.get("items").and_then(Value::as_u64).unwrap_or(0);
                    if !(items > 0 || (folder && depth == 0)) {
                        return None;
                    }
                    Some(Strip {
                        guid: t.get("guid")?.as_str()?.to_string(),
                        name: t
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        path,
                        depth,
                        folder,
                        volume_db: t.get("volume_db").and_then(Value::as_f64).unwrap_or(0.0),
                        pan: t.get("pan").and_then(Value::as_f64).unwrap_or(0.0),
                        muted: t.get("muted").and_then(Value::as_bool).unwrap_or(false),
                        soloed: t.get("soloed").and_then(Value::as_bool).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The fader's range: -60 dB at the foot, +12 at the top, linear in dB.
#[cfg(feature = "native")]
pub fn fader_db(at: f64) -> f64 {
    -60.0 + 72.0 * at.clamp(0.0, 1.0)
}

#[cfg(feature = "native")]
pub fn fader_at(db: f64) -> f64 {
    ((db + 60.0) / 72.0).clamp(0.0, 1.0)
}

/// The engine's song as a mixer: a strip per track, its fader, pan, mute
/// and solo, driving the engine's tracks (`TRACK_SET`).
#[component]
pub fn MixerView() -> Element {
    #[cfg(feature = "native")]
    {
        rsx! {
            EngineMixer {}
        }
    }
    #[cfg(not(feature = "native"))]
    {
        rsx! {
            p { class: "pline", "The mixer drives the engine, in the desktop app." }
        }
    }
}

#[cfg(feature = "native")]
#[component]
fn EngineMixer() -> Element {
    let mut strips_now = use_signal(Vec::<Strip>::new);
    let mut problem = use_signal(|| None::<String>);
    // Read every so often: the engine's faders move with automation and
    // other windows too.
    use_future(move || async move {
        loop {
            match crate::engine::call("SESSION_TRACKS", serde_json::Value::Null).await {
                Ok(v) => {
                    let s = strips(&v);
                    if *strips_now.peek() != s {
                        strips_now.set(s);
                    }
                    if problem.peek().is_some() {
                        problem.set(None);
                    }
                }
                Err(e) => {
                    if problem.peek().as_deref() != Some(e.as_str()) {
                        problem.set(Some(e));
                    }
                }
            }
            crate::state::sleep_ms(700).await;
        }
    });
    let all = strips_now();
    rsx! {
        div { class: "mixer",
            if let Some(p) = problem() {
                p { class: "pline", "The mixer waits for the engine: {p}" }
            }
            div { class: "mixstrips",
                for s in all {
                    StripView { key: "{s.guid}", strip: s }
                }
            }
        }
    }
}

#[cfg(feature = "native")]
fn set_track(guid: String, args: serde_json::Value) {
    spawn(async move {
        let mut a = args;
        a["track"] = serde_json::Value::String(guid);
        if let Err(e) = crate::engine::call("TRACK_SET", a).await {
            tracing::warn!(error = %e, "mixer: the engine refused a change");
        }
    });
}

#[cfg(feature = "native")]
#[component]
fn StripView(strip: Strip) -> Element {
    use std::rc::Rc;
    let el = use_signal(|| None::<Rc<MountedData>>);
    let mut dragging = use_signal(|| false);
    // What the hand has set, shown until the engine says it back.
    let mut held = use_signal(|| None::<f64>);
    let db = held().unwrap_or(strip.volume_db);
    let at = fader_at(db) * 100.0;
    let unity = fader_at(0.0) * 100.0;
    let guid = strip.guid.clone();
    let move_to = move |y: f64, send: bool| {
        let guid = guid.clone();
        spawn(async move {
            let Some(el) = el() else { return };
            if let Ok(rect) = el.get_client_rect().await
                && rect.height() > 0.0
            {
                let at = 1.0 - ((y - rect.min_y()) / rect.height());
                let db = (fader_db(at) * 10.0).round() / 10.0;
                held.set(Some(db));
                if send {
                    set_track(guid, serde_json::json!({ "volume_db": db }));
                }
            }
        });
    };
    let (g1, g2) = (strip.guid.clone(), strip.guid.clone());
    let (muted, soloed) = (strip.muted, strip.soloed);
    let down = move_to.clone();
    let mv = move_to.clone();
    rsx! {
        div { class: if strip.folder { "mstrip tfolder" } else { "mstrip" },
            div { class: "mdb", "{db:.1}" }
            div {
                class: "mfader",
                onmounted: move |e| {
                    let mut el = el;
                    el.set(Some(e.data()));
                },
                onmousedown: move |e: MouseEvent| {
                    dragging.set(true);
                    down(e.client_coordinates().y, true);
                },
                onmousemove: move |e: MouseEvent| {
                    if dragging() {
                        mv(e.client_coordinates().y, true);
                    }
                },
                onmouseup: move |_| dragging.set(false),
                onmouseleave: move |_| dragging.set(false),
                div { class: "munity", style: "bottom:{unity}%;" }
                div { class: "mfill", style: "height:{at}%;" }
                div { class: "mknob", style: "bottom:{at}%;" }
            }
            div { class: "mbtns",
                button {
                    class: if muted { "mbtn m on" } else { "mbtn m" },
                    onclick: move |_| set_track(g1.clone(), serde_json::json!({ "mute": !muted })),
                    "M"
                }
                button {
                    class: if soloed { "mbtn s on" } else { "mbtn s" },
                    onclick: move |_| set_track(g2.clone(), serde_json::json!({ "solo": !soloed })),
                    "S"
                }
            }
            div { class: "mname", title: "{strip.path}", "{strip.name}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_parse_by_slug() {
        assert_eq!(View::parse("daw"), Some(View::Arrangement));
        assert_eq!(View::parse("Mixer"), Some(View::Mixer));
        assert_eq!(View::parse("nope"), None);
        for v in View::ALL {
            assert_eq!(View::parse(v.slug()), Some(v));
        }
    }

    #[test]
    fn the_set_progresses_by_its_songs_lengths() {
        let song = |len| Song {
            title: "x",
            artist: None,
            key: None,
            tempo: None,
            len: Some(len),
            ready: "",
        };
        let songs = [song(100), song(300)];
        // Read lengths win over the list's.
        assert_eq!(set_progress(&[100.0, 100.0], &songs, 1, 50.0), 0.75);
        assert_eq!(set_progress(&[], &songs, 1, 0.0), 0.25);
        assert_eq!(set_progress(&[], &songs, 0, 0.0), 0.0);
    }

    #[cfg(feature = "native")]
    #[test]
    fn strips_are_folders_on_top_and_tracks_with_media() {
        let v = serde_json::json!([
            {"guid": "a", "name": "Drums", "path": "Drums", "folder": true, "items": 0, "volume_db": -3.0},
            {"guid": "b", "name": "Kick", "path": "Drums/Kick", "folder": false, "items": 2, "muted": true},
            {"guid": "c", "name": "Empty", "path": "Drums/Empty", "folder": false, "items": 0},
            {"guid": "d", "name": "Sub", "path": "Drums/Sub", "folder": true, "items": 0}
        ]);
        let s = strips(&v);
        let names: Vec<_> = s.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Drums", "Kick"]);
        assert!(s[1].muted);
        assert_eq!(s[0].volume_db, -3.0);
    }

    #[cfg(feature = "native")]
    #[test]
    fn the_fader_reads_back() {
        assert_eq!(fader_db(fader_at(0.0)), 0.0);
        assert_eq!(fader_db(0.0), -60.0);
        assert_eq!(fader_db(1.0), 12.0);
    }
}
