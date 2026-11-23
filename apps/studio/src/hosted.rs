//! The session's views are Studio's real extensions, linked into the app
//! and hosted by app-shell-extensions: `studio.arrangement` (the main
//! view, a pane of the session's layout), `studio.mixer` (a window of its
//! own), `studio.lyrics` and `studio.navigator`. Each is a Studio window
//! on the engine (`studio_view_link`): the song read through the window
//! protocol's replicas, its edits and transport sent back, the same panels
//! the old Session window drew.
//!
//! The engine is the one [`crate::engine`] found or started and checked
//! (its pid owns its port): the views attach to that one
//! ([`use_engine_link`]), never to whichever engine answers first.

use std::sync::OnceLock;

use app_shell::ui::{TabCtx, TabKind, TabKinds};
use app_shell::{Pane, PaneId, Rect, Target, Workspace};
use app_shell_extensions::ExtensionUi;
use architect_extensions::extensions::Extensions;
use architect_extensions::host::{Host, SharedSections};
use architect_extensions::install::Installs;
use architect_settings::Registry;
use dioxus::prelude::*;

/// The arrangement's tab kind: `<extension>/<view>`.
pub const ARRANGEMENT: &str = "studio.arrangement/arrangement";
/// The mixer's.
pub const MIXER: &str = "studio.mixer/mixer";
/// The lyrics'.
pub const LYRICS: &str = "studio.lyrics/lyrics";
/// The session's own pages (the chart, the overview, the views not yet
/// an extension), drawn by the app: the pane's state is the view's slug.
pub const PAGE: &str = "studio.page";

static EXTENSIONS: OnceLock<Extensions> = OnceLock::new();

thread_local! {
    /// The session's window while it is open: its extensions' UI and its
    /// layout, for the actions that open its windows.
    static SESSION: std::cell::Cell<Option<(ExtensionUi, Signal<Workspace>)>> =
        const { std::cell::Cell::new(None) };
}

/// The session's window is `ui` over `layout` (or gone, `None`).
pub fn set_session(now: Option<(ExtensionUi, Signal<Workspace>)>) {
    SESSION.with(|s| s.set(now));
}

/// Opens the mixer's window from the open session (`STUDIO_MIXER_WINDOW`).
pub fn open_mixer_in_session() -> Result<(), String> {
    let (ui, layout) = SESSION
        .with(std::cell::Cell::get)
        .ok_or("the mixer's window opens from the session: open one first (STUDIO_SESSION_OPEN)")?;
    open_mixer(ui, layout)
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
        let installs = crate::storage::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("fts")
            .join("studio-extensions");
        let linked: [&dyn architect_extensions::linked::Extension; 4] = [
            &studio_arrangement::Arrangement,
            &studio_mixer::Mixer,
            &studio_lyrics::Lyrics,
            &studio_navigator::Navigator,
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
        let _ = ext.sync();
        ext
    })
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

/// The pane a session view is shown in: an extension's view, or the app's
/// page for it (`state` its slug).
pub fn pane_for(kind: &str, slug: &str, title: &str) -> Pane {
    if kind == PAGE {
        let mut pane = Pane::new(format!("{PAGE}:{slug}"), PAGE, title);
        pane.state = slug.to_owned();
        pane
    } else {
        Pane::new(kind, kind, title)
    }
}

/// Shows `pane` as the session's one view, in place of what was.
pub fn show(ui: ExtensionUi, layout: Signal<Workspace>, pane: Pane) {
    let id = pane.id.clone();
    let already = layout.peek().pane(&id).is_some();
    if already {
        let _ = ui.apply(|w| w.activate(&id));
    } else {
        let _ = ui.apply(|w| w.open(pane, Target::Here));
    }
}

/// The mixer, in a window of its own: the mixer extension's view popped
/// out of the session's layout (app-shell-native opens it as an OS
/// window, drawn by [`crate::popout`]). A second press brings the one
/// open forward.
pub fn open_mixer(ui: ExtensionUi, layout: Signal<Workspace>) -> Result<(), String> {
    let mut span = crate::trace::Span::root("studio.mixer.window");
    span.set("studio.window", MIXER);
    let id = PaneId::new(MIXER);
    if layout.peek().pane(&id).is_some() {
        span.set("studio.window.outcome", "shown");
        ui.apply(|w| w.activate(&id));
        return Ok(());
    }
    let opened = ui.apply(|w| {
        w.open(
            Pane::new(MIXER, MIXER, "Mixer"),
            Target::Window(Some(Rect {
                x: 160.0,
                y: 140.0,
                w: 1280.0,
                h: 620.0,
            })),
        )
    });
    span.set(
        "studio.window.outcome",
        if opened.is_some() {
            "opened"
        } else {
            "refused"
        },
    );
    opened
        .map(drop)
        .ok_or_else(|| "the mixer's window could not open".to_owned())
}
