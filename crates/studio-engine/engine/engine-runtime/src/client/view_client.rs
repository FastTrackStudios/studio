//! A Studio window's link to its engine: the views' contract's
//! (`daw_view::view_client`, re-exported here) and Studio's replica of the
//! engine's set beside it — the setlist the set reads back as, its songs,
//! how far an open has got — which the link hands Studio as each frame
//! arrives (`crate::client::host`).

use std::sync::{Mutex, OnceLock};

use tokio::sync::watch;

pub use daw_view::view_client::*;

use crate::client::view::SetFrame;

/// Link this window to the engine `caller` reaches, as
/// [`daw_view::view_client::connect`] does, with Studio behind it.
pub fn connect(caller: vox::Caller) {
    crate::client::host::install();
    daw_view::view_client::connect(caller);
}

// ── The set ──────────────────────────────────────────────────────────

/// The set as this window shows it: the engine's, each song read back
/// through the facade.
fn set_cell() -> &'static watch::Sender<crate::client::studio_engine::set::SetView> {
    static CELL: OnceLock<watch::Sender<crate::client::studio_engine::set::SetView>> =
        OnceLock::new();
    CELL.get_or_init(|| watch::Sender::new(crate::client::studio_engine::set::SetView::default()))
}

/// The set this window shows, from now on.
#[must_use]
pub fn subscribe_set() -> watch::Receiver<crate::client::studio_engine::set::SetView> {
    set_cell().subscribe()
}

/// The set this window shows now.
#[must_use]
pub fn set_view() -> crate::client::studio_engine::set::SetView {
    set_cell().borrow().clone()
}

/// `f` with the set this window shows, without copying it.
pub fn peek_set<R>(f: impl FnOnce(&crate::client::studio_engine::set::SetView) -> R) -> R {
    f(&set_cell().borrow())
}

/// Show `setlist` as this window's set: one it drives on another system
/// (a REAPER, Remote or Cue), which has no engine set to follow.
pub fn show_set(setlist: crate::client::setlist::Setlist) {
    set_cell().send_modify(|view| {
        view.setlist = Some(setlist);
        view.epoch += 1;
        view.opening = crate::client::studio_engine::set::Opening::Idle;
        view.revision += 1;
    });
}

/// Song `index` as the current one of the set this window shows — a song
/// already current on the system it drives (a tab picked in a REAPER) —
/// the one left keeping where its playhead had got to. The project, or
/// `None` when it is no song or current already.
pub fn show_current(index: usize) -> Option<String> {
    let mut picked = None;
    set_cell().send_if_modified(|view| {
        let Some(setlist) = view.setlist.as_mut() else {
            return false;
        };
        let left_at = setlist
            .songs
            .get(setlist.at)
            .and_then(|s| playhead_of(&s.project))
            .map_or(0.0, |(at, _)| at);
        picked = setlist
            .pick(index, left_at)
            .map(|song| song.project.clone());
        if picked.is_some() {
            view.revision += 1;
        }
        picked.is_some()
    });
    picked
}

/// What the window does with a set the engine sent: its songs read when
/// they are new (on a thread of its own: reading is facade calls), the
/// rest followed in place.
pub(crate) fn set_arrived(frame: SetFrame) {
    let shown: Vec<String> = peek_set(|view| {
        view.setlist
            .as_ref()
            .map(|s| s.songs.iter().map(|song| song.project.clone()).collect())
            .unwrap_or_default()
    });
    let engine: Vec<String> = frame.songs.iter().map(|(_, p)| p.clone()).collect();
    match crate::client::studio_engine::link::changed_songs(&shown, &engine) {
        crate::client::studio_engine::link::SongsChanged::Same => follow_in_place(&frame),
        crate::client::studio_engine::link::SongsChanged::Read => read_set(frame),
    }
}

/// The same songs: the current one, their colours, what is arriving, the
/// opening — followed in the replica.
fn follow_in_place(frame: &SetFrame) {
    set_cell().send_if_modified(|view| {
        let mut changed = false;
        if view.epoch != frame.epoch {
            view.epoch = frame.epoch;
            changed = true;
        }
        let opening = opening_of(&frame.opening);
        if view.opening != opening {
            view.opening = opening;
            changed = true;
        }
        if let Some(setlist) = view.setlist.as_mut() {
            changed |= follow_setlist(setlist, frame);
        }
        if changed {
            view.revision += 1;
        }
        changed
    });
}

/// `setlist` (this window's) made to say what `frame` (the engine's) says
/// of the same songs; whether anything changed.
fn follow_setlist(setlist: &mut crate::client::setlist::Setlist, frame: &SetFrame) -> bool {
    let mut changed = false;
    if let Some(at) = frame.current.map(|at| at as usize)
        && at != setlist.at
        && at < setlist.songs.len()
    {
        let left_at = playhead_of(&setlist.songs[setlist.at].project).map_or(0.0, |p| p.0);
        setlist.pick(at, left_at);
        changed = true;
    }
    for (song, color) in setlist.songs.iter_mut().zip(&frame.colors) {
        if !color.is_empty() && &song.color != color {
            song.color.clone_from(color);
            changed = true;
        }
    }
    if setlist.pending != frame.arriving {
        setlist.pending.clone_from(&frame.arriving);
        changed = true;
    }
    changed
}

fn opening_of(
    opening: &crate::client::view::SetOpening,
) -> crate::client::studio_engine::set::Opening {
    match opening {
        crate::client::view::SetOpening::Idle => crate::client::studio_engine::set::Opening::Idle,
        crate::client::view::SetOpening::Busy(p) => {
            crate::client::studio_engine::set::Opening::Busy(p.clone())
        }
        crate::client::view::SetOpening::Failed(why) => {
            crate::client::studio_engine::set::Opening::Failed(why.clone())
        }
    }
}

/// Another set (or other songs in it): its new songs read through the
/// facade, the ones shown kept, then shown — newest frame wins.
fn read_set(frame: SetFrame) {
    static LATEST: Mutex<Option<SetFrame>> = Mutex::new(None);
    static READING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if let Ok(mut latest) = LATEST.lock() {
        *latest = Some(frame);
    }
    if READING.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("view-set-read".into())
        .spawn(|| {
            loop {
                let Some(frame) = LATEST.lock().ok().and_then(|mut l| l.take()) else {
                    READING.store(false, std::sync::atomic::Ordering::Release);
                    // A frame that came while the flag was going down.
                    if LATEST.lock().is_ok_and(|l| l.is_some())
                        && !READING.swap(true, std::sync::atomic::Ordering::AcqRel)
                    {
                        continue;
                    }
                    return;
                };
                install_read(&frame);
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "view link: no thread to read the set on");
        READING.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Read `frame`'s songs (those not shown already) and show the set.
fn install_read(frame: &SetFrame) {
    let have: Vec<crate::client::setlist::Song> = peek_set(|view| {
        view.setlist
            .as_ref()
            .map(|s| s.songs.clone())
            .unwrap_or_default()
    });
    let opening = opening_of(&frame.opening);
    if frame.songs.is_empty() {
        set_cell().send_modify(|view| {
            view.setlist = None;
            view.epoch = frame.epoch;
            view.opening = opening;
            view.revision += 1;
        });
        return;
    }
    let span = tracing::info_span!(
        "view.set_read",
        set.songs = frame.songs.len(),
        set.read = tracing::field::Empty,
        set.outcome = tracing::field::Empty
    );
    let _in = span.enter();
    let read = crate::client::setlist::Setlist::from_link(frame, have);
    match read {
        Ok((mut setlist, read)) => {
            span.record("set.read", read);
            span.record("set.outcome", "shown");
            follow_setlist(&mut setlist, frame);
            set_cell().send_modify(|view| {
                view.setlist = Some(setlist);
                view.epoch = frame.epoch;
                view.opening = opening;
                view.revision += 1;
            });
        }
        Err(e) => {
            span.record("set.outcome", "unread");
            tracing::warn!(error = %e, "view link: the engine's set could not be read");
        }
    }
}

/// What the window asks of the engine's set — each an action on the
/// engine's list, run off the caller's thread; the window shows what the
/// engine then publishes.
pub mod ask {
    /// Run `id` with `args` on the engine behind the window; a refusal said.
    fn off_the_window(id: &'static str, args: serde_json::Value) {
        let spawned = std::thread::Builder::new()
            .name("view-ask".into())
            .spawn(move || {
                if let Err(e) = super::run(id, args.to_string().as_bytes()) {
                    tracing::warn!(action.id = id, error = %e, "view link: the engine refused");
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(action.id = id, error = %e, "view link: no thread to ask on");
        }
    }

    /// Go to song `index` (0 the first): shown at once (the one left
    /// keeps where its playhead had got to), and asked of the engine. The
    /// project gone to, or `None` when it is no song or current already.
    pub fn pick(index: usize) -> Option<String> {
        let picked = super::show_current(index)?;
        off_the_window(
            "SESSION_SONG",
            serde_json::json!({ "to": (index + 1).to_string() }),
        );
        Some(picked)
    }

    /// Give song `index` a colour (`None`: the one its title makes).
    pub fn recolor(index: usize, color: Option<String>) {
        off_the_window(
            "SESSION_SONG_SET",
            serde_json::json!({ "song": index + 1, "color": color.unwrap_or_default() }),
        );
    }

    /// Move song `song` to `to` (0 the first).
    pub fn reorder(song: usize, to: usize) {
        off_the_window(
            "SESSION_SONG_SET",
            serde_json::json!({ "song": song + 1, "to": to + 1 }),
        );
    }

    /// Take song `song` out of the set.
    pub fn remove(song: usize) {
        off_the_window(
            "SESSION_SONG_SET",
            serde_json::json!({ "song": song + 1, "remove": true }),
        );
    }

    /// Close the engine's set (the transport stopped first).
    pub fn close() {
        off_the_window("SESSION_CLOSE", serde_json::json!({}));
    }

    /// A new, untitled song as the engine's set. Blocks on the engine's
    /// answer, as [`open_path`] does: call it off the UI thread.
    ///
    /// # Errors
    ///
    /// The engine refused (another open is under way, or no engine).
    pub fn new_song() -> Result<String, String> {
        super::run("SESSION_NEW", b"{}")
    }

    /// Put the set in Live, or take it out.
    pub fn live(on: bool) {
        off_the_window("SESSION_LIVE", serde_json::json!({ "on": on }));
    }

    /// Open `path` (a song, a setlist, a `.session`) as the engine's set.
    ///
    /// # Errors
    ///
    /// The engine refused (another open is under way).
    pub fn open_path(path: &std::path::Path) -> Result<String, String> {
        let args = serde_json::json!({ "path": path.display().to_string() });
        super::run("SESSION_OPEN", args.to_string().as_bytes())
    }
}
