//! What Studio's welcome screen hands Session: what to open as Session
//! comes up ([`request`] an [`Entry`]: a song, a library setlist, a live
//! link, a new song), the songs opened lately, how to leave Session for the
//! welcome screen again ([`Host`]), and Session's Action List for the
//! welcome screen's command palette ([`actions`], [`run_action`]).
//!
//! Studio shows its library first; Session's own start screen is never a
//! page there. A library setlist is opened by the engine
//! (`session_engine::studio_engine::set::open_library`, as the Action List's
//! `SESSION_OPEN_SETLIST` opens one); a song, a live link or a new song is
//! handed to Session's start screen, which opens it in the engine too. A
//! hosted start screen draws only its loading screen (`super::start`).

use std::path::{Path, PathBuf};

pub use super::start::{
    WelcomeAccount, WelcomeAccountMenu, WelcomeAccountSheet, WelcomeLists, use_welcome_account,
    welcome_org,
};
use dioxus::prelude::*;
pub use session_engine::projects::{Recent, ago, open_next, recent};
pub use session_engine::stream_set::{LibrarySetlist, LibrarySong, ListKind};

static JOIN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Whether a new, untitled song was asked for ([`Entry::New`]).
static NEW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Join `link` (a live link, a REAPER bridge's or an engine's address, or
/// `demo`) as soon as the start screen is shown.
pub fn join_on_start(link: String) {
    *JOIN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(link);
}

/// What was asked to be joined, once.
pub(super) fn take_join() -> Option<String> {
    JOIN.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

/// Whether a new song was asked for, once.
pub(super) fn take_new() -> bool {
    NEW.swap(false, std::sync::atomic::Ordering::AcqRel)
}

/// Something for Session to open as it comes up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A song, a `.session`, a song's folder or a `.setlist` on this device.
    Path(PathBuf),
    /// A setlist of an org's Task library, by its title; `org` the one
    /// shown when `None`.
    Setlist { org: Option<String>, title: String },
    /// A live link, a REAPER bridge's or an engine's address, or `demo`.
    Join(String),
    /// A new, untitled song.
    New,
}

impl Entry {
    /// What `text` names, the way a command line or `FTS_STUDIO_OPEN` says
    /// it: `new`; `demo`, a live link or an engine's address; a path that
    /// `exists`; else the title of a library setlist. `None` for nothing.
    pub fn parse(text: &str, exists: impl Fn(&Path) -> bool) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        if text.eq_ignore_ascii_case("new") {
            return Some(Self::New);
        }
        if text.eq_ignore_ascii_case("demo") || is_link(text) {
            return Some(Self::Join(text.to_owned()));
        }
        let path = PathBuf::from(text);
        if exists(&path) {
            return Some(Self::Path(path));
        }
        Some(Self::Setlist {
            org: None,
            title: text.to_owned(),
        })
    }
}

/// Whether `text` is a link or an address rather than a name: a URL, an
/// engine id, or `host.tld/path`.
fn is_link(text: &str) -> bool {
    let bare_id = text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit());
    if text.contains("://") || text.starts_with("fts-engine:") || bare_id {
        return true;
    }
    !text.starts_with('/')
        && !text.contains(' ')
        && text
            .split_once('/')
            .is_some_and(|(host, _)| host.contains('.') && !host.starts_with('.'))
}

/// Hand `entry` to Session: its start screen opens it as soon as it is
/// shown (or, a song already open, once it is back out of it).
pub fn request(entry: Entry) {
    match entry {
        Entry::Path(path) => open_next(path),
        Entry::Setlist { org, title } => {
            let request = session_engine::studio_engine::set::LibraryOpen {
                org,
                setlist: title,
                song: None,
            };
            if let Err(busy) = session_engine::view_client::ask::open_library(&request) {
                tracing::warn!(error = %busy, "open: the engine did not take the setlist");
            }
        }
        Entry::Join(link) => join_on_start(link),
        Entry::New => NEW.store(true, std::sync::atomic::Ordering::Release),
    }
}

/// Whether something waits for Session to open it: asked for here, or over
/// the Action List (`SESSION_OPEN`, `SESSION_OPEN_SETLIST`, `SESSION_NEW`)
/// — from the CLI or a model while the welcome screen is up.
#[must_use]
pub fn pending() -> bool {
    session_engine::projects::pending_open()
        || NEW.load(std::sync::atomic::Ordering::Acquire)
        || JOIN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
}

/// Going into Session for what is [`pending`]: the "out of the song" an
/// open from outside raised is spent, since there is no song to leave.
pub fn entering() {
    let _ = session_engine::projects::take_back();
    session_engine::projects::entered_set();
}

/// Ask Session, a song open, to leave it for what was just [`request`]ed.
pub fn leave_song_for_request() {
    session_engine::projects::go_back();
}

/// The file of a session named `title` on this device: one opened lately,
/// or one in the app's documents folder. Matched by name, ignoring case.
#[must_use]
pub fn session_named(title: &str) -> Option<PathBuf> {
    let recent = recent().into_iter().map(|r| (r.name(), r.opened));
    let local = super::start::documents()
        .into_iter()
        .map(|p| (super::start::title_of(&p), p));
    find_named(title, recent.chain(local))
}

/// The first of `candidates` (name, file) whose name is `title`, ignoring
/// case and the space around it.
fn find_named(
    title: &str,
    candidates: impl IntoIterator<Item = (String, PathBuf)>,
) -> Option<PathBuf> {
    let title = title.trim();
    candidates
        .into_iter()
        .find(|(name, _)| name.trim().eq_ignore_ascii_case(title))
        .map(|(_, path)| path)
}

/// Session inside Studio: how it leaves a song for the welcome screen.
/// Provided by the host; without it, Back goes to Session's start screen.
#[derive(Clone, Copy)]
pub struct Host {
    /// Back out of the song, nothing else to open: the welcome screen.
    pub on_leave: Callback<()>,
}

/// One action of Session's Action List, as a palette lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionRow {
    pub id: String,
    pub label: String,
    pub group: String,
    /// How a key reaches it (`"Shift+G"`); empty when none does.
    pub shortcut: String,
    pub description: String,
    pub enabled: bool,
}

/// Session's Action List — the actions its keys, its menus, the CLI and a
/// model over MCP run — that a palette can run as they are: those that take
/// nothing and run at once.
///
/// Read from the engine behind the window, a second old at most: a
/// palette drawn while the engine is slow to answer is not held still.
#[must_use]
pub fn actions() -> Vec<ActionRow> {
    use std::sync::{Mutex, OnceLock};
    static LISTED: Mutex<Option<Vec<ActionRow>>> = Mutex::new(None);
    static READER: OnceLock<()> = OnceLock::new();
    READER.get_or_init(|| {
        let _ = std::thread::Builder::new()
            .name("palette-actions".into())
            .spawn(|| {
                loop {
                    let rows = engine_actions();
                    if let Ok(mut listed) = LISTED.lock() {
                        *listed = Some(rows);
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            });
    });
    // The first time, the engine's first answer if it is quick.
    let began = std::time::Instant::now();
    loop {
        if let Some(rows) = LISTED.lock().ok().and_then(|l| l.clone()) {
            return rows;
        }
        if began.elapsed() > std::time::Duration::from_millis(150) {
            return Vec::new();
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// The engine's list, with what can run now as the engine says: the
/// window's own actions are on it too, forwarded to the window.
fn engine_actions() -> Vec<ActionRow> {
    session_engine::actions::palette()
        .into_iter()
        .map(|a| ActionRow {
            id: a.id,
            label: a.label,
            group: a.group,
            shortcut: a.shortcut,
            description: a.description,
            enabled: a.enabled,
        })
        .collect()
}

/// Run `id` from Session's Action List, as a key would: on the engine,
/// behind the window; a refusal is logged.
///
/// # Errors
///
/// No thread to run it on.
pub fn run_action(id: &str) -> Result<(), String> {
    // On the engine, behind the window: a refusal is said where it is
    // known.
    let id = id.to_owned();
    std::thread::Builder::new()
        .name("palette-run".into())
        .spawn(move || {
            if let Err(error) = session_engine::actions::run(&id, b"") {
                tracing::warn!(action.id = %id, %error, "palette: the action did not run");
            }
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// An overlay over the window takes the keyboard (`true`) or gives it
/// back: Session's keys stand aside while it has it.
pub fn keys_taken(on: bool) {
    session_ui::keys::set_overlay(on);
}

/// Whether a text field of Session's has the keyboard (a rename, the chart
/// editor): a plain `?` is its, not the shortcuts sheet's.
#[must_use]
pub fn typing() -> bool {
    session_ui::keys::typing()
}

/// Stop the transport: Session is being left (or replaced) from outside.
pub fn stop() {
    session_engine::engine::transport(session_engine::engine::Move::Stop, 0.0);
}

/// What the DAW view does to the transport as it takes what was handed to
/// it: nothing. The engine owns the set and its transport, and a window
/// coming up over it (a relaunch after a crash, a second window) shows
/// what it plays without stopping it.
pub const fn stop_for_entry() {}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{Entry, find_named};

    fn none(_: &Path) -> bool {
        false
    }

    #[test]
    fn words_name_a_setlist_and_links_join() {
        assert_eq!(Entry::parse("  ", none), None);
        assert_eq!(Entry::parse("new", none), Some(Entry::New));
        assert_eq!(Entry::parse("demo", none), Some(Entry::Join("demo".into())));
        assert_eq!(
            Entry::parse("https://task.example.app/org/x/share/abc", none),
            Some(Entry::Join(
                "https://task.example.app/org/x/share/abc".into()
            ))
        );
        assert_eq!(
            Entry::parse("task.example.app/org/x/share/abc", none),
            Some(Entry::Join("task.example.app/org/x/share/abc".into()))
        );
        assert_eq!(
            Entry::parse("ws://127.0.0.1:4040/vox", none),
            Some(Entry::Join("ws://127.0.0.1:4040/vox".into()))
        );
        assert_eq!(
            Entry::parse("JHM Sunday", none),
            Some(Entry::Setlist {
                org: None,
                title: "JHM Sunday".into()
            })
        );
    }

    #[test]
    fn a_path_that_is_there_opens_as_a_file() {
        let there = |p: &Path| p == Path::new("/songs/Always On Time.RPP");
        assert_eq!(
            Entry::parse("/songs/Always On Time.RPP", there),
            Some(Entry::Path(PathBuf::from("/songs/Always On Time.RPP")))
        );
        // Not there: a name, not a file (and never a link).
        assert_eq!(
            Entry::parse("/songs/Gone.RPP", there),
            Some(Entry::Setlist {
                org: None,
                title: "/songs/Gone.RPP".into()
            })
        );
    }

    #[test]
    fn a_session_is_found_by_its_name() {
        let files = vec![
            ("Always On Time".to_owned(), PathBuf::from("/a.RPP")),
            ("Paper Boats".to_owned(), PathBuf::from("/b.RPP")),
        ];
        assert_eq!(
            find_named(" paper boats ", files.clone()),
            Some(PathBuf::from("/b.RPP"))
        );
        assert_eq!(find_named("Reckless Love", files), None);
    }
}
