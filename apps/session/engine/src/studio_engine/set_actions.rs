//! The set's actions on the Action List: open a set, list its songs, go to
//! one — run against the engine's set ([`super::set`]), so they work with a
//! window or without one (`studio engine`). A window shows what they did;
//! nothing here waits for it.

use architect::action::{ActionList, ActionMeta, Effect};
use facet::Facet;

use super::set::{self, Opening};
use crate::actions::meta;
use crate::open_progress::Progress;

static OPEN: ActionMeta = meta(
    "SESSION_OPEN",
    "session_open",
    "Open a song or version",
    "Leave the set open now and open another: `name` a version of this song (\"The song\", or a checkpoint's name — SESSION_ALTERNATIVES lists them), or `path` any song, setlist or `.session`. What is open is not saved first — SESSION_SAVE it if it should be. Opens in the engine, a window or none; returns at once — SESSION_OPENING says how it goes, SESSION_SONGS lists the set once it is open.",
    "Session",
    "",
);

/// Which to open: a version of this song by name, or anything by path.
#[derive(Facet)]
struct OpenArg {
    name: Option<String>,
    path: Option<String>,
}

static OPEN_SETLIST: ActionMeta = meta(
    "SESSION_OPEN_SETLIST",
    "session_open_setlist",
    "Open a setlist from the library",
    "Stream a setlist of an org's Task library in, as a click on it does: `setlist` its title, `org` the org whose library holds it (FTS_TASK_ORG, else the one last picked, when left out), `song` one of its songs (slug or title) to open alone — or, with no `setlist`, a song of the library in no list. What is open is not saved first. Opens in the engine, as the person signed in on this machine, a window or none; returns at once — SESSION_OPENING says how it goes, SESSION_SONGS lists the songs as they open.",
    "Session",
    "",
);

/// Which setlist of which library, and optionally which of its songs.
#[derive(Facet)]
struct OpenSetlistArg {
    setlist: String,
    org: Option<String>,
    song: Option<String>,
}

static OPENING: ActionMeta = meta(
    "SESSION_OPENING",
    "session_opening",
    "How the open is going",
    "Whether a set is being opened: `state` idle, opening (with `step`: joining, fetching or opening, and the song's `title`), or failed (with `why`). Read-only.",
    "Session",
    "",
);

/// How the open is going, as `SESSION_OPENING` says it.
#[derive(Facet)]
struct OpeningRow {
    state: String,
    step: Option<String>,
    title: Option<String>,
    why: Option<String>,
}

static SONGS: ActionMeta = meta(
    "SESSION_SONGS",
    "session_songs",
    "The set's songs",
    "The songs of the set open in the engine, in order: each one's `number` (1 the first), `title`, engine `project`, whether it is `current`, and `state` — open, or arriving (still on its way). Empty when no set is open. Read-only.",
    "Session",
    "",
);

static SONG: ActionMeta = meta(
    "SESSION_SONG",
    "session_song",
    "Go to a song",
    "Go to a song of the open set, as a click on its tab does: `to` its number (1 the first, as SESSION_SONGS lists them), its title, or `next` / `previous`. The engine's current song from now on; a window follows. Refused past either end of the set, or for a song still arriving.",
    "Session",
    "",
);

/// Which song to go to.
#[derive(Facet)]
struct SongArg {
    to: String,
}

static SONG_NEXT: ActionMeta = meta(
    "SESSION_SONG_NEXT",
    "session_song_next",
    "Next song",
    "Go to the next song of the open set (SESSION_SONG `next`).",
    "Session",
    "",
);

static SONG_PREV: ActionMeta = meta(
    "SESSION_SONG_PREV",
    "session_song_prev",
    "Previous song",
    "Go to the song before in the open set (SESSION_SONG `previous`).",
    "Session",
    "",
);

static LIVE: ActionMeta = meta(
    "SESSION_LIVE",
    "live",
    "Live roll-on",
    "Whether the set rolls on: in Live, when the song playing reaches its end the next one plays from the same frame. `on` turns it on or off (a window in Live mode turns it on); without it, says whether it is on.",
    "Set",
    "",
);

/// On or off; left out, only asked.
#[derive(Facet)]
struct LiveArg {
    on: Option<bool>,
}

static CLOSE: ActionMeta = meta(
    "SESSION_CLOSE",
    "session_close",
    "Close the set",
    "Stop and close the set open in the engine (nothing is saved first — SESSION_SAVE what should be). A window goes back to its start screen.",
    "Set",
    "",
);

static JOIN: ActionMeta = meta(
    "SESSION_JOIN",
    "session_join",
    "Join a live set",
    "Join a set Task keeps, by its live link (`link`, `https://…/org/<org>/share/<id>`), as `name` (left out: Guest): its songs stream into the engine and play, the set followed with everyone in it. Returns at once — SESSION_OPENING says how it goes.",
    "Set",
    "",
);

/// Which live set, as whom.
#[derive(Facet)]
struct JoinArg {
    link: String,
    name: Option<String>,
}

static SONG_SET: ActionMeta = meta(
    "SESSION_SONG_SET",
    "session_song_set",
    "Change a song of the set",
    "Change song `song` of the open set (its number, 1 the first, as SESSION_SONGS lists them): `color` gives it a colour (`#rrggbb`; empty: the one its title makes), `to` moves it to that place, `remove` takes it out of the set. What a window's set editor does.",
    "Set",
    "",
);

/// Which song, and what to change of it.
#[derive(Facet)]
struct SongSetArg {
    song: u32,
    color: Option<String>,
    to: Option<u32>,
    remove: Option<bool>,
}

static TRANSPORT: ActionMeta = meta(
    "SESSION_TRANSPORT",
    "transport",
    "Where it plays",
    "The current song's transport as the engine has it: `song` (its project), `playing`, `position` (seconds). Read-only — what a script or a window checks that the engine plays, with no window or with one.",
    "Set",
    "",
);

/// The transport, as said.
#[derive(Facet)]
struct TransportRow {
    song: Option<String>,
    playing: bool,
    position: Option<f64>,
}

static EDIT_STATE: ActionMeta = meta(
    "SESSION_EDIT_STATE",
    "edit_state",
    "Edit cursor and selection",
    "Where the current song's edit cursor is and its time selection (seconds), as the engine has them — what an edit acts on when it is not told where. Read-only; TIME_SELECTION_SET and the cursor actions move them.",
    "Set",
    "",
);

/// The edit state, as said.
#[derive(Facet)]
struct EditStateRow {
    song: Option<String>,
    cursor: Option<f64>,
    selection_start: Option<f64>,
    selection_end: Option<f64>,
}

static TOGETHER_STATUS: ActionMeta = meta(
    "SESSION_TOGETHER",
    "session_together",
    "The live session",
    "The live session the engine is in, as the collaboration control shows it: `status` (the set, hosting or joined, the ticket or link, how many others, whether the transport is shared, this person's name and colour), `joining`, `error` (why the last share or join failed), `locked` (this device is not syncing), `hosted` (the set the engine hosts in Task by itself, signed in) and `host_error`, and `people` (everyone else, and the song each is on). Read-only.",
    "Set",
    "",
);

static TOGETHER_SHARE: ActionMeta = meta(
    "SESSION_TOGETHER_SHARE",
    "session_together_share",
    "Share the set",
    "Share the set open in the engine with whoever has its ticket (iroh), as `name`: each song's own doc is what they edit. Returns at once — SESSION_TOGETHER has the ticket once it is shared.",
    "Set",
    "",
);

static TOGETHER_JOIN: ActionMeta = meta(
    "SESSION_TOGETHER_JOIN",
    "session_together_join",
    "Join a shared set",
    "Join a set someone shares by its ticket (`ticket`, `fts-session:…`), as `name`, with the same setlist open in the engine: each song is kept with theirs through its own file. Returns at once — SESSION_TOGETHER says how it went.",
    "Set",
    "",
);

static TOGETHER_LEAVE: ActionMeta = meta(
    "SESSION_TOGETHER_LEAVE",
    "session_together_leave",
    "Leave the live session",
    "Leave the live session (or stop sharing it). The songs keep everything edited together in their files.",
    "Set",
    "",
);

static TOGETHER_TRANSPORT: ActionMeta = meta(
    "SESSION_TOGETHER_TRANSPORT",
    "session_together_transport",
    "Play together or apart",
    "In a live session: `together` true plays one transport for everyone, from where this engine is; false lets everyone play on their own.",
    "Set",
    "",
);

static TOGETHER_LOCK: ActionMeta = meta(
    "SESSION_TOGETHER_LOCK",
    "session_together_lock",
    "Lock this device",
    "`locked` true: this device stops syncing — the others' edits wait, its own stay here. False unlocks it and merges both ways. Remembered on this device.",
    "Set",
    "",
);

/// Who shares, as whom.
#[derive(Facet)]
struct ShareArg {
    name: Option<String>,
}

/// Which shared set, as whom.
#[derive(Facet)]
struct TicketArg {
    ticket: String,
    name: Option<String>,
}

/// Together or apart.
#[derive(Facet)]
struct TogetherArg {
    together: bool,
}

/// Locked or not.
#[derive(Facet)]
struct LockArg {
    locked: bool,
}

/// Who this person is in a set, when they are not named: the account's
/// name, else the machine user's.
#[must_use]
pub fn person_name() -> String {
    std::env::var("FTS_COLLAB_NAME")
        .ok()
        .or_else(|| std::env::var("USER").ok())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "Someone".to_owned())
}

/// The set's actions, into `list`.
///
/// # Errors
///
/// The first registration the list refuses (a duplicate id).
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&OPEN, Effect::Mutating, |a: OpenArg| {
        let path = path_to_open(a)?;
        set::open_path(path.clone()).map_err(|e| e.to_string())?;
        super::link::remember_open(
            "SESSION_OPEN",
            &serde_json::json!({ "path": path.display().to_string() }),
        );
        Ok(format!("opening {}", path.display()))
    })?;
    list.register(&OPEN_SETLIST, Effect::Mutating, |a: OpenSetlistArg| {
        let request = library_open(a)?;
        let said = opening_said(&request);
        let kept = serde_json::json!({
            "setlist": request.setlist,
            "org": request.org,
            "song": request.song,
        });
        set::open_library(request).map_err(|e| e.to_string())?;
        super::link::remember_open("SESSION_OPEN_SETLIST", &kept);
        Ok(said)
    })?;
    list.register(&OPENING, Effect::ReadOnly, |(): ()| {
        Ok(opening_row(&set::view().opening))
    })?;
    list.register(&SONGS, Effect::ReadOnly, |(): ()| {
        Ok(set::lineup().map(|l| set::rows(&l)).unwrap_or_default())
    })?;
    list.register(&SONG, Effect::Mutating, |a: SongArg| {
        set::pick_by(&a.to).map_err(|e| e.to_string())
    })?;
    list.register(&SONG_NEXT, Effect::Mutating, |(): ()| {
        set::pick_by("next").map_err(|e| e.to_string())
    })?;
    list.register(&SONG_PREV, Effect::Mutating, |(): ()| {
        set::pick_by("previous").map_err(|e| e.to_string())
    })?;
    list.register(&LIVE, Effect::Mutating, |a: LiveArg| {
        if let Some(on) = a.on {
            super::live::set_live(on);
        }
        Ok(if super::live::is_live() {
            "live: on"
        } else {
            "live: off"
        }
        .to_owned())
    })?;
    list.register(&CLOSE, Effect::Mutating, |(): ()| {
        crate::engine::transport_here(crate::engine::Move::Stop, 0.0);
        set::close();
        Ok("the set is closed".to_owned())
    })?;
    list.register(&JOIN, Effect::Mutating, |a: JoinArg| {
        let name = a
            .name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "Guest".to_owned());
        set::open_live(a.link.clone(), name.clone()).map_err(|e| e.to_string())?;
        super::link::remember_open(
            "SESSION_JOIN",
            &serde_json::json!({ "link": a.link, "name": name }),
        );
        Ok(format!("joining {}", a.link))
    })?;
    list.register(&SONG_SET, Effect::Mutating, |a: SongSetArg| {
        let index = usize::try_from(a.song.saturating_sub(1)).unwrap_or(usize::MAX);
        let len = set::lineup().map_or(0, |l| l.songs.len());
        if a.song == 0 || index >= len {
            return Err(format!("no song {} in a set of {len}", a.song));
        }
        let mut said = Vec::new();
        // Where the song is now: a move puts it elsewhere before a remove.
        let mut at = index;
        if let Some(color) = a.color {
            said.push(format!(
                "coloured {}",
                if color.is_empty() {
                    "by its title"
                } else {
                    &color
                }
            ));
            set::recolor(index, Some(color).filter(|c| !c.is_empty()));
        }
        if let Some(to) = a.to {
            let to = usize::try_from(to.saturating_sub(1))
                .unwrap_or(0)
                .min(len - 1);
            set::reorder(index, to);
            at = to;
            said.push(format!("moved to {}", to + 1));
        }
        if a.remove == Some(true) {
            set::remove(at);
            said.push("removed".to_owned());
        }
        Ok(format!("song {}: {}", a.song, said.join(", ")))
    })?;
    list.register(&TRANSPORT, Effect::ReadOnly, |(): ()| {
        let song = crate::open::current_song();
        let at = song.as_deref().and_then(crate::stream_set::transport_of);
        Ok(TransportRow {
            song,
            playing: at.is_some_and(|(_, playing)| playing),
            position: at.map(|(position, _)| position),
        })
    })?;
    list.register(&TOGETHER_STATUS, Effect::ReadOnly, |(): ()| {
        Ok(super::view_host::live_now())
    })?;
    list.register(&TOGETHER_SHARE, Effect::Mutating, |a: ShareArg| {
        crate::collab::host_in_background(a.name.unwrap_or_else(person_name));
        Ok("sharing the set".to_owned())
    })?;
    list.register(&TOGETHER_JOIN, Effect::Mutating, |a: TicketArg| {
        let ticket = a.ticket.trim().to_owned();
        if !ticket.starts_with("fts-session:") {
            return Err("`ticket`: a session ticket (fts-session:…)".to_owned());
        }
        crate::collab::join_in_background(ticket, a.name.unwrap_or_else(person_name));
        Ok("joining the shared set".to_owned())
    })?;
    list.register(&TOGETHER_LEAVE, Effect::Mutating, |(): ()| {
        crate::collab::leave();
        Ok("left the live session".to_owned())
    })?;
    list.register(&TOGETHER_TRANSPORT, Effect::Mutating, |a: TogetherArg| {
        crate::collab::set_shared_transport(a.together);
        Ok(if a.together {
            "playing together"
        } else {
            "playing apart"
        }
        .to_owned())
    })?;
    list.register(&TOGETHER_LOCK, Effect::Mutating, |a: LockArg| {
        crate::collab::set_sync_locked(a.locked);
        Ok(if a.locked {
            "this device is locked"
        } else {
            "this device syncs"
        }
        .to_owned())
    })?;
    list.register(&EDIT_STATE, Effect::ReadOnly, |(): ()| {
        let edit = crate::edit_state::current();
        let selection = edit.and_then(|e| e.selection);
        Ok(EditStateRow {
            song: crate::open::current_song(),
            cursor: edit.map(|e| e.at),
            selection_start: selection.map(|s| s.start),
            selection_end: selection.map(|s| s.end),
        })
    })?;
    Ok(())
}

/// The file `SESSION_OPEN` names: a version of the open song by name, or a
/// path that is there.
fn path_to_open(a: OpenArg) -> Result<std::path::PathBuf, String> {
    let path = match (a.name, a.path) {
        (_, Some(path)) => std::path::PathBuf::from(path),
        (Some(name), None) => crate::actions::open_song_versions()
            .into_iter()
            .find(|v| v.name.eq_ignore_ascii_case(name.trim()))
            .map(|v| v.path)
            .ok_or_else(|| format!("no version {name:?} of this song (SESSION_ALTERNATIVES)"))?,
        (None, None) => return Err("`name` (a version of this song) or `path`".to_owned()),
    };
    if path.exists() {
        Ok(path)
    } else {
        Err(format!("{} is not there", path.display()))
    }
}

/// `SESSION_OPEN_SETLIST`'s arguments as a request, trimmed; an empty
/// title refused.
fn library_open(a: OpenSetlistArg) -> Result<set::LibraryOpen, String> {
    let setlist = a.setlist.trim().to_owned();
    let given = |s: Option<String>| s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    let song = given(a.song);
    // No list: one song of the library, alone.
    if setlist.is_empty() && song.is_none() {
        return Err(
            "`setlist`: the title of a setlist in the library (or `song` alone)".to_owned(),
        );
    }
    Ok(set::LibraryOpen {
        org: given(a.org),
        setlist,
        song,
    })
}

/// What `SESSION_OPEN_SETLIST` says it is doing.
fn opening_said(request: &set::LibraryOpen) -> String {
    format!(
        "opening the setlist {:?}{}{}",
        request.setlist,
        request
            .song
            .as_deref()
            .map(|s| format!(", its song {s:?}"))
            .unwrap_or_default(),
        request
            .org
            .as_deref()
            .map(|o| format!(" from {o}'s library"))
            .unwrap_or_default()
    )
}

fn opening_row(opening: &Opening) -> OpeningRow {
    let (step, title) = match opening {
        Opening::Busy(Progress::Joining { .. }) => (Some("joining"), None),
        Opening::Busy(Progress::Fetching { title, .. }) => (Some("fetching"), Some(title.clone())),
        Opening::Busy(Progress::Opening { title }) => (Some("opening"), Some(title.clone())),
        Opening::Idle | Opening::Failed(_) => (None, None),
    };
    OpeningRow {
        state: match opening {
            Opening::Idle => "idle",
            Opening::Busy(_) => "opening",
            Opening::Failed(_) => "failed",
        }
        .to_owned(),
        step: step.map(str::to_owned),
        title,
        why: match opening {
            Opening::Failed(why) => Some(why.clone()),
            _ => None,
        },
    }
}
