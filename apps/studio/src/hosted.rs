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
    for id in [crate::stage::LIGHTING_WINDOW, crate::stage::CHART_WINDOW] {
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
        session_engine::actions::in_host(true);
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
        let host = match architect_extensions::process::ProcessRunner::new() {
            Ok(runner) => host.with_runner(runner),
            Err(e) => {
                tracing::warn!(error = %e, "studio: no process extensions");
                host
            }
        };
        let installs = crate::storage::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("fts")
            .join("studio-extensions");
        let linked: [&dyn architect_extensions::linked::Extension; 6] = [
            &studio_arrangement::Arrangement,
            &studio_mixer::Mixer,
            &studio_lyrics::Lyrics,
            &studio_navigator::Navigator,
            &keyflow_extension::Keyflow,
            &ignition_pane::ext::Lighting,
        ];
        let mut ext = Extensions::new(host, Installs::new(installs));
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
        match daw_package() {
            Ok(dir) => {
                tell(&ext.dev(dir));
                ext.host().use_provider("fts.session", Some("daw"));
            }
            Err(e) => tracing::warn!(error = %e, "studio: the daw extension is not here"),
        }
        ext
    })
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
fn daw_package() -> Result<std::path::PathBuf, String> {
    const MANIFEST: &str = include_str!("../../../extensions/daw/extension.toml");
    let bin = crate::engine::daw_extension_bin().ok_or("no daw-extension program")?;
    let dir = crate::storage::config_dir()
        .ok_or("no config folder")?
        .join("fts")
        .join("studio-extensions")
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

/// Keyflow's chart and Ignition's room follow the song the engine has:
/// told again (`KEYFLOW_SESSION_FOLLOW`, `IGNITION_SESSION_ROOM`) as the
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

async fn follow(ui: ExtensionUi, layout: Signal<Workspace>, f: &Followed) {
    let mut span = crate::trace::Span::root("studio.session.follow");
    span.set("studio.session.doc", f.id.clone());
    match run(
        "KEYFLOW_SESSION_FOLLOW",
        serde_json::json!({ "url": f.url, "id": f.id }),
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
        "IGNITION_SESSION_ROOM",
        serde_json::json!({ "url": f.url, "doc": f.id, "song": song, "lane": lane }),
    )
    .await
    {
        Ok(_) => span.set("studio.follow.room", "ok"),
        Err(e) => span.set("studio.follow.room", e),
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
                match studio_view_link::attach_at(
                    found.pid,
                    found.socket.clone(),
                    found.mcp.clone(),
                ) {
                    Ok(()) => attached = Some(found.pid),
                    Err(e) => {
                        tracing::warn!(error = %e, engine.pid = found.pid, "studio: the views could not attach to the engine");
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
