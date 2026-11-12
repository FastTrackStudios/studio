//! The extension: one `linked::Extension`.
//!
//! Compiled into Studio (or any Architect build that wants it) and served
//! as a process the Architect app installs (`src/main.rs`,
//! `extension.toml`'s `[entry]`). It reaches
//! the vault and share links only through the app's actions
//! ([`crate::host`]), so both copies do the same thing.

use architect_extensions::linked::{Builder, Extension};
use dioxus::prelude::*;
use serde_json::{Value, json};

use crate::host::{self, SetlistRow, SongRow};
use crate::layout::{SongRef, setlist_note};
use crate::nav::{self, LibrariesNav, SetlistsNav};
use crate::setlist::Setlist;
use crate::workspace::{ChartAccess, Loaded, chart_fence, lyrics_source};
use crate::{check_name, frontmatter};

/// The extension.
#[derive(Debug, Clone, Copy, Default)]
pub struct Setlists;

impl Extension for Setlists {
    fn manifest(&self) -> &'static str {
        include_str!("../extension.toml")
    }

    fn files(&self) -> &'static [(&'static str, &'static str)] {
        &[("song.json", SONG_LAYOUT)]
    }

    fn build(&self, ext: &mut Builder) {
        host::connect(ext.handle());
        ext.navigator(nav::SETLISTS, SetlistsNav);
        ext.navigator(nav::LIBRARIES, LibrariesNav);
        ext.view("song", SongView)
            .view("setlist", SetlistView)
            .action("STUDIO_SETLISTS_LIST", |_| list())
            .action("STUDIO_SETLISTS_CREATE", create)
            .action("STUDIO_SETLISTS_ADD", add)
            .action("STUDIO_SETLISTS_REMOVE", |a| async move {
                let song = text(&a, "song")?;
                edit(&a, move |s| s.remove(&song).map(drop)).await
            })
            .action("STUDIO_SETLISTS_MOVE", |a| async move {
                let song = text(&a, "song")?;
                let to = usize::try_from(a.get("to").and_then(Value::as_u64).ok_or("give `to`")?)
                    .map_err(|e| e.to_string())?;
                let section = opt(&a, "section");
                edit(&a, move |s| s.move_song(&song, to, section.as_deref())).await
            })
            .action("STUDIO_SETLISTS_SHARE", share)
            .action("STUDIO_SETLISTS_SONGS", song_list)
            .action("STUDIO_SETLISTS_LIBRARIES", |_| libraries());
    }
}

fn text(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("give `{key}`"))
}

fn opt(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
}

/// The song `spec` names (`[Library/]Song[#Arrangement]`), `library` its
/// default; a bare title is found in whichever library has it, when
/// only one does.
fn resolve(spec: &str, library: Option<&str>, songs: &[SongRow]) -> Result<SongRef, String> {
    if let Some(at) = SongRef::parse(spec, library) {
        return songs
            .iter()
            .any(|s| s.at.library == at.library && s.at.title.eq_ignore_ascii_case(&at.title))
            .then_some(at.clone())
            .ok_or_else(|| format!("no song `{}` in the library {}", at.title, at.library));
    }
    let (title, arrangement) = spec
        .split_once('#')
        .map_or((spec, None), |(t, a)| (t, Some(a)));
    let found: Vec<&SongRow> = songs
        .iter()
        .filter(|s| s.at.title.eq_ignore_ascii_case(title.trim()))
        .collect();
    match found.as_slice() {
        [one] => Ok(SongRef::new(&one.at.library, &one.at.title, arrangement)),
        [] => Err(format!("no song `{title}` in any library")),
        many => Err(format!(
            "`{title}` is in {} libraries ({}): name one, `Library/{title}`",
            many.len(),
            many.iter()
                .map(|s| s.at.library.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn known<'a>(songs: &'a [SongRow], at: &SongRef) -> Option<&'a SongRow> {
    songs
        .iter()
        .find(|r| r.at.library == at.library && r.at.title == at.title)
}

fn setlist_json(s: &SetlistRow, songs: &[SongRow]) -> Value {
    let set = s.setlist();
    let song = |at: &SongRef| {
        json!({
            "song": at.label(),
            "library": at.library,
            "summary": known(songs, at).map(SongRow::summary),
        })
    };
    let mut out = json!({
        "title": s.title,
        "path": s.path,
        "date": s.date,
        "share": s.share,
        "songs": s.songs.iter().map(song).collect::<Vec<_>>(),
    });
    if let (false, Some(fields)) = (set.sets.is_empty(), out.as_object_mut()) {
        fields.insert(
            "sets".to_owned(),
            json!(
                set.sections()
                    .iter()
                    .map(|x| json!({
                        "set": x.name,
                        "songs": x.songs.iter().map(song).collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>()
            ),
        );
    }
    out
}

async fn list() -> Result<Value, String> {
    let songs = host::songs().await.unwrap_or_default();
    Ok(json!(
        host::setlists()
            .await?
            .iter()
            .map(|s| setlist_json(s, &songs))
            .collect::<Vec<_>>()
    ))
}

async fn create(a: Value) -> Result<Value, String> {
    let title = text(&a, "title")?;
    check_name(&title).map_err(|e| e.to_string())?;
    let library = opt(&a, "library");
    let all = host::songs().await?;
    let specs: Vec<String> = a
        .get("songs")
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let songs = specs
        .iter()
        .map(|s| resolve(s, library.as_deref(), &all))
        .collect::<Result<Vec<_>, _>>()?;
    let mut set = Setlist::new(&title, Vec::new());
    set.date = opt(&a, "date");
    let section = opt(&a, "section");
    for song in &songs {
        set.add(song.clone(), None, section.as_deref())
            .map_err(|e| e.to_string())?;
    }
    let mut props = serde_json::Map::new();
    props.insert("type".to_owned(), json!(crate::setlist::KIND));
    props.insert("title".to_owned(), json!(title));
    props.insert("songs".to_owned(), set.songs_property());
    if let Some(sets) = set.sets_property() {
        props.insert("sets".to_owned(), sets);
    }
    if let Some(date) = &set.date {
        props.insert("date".to_owned(), json!(date));
    }
    let path = setlist_note(&title);
    let body = set.page(|at| known(&all, at).map(SongRow::info));
    host::run(
        "VAULT_CREATE",
        json!({ "path": path, "properties": Value::Object(props).to_string(), "body": body }),
    )
    .await?;
    Ok(
        json!({ "title": title, "path": path, "songs": songs.iter().map(SongRef::label).collect::<Vec<_>>() }),
    )
}

/// The setlist `args.setlist` names.
async fn find(args: &Value) -> Result<SetlistRow, String> {
    let name = text(args, "setlist")?;
    host::setlists()
        .await?
        .into_iter()
        .find(|s| s.title.eq_ignore_ascii_case(&name) || s.path == name)
        .ok_or_else(|| format!("no setlist `{name}`"))
}

/// Change setlist `args.setlist`'s songs with `f`, written back as its
/// `songs` and `sets` properties.
///
/// Its page (the note's body, written when it was made) keeps the running
/// order it was made with: the app has no action that rewrites a note's
/// body (FastTrackStudios/architect#492).
async fn edit(
    args: &Value,
    f: impl FnOnce(&mut Setlist) -> crate::Result<()>,
) -> Result<Value, String> {
    let row = find(args).await?;
    let mut set = row.setlist();
    f(&mut set).map_err(|e| e.to_string())?;
    host::run(
        "VAULT_SET_PROPERTY",
        json!({ "path": row.path, "key": "songs", "value": set.songs_property().to_string() }),
    )
    .await?;
    match set.sets_property() {
        Some(sets) => {
            host::run(
                "VAULT_SET_PROPERTY",
                json!({ "path": row.path, "key": "sets", "value": sets.to_string() }),
            )
            .await?;
        }
        None if !row.sets.is_empty() => {
            host::run(
                "VAULT_REMOVE_PROPERTY",
                json!({ "path": row.path, "key": "sets" }),
            )
            .await?;
        }
        None => {}
    }
    let songs = host::songs().await.unwrap_or_default();
    Ok(setlist_json(
        &SetlistRow {
            songs: set.songs,
            sets: set.sets,
            ..row
        },
        &songs,
    ))
}

async fn add(a: Value) -> Result<Value, String> {
    let all = host::songs().await?;
    let at = resolve(&text(&a, "song")?, opt(&a, "library").as_deref(), &all)?;
    let position = a
        .get("at")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .map(|n| usize::try_from(n).unwrap_or(usize::MAX));
    let section = opt(&a, "section");
    edit(&a, move |s| s.add(at, position, section.as_deref())).await
}

/// Share the setlist with Architect's share links: the setlist's note
/// and its songs', for `audience` (link by default).
async fn share(a: Value) -> Result<Value, String> {
    let row = find(&a).await?;
    let mut notes = vec![row.path.clone()];
    for s in &row.songs {
        let note = format!("{}.md", s.library().song_link_target(&s.title));
        if !notes.contains(&note) {
            notes.push(note);
        }
    }
    let out = host::run(
        "SHARE_CREATE",
        json!({
            "notes": notes,
            "audience": opt(&a, "audience").unwrap_or_else(|| "link".to_owned()),
            "title": row.title,
        }),
    )
    .await?;
    let code = out
        .get("share")
        .and_then(|s| s.get("code"))
        .or_else(|| out.get("code"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    if let Some(code) = &code {
        host::run(
            "VAULT_SET_PROPERTY",
            json!({ "path": row.path, "key": "share", "value": json!(code).to_string() }),
        )
        .await?;
    }
    Ok(json!({ "code": code, "url": out.get("url") }))
}

async fn song_list(a: Value) -> Result<Value, String> {
    let library = opt(&a, "library");
    let category = opt(&a, "category");
    Ok(json!(
        host::songs()
            .await?
            .iter()
            .filter(|s| library
                .as_ref()
                .is_none_or(|l| s.at.library.eq_ignore_ascii_case(l)))
            .filter(|s| category
                .as_ref()
                .is_none_or(|c| s.categories.iter().any(|x| x.eq_ignore_ascii_case(c))))
            .map(|s| json!({
                "song": s.at.title,
                "library": s.at.library,
                "categories": s.categories,
                "arrangements": s.arrangements,
                "summary": s.summary(),
                "path": s.path,
            }))
            .collect::<Vec<_>>()
    ))
}

async fn libraries() -> Result<Value, String> {
    let snap = nav::libraries_snapshot(&host::songs().await?);
    Ok(json!(
        snap.tree
            .iter()
            .map(|lib| json!({
                "library": lib.title,
                "categories": lib.children.iter().map(|c| json!({
                    "category": c.title,
                    "songs": snap.items.iter().filter(|i| i.container == c.id).count(),
                })).collect::<Vec<_>>(),
                "uncategorised": snap.items.iter().filter(|i| i.container == lib.id).count(),
            }))
            .collect::<Vec<_>>()
    ))
}

/// A song on show: its note's path, its title, its tab's properties.
type DeckSong = (String, String, Vec<(String, String)>);

/// The song on show in this deck.
fn deck_song() -> Option<DeckSong> {
    architect_workspace::use_deck_context()
        .filter(|c| c.kind == nav::SONG)
        .map(|c| {
            let props = c.properties.into_iter().map(|p| (p.key, p.value)).collect();
            (c.id, c.title, props)
        })
}

/// A song's workspace: its note, read through the app, as Session's
/// Overview (`crate::workspace_view::SongWorkspace`): the progress, the
/// chart's text over the lyrics, the chart.
#[allow(non_snake_case)]
fn SongView() -> Element {
    let song = deck_song();
    let Some((path, title, props)) = song else {
        return rsx! { div { style: "padding: 24px; opacity: 0.7;", "Open a song from a setlist or a library." } };
    };
    rsx! { SongLoader { key: "{path}", path, title, props } }
}

/// One song's workspace, once its files are read. Keyed by the note's
/// path, so another song starts from nothing.
#[component]
fn SongLoader(path: String, title: String, props: Vec<(String, String)>) -> Element {
    let loaded = use_resource(move || {
        let (path, title, props) = (path.clone(), title.clone(), props.clone());
        async move { load(&path, &title, &props).await }
    });
    let ready = loaded.read().clone();
    ready.map_or_else(
        || rsx! { div { style: "padding: 24px; opacity: 0.7;", "Opening the song…" } },
        |loaded| rsx! { crate::workspace_view::SongWorkspace { loaded } },
    )
}

/// What a song's workspace needs from its note `path`: its chart's source
/// (its `.kf`, or the chart its page carries when the file cannot be read)
/// and its one lyrics source.
async fn load(path: &str, title: &str, props: &[(String, String)]) -> Loaded {
    let note = host::run("VAULT_READ", json!({ "path": path })).await.ok();
    let property = |key: &str| {
        note.as_ref()
            .and_then(|n| n.get("properties"))
            .and_then(|p| p.get(key))
            .and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Array(a) => a.first().and_then(Value::as_str).map(str::to_owned),
                _ => None,
            })
            .or_else(|| props.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()))
            .filter(|v| !v.trim().is_empty())
            .map(|v| frontmatter::link_target(&v).to_owned())
    };
    let body = note
        .as_ref()
        .and_then(|n| n.get("body"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let chart_file = property("chart");
    // The `.kf` when it reads; else the note's copy, read-only (it is never
    // written over the `.kf`: an old copy would overwrite a saved edit).
    let read = match &chart_file {
        Some(file) => Some(host::read_vault_file(file).await),
        None => None,
    };
    let chart = ChartAccess::open(chart_file, read, chart_fence(&body));
    let lrc = match property("lyrics").filter(|f| f.to_ascii_lowercase().ends_with(".lrc")) {
        Some(file) => host::read_vault_file(&file).await.ok(),
        None => None,
    };
    let lyrics = lyrics_source(lrc.as_deref(), Some(&chart.text));
    let recording = note
        .as_ref()
        .and_then(|n| n.get("properties"))
        .and_then(|p| p.get("media"))
        .and_then(Value::as_array)
        .and_then(|media| {
            media
                .iter()
                .filter_map(Value::as_str)
                .map(|l| frontmatter::link_target(l).to_owned())
                .find(|m| crate::song::is_playable(m))
        });
    // The share links of the setlists the song is in.
    let shared = match SongRef::from_link_target(path.trim_end_matches(".md")) {
        Some(at) => host::setlists()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|s| {
                s.songs
                    .iter()
                    .any(|x| x.library == at.library && x.title == at.title)
            })
            .filter_map(|s| s.share)
            .collect(),
        None => Vec::new(),
    };
    Loaded {
        title: property("title").unwrap_or_else(|| title.to_owned()),
        artist: property("artist"),
        chart,
        lyrics,
        recording,
        shared,
    }
}

/// Every setlist and its songs in order: the setlists' own tab. (A
/// process view is not told which pane it is in; the navigator picks one
/// setlist, this shows them all.)
#[allow(non_snake_case)]
fn SetlistView() -> Element {
    let data = use_resource(|| async {
        let sets = host::setlists().await.ok()?;
        let songs = host::songs().await.unwrap_or_default();
        Some((sets, songs))
    });
    let (sets, songs) = data.read().clone().flatten().unwrap_or_default();
    let count = sets.len();
    let empty = sets.is_empty();
    rsx! {
        div { "data-studio-setlists": "{count}",
            style: "height: 100%; overflow-y: scroll; padding: 20px 24px; box-sizing: border-box;",
            if empty {
                div { style: "opacity: 0.7;", "No setlists yet: STUDIO_SETLISTS_CREATE makes one." }
            }
            for set in sets {
                div { key: "{set.path}", style: "margin-bottom: 20px;",
                    h2 { style: "margin: 0 0 8px; font-size: 18px;", "{set.title}" }
                    for (i, at) in set.songs.iter().enumerate() {
                        div { key: "{i}", style: "display: flex; gap: 12px; padding: 4px 0; font-size: 13px;",
                            span { style: "width: 20px; opacity: 0.5;", "{i + 1}" }
                            span { style: "flex: 1;", "{at.label()}" }
                            span { style: "opacity: 0.6;",
                                {songs.iter().find(|s| s.at.library == at.library && s.at.title == at.title).map(SongRow::summary).unwrap_or_default()}
                            }
                            span { style: "opacity: 0.45;", "{at.library}" }
                        }
                    }
                }
            }
        }
    }
}

/// The "Song" screenset every song starts in: its page, the navigator in
/// the left dock.
const SONG_LAYOUT: &str = r#"{"windows":[{"id":"main",
  "center":{"group":{"tabs":[{"pane":"studio-song","view":"studio-setlists/song","title":"Song"}]}},
  "docks":[{"side":"left","root":{"group":{"tabs":[{"pane":"files","view":"files","title":"Files"}]}}}]}],
  "active":"studio-song"}"#;
