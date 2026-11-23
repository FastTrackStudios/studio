//! Native playback: Now Playing handed to the engine (`crate::engine`) and
//! the engine's transport handed back to Now Playing, so the playheads
//! follow what the engine plays. Until the engine is there, or when it
//! cannot be, the shell's timer moves the playhead and the window says why.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use dioxus::prelude::*;

use crate::engine::{Engine, Want};
use crate::state::{EngineStatus, use_app};

/// The engine for the window: mounted once, in the shell.
#[component]
pub fn EngineHost() -> Element {
    let mut app = use_app();
    let engine = use_hook(|| Rc::new(Engine::new()));
    let round = use_hook(|| Rc::new(Cell::new(0u64)));
    // What was last asked: the set's files, the song's place, whether it plays.
    type Asked = (Vec<Option<PathBuf>>, usize, bool);
    let last = use_hook(|| Rc::new(RefCell::new(None::<Asked>)));
    // Each song folder's one file, looked up once.
    let files = use_hook(|| Rc::new(RefCell::new(HashMap::<String, Option<PathBuf>>::new())));

    // What plays, to the engine.
    {
        let (engine, round, last, files) =
            (engine.clone(), round.clone(), last.clone(), files.clone());
        use_effect(move || {
            let Some(n) = (app.np)() else { return };
            let title = n.song().map(|s| s.title.to_string()).unwrap_or_default();
            let file_of = |p: &crate::state::Place| -> Option<PathBuf> {
                if p.location != crate::files::DISK || p.folder.is_empty() {
                    return None;
                }
                files
                    .borrow_mut()
                    .entry(p.folder.clone())
                    .or_insert_with(|| crate::arrangement::song_file(&p.folder))
                    .clone()
            };
            let set: Vec<Option<PathBuf>> = n
                .places
                .iter()
                .map(|p| p.as_ref().and_then(file_of))
                .collect();
            let song = set.get(n.i).cloned().flatten().map(|f| (f, title));
            let key = (set.clone(), n.i, n.playing);
            if last.borrow().as_ref() == Some(&key) {
                return;
            }
            *last.borrow_mut() = Some(key);
            round.set(round.get() + 1);
            engine.want(Want {
                set,
                set_name: n.from.clone(),
                current: n.i,
                song,
                playing: n.playing,
                from: n.secs().0,
                round: round.get(),
            });
        });
    }

    // A jump asked for (the progress bar, a section, an action).
    {
        let engine = engine.clone();
        use_effect(move || {
            let Some(t) = (app.seek)() else { return };
            if *app.engine_drives.peek() {
                engine.seek(t);
            }
            app.np.with_mut(|n| {
                if let Some(n) = n {
                    let (_, len) = n.secs();
                    if len > 0.0 {
                        n.pos = (t / len).clamp(0.0, 1.0);
                    }
                }
            });
            app.seek.set(None);
        });
    }

    // What the engine does, to Now Playing.
    use_future(move || {
        let (engine, round, last) = (engine.clone(), round.clone(), last.clone());
        async move {
            loop {
                crate::state::sleep_ms(100).await;
                let s = engine.snapshot();
                if *app.engine.peek() != s.status {
                    app.engine.set(s.status.clone());
                }
                let asked = last
                    .borrow()
                    .as_ref()
                    .and_then(|k| k.0.get(k.1).cloned().flatten());
                let drives = matches!(s.status, EngineStatus::Ready(_))
                    && s.song.is_some()
                    && s.song == asked;
                if drives != *app.engine_drives.peek() {
                    app.engine_drives.set(drives);
                }
                if !drives || s.round != round.get() {
                    continue;
                }
                let Some(n) = app.np.peek().clone() else {
                    continue;
                };
                // A change the window has not sent yet wins over what the
                // engine says (it is on its way to the engine).
                let sent = last.borrow().as_ref().map(|k| k.2);
                if sent != Some(n.playing) {
                    continue;
                }
                let (_, len) = n.secs();
                let ended = n.playing && !s.playing && len > 0.0 && s.position >= len - 1.0;
                let moved = len > 0.0 && ((n.pos * len) - s.position).abs() > 0.05;
                if !ended && n.playing == s.playing && !moved {
                    continue;
                }
                app.np.with_mut(|n| {
                    let Some(n) = n else { return };
                    if ended {
                        // The set plays on: the next song, from its start.
                        if !n.next() {
                            n.playing = false;
                        }
                        return;
                    }
                    n.playing = s.playing;
                    if len > 0.0 {
                        n.pos = (s.position / len).clamp(0.0, 1.0);
                    }
                });
            }
        }
    });
    rsx! {}
}
