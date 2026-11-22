//! The player, in a browser: each song's reference (the mix its session
//! bounced, `reference`) streamed through Drive from the computer that
//! has the session, and its sections read from the session file
//! (`session_file`), in one `<audio>` element the page keeps.
//!
//! The element lives in the page, not in the component tree, so it plays
//! on while sheets and bars come and go; a script holds it and talks to
//! [`Player`] over `dioxus.send` / `dioxus.recv`.

use std::cell::RefCell;
use std::collections::HashMap;

use dioxus::prelude::*;
use serde_json::{Value, json};

use crate::files::Hit;
use crate::files_live::web;
use crate::session_file::{self, Timeline};
use crate::state::{Place, Source, use_app};

/// The page's audio element and its messages: what it is doing goes up on
/// every change; `src`, `play` and `seek` come down.
const SCRIPT: &str = r#"
let a = document.getElementById('studio-player');
if (!a) {
  a = document.createElement('audio');
  a.id = 'studio-player';
  a.preload = 'auto';
  a.style.display = 'none';
  document.body.appendChild(a);
}
const send = (ended) => dioxus.send({
  t: a.currentTime || 0,
  d: isFinite(a.duration) ? a.duration : 0,
  p: !a.paused,
  e: ended,
  x: a.ended,
  err: a.error ? a.error.code : 0,
});
a.ontimeupdate = () => send(false);
a.ondurationchange = () => send(false);
a.onplay = () => send(false);
a.onpause = () => send(false);
a.onerror = () => send(false);
a.onended = () => send(true);
while (true) {
  const m = await dioxus.recv();
  if (m.src === '') {
    a.dataset.src = '';
    a.removeAttribute('src');
    a.load();
  } else if (m.src !== undefined && m.src !== a.dataset.src) {
    // Drop the last song's stream first: a browser otherwise keeps
    // loading it, and a few songs in, its connections run out.
    a.removeAttribute('src');
    a.load();
    a.dataset.src = m.src;
    a.src = m.src;
  }
  if (m.seek !== undefined && m.seek !== null) a.currentTime = m.seek;
  if (m.play === true) a.play().catch(() => send(false));
  if (m.play === false) a.pause();
}
"#;

thread_local! {
    /// Every reference each space's indexes have, asked once a page.
    static FOUND: RefCell<HashMap<String, Vec<Hit>>> = RefCell::new(HashMap::new());
    /// Each session file's timeline, read once.
    static TIMELINES: RefCell<HashMap<String, Timeline>> = RefCell::new(HashMap::new());
}

/// The references in one space's indexes.
async fn references(space: &str) -> Vec<Hit> {
    if let Some(h) = FOUND.with(|f| f.borrow().get(space).cloned()) {
        return h;
    }
    let hits: Vec<Hit> = web::search(space, "Reference.ogg", &["ogg".to_string()], 1000)
        .await
        .into_iter()
        .filter(|h| crate::reference::folder_of(&h.path).is_some())
        .collect();
    FOUND.with(|f| f.borrow_mut().insert(space.to_string(), hits.clone()));
    hits
}

/// The reference in a known session folder, when it has been bounced.
async fn in_folder(p: &Place) -> Option<Hit> {
    let proxies = format!("{}/Media/Proxies", p.folder);
    web::list(&p.space, &p.location, &proxies)
        .await
        .into_iter()
        .find(|h| h.name == "Reference.ogg")
}

/// A song's reference: in its session folder when the list knows it,
/// else from the first space (in order) that has one.
async fn find_for(place: Option<&Place>, spaces: &[String], title: &str) -> Option<Hit> {
    match place {
        Some(p) => in_folder(p).await,
        None => find(spaces, title).await,
    }
}

/// A song's reference, from the first space (in order) that has one.
async fn find(spaces: &[String], title: &str) -> Option<Hit> {
    for s in spaces {
        let hits = references(s).await;
        if let Some(h) = crate::reference::pick(title, &hits) {
            return Some(h.clone());
        }
    }
    None
}

/// The timeline of the session beside a reference (its folder's
/// `.fts-session`), when there is one.
async fn timeline(reference: &Hit, bpm: f64) -> Timeline {
    let Some(folder) = crate::reference::folder_of(&reference.path) else {
        return Timeline::default();
    };
    let key = format!("{}/{}/{folder}", reference.space, reference.location);
    if let Some(t) = TIMELINES.with(|t| t.borrow().get(&key).cloned()) {
        return t;
    }
    let files = web::list(&reference.space, &reference.location, folder).await;
    let Some(file) = files.iter().find(|h| h.name.ends_with(".fts-session")) else {
        return Timeline::default();
    };
    let t = match web::bytes("GET", &web::file_url(file), None).await {
        Ok((200 | 206, b)) => session_file::read(&b, bpm).unwrap_or_default(),
        _ => Timeline::default(),
    };
    TIMELINES.with(|m| m.borrow_mut().insert(key, t.clone()));
    t
}

/// Ask for a file's first bytes, so the computer that has it gets it
/// ready (its first request is the slow one).
async fn warm(url: &str) {
    use wasm_bindgen::JsCast;
    let init = web_sys::RequestInit::new();
    init.set_method("GET");
    let Ok(h) = web_sys::Headers::new() else {
        return;
    };
    let _ = h.set("range", "bytes=0-1");
    if let Some(b) = drive_dioxus::auth::bearer() {
        let _ = h.set("authorization", &b);
    }
    init.set_headers(&h);
    let Some(w) = web_sys::window() else { return };
    if let Ok(r) = wasm_bindgen_futures::JsFuture::from(w.fetch_with_str_and_init(url, &init)).await
        && let Ok(r) = r.dyn_into::<web_sys::Response>()
        && let Ok(body) = r.array_buffer()
    {
        let _ = wasm_bindgen_futures::JsFuture::from(body).await;
    }
}

/// Plays what Now Playing holds. Mounted once, in the shell.
#[component]
pub fn Player() -> Element {
    let app = use_app();
    let eval = use_hook(|| document::eval(SCRIPT));

    // What the element says: the playhead, the length, the end.
    {
        let mut eval = eval.clone();
        use_future(move || {
            let mut eval = eval.clone();
            async move {
                let mut np = app.np;
                while let Ok(m) = eval.recv::<Value>().await {
                    let (t, d) = (
                        m["t"].as_f64().unwrap_or(0.0),
                        m["d"].as_f64().unwrap_or(0.0),
                    );
                    let ended = m["e"].as_bool().unwrap_or(false);
                    let err = m["err"].as_i64().unwrap_or(0);
                    let playing = m["p"].as_bool().unwrap_or(false);
                    // The pause an ending fires comes before its end: the
                    // end moves on, still playing.
                    if !ended && m["x"].as_bool().unwrap_or(false) {
                        continue;
                    }
                    np.with_mut(|n| {
                        let Some(n) = n else { return };
                        if ended {
                            if !n.next() {
                                n.playing = false;
                                n.pos = 1.0;
                            }
                            return;
                        }
                        if err != 0 && n.source == Source::Ready {
                            n.source = Source::Missing("its computer did not send it".into());
                        }
                        if d > 0.0 {
                            n.dur = d;
                            n.pos = (t / d).clamp(0.0, 1.0);
                        }
                        n.playing = playing;
                    });
                }
            }
        });
        let _ = &mut eval;
    }

    // The song: find its reference and session, then hand the element its URL.
    let np = (app.np)();
    let key = np
        .as_ref()
        .and_then(|n| n.song().map(|s| format!("{}#{}", n.i, s.title)));
    let title = np
        .as_ref()
        .and_then(|n| n.song().map(|s| s.title.to_string()));
    let place = np.as_ref().and_then(|n| n.place().cloned());
    let bpm = np
        .as_ref()
        .and_then(|n| n.song().and_then(|s| s.tempo))
        .unwrap_or(120) as f64;
    let spaces = {
        let here = (app.here)();
        let mut v = vec![here];
        for o in (app.orgs)() {
            if !v.contains(&o) {
                v.push(o);
            }
        }
        v
    };
    {
        let (eval, spaces) = (eval.clone(), spaces.clone());
        use_effect(use_reactive!(|key| {
            let (eval, title, spaces, place) =
                (eval.clone(), title.clone(), spaces.clone(), place.clone());
            let mut np = app.np;
            if key.is_none() {
                let _ = eval.send(json!({ "play": false }));
                return;
            }
            spawn(async move {
                let Some(title) = title else { return };
                np.with_mut(|n| {
                    if let Some(n) = n {
                        n.source = Source::Finding;
                    }
                });
                let Some(hit) = find_for(place.as_ref(), &spaces, &title).await else {
                    let _ = eval.send(json!({ "src": "", "play": false }));
                    np.with_mut(|n| {
                        if let Some(n) = n {
                            n.source = Source::Missing(if place.is_some() {
                                "its session has no reference bounce yet".into()
                            } else {
                                "no reference bounce on any computer".into()
                            });
                            // Playing a set: on to the next song that has one.
                            if n.playing && !n.next() {
                                n.playing = false;
                            }
                        }
                    });
                    return;
                };
                let playing = np.peek().as_ref().is_some_and(|n| n.playing);
                let _ = eval.send(json!({ "src": web::file_url(&hit), "play": playing }));
                np.with_mut(|n| {
                    if let Some(n) = n {
                        n.source = Source::Ready;
                    }
                });
                let t = timeline(&hit, bpm).await;
                np.with_mut(|n| {
                    if let Some(n) = n
                        && n.song().is_some_and(|s| s.title == title)
                    {
                        n.timeline = t;
                    }
                });
            });
        }));
    }

    // A list started: every song after this one made ready, in order.
    let list_key = np.as_ref().map(|n| {
        let titles: Vec<&str> = n.list.iter().map(|s| s.title).collect();
        format!("{}|{}", n.from, titles.join("|"))
    });
    {
        let spaces = spaces.clone();
        use_effect(use_reactive!(|list_key| {
            let spaces = spaces.clone();
            if list_key.is_none() {
                return;
            }
            let (titles, places, i) = match app.np.peek().as_ref() {
                Some(n) => (
                    n.list
                        .iter()
                        .map(|s| s.title.to_string())
                        .collect::<Vec<_>>(),
                    n.places.clone(),
                    n.i,
                ),
                None => return,
            };
            spawn(async move {
                for (k, t) in titles.iter().enumerate().skip(i + 1) {
                    let place = places.get(k).and_then(Option::as_ref);
                    if let Some(h) = find_for(place, &spaces, t).await {
                        warm(&web::file_url(&h)).await;
                    }
                }
            });
        }));
    }

    // Play and pause as Now Playing says.
    let playing = np.as_ref().is_some_and(|n| n.playing);
    {
        let eval = eval.clone();
        use_effect(use_reactive!(|playing| {
            let _ = eval.send(json!({ "play": playing }));
        }));
    }

    // A jump.
    {
        let eval = eval.clone();
        let mut seek = app.seek;
        use_effect(move || {
            if let Some(t) = seek() {
                let _ = eval.send(json!({ "seek": t }));
                seek.set(None);
            }
        });
    }

    rsx! {}
}
