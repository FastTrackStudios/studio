//! Studio's Action List: everything an agent (or a script, or the command
//! line) can do in the running app, by id with JSON in and out, and the
//! doors it is served through (native builds):
//!
//! - **MCP** on `127.0.0.1` (`architect-mcp`'s Streamable HTTP), every
//!   request carrying a bearer token made fresh each launch. Where it is
//!   and the token are in `<config>/fts/studio-mcp.json` (mode `0600`),
//!   Architect's discovery file, so a client finds the running app.
//! - **The control socket** (`architect-control`), `<runtime dir>/fts/
//!   studio.sock`, this user only: the `actions` command line's door.
//! - **Luau** (`architect-scripting`): `SCRIPT_RUN` runs a script over
//!   every action as one step; a saved script is an action of its own.
//!
//! An action's handler runs wherever its caller is (an MCP thread, the
//! socket's thread, a script); the app's state is the UI thread's
//! (signals, documents). So each `STUDIO_*` action checks its arguments,
//! hands the call to [`ActionHost`] — mounted in the shell, inside the
//! router — and awaits the answer it sends back.

pub use imp::*;

#[cfg(not(feature = "native"))]
mod imp {
    use dioxus::prelude::*;

    /// Nothing to host without the native doors.
    #[component]
    pub fn ActionHost() -> Element {
        rsx! {}
    }
}

#[cfg(feature = "native")]
mod imp {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::str::FromStr as _;
    use std::sync::{Arc, Mutex, OnceLock};

    use architect::action::{ActionError, ActionFuture, ActionList, ActionMeta, Effect};
    use dioxus::prelude::*;
    use facet::Facet;
    use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
    use futures_channel::oneshot;
    use futures_util::StreamExt as _;
    use serde_json::{Value, json};

    use crate::Route;
    use crate::state::use_app;
    use crate::{orgs, setlist, songs};

    /// The name the app's doors go by (`studio-mcp.json`, `studio.sock`).
    pub const NAME: &str = "studio";

    /// One call, on its way to the UI thread.
    struct Call {
        id: &'static str,
        args: Vec<u8>,
        answer: oneshot::Sender<Result<Value, String>>,
    }

    struct Calls {
        send: UnboundedSender<Call>,
        take: Mutex<Option<UnboundedReceiver<Call>>>,
    }

    static LIST: OnceLock<ActionList> = OnceLock::new();
    static CALLS: OnceLock<Calls> = OnceLock::new();

    fn calls() -> &'static Calls {
        CALLS.get_or_init(|| {
            let (send, take) = unbounded();
            Calls {
                send,
                take: Mutex::new(Some(take)),
            }
        })
    }

    // ── the actions ─────────────────────────────────────────────────────

    macro_rules! meta {
        ($name:ident, $id:literal, $display:literal, $category:literal, $desc:literal) => {
            static $name: ActionMeta = ActionMeta {
                id: $id,
                trait_name: "",
                method_name: "",
                display_name: $display,
                description: $desc,
                category: $category,
                group: "Studio",
                shortcut: "",
                toggleable: false,
                undo: false,
            };
        };
    }

    meta!(
        NAVIGATE,
        "STUDIO_NAVIGATE",
        "Go to a page",
        "Navigation",
        "Opens the page at `path` (a route of the app: `/`, `/setlists`, `/set/<id>`, `/library`, `/now/chart`, `/drive/<space>/<location>/<path…>`). Answers the page now showing."
    );
    meta!(
        ROUTE,
        "STUDIO_ROUTE",
        "The page showing",
        "Navigation",
        "The route of the page the app shows now."
    );
    meta!(
        ORGS,
        "STUDIO_ORGS",
        "Orgs",
        "Orgs",
        "The org Studio is in (`here`) and every org this person is in."
    );
    meta!(
        ORG_SWITCH,
        "STUDIO_ORG_SWITCH",
        "Switch org",
        "Orgs",
        "Puts Studio in org `org` (one of STUDIO_ORGS' `orgs`): its sets, library and team."
    );
    meta!(
        SETLIST_FILES,
        "STUDIO_SETLIST_FILES",
        "Setlist files in Drive",
        "Setlists",
        "The `.setlist` files Drive has in the orgs this person is in, each with the `space`, `location` and `path` STUDIO_SETLIST_FILE_OPEN takes. Empty until the app reaches Drive."
    );
    meta!(
        SETLIST_FILE_OPEN,
        "STUDIO_SETLIST_FILE_OPEN",
        "Open a setlist file",
        "Setlists",
        "Opens the setlist file at `path` in `location` of `space` (from STUDIO_SETLIST_FILES) as a set page."
    );
    meta!(
        NOW_PLAYING,
        "STUDIO_NOW_PLAYING",
        "Now Playing",
        "Playback",
        "What plays: the list's titles and where it is from, the song (`i`, from 0) and whether it plays, its position and length in seconds, and the section playing."
    );
    meta!(
        PLAYBACK,
        "STUDIO_PLAYBACK",
        "Play or pause",
        "Playback",
        "Plays (`playing`: true) or pauses (false) what Now Playing holds; flips it when `playing` is left out."
    );
    meta!(
        NEXT,
        "STUDIO_NEXT",
        "Next song",
        "Playback",
        "Goes to the next song of what plays, from its start. Answers false at the end of the list."
    );
    meta!(
        PREVIOUS,
        "STUDIO_PREVIOUS",
        "Previous song",
        "Playback",
        "Goes to the song before, from its start; at the first song, back to its start."
    );
    meta!(
        SEEK,
        "STUDIO_SEEK",
        "Jump in the song",
        "Playback",
        "Moves the playing song to `seconds` from its start."
    );
    meta!(
        SETS,
        "STUDIO_SETS",
        "Sets",
        "Setlists",
        "The sets in the org's index: each one's `id`, title, kind, date, song count and first songs, and the org's recurring events."
    );
    meta!(
        SET_CREATE,
        "STUDIO_SET_CREATE",
        "New set",
        "Setlists",
        "Makes a set in the org: `title`, `date` (YYYY-MM-DD), `kind` (service, rehearsal, show, template) and the recurring event `series` it meets for. Answers its `id`."
    );
    meta!(
        SET_SONGS,
        "STUDIO_SET_SONGS",
        "A set's running order",
        "Setlists",
        "Set `id`'s running order: each entry's `entry` id, kind, title, artist, key (the set's over the song's) and tempo."
    );
    meta!(
        SET_SONG_ADD,
        "STUDIO_SET_SONG_ADD",
        "Add a song to a set",
        "Setlists",
        "Adds the song titled `title` to set `id` (the library's song of that title when there is one), at position `at` (from 0; the end when left out), in `key` when given. Answers its `entry` id."
    );
    meta!(
        SET_SONG_REMOVE,
        "STUDIO_SET_SONG_REMOVE",
        "Remove a song from a set",
        "Setlists",
        "Takes entry `entry` out of set `id`."
    );
    meta!(
        SET_DELETE,
        "STUDIO_SET_DELETE",
        "Delete a set",
        "Setlists",
        "Takes set `id` out of the org's list of sets. Its document is kept, and a setlist file it was written to stays on disk as a plain file (its path is given back)."
    );
    meta!(
        SET_SONG_MOVE,
        "STUDIO_SET_SONG_MOVE",
        "Move a song in a set",
        "Setlists",
        "Moves entry `entry` of set `id` by `by` places (negative: earlier)."
    );
    meta!(
        SET_SONG_KEY,
        "STUDIO_SET_SONG_KEY",
        "Set a song's key",
        "Setlists",
        "Plays entry `entry` of set `id` in `key` (`E`, `F#m`, …); an empty key goes back to the song's own."
    );
    meta!(
        SET_PLAY,
        "STUDIO_SET_PLAY",
        "Play a set",
        "Playback",
        "Plays set `id`'s songs in order from song `from` (from 0; the first when left out)."
    );

    #[derive(Facet)]
    struct NavigateArgs {
        path: String,
    }
    #[derive(Facet)]
    struct OrgArgs {
        org: String,
    }
    #[derive(Facet)]
    struct FileArgs {
        space: String,
        location: String,
        path: String,
    }
    #[derive(Facet)]
    struct PlaybackArgs {
        #[facet(default)]
        playing: Option<bool>,
    }
    #[derive(Facet)]
    struct SeekArgs {
        seconds: f64,
    }
    #[derive(Facet)]
    struct CreateArgs {
        title: String,
        #[facet(default)]
        date: Option<String>,
        #[facet(default)]
        kind: Option<String>,
        #[facet(default)]
        series: Option<String>,
    }
    #[derive(Facet)]
    struct SetArgs {
        id: String,
    }
    #[derive(Facet)]
    struct AddArgs {
        id: String,
        title: String,
        #[facet(default)]
        at: Option<usize>,
        #[facet(default)]
        key: Option<String>,
    }
    #[derive(Facet)]
    struct EntryArgs {
        id: String,
        entry: String,
    }
    #[derive(Facet)]
    struct MoveArgs {
        id: String,
        entry: String,
        by: i32,
    }
    #[derive(Facet)]
    struct KeyArgs {
        id: String,
        entry: String,
        key: String,
    }
    #[derive(Facet)]
    struct PlaySetArgs {
        id: String,
        #[facet(default)]
        from: Option<usize>,
    }

    /// Put an action on the list whose work is the UI thread's: its
    /// arguments checked against `A` here, the call answered there.
    fn forward<A: Facet<'static>>(
        list: &ActionList,
        meta: &'static ActionMeta,
        effect: Effect,
        takes: bool,
    ) {
        let id = meta.id;
        let handler = Arc::new(move |args: &[u8]| -> Result<ActionFuture, ActionError> {
            let args = if args.is_empty() {
                b"{}".as_slice()
            } else {
                args
            };
            if takes {
                facet_json::from_slice::<A>(args).map_err(|e| ActionError::BadArgs {
                    id: id.to_owned(),
                    message: e.to_string(),
                    expected: None,
                })?;
            }
            let (answer, answered) = oneshot::channel();
            calls()
                .send
                .unbounded_send(Call {
                    id,
                    args: args.to_vec(),
                    answer,
                })
                .map_err(|_| failed(id, "the app's window is gone"))?;
            Ok(Box::pin(async move {
                match answered.await {
                    Ok(Ok(v)) => serde_json::to_vec(&v).map_err(|e| failed(id, &e.to_string())),
                    Ok(Err(e)) => Err(failed(id, &e)),
                    Err(_) => Err(failed(id, "the app's window did not answer")),
                }
            }) as ActionFuture)
        });
        let params = takes.then_some(A::SHAPE);
        if let Err(e) = list.register_dynamic_async(meta, effect, params, None, handler) {
            tracing::warn!(action.id = id, error = %e, "an action could not be registered");
        }
    }

    fn failed(id: &str, message: &str) -> ActionError {
        ActionError::Failed {
            id: id.to_owned(),
            message: message.to_owned(),
        }
    }

    /// The app's Action List: Studio's actions and the scripts' (made once).
    pub fn list() -> ActionList {
        LIST.get_or_init(|| {
            let list = ActionList::new();
            use Effect::{Mutating as M, ReadOnly as R};
            forward::<NavigateArgs>(&list, &NAVIGATE, M, true);
            forward::<()>(&list, &ROUTE, R, false);
            forward::<()>(&list, &ORGS, R, false);
            forward::<OrgArgs>(&list, &ORG_SWITCH, M, true);
            forward::<()>(&list, &SETLIST_FILES, R, false);
            forward::<FileArgs>(&list, &SETLIST_FILE_OPEN, M, true);
            forward::<()>(&list, &NOW_PLAYING, R, false);
            forward::<PlaybackArgs>(&list, &PLAYBACK, M, true);
            forward::<()>(&list, &NEXT, M, false);
            forward::<()>(&list, &PREVIOUS, M, false);
            forward::<SeekArgs>(&list, &SEEK, M, true);
            forward::<()>(&list, &SETS, R, false);
            forward::<CreateArgs>(&list, &SET_CREATE, M, true);
            forward::<SetArgs>(&list, &SET_SONGS, R, true);
            forward::<AddArgs>(&list, &SET_SONG_ADD, M, true);
            forward::<EntryArgs>(&list, &SET_SONG_REMOVE, Effect::Destructive, true);
            forward::<SetArgs>(&list, &SET_DELETE, Effect::Destructive, true);
            forward::<MoveArgs>(&list, &SET_SONG_MOVE, M, true);
            forward::<KeyArgs>(&list, &SET_SONG_KEY, M, true);
            forward::<PlaySetArgs>(&list, &SET_PLAY, M, true);
            let scripting = architect_scripting::actions::Scripting::new(
                list.clone(),
                architect_scripting::Library::new(architect_scripting::MemoryStore::default()),
            );
            if let Err(e) = scripting.install() {
                tracing::warn!(error = %e, "scripts could not be put on the list");
            }
            list
        })
        .clone()
    }

    // ── the doors ───────────────────────────────────────────────────────

    /// What keeps the doors open for the app's life.
    struct Doors {
        _control: Option<architect_control::server::Serving>,
    }

    static DOORS: Mutex<Option<Doors>> = Mutex::new(None);

    /// A fresh token: 32 random bytes, hex.
    fn token() -> std::io::Result<String> {
        use std::io::Read as _;
        let mut bytes = [0u8; 32];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
    }

    /// Open MCP (loopback, a token, published) and the control socket.
    /// On the app's Tokio runtime (app-shell-native enters one).
    fn open_doors(list: ActionList) {
        let version = env!("CARGO_PKG_VERSION");
        let control = architect_control::server::Control::new(
            NAME,
            architect_control::Surface::new(NAME, version, list.clone()),
        )
        .and_then(architect_control::server::Control::serve);
        let control = match control {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!(error = %e, "no control socket");
                None
            }
        };
        if let Ok(mut d) = DOORS.lock() {
            *d = Some(Doors { _control: control });
        }
        if architect_mcp::discovery::disabled() {
            return;
        }
        let token = match token() {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "no token, so no MCP server");
                return;
            }
        };
        let server = architect_mcp::Server::new(NAME, version, list).with_instructions(
            "This is Studio, the app the person has open: their orgs, setlists, the songs of \
             each set, and what plays. Every STUDIO_* action is what the app's own pages do. \
             `actions` lists them, `describe` says what one takes; SCRIPT_RUN runs a Luau \
             script over every action as one step (SCRIPT_DESCRIBE has its API).",
        );
        tokio::spawn(async move {
            let listener =
                match tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::warn!(error = %e, "MCP could not bind");
                        return;
                    }
                };
            let url = match listener.local_addr() {
                Ok(a) => format!("http://{a}/mcp"),
                Err(_) => return,
            };
            match architect_mcp::publish(NAME, &url, Some(&token)) {
                Ok(path) => tracing::info!(mcp.url = %url, mcp.discovery = %path.display(), "MCP"),
                Err(e) => tracing::warn!(error = %e, "MCP's discovery file could not be written"),
            }
            if let Err(e) = architect_mcp::http::serve(listener, server, Some(token)).await {
                tracing::warn!(error = %e, "MCP stopped");
            }
        });
    }

    // ── the UI side ─────────────────────────────────────────────────────

    /// Answers the actions' calls with the app's state; opens the doors.
    /// Mounted once, in the shell (inside the router: it navigates).
    #[component]
    pub fn ActionHost() -> Element {
        let app = use_app();
        let route = use_route::<Route>();
        let index = crate::sets::use_index();
        let library = crate::songs::use_library();
        // What the calls read: the latest of each, kept past this render.
        let now = use_hook(|| {
            Rc::new(RefCell::new(
                None::<(String, crate::sets::Index, crate::songs::Library)>,
            ))
        });
        *now.borrow_mut() = Some((route.to_string(), index, library));
        use_hook(|| open_doors(list()));
        let held = now.clone();
        use_future(move || {
            let held = held.clone();
            async move {
                let Some(mut calls) = calls().take.lock().ok().and_then(|mut t| t.take()) else {
                    return;
                };
                while let Some(call) = calls.next().await {
                    let Some((route, index, library)) = held.borrow().clone() else {
                        let _ = call.answer.send(Err("the app is starting".into()));
                        continue;
                    };
                    let answer = answer(app, &route, &index, &library, call.id, &call.args);
                    let _ = call.answer.send(answer);
                }
            }
        });
        rsx! {}
    }

    fn args<T: Facet<'static>>(bytes: &[u8]) -> Result<T, String> {
        facet_json::from_slice::<T>(bytes).map_err(|e| e.to_string())
    }

    /// A set's document in the org, brought to its version (as a set's
    /// page does when it opens it).
    fn set_doc(org: &str, id: &str) -> loro::LoroDoc {
        songs::backing::create(&orgs::qualified(org, &setlist::setlist_doc(id)), |d| {
            if drive_spine::schema::version(d) < setlist::SETLIST_STEPS.len() as i64 {
                setlist::stamp_set(d, org, id);
                drive_spine::schema::upgrade(d, setlist::SETLIST_STEPS);
            }
        })
    }

    /// After an edit: its face in the index, so the pages list it so.
    fn edited(doc: &loro::LoroDoc, index: &crate::sets::Index) {
        doc.commit();
        index.put_face(&setlist::face_of(doc));
    }

    fn entry_json(e: &setlist::Entry) -> Value {
        json!({
            "entry": e.id,
            "kind": format!("{:?}", e.kind).to_lowercase(),
            "title": e.title,
            "artist": e.artist,
            "key": e.over.key.clone().or_else(|| e.key.clone()),
            "tempo": e.over.tempo.or(e.tempo),
        })
    }

    fn answer(
        app: crate::state::AppState,
        route: &str,
        index: &crate::sets::Index,
        library: &crate::songs::Library,
        id: &str,
        raw: &[u8],
    ) -> Result<Value, String> {
        let here = (app.here)();
        let mut np = app.np;
        match id {
            "STUDIO_NAVIGATE" => {
                let a: NavigateArgs = args(raw)?;
                let to =
                    Route::from_str(&a.path).map_err(|e| format!("no page at {}: {e}", a.path))?;
                navigator().push(to.clone());
                Ok(json!({ "route": to.to_string() }))
            }
            "STUDIO_ROUTE" => Ok(json!({ "route": route })),
            "STUDIO_ORGS" => Ok(json!({ "here": here, "orgs": (app.orgs)() })),
            "STUDIO_ORG_SWITCH" => {
                let a: OrgArgs = args(raw)?;
                if !(app.orgs)().contains(&a.org) {
                    return Err(format!("not in org {} (STUDIO_ORGS lists them)", a.org));
                }
                app.switch_org(&a.org);
                Ok(json!({ "here": a.org }))
            }
            "STUDIO_SETLIST_FILES" => Ok(Value::Array(
                (app.drive_sets)()
                    .unwrap_or_default()
                    .iter()
                    .map(|h| {
                        json!({
                            "name": crate::setfile::name_of(&h.name),
                            "space": h.space, "location": h.location, "path": h.path,
                            "location_name": h.location_name, "device_name": h.device_name,
                        })
                    })
                    .collect(),
            )),
            "STUDIO_SETLIST_FILE_OPEN" => {
                let a: FileArgs = args(raw)?;
                let to = Route::DriveSet {
                    space: a.space,
                    location: a.location,
                    path: a.path.split('/').map(str::to_string).collect(),
                };
                navigator().push(to.clone());
                Ok(json!({ "route": to.to_string() }))
            }
            "STUDIO_NOW_PLAYING" => Ok(match np() {
                Some(n) => {
                    let (at, len) = n.secs();
                    json!({
                        "from": n.from,
                        "list": n.list.iter().map(|s| s.title).collect::<Vec<_>>(),
                        "i": n.i,
                        "title": n.song().map(|s| s.title),
                        "playing": n.playing,
                        "seconds": at,
                        "length": len,
                        "section": n.timeline.at(at).map(|s| s.name.clone()),
                    })
                }
                None => Value::Null,
            }),
            "STUDIO_PLAYBACK" => {
                let a: PlaybackArgs = args(raw)?;
                let mut now = false;
                np.with_mut(|n| {
                    if let Some(n) = n {
                        n.playing = a.playing.unwrap_or(!n.playing);
                        now = n.playing;
                    }
                });
                Ok(json!({ "playing": now }))
            }
            "STUDIO_NEXT" => {
                let mut moved = false;
                np.with_mut(|n| {
                    if let Some(n) = n {
                        moved = n.next();
                    }
                });
                Ok(json!(moved))
            }
            "STUDIO_PREVIOUS" => {
                np.with_mut(|n| {
                    if let Some(n) = n {
                        n.i = n.i.saturating_sub(1);
                        n.pos = 0.0;
                        n.dur = 0.0;
                        n.timeline = Default::default();
                    }
                });
                Ok(json!(true))
            }
            "STUDIO_SEEK" => {
                let a: SeekArgs = args(raw)?;
                let mut seek = app.seek;
                seek.set(Some(a.seconds.max(0.0)));
                np.with_mut(|n| {
                    if let Some(n) = n {
                        let (_, len) = n.secs();
                        if len > 0.0 {
                            n.pos = (a.seconds / len).clamp(0.0, 1.0);
                        }
                    }
                });
                Ok(json!({ "seconds": a.seconds }))
            }
            "STUDIO_SETS" => Ok(json!({
                "org": index.org,
                "sets": index.faces().iter().map(|f| json!({
                    "id": f.id, "title": f.title,
                    "kind": format!("{:?}", f.kind).to_lowercase(),
                    "date": f.date, "series": f.series, "songs": f.songs, "first": f.first,
                })).collect::<Vec<_>>(),
                "recurring": index.series().iter().map(|s| json!({
                    "id": s.id, "name": s.name, "every": s.every(), "room": s.room,
                    "starts": s.starts, "call": s.call,
                })).collect::<Vec<_>>(),
            })),
            "STUDIO_SET_CREATE" => {
                let a: CreateArgs = args(raw)?;
                let kind = match a.kind.as_deref().map(str::to_lowercase).as_deref() {
                    None | Some("service") => setlist::Kind::Service,
                    Some("rehearsal") => setlist::Kind::Rehearsal,
                    Some("show") => setlist::Kind::Show,
                    Some("template") => setlist::Kind::Template,
                    Some(k) => {
                        return Err(format!("no kind {k} (service, rehearsal, show, template)"));
                    }
                };
                let info = setlist::Info {
                    id: crate::sets::new_id(),
                    title: a.title,
                    kind,
                    date: a.date.unwrap_or_default(),
                    series: a.series.unwrap_or_default(),
                    ..Default::default()
                };
                let id = crate::sets::create(&index.org, &info, None, index);
                Ok(json!({ "id": id }))
            }
            "STUDIO_SET_SONGS" => {
                let a: SetArgs = args(raw)?;
                let doc = set_doc(&index.org, &a.id);
                let info = setlist::info(&doc);
                Ok(json!({
                    "id": a.id, "title": info.title,
                    "songs": setlist::entries(&doc).iter().map(entry_json).collect::<Vec<_>>(),
                }))
            }
            "STUDIO_SET_SONG_ADD" => {
                let a: AddArgs = args(raw)?;
                let doc = set_doc(&index.org, &a.id);
                let mut e = library
                    .entries()
                    .into_iter()
                    .find(|s| s.title.eq_ignore_ascii_case(&a.title))
                    .map(|s| setlist::Entry::of_song(&index.org, &s))
                    .unwrap_or_else(|| setlist::Entry {
                        kind: setlist::ItemKind::Song,
                        title: a.title.clone(),
                        ..Default::default()
                    });
                if let Some(k) = a.key.filter(|k| !k.is_empty()) {
                    e.over.key = Some(k);
                }
                let entry = setlist::insert_entry(&doc, a.at, &e).map_err(|e| e.to_string())?;
                edited(&doc, index);
                Ok(json!({ "entry": entry }))
            }
            "STUDIO_SET_SONG_REMOVE" => {
                let a: EntryArgs = args(raw)?;
                let doc = set_doc(&index.org, &a.id);
                setlist::entry(&doc, &a.entry)
                    .ok_or_else(|| format!("no entry {} in set {}", a.entry, a.id))?;
                setlist::remove_entry(&doc, &a.entry).map_err(|e| e.to_string())?;
                edited(&doc, index);
                Ok(json!({ "removed": a.entry }))
            }
            "STUDIO_SET_DELETE" => {
                let a: SetArgs = args(raw)?;
                let face = index
                    .face(&a.id)
                    .ok_or_else(|| format!("no set {} in {}", a.id, index.org))?;
                let when = format!("{:.0}", drive_dioxus::env::now_ms());
                index.remove(&a.id, &when);
                let file = crate::storage::setlists()
                    .and_then(|dir| crate::set_files::file_of(&dir, &a.id))
                    .map(|p| p.display().to_string());
                Ok(json!({ "deleted": a.id, "title": face.title, "file": file }))
            }
            "STUDIO_SET_SONG_MOVE" => {
                let a: MoveArgs = args(raw)?;
                let doc = set_doc(&index.org, &a.id);
                setlist::entry(&doc, &a.entry)
                    .ok_or_else(|| format!("no entry {} in set {}", a.entry, a.id))?;
                setlist::move_by(&doc, &a.entry, a.by).map_err(|e| e.to_string())?;
                edited(&doc, index);
                Ok(json!({
                    "order": setlist::entries(&doc).iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
                }))
            }
            "STUDIO_SET_SONG_KEY" => {
                let a: KeyArgs = args(raw)?;
                let doc = set_doc(&index.org, &a.id);
                let mut e = setlist::entry(&doc, &a.entry)
                    .ok_or_else(|| format!("no entry {} in set {}", a.entry, a.id))?;
                e.over.key = Some(a.key.clone()).filter(|k| !k.is_empty());
                setlist::update_entry(&doc, &e).map_err(|e| e.to_string())?;
                edited(&doc, index);
                Ok(entry_json(&e))
            }
            "STUDIO_SET_PLAY" => {
                let a: PlaySetArgs = args(raw)?;
                let doc = set_doc(&index.org, &a.id);
                let info = setlist::info(&doc);
                let titles: Vec<String> = setlist::songs_of(&doc)
                    .iter()
                    .map(|e| e.title.clone())
                    .collect();
                let first = titles
                    .get(a.from.unwrap_or(0))
                    .cloned()
                    .ok_or_else(|| format!("set {} has no song {}", a.id, a.from.unwrap_or(0)))?;
                app.play_titles(&titles, info.title, Some(&first));
                Ok(json!({ "playing": first, "of": titles.len() }))
            }
            other => Err(format!("{other} is not one of Studio's actions")),
        }
    }
}
