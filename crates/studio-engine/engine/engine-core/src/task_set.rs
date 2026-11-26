//! A set Task keeps, reached: which set it is ([`TaskSet`]), joined by its
//! live link, and tried again until Task answers — a page and the app join
//! the one way. And whether what is open came from a library's set
//! ([`opened_from_library`]), which hosting it in Task reads
//! (engine-collab's `live_host`).

#[cfg(feature = "native")]
use std::sync::atomic::{AtomicBool, Ordering};

/// A set Task keeps (`live-proto`): where to reach Task, and which
/// setlist — a member's org lane with its token, or a live share link's
/// guest lane with none.
#[derive(Clone, Debug)]
pub struct TaskSet {
    /// The vox URL (`wss://…/org/<org>/vox`, or `…/org/<org>/share/<token>/vox`).
    pub url: String,
    /// The member's bearer token; `None` on a share link.
    pub token: Option<String>,
    /// The setlist's id in Task.
    pub setlist: String,
}

/// How the environment names a set Task keeps (`FTS_COLLAB_TASK`): a
/// setlist's id, joined as a member of the library's org (`FTS_TASK_*`), or
/// `share:<live share link>`, joined as a guest of the link's set.
pub const TASK_SET_ENV: &str = "FTS_COLLAB_TASK";

impl TaskSet {
    /// Joined through a live share link (`…/share/<token>/vox`): a guest,
    /// who plays the set but never seeds it (`collab::may_seed`).
    #[must_use]
    pub fn is_guest(&self) -> bool {
        self.url.contains("/share/")
    }

    /// The set `FTS_COLLAB_TASK` names, if it names one.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let value = std::env::var(TASK_SET_ENV)
            .ok()
            .filter(|v| !v.trim().is_empty())?;
        Some(Self::parse(value.trim()))
    }

    /// A setlist id, or `share:<link>`.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value.strip_prefix("share:") {
            Some(link) => {
                let link = link.trim_end_matches('/');
                let base = link.split('?').next().unwrap_or(link);
                let url = base
                    .replacen("https://", "wss://", 1)
                    .replacen("http://", "ws://", 1);
                Self {
                    url: format!("{url}/vox"),
                    token: None,
                    setlist: String::new(),
                }
            }
            #[cfg(feature = "native")]
            None => {
                let library = library_sync::Library::from_env();
                Self {
                    url: library.org_url(),
                    token: library.token,
                    setlist: value.to_owned(),
                }
            }
            // A page joins by link: a member's lane is the app's.
            #[cfg(not(feature = "native"))]
            None => Self {
                url: String::new(),
                token: None,
                setlist: value.to_owned(),
            },
        }
    }
}

/// What is open came from a set of the library (`stream_set::open`, a
/// library's set or one song of it) — until something else is opened.
#[cfg(feature = "native")]
static FROM_LIBRARY: AtomicBool = AtomicBool::new(false);

/// Say whether what is open now was streamed in from a library set.
#[cfg(feature = "native")]
pub fn opened_from_library(yes: bool) {
    FROM_LIBRARY.store(yes, Ordering::Release);
}

/// Whether what is open now was streamed in from a library set.
#[cfg(feature = "native")]
#[must_use]
pub fn from_library() -> bool {
    FROM_LIBRARY.load(Ordering::Acquire)
}

/// Try `attempt` until it works — or `limit` times — waiting between
/// tries from a second, doubling to fifteen; `failed` hears why each one
/// did not, and when the next is.
///
/// # Errors
///
/// The last try's, once `limit` tries have failed.
pub async fn until_ok<T, Fut>(
    limit: Option<u32>,
    mut attempt: impl FnMut() -> Fut,
    mut failed: impl FnMut(String),
) -> eyre::Result<T>
where
    Fut: std::future::Future<Output = eyre::Result<T>>,
{
    let mut wait = std::time::Duration::from_secs(1);
    let mut tries = 0u32;
    loop {
        match attempt().await {
            Ok(done) => return Ok(done),
            Err(e) => {
                tries += 1;
                if limit.is_some_and(|limit| tries >= limit) {
                    return Err(e);
                }
                tracing::debug!(error = %e, tries, "task: trying again");
                failed(format!("{e} — trying again in {} s", wait.as_secs()));
                architect::platform::sleep(wait).await;
                wait = (wait * 2).min(std::time::Duration::from_secs(15));
            }
        }
    }
}

/// Join the set a live link opens, on the Task at `url` (its vox lane —
/// [`TaskSet::parse`] makes it from the link).
///
/// # Errors
///
/// Task is not answering, or would not let the set be joined.
pub async fn join(url: &str) -> eyre::Result<live_proto::LiveSet> {
    use live_proto::LiveSessionsClient;
    let lane: LiveSessionsClient = task_dial::establish_at(url, None)
        .await
        .map_err(|e| eyre::eyre!("Session's servers are not answering ({})", brief(&e)))?;
    lane.join(String::new())
        .await
        .map_err(|e| eyre::eyre!("the set could not be joined ({e:?})"))
}

/// An error, said briefly: the set's.
pub use crate::studio_engine::set::brief;
