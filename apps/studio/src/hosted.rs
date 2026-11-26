//! The session's views are Studio's real extensions, linked into the app
//! and hosted by app-shell-extensions in the panes of the session's
//! Stage (`crate::stage`): `studio.arrangement`, `studio.mixer`,
//! `studio.navigator`'s progress bar, `studio.lyrics`, Keyflow's chart
//! (the live editor, its note-input toolbar on its pane) and Ignition's
//! room and cue list. Studio's own views are each a Studio window on the
//! engine (`studio_view_link`): the song read through the window
//! protocol's replicas, its edits and transport sent back, the same panels
//! the old Session window drew.
//!
//! The engine is the one [`crate::engine`] found or started and checked
//! (its pid owns its port): the views attach to that one
//! ([`use_engine_link`]), never to whichever engine answers first.

use std::sync::OnceLock;

use app_shell::ui::{TabCtx, TabKind, TabKinds};
use app_shell::{PaneId, Workspace};
use app_shell_extensions::ExtensionUi;
use architect_extensions::extensions::Extensions;
use architect_extensions::host::{Host, SharedSections};
use architect_extensions::install::Installs;
use architect_settings::Registry;
use dioxus::prelude::*;

use crate::stage::CHART;

/// The session's own pages (the overview, the views not yet an
/// extension), drawn by the app: the pane's state is the view's slug.
pub const PAGE: &str = "studio.page";

static EXTENSIONS: OnceLock<Extensions> = OnceLock::new();

thread_local! {
    /// The session's window while it is open, for what closes its Stage.
    static SESSION_UI: std::cell::Cell<Option<ExtensionUi>> = const { std::cell::Cell::new(None) };
}

/// The session's window is `ui` (or gone).
pub fn set_session_ui(ui: Option<ExtensionUi>) {
    SESSION_UI.with(|s| s.set(ui));
}

/// Opens the launcher in a window of its own (titled "Launcher") beside
/// the session, or brings it forward: whether a session is open.
pub fn open_launcher_window() -> bool {
    let Some(ui) = SESSION_UI.with(std::cell::Cell::get) else {
        return false;
    };
    let pane = PaneId::new(crate::stage::LAUNCHER);
    let _ = ui.apply(|w| {
        if w.pane(&pane).is_some() {
            return w.activate(&pane);
        }
        w.open(
            app_shell::Pane::new(crate::stage::LAUNCHER, crate::stage::LAUNCHER, "Launcher"),
            app_shell::Target::Window(Some(app_shell::Rect::new(80.0, 80.0, 1440.0, 900.0))),
        )
        .map(drop)
    });
    true
}

/// Closes the Stage's Lighting and Chart windows, when a session is open:
/// whether it was.
pub fn close_stage_windows() -> bool {
    let Some(ui) = SESSION_UI.with(std::cell::Cell::get) else {
        return false;
    };
    for id in [
        crate::stage::LIGHTING_WINDOW,
        crate::stage::MIXER_WINDOW,
        crate::stage::CHART_WINDOW,
    ] {
        let window = app_shell::WindowId::new(id);
        let _ = ui.apply(|w| w.close_window(&window));
    }
    // The launcher's window too: the main one is the launcher now.
    let launcher = PaneId::new(crate::stage::LAUNCHER);
    let _ = ui.apply(|w| w.close(&launcher));
    true
}

/// The app's extensions: the session's views, linked in, on the app's
/// Action List (their actions are the app's, on MCP and the socket).
pub fn extensions() -> &'static Extensions {
    EXTENSIONS.get_or_init(|| {
        // The views' windows run inside this app's process, on its thread:
        // an async window action is started, never waited for.
        engine_client::actions::in_host(true);
        // The views attach to the engine the app links (use_engine_link),
        // never to whichever engine answers first.
        studio_view_link::attach_by_host();
        let host = Host::new(crate::actions::list(), SharedSections::new(Registry::new()));
        // Views another program draws (Ignition's room), and process
        // extensions (the engine's `daw`), run by a runner of their own.
        let mut kinds = host.view_kinds();
        if let Err(e) = kinds.register(architect_views::KindSpec::process()) {
            tracing::warn!(error = %e, "studio: no process views");
        }
        let host = host.with_view_kinds(kinds);
        let host = match architect_extensions::process::ProcessRunner::new()
            // Its processes are the ones the panes draw views from.
            .map(|r| r.with_processes(app_shell_extensions::processes()))
        {
            Ok(runner) => host.with_runner(runner),
            Err(e) => {
                tracing::warn!(error = %e, "studio: no process extensions");
                host
            }
        };
        let installs = crate::storage::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("fts")
            .join(crate::instance::extensions_folder());
        let linked: [&dyn architect_extensions::linked::Extension; 6] = [
            &studio_arrangement::Arrangement,
            &studio_mixer::Mixer,
            &studio_lyrics::Lyrics,
            &studio_navigator::Navigator,
            &keyflow_extension::Keyflow,
            &ignition_pane::ext::Lighting,
        ];
        let dev_root = installs.join("dev");
        let mut ext = Extensions::new(host, Installs::new(installs)).with_dev_root(&dev_root);
        if let Some(target) = std::env::var_os(DEV_TARGET) {
            ext = ext.with_build_target(std::path::PathBuf::from(target));
        }
        for one in linked {
            ext = match ext.clone().with_linked(one) {
                Ok(next) => next,
                Err(e) => {
                    tracing::warn!(error = %e, "studio: an extension's manifest was refused");
                    ext
                }
            };
        }
        tell(&ext.sync());
        // The engine's own extension, `daw`, as the process it is: it
        // provides `fts.session` for the song the engine has (its one
        // file), which Keyflow's chart and Ignition's cues keep their data
        // in, and leads its transport.
        #[cfg(not(target_os = "ios"))]
        match daw_package() {
            Ok(dir) => {
                tell(&ext.dev(dir));
                ext.host().use_provider("fts.session", Some("daw"));
            }
            Err(e) => tracing::warn!(error = %e, "studio: the daw extension is not here"),
        }
        // On iPhone and iPad an app runs no second process: the extension
        // is linked in like the views, and its `build` starts the engine in
        // this process (`crate::engine`'s module docs).
        #[cfg(target_os = "ios")]
        match ext.clone().with_linked(&daw_extension::Daw) {
            Ok(next) => {
                ext = next;
                tell(&ext.sync());
                ext.host().use_provider("fts.session", Some("daw"));
            }
            Err(e) => tracing::warn!(error = %e, "studio: the daw extension's manifest was refused"),
        }
        // EXT_LIST, EXT_CLONE, EXT_RELOAD, EXT_LOGS, …: on MCP and the
        // socket like the app's own.
        if let Err(e) = ext.install_actions() {
            tracing::warn!(error = %e, "studio: no EXT_* actions");
        }
        // The overrides a person set (STUDIO_EXT_DEV), kept across starts,
        // and any the environment names for this run: in every build.
        let overrides = Overrides::at(&dev_root);
        for dir in overrides.folders().into_iter().chain(
            std::env::var_os(DEV_EXTENSIONS)
                .map(|v| std::env::split_paths(&v).collect::<Vec<_>>())
                .unwrap_or_default(),
        ) {
            if let Err(e) = override_with(&ext, &dir) {
                tracing::warn!(ext.dev = %dir.display(), error = %e, "studio: an override did not start");
            }
        }
        if let Err(e) = install_dev_actions(&ext, overrides) {
            tracing::warn!(error = %e, "studio: no STUDIO_EXT_DEV actions");
        }
        watch_dev_copies(ext.clone());
        ext
    })
}

/// Folders (`:`-separated) to override linked extensions with for this
/// run only, beside the ones kept by STUDIO_EXT_DEV.
pub const DEV_EXTENSIONS: &str = "STUDIO_DEV_EXTENSIONS";

/// Where overrides built from source build (`CARGO_TARGET_DIR`); the
/// override folder's `.target` unless given.
pub const DEV_TARGET: &str = "STUDIO_DEV_TARGET";

static DEV: architect::action::ActionMeta = dev_meta(
    "STUDIO_EXT_DEV",
    "Override an extension",
    "Runs the extension in `folder` in place of the copy linked into Studio, from now and at every start until STUDIO_EXT_DEV_END, in any build. A crate (its `extension.toml` and `Cargo.toml`, e.g. keyflow's features/extension) is built as a process with cargo, overrides the linked copy of its id once its first build is good, and is rebuilt and swapped in when its sources change (a failed build keeps the last good one; EXT_LOGS says why). A built package (its `extension.toml` and program) runs as it is and reloads when its files change.",
);
static DEV_END: architect::action::ActionMeta = dev_meta(
    "STUDIO_EXT_DEV_END",
    "Use the linked extension",
    "Ends the override in `folder`: the copy linked into Studio runs again, now and at the next start.",
);
static DEV_LIST: architect::action::ActionMeta = dev_meta(
    "STUDIO_EXT_DEV_LIST",
    "Extension overrides",
    "The folders overriding linked extensions (kept across starts), each with its extension's id and how its build is.",
);

const fn dev_meta(
    id: &'static str,
    name: &'static str,
    description: &'static str,
) -> architect::action::ActionMeta {
    architect::action::ActionMeta {
        id,
        trait_name: "",
        method_name: "",
        display_name: name,
        description,
        category: "Extensions",
        group: "Studio",
        shortcut: "",
        toggleable: false,
        undo: false,
    }
}

/// `STUDIO_EXT_DEV`'s and `STUDIO_EXT_DEV_END`'s arguments.
#[derive(Debug, facet::Facet)]
struct DevArgs {
    /// The extension's folder: its crate, or its built package.
    folder: String,
}

/// The overrides kept across starts: `overrides.json` in the override
/// folder, a list of folders.
#[derive(Clone)]
struct Overrides {
    file: std::path::PathBuf,
}

impl Overrides {
    fn at(dev_root: &std::path::Path) -> Self {
        Self {
            file: dev_root.join("overrides.json"),
        }
    }

    fn folders(&self) -> Vec<std::path::PathBuf> {
        std::fs::read(&self.file)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect()
    }

    fn set(&self, dir: &std::path::Path, on: bool) -> Result<(), String> {
        let mut all = self.folders();
        all.retain(|d| d != dir);
        if on {
            all.push(dir.to_path_buf());
        }
        let text: Vec<String> = all.iter().map(|d| d.display().to_string()).collect();
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(&text).map_err(|e| e.to_string())?;
        std::fs::write(&self.file, json).map_err(|e| e.to_string())
    }
}

fn install_dev_actions(
    ext: &Extensions,
    overrides: Overrides,
) -> Result<(), architect::action::ActionError> {
    use architect::action::{ActionCaller, Effect, RawJson};
    let list = ext.host().list().clone();
    let (me, kept) = (ext.clone(), overrides.clone());
    list.register_with(
        &DEV,
        Effect::Mutating,
        move |_: &ActionCaller, a: DevArgs| {
            let dir = std::path::PathBuf::from(&a.folder);
            let id = override_with(&me, &dir)?;
            kept.set(&dir, true)?;
            Ok::<_, String>(RawJson::from(
                serde_json::json!({ "folder": a.folder, "id": id, "build": me.build_of(&id) })
                    .to_string(),
            ))
        },
    )?;
    let (me, kept) = (ext.clone(), overrides.clone());
    list.register_with(
        &DEV_END,
        Effect::Mutating,
        move |_: &ActionCaller, a: DevArgs| {
            let dir = std::path::PathBuf::from(&a.folder);
            tell(&me.end_dev(&dir));
            kept.set(&dir, false)?;
            Ok::<_, String>(RawJson::from(
                serde_json::json!({ "folder": a.folder }).to_string(),
            ))
        },
    )?;
    let me = ext.clone();
    list.register_with(
        &DEV_LIST,
        Effect::ReadOnly,
        move |_: &ActionCaller, (): ()| {
            let rows: Vec<serde_json::Value> = overrides
            .folders()
            .into_iter()
            .map(|dir| {
                let id = architect_extensions::package::Package::read(&dir)
                    .map(|p| p.manifest.id)
                    .ok();
                let build = id.as_deref().map(|id| me.build_of(id));
                serde_json::json!({ "folder": dir.display().to_string(), "id": id, "build": build })
            })
            .collect();
            Ok::<_, String>(RawJson::from(
                serde_json::json!({ "overrides": rows }).to_string(),
            ))
        },
    )?;
    Ok(())
}

/// Overrides the linked copy of the extension in `dir` with it: built
/// from source when `dir` is a crate, run as it is when a built package.
/// Its id.
fn override_with(ext: &Extensions, dir: &std::path::Path) -> Result<String, String> {
    let id = architect_extensions::package::Package::read(dir)
        .map_err(|e| e.to_string())?
        .manifest
        .id;
    let events = if dir.join("Cargo.toml").is_file() {
        ext.dev_crate(dir)?
    } else {
        ext.dev(dir.to_path_buf())
    };
    tracing::info!(ext.id = %id, ext.dev = %dir.display(), "studio: an extension is overridden");
    tell(&events);
    Ok(id)
}

/// Every second: rebuilds the development copies whose sources changed,
/// swaps in what built, and reloads a dev package whose files changed.
fn watch_dev_copies(ext: Extensions) {
    let spawned = std::thread::Builder::new()
        .name("studio-ext-dev".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                tell(&ext.reload_changed());
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "studio: development copies are not watched");
    }
}

/// What loading the extensions did, a failure as one warning each.
fn tell(events: &[architect_extensions::host::Event]) {
    for event in events {
        match event {
            architect_extensions::host::Event::Failed { id, error } => {
                tracing::warn!(ext.id = %id, error = %error, "studio: an extension did not load");
            }
            architect_extensions::host::Event::Loaded(change) => {
                tracing::info!(ext.id = %change.id, "studio: extension loaded");
            }
            architect_extensions::host::Event::Unloaded { .. } => {}
        }
    }
}

/// The daw extension's package: its manifest and its program
/// (`daw-extension`, as the engine is found), in Studio's folder.
#[cfg(not(target_os = "ios"))]
fn daw_package() -> Result<std::path::PathBuf, String> {
    const MANIFEST: &str =
        include_str!("../../../crates/studio-engine/extensions/daw/extension.toml");
    let bin = crate::engine::daw_extension_bin().ok_or("no daw-extension program")?;
    let dir = crate::storage::config_dir()
        .ok_or("no config folder")?
        .join("fts")
        .join(crate::instance::extensions_folder())
        .join("dev")
        .join("daw");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("extension.toml"), MANIFEST).map_err(|e| e.to_string())?;
    let link = dir.join("daw-extension");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&bin, &link).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// The song's session as the engine's daw extension has it (`DAW_STATE`'s
/// `session`): its document's id and the address its transport is led
/// from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Followed {
    pub id: String,
    pub url: String,
    pub path: String,
}

async fn run(id: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
    let out = crate::actions::list()
        .run_json_async(id, args.to_string().as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&out).map_err(|e| e.to_string())
}

/// One MCP server: the engine's own actions (`SESSION_*`, `HITS_*`,
/// `QUANTIZE`, …) put on Studio's list too, each run through the daw
/// extension's `DAW_RUN`, so Studio's MCP server, its control socket and a
/// Luau script reach the song with nothing else to find. Asked of the
/// engine (`DAW_ACTIONS`) until it answers, then once for the app's life;
/// an id Studio already has keeps Studio's.
pub fn use_engine_actions() {
    use_future(|| async {
        static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        while !DONE.load(std::sync::atomic::Ordering::Relaxed) {
            match run("DAW_ACTIONS", serde_json::json!({})).await {
                Ok(serde_json::Value::Array(actions)) if !actions.is_empty() => {
                    let added = forward_engine_actions(&actions);
                    tracing::info!(
                        engine.actions = actions.len(),
                        engine.forwarded = added,
                        "studio: the engine's actions are on Studio's list"
                    );
                    DONE.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                _ => crate::state::sleep_ms(1000).await,
            }
        }
    });
}

/// The engine's actions that render offline on copies of the song's
/// plugins. Called by a model or the CLI, each carries `"_origin":
/// "agent"` to the engine, which then renders it in its render worker, so
/// a plugin that crashes there never takes the person's song down
/// (`studio_engine::render_worker::policy::RENDERS`).
const RENDERS: &[&str] = &[
    "LISTEN",
    "LISTEN_BLOCK",
    "LISTEN_AB",
    "LISTEN_RENDER",
    "MIX_MEASURE",
    "MIX_RENDER",
];

/// Whether `caller` is a model, the CLI or a script: a principal (Studio's
/// MCP door), or an agent's invocation (its control socket).
fn from_agent(caller: &architect::action::ActionCaller) -> bool {
    caller.principal().is_some()
        || caller
            .invocation()
            .is_some_and(|i| i.source == architect::action::InvocationSource::Agent)
}

/// `args` (an object or nothing) saying it is an agent's call.
fn mark_agent(args: &mut serde_json::Value) {
    if args.is_null() {
        *args = serde_json::json!({});
    }
    if let Some(map) = args.as_object_mut() {
        map.entry("_origin")
            .or_insert_with(|| serde_json::Value::from("agent"));
    }
}

/// Registers each of `actions` (as `DAW_ACTIONS` gives them) on Studio's
/// list, forwarded through `DAW_RUN`: how many were added.
fn forward_engine_actions(actions: &[serde_json::Value]) -> usize {
    use architect::action::{ActionError, ActionFuture, ActionInfo, Effect};
    let list = crate::actions::list();
    let text = |a: &serde_json::Value, k: &str| {
        a.get(k)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let mut added = 0;
    for a in actions {
        let id = text(a, "id");
        if id.is_empty() || list.info(&id).is_some() {
            continue;
        }
        let info = ActionInfo {
            id: id.clone(),
            display_name: text(a, "name"),
            description: text(a, "description"),
            category: text(a, "category"),
            group: text(a, "group"),
            shortcut: text(a, "shortcut"),
            effect: match text(a, "effect").as_str() {
                "read_only" => Effect::ReadOnly,
                "destructive" => Effect::Destructive,
                _ => Effect::Mutating,
            },
            params_schema: a
                .get("params")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            result_schema: a
                .get("result")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            enabled: true,
            disabled_reason: None,
            checked: None,
        };
        let handler: architect::action::CallerAsyncHandler = std::sync::Arc::new(
            move |caller: &architect::action::ActionCaller, raw: &[u8]| {
                let mut args: serde_json::Value = if raw.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::from_slice(raw).map_err(|e| ActionError::BadArgs {
                        id: id.clone(),
                        message: e.to_string(),
                        expected: None,
                    })?
                };
                if from_agent(caller) && RENDERS.contains(&id.as_str()) {
                    mark_agent(&mut args);
                }
                let call = serde_json::json!({ "id": id, "args": args }).to_string();
                let future: ActionFuture = Box::pin(async move {
                    crate::actions::list()
                        .run_json_async("DAW_RUN", call.as_bytes())
                        .await
                });
                Ok(future)
            },
        );
        if list.register_forwarded_with(&info, handler).is_ok() {
            added += 1;
        }
    }
    added
}

/// Keyflow's chart and Ignition's room follow the song the engine has:
/// told again (`KEYFLOW_SESSION_FOLLOW`, `IGNITION_SESSION_VIZ`) as the
/// engine goes to another song, and the chart's pane moved to its
/// session. The signal is the session followed now.
pub fn use_follow_session(ui: ExtensionUi, layout: Signal<Workspace>) -> Signal<Option<Followed>> {
    let mut followed = use_signal(|| None::<Followed>);
    use_future(move || async move {
        loop {
            let now = run("DAW_STATE", serde_json::json!({}))
                .await
                .ok()
                .and_then(|s| {
                    let text = |k: &str| {
                        s.pointer(&format!("/session/{k}"))
                            .and_then(serde_json::Value::as_str)
                            .filter(|t| !t.is_empty())
                            .map(str::to_owned)
                    };
                    Some(Followed {
                        id: text("id")?,
                        url: text("url")?,
                        path: text("path")?,
                    })
                });
            if now.is_some() && now != *followed.peek() {
                if let Some(f) = &now {
                    follow(ui, layout, f).await;
                }
                followed.set(now);
            }
            crate::state::sleep_ms(1000).await;
        }
    });
    followed
}

/// The current song's title as the set has it, its artist left off
/// (`Title - Artist`).
async fn current_song_title() -> Option<String> {
    let songs = run("DAW_SONGS", serde_json::json!({})).await.ok()?;
    let at = usize::try_from(songs.get("at")?.as_u64()?).ok()?;
    let title = songs
        .get("songs")?
        .as_array()?
        .get(at.checked_sub(1)?)?
        .get("title")?
        .as_str()?;
    Some(display_title(title))
}

/// A set's song title as a chart heads it: the artist left off
/// (`Title - Artist`) and a file variant's suffix dropped
/// (`Unbreakable.comped` → `Unbreakable`): a final `.` followed by one
/// lowercase word and nothing else. A dot inside a title (`Mr. Blue Sky`)
/// stays.
fn display_title(raw: &str) -> String {
    let title = raw.split(" - ").next().unwrap_or(raw).trim();
    match title.rsplit_once('.') {
        Some((head, variant))
            if !head.is_empty()
                && !variant.is_empty()
                && variant.bytes().all(|b| b.is_ascii_lowercase()) =>
        {
            head.trim_end().to_owned()
        }
        _ => title.to_owned(),
    }
}

#[cfg(test)]
mod display_title_tests {
    use super::display_title;

    #[test]
    fn the_artist_and_a_variant_suffix_are_left_off() {
        assert_eq!(
            display_title("Unbreakable.comped - Crescendum"),
            "Unbreakable"
        );
        assert_eq!(display_title("Unbreakable - Crescendum"), "Unbreakable");
        assert_eq!(display_title("Mr. Blue Sky - ELO"), "Mr. Blue Sky");
        assert_eq!(display_title("Song 2.0"), "Song 2.0");
        assert_eq!(display_title("Track.Final"), "Track.Final");
    }
}

async fn follow(ui: ExtensionUi, layout: Signal<Workspace>, f: &Followed) {
    let mut span = crate::trace::Span::root("studio.session.follow");
    span.set("studio.session.doc", f.id.clone());
    // The chart draws the session itself (its sections, tempo map and
    // markers, Keyflow's typed items): it is told only which session, and
    // the song's title to head it with.
    let title = current_song_title().await.unwrap_or_default();
    match run(
        "KEYFLOW_SESSION_FOLLOW",
        serde_json::json!({ "url": f.url, "id": f.id, "title": title }),
    )
    .await
    {
        Ok(_) => span.set("studio.follow.chart", "ok"),
        Err(e) => span.set("studio.follow.chart", e),
    }
    let chart = PaneId::new(CHART);
    if layout.peek().pane(&chart).is_some() {
        let state = format!("session:{}", f.id);
        ui.apply(|w| w.navigate(&chart, state));
    }
    let song = song_slug(
        std::path::Path::new(&f.path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("song"),
    );
    // The song's cue list into the session, when the night has one: its
    // file in IGNITION_CUES (a folder of `<lane>.json`), resolved against
    // the Live set (IGNITION_LIVE_SET) as the night does.
    let title = std::path::Path::new(&f.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("song")
        .to_owned();
    let mut lane = song.clone();
    if let (Some(dir), Some(live_set)) = (
        std::env::var_os("IGNITION_CUES"),
        std::env::var_os("IGNITION_LIVE_SET"),
    ) && let Some(cues) = cue_file(std::path::Path::new(&dir), &title)
    {
        lane = cues
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&song)
            .to_owned();
        let stored = run(
            "IGNITION_SESSION_SONG",
            serde_json::json!({
                "doc": f.id, "lane": lane, "live_set": live_set.to_string_lossy(),
                "locator": title, "cues": cues.display().to_string(), "tempo": false,
            }),
        )
        .await;
        span.set(
            "studio.follow.cues",
            stored.map_or_else(|e| e, |_| "ok".to_owned()),
        );
    }
    match run(
        "IGNITION_SESSION_VIZ",
        serde_json::json!({ "url": f.url, "doc": f.id, "song": song, "lane": lane }),
    )
    .await
    {
        Ok(_) => span.set("studio.follow.lighting", "ok"),
        Err(e) => span.set("studio.follow.lighting", e),
    }
}

/// The host the session's window draws the extensions with.
pub fn host() -> Host {
    extensions().host().clone()
}

/// Keeps the views linked to the engine the app drives: as the app links
/// one (found or started, its port checked as its own), the views attach
/// to that same one.
pub fn use_engine_link() {
    use_future(move || async move {
        let mut attached: Option<u32> = None;
        loop {
            if let Some(found) = crate::engine::linked()
                && attached != Some(found.pid)
            {
                match &found.door {
                    // In this process: the daw extension linked the views to
                    // it as it started.
                    crate::engine::Door::Here => attached = Some(found.pid),
                    crate::engine::Door::Mcp(mcp) => {
                        match studio_view_link::attach_at(
                            found.pid,
                            found.socket.clone(),
                            mcp.clone(),
                        ) {
                            Ok(()) => attached = Some(found.pid),
                            Err(e) => {
                                tracing::warn!(error = %e, engine.pid = found.pid, "studio: the views could not attach to the engine");
                            }
                        }
                    }
                }
            }
            crate::state::sleep_ms(250).await;
        }
    });
}

/// The session's tab kinds: every hosted extension view's, and the app's
/// own page kind drawing `page` for a view's slug.
pub fn kinds(ui: ExtensionUi, page: fn(String) -> Element) -> TabKinds {
    let mut kinds = TabKinds::new();
    ui.register_kinds(&mut kinds);
    kinds.register(TabKind::new(PAGE, "Session", move |ctx: TabCtx| {
        page(ctx.pane.state.clone())
    }));
    kinds
}

/// The cue list of song `title` in folder `dir`: `<lane>.json` whose lane
/// is the song's name as the night spells it, with or without its "(…)",
/// or one that ends in it (`dtla-cinematic-intro`).
fn cue_file(dir: &std::path::Path, title: &str) -> Option<std::path::PathBuf> {
    let short = song_slug(title);
    let long = song_slug(&title.replace(['(', ')'], ""));
    let digits = short.replace('7', "seven");
    let files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    let stem = |p: &std::path::PathBuf| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_owned()
    };
    files
        .iter()
        .find(|p| [&short, &long, &digits].iter().any(|n| stem(p) == **n))
        .or_else(|| {
            files
                .iter()
                .find(|p| stem(p).ends_with(&format!("-{short}")))
        })
        .cloned()
}

/// A song as the lighting's night names it: its title without a trailing
/// "(…)", lower case, words joined by `-`, nothing else kept ("Heart Isn't
/// Ready" is `heart-isnt-ready`, "Know Me (Extended)" `know-me`).
pub fn song_slug(title: &str) -> String {
    let bare = title.split(" (").next().unwrap_or(title);
    let mut out = String::new();
    for c in bare.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if (c == ' ' || c == '-' || c == '_') && !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::song_slug;

    #[test]
    fn songs_are_named_as_the_night_names_them() {
        assert_eq!(song_slug("Drive To LA"), "drive-to-la");
        assert_eq!(song_slug("Heart Isn't Ready"), "heart-isnt-ready");
        assert_eq!(song_slug("Know Me (Extended)"), "know-me");
        assert_eq!(song_slug("7 Nation Army"), "7-nation-army");
        assert_eq!(song_slug("WORST PART OF YOU"), "worst-part-of-you");
    }
}
