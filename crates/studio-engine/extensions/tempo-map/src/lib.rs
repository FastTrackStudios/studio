//! `studio.tempo-map`: tempo mapping as an extension.
//!
//! The algorithms are the `tempo-map` crate; this is their Action List in
//! Studio: analyse a song from its stems, list the candidates for bar 1
//! and set it, fix by hand, read the map, apply it to the session (tempo
//! points, time signatures and bar 1, through the engine's `TEMPO_MAP_SET`),
//! read the kit's hits and write them as track markers, evaluate a map,
//! compare two, and run the benchmark.
//!
//! **A process, on purpose.** The point of making this an extension is to
//! change an algorithm without rebuilding Studio: a process extension is
//! its own program (this crate and `tempo-map`, nothing of the app), the
//! host watches its package and restarts it when the program changes, and
//! an analysis is a file, so it outlives the swap.
//!
//! **Any song of a set.** Every action names its song (`song`: its number
//! in the set or its title; `name`: the analysis kept under that name)
//! and the engine is asked to act on that song without going to it
//! (`fts.session`'s `run`, the `daw` extension's `DAW_RUN`), so a script
//! maps a whole set while the window stays where it is.

#![allow(
    clippy::useless_let_if_seq,
    clippy::option_if_let_else,
    clippy::significant_drop_tightening,
    reason = "each action's body reads top to bottom as the steps it takes"
)]

pub mod ops;
pub mod store;

use std::sync::{Arc, OnceLock};

use architect_extensions::linked::{Builder, Extension, Handle};
use serde_json::{Value as Json, json};

use crate::store::Store;

pub const MANIFEST: &str = include_str!("../extension.toml");

/// Which build of the algorithms this process is: said by
/// `STUDIO_TEMPO_MAP_STATE`, so a swapped program can be told from the
/// one before it.
pub const BUILD: &str = "tempo-map 2: scripts ported; edges kept, miscounted spans repaired; the count-in lands the grid on bar 1";

fn store() -> Arc<Store> {
    static STORE: OnceLock<Arc<Store>> = OnceLock::new();
    STORE
        .get_or_init(|| Arc::new(Store::at(store::default_dir())))
        .clone()
}

/// Runs `work` off the runtime's threads: an analysis reads a song's
/// stems and takes seconds.
async fn heavy<T: Send + 'static>(
    work: impl FnOnce(&Store) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let store = store();
    tokio::task::spawn_blocking(move || work(&store))
        .await
        .map_err(|e| format!("the work stopped: {e}"))?
}

/// Asks Studio's engine (`fts.session`, the `daw` extension).
async fn engine(handle: &Handle, method: &str, args: Json) -> Result<Json, String> {
    handle
        .service("fts.session", method, args)
        .await
        .map_err(|e| format!("Studio's engine ({method}): {e}"))
}

/// Runs action `id` of the engine's list on `song` (the song it is on
/// when none).
async fn run_in(
    handle: &Handle,
    id: &str,
    args: Json,
    song: Option<&Json>,
) -> Result<Json, String> {
    let mut call = json!({ "id": id, "args": args });
    if let (Some(song), Some(map)) = (song, call.as_object_mut()) {
        map.insert("song".to_owned(), song.clone());
    }
    engine(handle, "run", call).await
}

/// The title of the set's song `song` names (its number from 1, or its
/// title), or of the song the engine is on.
async fn title(handle: &Handle, song: Option<&Json>) -> Result<String, String> {
    let set = engine(handle, "songs", Json::Null).await?;
    let songs = set
        .get("songs")
        .and_then(Json::as_array)
        .ok_or("no set is open in Studio's engine: name the analysis (`name`) or its `stems`")?;
    let named = |s: &Json| s.get("title").and_then(Json::as_str).map(str::to_owned);
    let found = match song {
        Some(Json::Number(n)) => songs
            .iter()
            .find(|s| s.get("number").and_then(Json::as_u64) == n.as_u64()),
        Some(Json::String(text)) => songs.iter().find(|s| {
            named(s).is_some_and(|t| t.eq_ignore_ascii_case(text.trim()))
                || s.get("project").and_then(Json::as_str) == Some(text)
        }),
        _ => songs.iter().find(|s| {
            s.get("number").and_then(Json::as_u64) == set.get("at").and_then(Json::as_u64)
        }),
    };
    found
        .and_then(named)
        .ok_or_else(|| format!("no such song in the set: {}", song.unwrap_or(&Json::Null)))
}

/// The song an action is about: the engine's target (`song`, when given)
/// and the name its analysis is kept under: `name`, else the song's
/// title, else the stems folder's name, else the song the engine is on.
async fn song_of(handle: &Handle, args: &Json) -> Result<(String, Option<Json>), String> {
    let song = args.get("song").filter(|s| !s.is_null()).cloned();
    if let Some(name) = args.get("name").and_then(Json::as_str) {
        return Ok((store::key(name), song));
    }
    if let Some(Json::String(text)) = &song {
        // A title names the analysis even with no engine to ask.
        let known = title(handle, song.as_ref())
            .await
            .unwrap_or_else(|_| text.clone());
        return Ok((store::key(&known), song));
    }
    if song.is_some() {
        return Ok((store::key(&title(handle, song.as_ref()).await?), song));
    }
    if let Some(folder) = args.get("stems").and_then(Json::as_str) {
        let name = std::path::Path::new(folder)
            .file_name()
            .map_or_else(|| folder.to_owned(), |n| n.to_string_lossy().into_owned());
        return Ok((store::key(&name), None));
    }
    Ok((store::key(&title(handle, None).await?), None))
}

/// `STUDIO_TEMPO_MAP_APPLY`: the map into the song's session.
async fn apply(handle: &Handle, args: &Json) -> Result<Json, String> {
    let (name, song) = song_of(handle, args).await?;
    let offset = args.get("offset").and_then(Json::as_f64).unwrap_or(0.0);
    let (points, bar1) = {
        let name = name.clone();
        heavy(move |store| ops::points_for_engine(store, &name, offset)).await?
    };
    let count = points.as_array().map_or(0, Vec::len);
    let follow = args.get("follow").and_then(Json::as_bool).unwrap_or(false);
    let mut set = json!({ "points": points, "follow": follow });
    // Bar 1 is the bar SONGSTART falls in: the engine moves it there.
    let songstart = args.get("songstart").and_then(Json::as_bool) != Some(false);
    if let (true, Some(map)) = (songstart, set.as_object_mut()) {
        map.insert("bar_one".to_owned(), json!(bar1));
    }
    let said = run_in(handle, "TEMPO_MAP_SET", set, song.as_ref()).await?;
    Ok(json!({
        "song": name,
        "points": count,
        "bar1_at": bar1,
        "engine": said,
    }))
}

/// `STUDIO_TEMPO_MAP_HITS`: read, and with `write` put on the tracks.
async fn hits(handle: &Handle, args: &Json) -> Result<Json, String> {
    let (name, song) = song_of(handle, args).await?;
    let (mut answer, tracks) = {
        let (name, args) = (name.clone(), args.clone());
        heavy(move |store| ops::hits(store, &name, &args)).await?
    };
    if args.get("write").and_then(Json::as_bool) != Some(true) {
        return Ok(answer);
    }
    let mut written = serde_json::Map::new();
    for piece in tracks {
        let markers: Vec<Json> = piece
            .at
            .iter()
            .map(|at| json!({ "at": at, "name": "hit" }))
            .collect();
        let put = run_in(
            handle,
            "TRACK_MARKERS_SET",
            json!({ "track": piece.track, "markers": markers, "replace": "hit" }),
            song.as_ref(),
        )
        .await;
        written.insert(
            piece.piece,
            match put {
                Ok(said) => json!({ "track": piece.track, "hits": piece.at.len(), "engine": said }),
                Err(e) => json!({ "track": piece.track, "refused": e }),
            },
        );
    }
    if let Some(map) = answer.as_object_mut() {
        // What was written is the answer: the hits themselves are a read
        // away.
        map.remove("hits");
        map.insert("written".to_owned(), Json::Object(written));
    }
    Ok(answer)
}

pub struct TempoMap;

impl Extension for TempoMap {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        let handle = ext.handle();
        // An action about one song: the song resolved, then `$op` off the
        // runtime's threads.
        macro_rules! about_a_song {
            ($id:literal, $op:expr) => {{
                let handle = handle.clone();
                ext.action($id, move |args| {
                    let handle = handle.clone();
                    async move {
                        let (name, _) = song_of(&handle, &args).await?;
                        heavy(move |store| $op(store, &name, &args)).await
                    }
                });
            }};
        }
        ext.action("STUDIO_TEMPO_MAP_STATE", |_| async {
            heavy(|store| Ok(ops::state(store, BUILD))).await
        });
        about_a_song!("STUDIO_TEMPO_MAP_ANALYSE", ops::analyse);
        about_a_song!(
            "STUDIO_TEMPO_MAP_CANDIDATES",
            |store: &Store, name: &str, _: &Json| { ops::candidates(store, name) }
        );
        about_a_song!("STUDIO_TEMPO_MAP_SET_BAR1", ops::set_bar1);
        about_a_song!("STUDIO_TEMPO_MAP_EDIT", ops::edit);
        about_a_song!(
            "STUDIO_TEMPO_MAP_MAP",
            |store: &Store, name: &str, _: &Json| { ops::map(store, name) }
        );
        about_a_song!("STUDIO_TEMPO_MAP_EVALUATE", ops::evaluate);
        ext.action("STUDIO_TEMPO_MAP_COMPARE", |args| async move {
            heavy(move |store| ops::compare(store, &args)).await
        });
        ext.action("STUDIO_TEMPO_MAP_BENCH", |args| async move {
            heavy(move |_| ops::benchmark(&args)).await
        });
        {
            let handle = handle.clone();
            ext.action("STUDIO_TEMPO_MAP_APPLY", move |args| {
                let handle = handle.clone();
                async move { apply(&handle, &args).await }
            });
        }
        ext.action("STUDIO_TEMPO_MAP_HITS", move |args| {
            let handle = handle.clone();
            async move { hits(&handle, &args).await }
        });
    }
}
