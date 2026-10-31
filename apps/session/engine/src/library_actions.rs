//! The Task library's lists and songs, as named actions
//! (`docs/design/actions-everywhere.md`): what the start screen's library
//! editor does — a list made, renamed, copied, deleted, its songs set; a
//! song made, changed or deleted — as the person signed in on this machine,
//! in an org's library (`org`, else FTS_TASK_ORG, else the one last
//! picked). So `watch` sees each, and the CLI or a model edits the library
//! the same way.

use architect::action::{ActionList, Effect};
use facet::Facet;
use session::library::{Library, ListKind, Setlist, Song};

use crate::shared_actions::meta;

pub static LISTS: architect::action::ActionMeta = meta(
    "LIBRARY_LISTS",
    "library_lists",
    "The library's lists",
    "The setlists and song lists of an org's Task library (`org`, else the one last picked), each with its id, title, kind (`set` or `songs`) and its songs' slugs in order — the ids the LIBRARY_LIST_* actions take. Read-only.",
    "Library",
    "",
);
pub static LIST_CREATE: architect::action::ActionMeta = meta(
    "LIBRARY_LIST_CREATE",
    "library_list_create",
    "Make a list",
    "Make a list in the library: `kind` `set` (a setlist, in the order it is played) or `songs` (songs gathered to pick sets from), titled `title` (New setlist / New song list when left out). Says the list.",
    "Library",
    "",
);
pub static LIST_RENAME: architect::action::ActionMeta = meta(
    "LIBRARY_LIST_RENAME",
    "library_list_rename",
    "Rename a list",
    "Rename list `list` (its id, from LIBRARY_LISTS) to `title`.",
    "Library",
    "",
);
pub static LIST_DELETE: architect::action::ActionMeta = meta(
    "LIBRARY_LIST_DELETE",
    "library_list_delete",
    "Delete a list",
    "Delete list `list` (its id) from the library. Its songs stay in the library.",
    "Library",
    "",
);
pub static LIST_DUPLICATE: architect::action::ActionMeta = meta(
    "LIBRARY_LIST_DUPLICATE",
    "library_list_duplicate",
    "Copy a list",
    "A copy of list `list` (its id) with its songs in order, titled `title` (its title and (copy) when left out): last week's set as the start of this week's. Says the copy.",
    "Library",
    "",
);
pub static LIST_SONGS: architect::action::ActionMeta = meta(
    "LIBRARY_LIST_SONGS",
    "library_list_songs",
    "Set a list's songs",
    "Set list `list`'s songs (its id) to `songs`, slugs in order: songs added, removed or moved.",
    "Library",
    "",
);
pub static SONG_SAVE: architect::action::ActionMeta = meta(
    "LIBRARY_SONG_SAVE",
    "library_song_save",
    "Save a song",
    "Make a song in the library (no `slug`), or change song `slug`: its `title`, `key` and `writers`. Says the song.",
    "Library",
    "",
);
pub static SONG_DELETE: architect::action::ActionMeta = meta(
    "LIBRARY_SONG_DELETE",
    "library_song_delete",
    "Delete a song",
    "Delete song `slug` from the library.",
    "Library",
    "",
);
pub static ORG: architect::action::ActionMeta = meta(
    "LIBRARY_ORG",
    "library_org",
    "Pick the library's org",
    "Work in org `org`'s library from now on: what the start screen shows and the LIBRARY_* actions edit when no `org` is given. Remembered on this machine.",
    "Library",
    "",
);
pub static SIGN_OUT: architect::action::ActionMeta = meta(
    "ACCOUNT_SIGN_OUT",
    "account_sign_out",
    "Sign out",
    "Sign out of Task on this machine: the libraries are closed to it until it signs in again.",
    "Library",
    "",
);

/// Which org's library.
#[derive(Facet)]
struct OrgArg {
    #[facet(default)]
    org: Option<String>,
}

#[derive(Facet)]
struct CreateArg {
    kind: String,
    #[facet(default)]
    title: Option<String>,
    #[facet(default)]
    org: Option<String>,
}

#[derive(Facet)]
struct RenameArg {
    list: String,
    title: String,
    #[facet(default)]
    org: Option<String>,
}

#[derive(Facet)]
struct ListArg {
    list: String,
    #[facet(default)]
    org: Option<String>,
}

#[derive(Facet)]
struct DuplicateArg {
    list: String,
    #[facet(default)]
    title: Option<String>,
    #[facet(default)]
    org: Option<String>,
}

#[derive(Facet)]
struct SongsArg {
    list: String,
    songs: Vec<String>,
    #[facet(default)]
    org: Option<String>,
}

#[derive(Facet)]
struct SongArg {
    #[facet(default)]
    slug: Option<String>,
    title: String,
    #[facet(default)]
    key: String,
    #[facet(default)]
    writers: Vec<String>,
    #[facet(default)]
    org: Option<String>,
}

#[derive(Facet)]
struct SlugArg {
    slug: String,
    #[facet(default)]
    org: Option<String>,
}

#[derive(Facet)]
struct PickArg {
    org: String,
}

/// A list as an action says it.
#[derive(Facet)]
pub struct ListOut {
    pub id: String,
    pub title: String,
    /// `set` or `songs`.
    pub kind: String,
    /// Its songs' slugs, in order.
    pub songs: Vec<String>,
}

/// A song as an action says it.
#[derive(Facet)]
pub struct SongOut {
    pub slug: String,
    pub title: String,
    pub key: String,
    pub writers: Vec<String>,
}

fn list_out(list: &Setlist) -> ListOut {
    ListOut {
        id: list.id.clone(),
        title: list.title.clone(),
        kind: kind_name(list.kind).to_owned(),
        songs: list.songs.iter().map(|s| s.slug.clone()).collect(),
    }
}

fn song_out(song: &Song) -> SongOut {
    SongOut {
        slug: song.slug.clone(),
        title: song.title.clone(),
        key: song.key.clone(),
        writers: song.writers.clone(),
    }
}

const fn kind_name(kind: ListKind) -> &'static str {
    match kind {
        ListKind::Set => "set",
        ListKind::Songs => "songs",
    }
}

/// The library of `org` (else FTS_TASK_ORG, else the one last picked), as
/// the person signed in here.
fn library(org: Option<&str>) -> Result<Library, String> {
    use crate::task_account::AccountLibrary as _;
    let account =
        crate::task_account::signed_in().ok_or("not signed in to Task on this machine")?;
    let org = org
        .map(str::to_owned)
        .or_else(|| std::env::var("FTS_TASK_ORG").ok().filter(|o| !o.is_empty()))
        .or_else(crate::task_account::remembered_org)
        .ok_or("which org's library? (`org`)")?;
    Ok(account.library(&org))
}

/// `work` on the library of `org`, on a thread of its own (the library is
/// driven on the engine's runtime, which an action may already be on),
/// awaited.
async fn on_library<T, F, Fut>(org: Option<String>, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(Library) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = eyre::Result<T>>,
{
    let library = library(org.as_deref())?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("library-action".into())
        .spawn(move || {
            let done = crate::stream_set::with_library(&library, work).map_err(|e| e.to_string());
            let _ = tx.send(done);
        })
        .map_err(|e| e.to_string())?;
    rx.await
        .map_err(|_| "the library work stopped".to_owned())?
}

/// List `id` of the library.
async fn find_list(org: Option<String>, id: String) -> Result<Setlist, String> {
    let lists = on_library(org, |l| async move { l.setlists().await }).await?;
    lists
        .into_iter()
        .find(|l| l.id == id)
        .ok_or_else(|| format!("no list {id} (LIBRARY_LISTS)"))
}

/// Put the library's actions on the list.
///
/// # Errors
/// The first registration the list refuses (a duplicate id).
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register_async(&LISTS, Effect::ReadOnly, |a: OrgArg| async move {
        let lists = on_library(a.org, |l| async move { l.setlists().await }).await?;
        Ok(lists.iter().map(list_out).collect::<Vec<_>>())
    })?;
    list.register_async(&LIST_CREATE, Effect::Mutating, |a: CreateArg| async move {
        let kind = match a.kind.trim().to_ascii_lowercase().as_str() {
            "set" | "setlist" => ListKind::Set,
            "songs" | "song list" => ListKind::Songs,
            other => return Err(format!("`kind` is set or songs, not {other:?}")),
        };
        let title = a.title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| {
            match kind {
                ListKind::Set => "New setlist",
                ListKind::Songs => "New song list",
            }
            .to_owned()
        });
        let made = on_library(
            a.org,
            move |l| async move { l.create_list(&title, kind).await },
        )
        .await?;
        Ok(list_out(&made))
    })?;
    list.register_async(&LIST_RENAME, Effect::Mutating, |a: RenameArg| async move {
        let title = a.title.trim().to_owned();
        if title.is_empty() {
            return Err("a list needs a title".to_owned());
        }
        on_library(a.org, move |l| async move {
            l.rename_list(&a.list, &title).await
        })
        .await
    })?;
    list.register_async(&LIST_DELETE, Effect::Destructive, |a: ListArg| async move {
        on_library(a.org, move |l| async move { l.delete_list(&a.list).await }).await
    })?;
    list.register_async(
        &LIST_DUPLICATE,
        Effect::Mutating,
        |a: DuplicateArg| async move {
            let from = find_list(a.org.clone(), a.list).await?;
            let title = a
                .title
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| format!("{} (copy)", from.title));
            let copy = on_library(a.org, move |l| async move {
                l.duplicate_list(&from, &title).await
            })
            .await?;
            Ok(list_out(&copy))
        },
    )?;
    list.register_async(&LIST_SONGS, Effect::Mutating, |a: SongsArg| async move {
        on_library(a.org, move |l| async move {
            l.set_songs(&a.list, &a.songs).await
        })
        .await
    })?;
    list.register_async(&SONG_SAVE, Effect::Mutating, |a: SongArg| async move {
        let title = a.title.trim().to_owned();
        if title.is_empty() {
            return Err("a song needs a title".to_owned());
        }
        let key = a.key.trim().to_owned();
        let writers = a.writers;
        let saved = match a.slug {
            None => {
                on_library(a.org, move |l| async move {
                    l.create_song(&title, &key, &writers).await
                })
                .await?
            }
            Some(slug) => {
                on_library(a.org, move |l| async move {
                    let was = l
                        .songs()
                        .await?
                        .into_iter()
                        .map(|(song, _)| song)
                        .find(|s| s.slug == slug)
                        .ok_or_else(|| eyre::eyre!("no song {slug} in the library"))?;
                    let now = Song {
                        title,
                        key,
                        writers,
                        ..was
                    };
                    l.update_song(&now).await?;
                    Ok(now)
                })
                .await?
            }
        };
        Ok(song_out(&saved))
    })?;
    list.register_async(&SONG_DELETE, Effect::Destructive, |a: SlugArg| async move {
        on_library(a.org, move |l| async move { l.delete_song(&a.slug).await }).await
    })?;
    list.register(&ORG, Effect::Mutating, |a: PickArg| {
        let org = a.org.trim();
        if org.is_empty() {
            return Err("name an org".to_owned());
        }
        crate::task_account::remember_org(org);
        Ok(format!("working in {org}'s library"))
    })?;
    list.register(&SIGN_OUT, Effect::Mutating, |(): ()| {
        crate::task_account::sign_out();
        Ok("signed out".to_owned())
    })?;
    Ok(())
}
