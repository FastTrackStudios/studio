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
                edit(&a, move |s| s.move_song(&song, to)).await
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

fn setlist_json(s: &SetlistRow, songs: &[SongRow]) -> Value {
    json!({
        "title": s.title,
        "path": s.path,
        "date": s.date,
        "share": s.share,
        "songs": s.songs.iter().map(|at| {
            let known = songs.iter().find(|r| r.at.library == at.library && r.at.title == at.title);
            json!({
                "song": at.label(),
                "library": at.library,
                "summary": known.map(SongRow::summary),
            })
        }).collect::<Vec<_>>(),
    })
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

fn links(songs: &[SongRef]) -> Value {
    json!(
        songs
            .iter()
            .map(|s| frontmatter::wikilink(&s.link_target(), &s.label()))
            .collect::<Vec<_>>()
    )
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
    let mut props = serde_json::Map::new();
    props.insert("type".to_owned(), json!(crate::setlist::KIND));
    props.insert("title".to_owned(), json!(title));
    props.insert("songs".to_owned(), links(&songs));
    if let Some(date) = opt(&a, "date") {
        props.insert("date".to_owned(), json!(date));
    }
    let path = setlist_note(&title);
    host::run(
        "VAULT_CREATE",
        json!({ "path": path, "properties": Value::Object(props).to_string(), "body": format!("# {title}\n") }),
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
/// `songs` property.
async fn edit(
    args: &Value,
    f: impl FnOnce(&mut Setlist) -> crate::Result<()>,
) -> Result<Value, String> {
    let row = find(args).await?;
    let mut set = Setlist::new(&row.title, row.songs.clone());
    f(&mut set).map_err(|e| e.to_string())?;
    host::run(
        "VAULT_SET_PROPERTY",
        json!({ "path": row.path, "key": "songs", "value": links(&set.songs).to_string() }),
    )
    .await?;
    let songs = host::songs().await.unwrap_or_default();
    Ok(setlist_json(
        &SetlistRow {
            songs: set.songs,
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
    edit(&a, move |s| s.add(at, position)).await
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

/// A song's page: its note, read through the app.
#[allow(non_snake_case)]
fn SongView() -> Element {
    let song = deck_song();
    let path = song.as_ref().map(|(p, _, _)| p.clone());
    let note = use_resource(move || {
        let path = path.clone();
        async move {
            let path = path?;
            host::run("VAULT_READ", json!({ "path": path }))
                .await
                .ok()
                .and_then(|v| {
                    v.get("body")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| v.as_str().map(str::to_owned))
                })
        }
    });
    let Some((path, title, props)) = song else {
        return rsx! { div { style: "padding: 24px; opacity: 0.7;", "Open a song from a setlist or a library." } };
    };
    let body = note.read().clone().flatten().unwrap_or_default();
    let (_, body) = frontmatter::split(&body);
    let body = body.to_owned();
    rsx! {
        div { "data-studio-song": "{path}",
            style: "height: 100%; overflow-y: scroll; padding: 20px 24px; box-sizing: border-box;",
            h2 { style: "margin: 0 0 8px; font-size: 20px;", "{title}" }
            div { style: "display: flex; gap: 16px; margin-bottom: 16px; font-size: 13px; opacity: 0.75;",
                for (k, v) in props {
                    span { key: "{k}", "{k}: {v}" }
                }
            }
            pre { style: "margin: 0; white-space: pre-wrap; font-size: 13px; line-height: 1.5;", "{body}" }
        }
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
