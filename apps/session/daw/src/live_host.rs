//! Session hosts what it has open in Task, by itself, whenever its person
//! is signed in: no Publish button.
//!
//! Every few seconds a thread of its own looks at three things — whether
//! this device is signed in to Task ([`crate::task_account`]), which songs
//! the engine has open, and whether a session is already live — and does
//! what [`next`] says:
//!
//! - **Host.** Signed in, songs open, no session live: the songs are made
//!   sure of in the org's library (a song it lacks is created under its
//!   name, slugged the way the live session matches songs), and so is the
//!   set they make — the library list of the open set's name (a setlist
//!   file's stem, or a lone song's name), created as a set when there is
//!   none, and given any open song it lacks at its end (nothing is removed
//!   or reordered). Then the set is joined as any set Task keeps is
//!   ([`crate::collab::join_task`]): each song's live doc in Task is seeded
//!   from this engine when it is empty, or adopted when someone is already
//!   in it, the set's presence is joined, and from then on members and
//!   agents edit it live — their edits are made in this engine through the
//!   doc ↔ engine bridge, and this engine's go out the same way.
//! - **Re-host.** A song opened that the hosted set lacks: the same again,
//!   so it joins the set too.
//! - **Leave.** Signed out while hosting: the session is left; the songs
//!   keep their history, shared with nobody.
//! - **Wait.** Anything else: a session someone started by hand (a ticket,
//!   a share link, a library set) is theirs; a set its person left stays
//!   left until what is open changes; a failure is tried again later, a
//!   little later each time.
//!
//! The per-device lock ([`crate::collab::set_sync_locked`]) is separate: a
//! locked device is hosted all the same — its people see it — but holds
//! what comes in and keeps what it does until it is unlocked.
//!
//! `FTS_LIVE_AUTOHOST=0` turns it off; so does a session the environment
//! starts (`FTS_COLLAB_TASK`, `FTS_COLLAB_HOST`, `FTS_COLLAB_JOIN`).
//! Which org: `FTS_TASK_ORG`, else the one this window last picked
//! ([`crate::task_account::remembered_org`]), else the person's first.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use session::library::{Library, ListKind, Setlist, slugify};

/// The environment switch that turns automatic hosting off (`0`, `off`,
/// `false`, `no`).
pub const AUTOHOST_ENV: &str = "FTS_LIVE_AUTOHOST";

/// How often the open songs and the account are looked at.
const POLL: Duration = Duration::from_secs(3);

/// The longest wait after a failure before trying again.
const MOST_BACKOFF: Duration = Duration::from_secs(120);

/// The set this device is hosting in Task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hosted {
    pub org: String,
    /// The library list's id.
    pub setlist: String,
    pub title: String,
    /// The open songs it was hosted with, by name.
    pub keys: Vec<String>,
}

static HOSTED: Mutex<Option<Hosted>> = Mutex::new(None);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);
static STARTED: AtomicBool = AtomicBool::new(false);

/// The set this device is hosting in Task, if it is.
#[must_use]
pub fn hosted() -> Option<Hosted> {
    HOSTED.lock().ok()?.clone()
}

/// Why hosting last failed, if it did (cleared once it works).
#[must_use]
pub fn last_error() -> Option<String> {
    LAST_ERROR.lock().ok()?.clone()
}

fn set_error(why: Option<String>) {
    if let Ok(mut slot) = LAST_ERROR.lock() {
        *slot = why;
    }
}

/// What to do next, from what is true now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    Wait,
    Host,
    Rehost,
    Leave,
}

/// What is true now, for [`next`].
#[derive(Debug, Clone, Default)]
pub struct Seen<'a> {
    pub signed_in: bool,
    /// A session is being joined (by anyone) right now.
    pub joining: bool,
    /// A session is live (shared with anyone).
    pub shared: bool,
    /// The songs the hosted set was hosted with, if this is hosting one.
    pub hosted: Option<&'a [String]>,
    /// The songs open now, by name.
    pub open: &'a [String],
    /// Its person left the hosted set, with these songs open.
    pub left_with: Option<&'a [String]>,
}

/// What automatic hosting does next (see the module docs).
#[must_use]
pub fn next(seen: &Seen<'_>) -> Next {
    if !seen.signed_in {
        return if seen.hosted.is_some() && seen.shared {
            Next::Leave
        } else {
            Next::Wait
        };
    }
    if seen.joining || seen.open.is_empty() {
        return Next::Wait;
    }
    // Left by hand: left, until what is open changes.
    if seen.left_with.is_some_and(|keys| keys == seen.open) {
        return Next::Wait;
    }
    match seen.hosted {
        // Hosting: a song opened that the set lacks joins it.
        Some(keys) if seen.shared => {
            if seen.open.iter().any(|k| !keys.contains(k)) {
                Next::Rehost
            } else {
                Next::Wait
            }
        }
        // Someone else's session (a ticket, a link, a library set).
        None if seen.shared => Next::Wait,
        _ => Next::Host,
    }
}

/// How the auto-host's library list for a set is arrived at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// The list of that name already has every song.
    Use { id: String },
    /// The list of that name, given the songs it lacks at its end.
    Update { id: String, songs: Vec<String> },
    /// No list of that name: a new set.
    Create,
}

/// The library list a set called `title` of songs `slugs` is hosted as:
/// the list of that name (a set before a song list), given what it lacks —
/// never anything removed or moved — or a new one.
#[must_use]
pub fn plan(lists: &[Setlist], title: &str, slugs: &[String]) -> Plan {
    let named = || lists.iter().filter(|l| l.title == title);
    let Some(list) = named()
        .find(|l| l.kind == ListKind::Set)
        .or_else(|| named().next())
    else {
        return Plan::Create;
    };
    let mut songs: Vec<String> = list.songs.iter().map(|s| s.slug.clone()).collect();
    let before = songs.len();
    for slug in slugs {
        if !songs.contains(slug) {
            songs.push(slug.clone());
        }
    }
    if songs.len() == before {
        Plan::Use {
            id: list.id.clone(),
        }
    } else {
        Plan::Update {
            id: list.id.clone(),
            songs,
        }
    }
}

/// Make sure the org's library has the songs `keys` (by name) and a list
/// called `title` holding them; its id.
///
/// # Errors
/// The library could not be reached, or refused a change.
pub async fn ensure_set(library: &Library, title: &str, keys: &[String]) -> eyre::Result<String> {
    let known = library.songs().await?;
    let mut slugs = Vec::with_capacity(keys.len());
    for key in keys {
        let slug = slugify(key);
        if known.iter().any(|(song, _)| song.slug == slug) {
            slugs.push(slug);
            continue;
        }
        let made = library.create_song(key, "", &[]).await?;
        if made.slug != slug {
            // The set matches its songs by `slugify(name)`: a song the
            // server slugs otherwise stays this machine's own.
            tracing::warn!(song.key = %key, song.slug = %made.slug, "live-host: the library slugged a new song differently");
        }
        tracing::info!(song.slug = %made.slug, "live-host: a song was added to the library");
        slugs.push(made.slug);
    }
    let lists = library.setlists().await?;
    match plan(&lists, title, &slugs) {
        Plan::Use { id } => Ok(id),
        Plan::Update { id, songs } => {
            library.set_songs(&id, &songs).await?;
            Ok(id)
        }
        Plan::Create => {
            let made = library.create_list(title, ListKind::Set).await?;
            library.set_songs(&made.id, &slugs).await?;
            tracing::info!(live.setlist = %made.id, live.title = %title, "live-host: a set was made for what is open");
            Ok(made.id)
        }
    }
}

/// Whether automatic hosting is off here.
fn disabled() -> bool {
    let off = std::env::var(AUTOHOST_ENV).is_ok_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        )
    });
    off || crate::collab::env_set(crate::collab::TASK_SET_ENV)
        || crate::collab::env_set("FTS_COLLAB_HOST")
        || crate::collab::env_set("FTS_COLLAB_JOIN")
}

/// Start hosting automatically (once per process; later calls do
/// nothing). `fallback_name` is who this person is in a set when the
/// account does not say.
pub fn start(fallback_name: String) {
    if disabled() || STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("live-host".into())
        .spawn(move || run(&fallback_name))
        .map_or_else(
            |e| tracing::warn!(error = %e, "live-host: could not start"),
            drop,
        );
}

/// A failure's wait before the next try: from [`POLL`], doubling.
struct Backoff {
    wait: Duration,
    until: Option<Instant>,
}

impl Backoff {
    const fn new() -> Self {
        Self {
            wait: POLL,
            until: None,
        }
    }

    fn ready(&self) -> bool {
        self.until.is_none_or(|t| Instant::now() >= t)
    }

    fn failed(&mut self) {
        self.until = Some(Instant::now() + self.wait);
        self.wait = (self.wait * 2).min(MOST_BACKOFF);
    }

    fn reset(&mut self) {
        *self = Self::new();
    }
}

fn run(fallback_name: &str) {
    let mut backoff = Backoff::new();
    let mut left_with: Option<Vec<String>> = None;
    let mut last_open: Vec<String> = Vec::new();
    loop {
        std::thread::sleep(POLL);
        let account = crate::task_account::signed_in();
        let Ok(open) = crate::collab::open_song_keys() else {
            continue;
        };
        if open != last_open {
            // Something else is open: try at once, and a set left by
            // hand is not this one.
            backoff.reset();
            last_open.clone_from(&open);
        }
        let shared = crate::collab::status().is_some();
        let mut hosted = hosted();
        // A session someone started by hand replaced the hosted one: it
        // is theirs now.
        let replaced = hosted
            .as_ref()
            .zip(crate::collab::status())
            .is_some_and(|(h, live)| live.set != h.title);
        if replaced {
            hosted = None;
            if let Ok(mut slot) = HOSTED.lock() {
                *slot = None;
            }
        }
        // Hosting, and no session: its person left it.
        if hosted.is_some() && !shared && !crate::collab::joining() {
            left_with = Some(open.clone());
            if let Ok(mut slot) = HOSTED.lock() {
                *slot = None;
            }
            tracing::info!("live-host: the set was left; it stays left until what is open changes");
            continue;
        }
        let seen = Seen {
            signed_in: account.is_some(),
            joining: crate::collab::joining(),
            shared,
            hosted: hosted.as_ref().map(|h| h.keys.as_slice()),
            open: &open,
            left_with: left_with.as_deref(),
        };
        match next(&seen) {
            Next::Wait => {}
            Next::Leave => {
                crate::collab::leave();
                if let Ok(mut slot) = HOSTED.lock() {
                    *slot = None;
                }
                tracing::info!("live-host: signed out; the set is left");
            }
            Next::Host | Next::Rehost => {
                let Some(account) = account else { continue };
                if !backoff.ready() {
                    continue;
                }
                left_with = None;
                match host(&account, fallback_name, open) {
                    Ok(set) => {
                        tracing::info!(live.setlist = %set.setlist, live.title = %set.title, live.songs = set.keys.len(), "live-host: hosting in Task");
                        set_error(None);
                        backoff.reset();
                        if let Ok(mut slot) = HOSTED.lock() {
                            *slot = Some(set);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "live-host: could not host in Task");
                        set_error(Some(crate::task_set::brief(&e.to_string())));
                        backoff.failed();
                    }
                }
            }
        }
    }
}

/// The org to host in.
fn org_for(account: &crate::task_account::Account) -> eyre::Result<String> {
    if let Some(org) = std::env::var("FTS_TASK_ORG")
        .ok()
        .filter(|o| !o.trim().is_empty())
    {
        return Ok(org.trim().to_owned());
    }
    let orgs = crate::task_account::orgs(account)?;
    if let Some(org) = crate::task_account::remembered_org().filter(|o| orgs.contains(o)) {
        return Ok(org);
    }
    orgs.into_iter()
        .next()
        .ok_or_else(|| eyre::eyre!("this account is in no org on {}", account.server))
}

/// Host the open songs (`keys`) in Task as `account`.
fn host(
    account: &crate::task_account::Account,
    fallback_name: &str,
    keys: Vec<String>,
) -> eyre::Result<Hosted> {
    let org = org_for(account)?;
    let library = account.library(&org);
    let title = crate::collab::open_set_title()?;
    let setlist = crate::collab::on_runtime(ensure_set(&library, &title, &keys))?;
    let set = crate::collab::TaskSet {
        url: library.org_url(),
        token: library.token.clone(),
        setlist: setlist.clone(),
    };
    let name = std::env::var("FTS_COLLAB_NAME")
        .ok()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| {
            let name = account.name();
            if name == "Me" {
                fallback_name.to_owned()
            } else {
                name
            }
        });
    crate::collab::join_task(&set, name)?;
    Ok(Hosted {
        org,
        setlist,
        title,
        keys,
    })
}

#[cfg(test)]
mod tests {
    use super::{Next, Plan, Seen, next, plan};
    use session::library::{ListKind, Setlist, Song};

    fn keys(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn signed_in_with_a_song_open_and_nothing_live_hosts() {
        let open = keys(&["Washed"]);
        let seen = Seen {
            signed_in: true,
            open: &open,
            ..Seen::default()
        };
        assert_eq!(next(&seen), Next::Host);
    }

    #[test]
    fn nothing_is_hosted_signed_out_with_nothing_open_or_while_joining() {
        let open = keys(&["Washed"]);
        let none: Vec<String> = Vec::new();
        assert_eq!(
            next(&Seen {
                open: &open,
                ..Seen::default()
            }),
            Next::Wait,
            "signed out"
        );
        assert_eq!(
            next(&Seen {
                signed_in: true,
                open: &none,
                ..Seen::default()
            }),
            Next::Wait,
            "nothing open"
        );
        assert_eq!(
            next(&Seen {
                signed_in: true,
                joining: true,
                open: &open,
                ..Seen::default()
            }),
            Next::Wait,
            "joining"
        );
    }

    #[test]
    fn someone_elses_session_is_left_alone() {
        let open = keys(&["Washed"]);
        let seen = Seen {
            signed_in: true,
            shared: true,
            open: &open,
            ..Seen::default()
        };
        assert_eq!(next(&seen), Next::Wait);
    }

    #[test]
    fn a_song_opened_later_joins_the_hosted_set() {
        let hosted = keys(&["Washed"]);
        let open = keys(&["Washed", "Goodness"]);
        let mut seen = Seen {
            signed_in: true,
            shared: true,
            hosted: Some(&hosted),
            open: &open,
            ..Seen::default()
        };
        assert_eq!(next(&seen), Next::Rehost);
        seen.open = &hosted;
        assert_eq!(next(&seen), Next::Wait, "the same songs: nothing to do");
    }

    #[test]
    fn signing_out_leaves_the_hosted_set() {
        let hosted = keys(&["Washed"]);
        let seen = Seen {
            shared: true,
            hosted: Some(&hosted),
            open: &hosted,
            ..Seen::default()
        };
        assert_eq!(next(&seen), Next::Leave);
    }

    #[test]
    fn a_set_left_by_hand_stays_left_until_what_is_open_changes() {
        let open = keys(&["Washed"]);
        let other = keys(&["Goodness"]);
        let mut seen = Seen {
            signed_in: true,
            open: &open,
            left_with: Some(&open),
            ..Seen::default()
        };
        assert_eq!(next(&seen), Next::Wait);
        seen.open = &other;
        assert_eq!(next(&seen), Next::Host);
    }

    fn list(id: &str, title: &str, kind: ListKind, slugs: &[&str]) -> Setlist {
        Setlist {
            id: id.into(),
            title: title.into(),
            kind,
            songs: slugs
                .iter()
                .map(|s| Song {
                    slug: (*s).into(),
                    title: (*s).into(),
                    writers: Vec::new(),
                    key: String::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_set_of_that_name_is_used_given_what_it_lacks_at_its_end() {
        let lists = vec![
            list("songs", "Worship Set", ListKind::Songs, &["washed"]),
            list("set", "Worship Set", ListKind::Set, &["goodness", "washed"]),
            list("other", "Sunday", ListKind::Set, &[]),
        ];
        assert_eq!(
            plan(&lists, "Worship Set", &keys(&["washed"])),
            Plan::Use { id: "set".into() },
            "a set before a song list of the same name"
        );
        assert_eq!(
            plan(&lists, "Worship Set", &keys(&["washed", "holy"])),
            Plan::Update {
                id: "set".into(),
                songs: keys(&["goodness", "washed", "holy"]),
            },
            "nothing removed or moved"
        );
    }

    #[test]
    fn a_lone_song_with_no_list_of_its_name_gets_a_new_set() {
        let lists = vec![list("set", "Sunday", ListKind::Set, &["washed"])];
        assert_eq!(plan(&lists, "Washed", &keys(&["washed"])), Plan::Create);
        let only_songs = vec![list("songs", "Washed", ListKind::Songs, &["washed"])];
        assert_eq!(
            plan(&only_songs, "Washed", &keys(&["washed"])),
            Plan::Use { id: "songs".into() },
            "a song list of the name, when there is no set"
        );
    }
}
