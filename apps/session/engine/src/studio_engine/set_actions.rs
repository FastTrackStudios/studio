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
    "Stream a setlist of an org's Task library in, as a click on it does: `setlist` its title, `org` the org whose library holds it (FTS_TASK_ORG, else the one last picked, when left out), `song` one of its songs (slug or title) to open alone. What is open is not saved first. Opens in the engine, as the person signed in on this machine, a window or none; returns at once — SESSION_OPENING says how it goes, SESSION_SONGS lists the songs as they open.",
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

/// The set's actions, into `list`.
///
/// # Errors
///
/// The first registration the list refuses (a duplicate id).
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&OPEN, Effect::Mutating, |a: OpenArg| {
        let path = path_to_open(a)?;
        set::open_path(path.clone()).map_err(|e| e.to_string())?;
        Ok(format!("opening {}", path.display()))
    })?;
    list.register(&OPEN_SETLIST, Effect::Mutating, |a: OpenSetlistArg| {
        let request = library_open(a)?;
        let said = opening_said(&request);
        set::open_library(request).map_err(|e| e.to_string())?;
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
    if setlist.is_empty() {
        return Err("`setlist`: the title of a setlist in the library".to_owned());
    }
    let given = |s: Option<String>| s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    Ok(set::LibraryOpen {
        org: given(a.org),
        setlist,
        song: given(a.song),
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
