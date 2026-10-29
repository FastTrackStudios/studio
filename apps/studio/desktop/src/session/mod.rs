//! Session in Studio: the DAW view and its engine, and Session's browser
//! remote. It was the `session-desktop` crate (apps/session/desktop) until
//! Studio became the one app.
//!
//! - Native (feature `session-app`): the DAW view on Blitz ([`native`],
//!   mounted behind the welcome screen as [`native::Embedded`]) and the
//!   engine with no window ([`engine_host`], `studio-desktop
//!   --session-engine`). The window is always a client of its engine.
//! - The browser (feature `session-web`, wasm): the setlist player as a
//!   remote of an engine serving SetlistService ([`launch_web`]), and the
//!   `/{org}/{collection}` song browser.
//!
//! Session's commands (`cli/`, feature `cli`) are the `studio` binary's: it
//! declares them itself (src/bin/studio/main.rs), beside [`engine_host`]
//! that `studio engine` runs.

// The shared "dial the engine" plumbing: the window's Remote on another
// Session, the browser's setlist player and collection browser.
/// The engine with no window, serving windows over its socket: what
/// `studio engine` and `studio-desktop --session-engine` run.
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
pub mod engine_host;
/// The app on Blitz — see `docs/app-on-blitz.md`. The desktop's, and the
/// iPhone's: the same renderer and the same panels, laid out for the screen.
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
mod remote;
// Browser flavor of the Session workspace: SetlistService over the shared
// `/vox` link to the engine.
#[cfg(target_arch = "wasm32")]
mod session_remote_view;
// Standalone web surface at /{org}/{collection}: dials the task-server's
// per-org CollectionService and lists that collection's songs. Additive —
// the wasm entry branches to it only when the URL matches (see
// `launch_web`); every other URL is the normal app shell.
#[cfg(target_arch = "wasm32")]
mod collection_browser;
// The browser chart pane: the active song's keyflow chart (CPU engraver →
// SVG) with a playhead highlight driven by the transport streams.
#[cfg(target_arch = "wasm32")]
mod session_chart_pane;

#[cfg(target_arch = "wasm32")]
pub use web::{launch_web, web_route_matches};

/// Session's browser remote: its own small shell (Home, Session) over the
/// network engine.
#[cfg(target_arch = "wasm32")]
mod web {
    use dioxus::prelude::*;

    use super::{collection_browser, session_remote_view};

    /// Whether this page is Session's rather than the rig's: a
    /// `/{org}/{collection}` URL (the collection browser), or a `#session`
    /// deep link.
    pub fn web_route_matches() -> bool {
        collection_browser::route_matches() || hash_workspace().is_some()
    }

    /// Session's browser remote: the collection browser on a
    /// `/{org}/{collection}` URL, else the app shell.
    pub fn launch_web() {
        if collection_browser::route_matches() {
            dioxus::launch(collection_browser::CollectionBrowser);
            return;
        }
        dioxus::launch(App);
    }

    /// Top-level workspaces. Home is the landing page the others hang off.
    #[derive(Clone, Copy, PartialEq)]
    enum Workspace {
        Home,
        Session,
    }

    impl Workspace {
        fn all() -> Vec<(Self, &'static str)> {
            vec![(Self::Home, "Home"), (Self::Session, "Session")]
        }

        fn label(self) -> &'static str {
            Self::all()
                .into_iter()
                .find(|(w, _)| *w == self)
                .map_or("?", |(_, l)| l)
        }

        /// The rail glyph. The rail is icon-only, so this is how a workspace is
        /// recognised — labels are tooltips.
        const fn icon(self) -> fts_audio_ui::chrome::Icon {
            use fts_audio_ui::chrome::Icon;
            match self {
                Self::Home => Icon::Home,
                Self::Session => Icon::Session,
            }
        }
    }

    // ── Landing / last-workspace persistence ────────────────────────────────

    fn load_last_workspace() -> Option<Workspace> {
        let saved = utils::prefs::get_raw("last-workspace")?;
        Workspace::all()
            .into_iter()
            .find(|(_, label)| *label == saved)
            .map(|(w, _)| w)
    }

    fn store_last_workspace(w: Workspace) {
        utils::prefs::set_raw("last-workspace", w.label());
    }

    /// Deep link: `#session` (first hash segment). `#home` is the landing
    /// page anyway, and the rig's page when the rig is in this build.
    fn hash_workspace() -> Option<Workspace> {
        let hash = web_sys::window()?.location().hash().ok()?;
        let first = hash.trim_start_matches('#').split('/').next()?;
        first
            .eq_ignore_ascii_case("session")
            .then_some(Workspace::Session)
    }

    /// Where the app lands: the URL hash, else the persisted last choice,
    /// else Home.
    fn initial_workspace() -> Option<Workspace> {
        if let Some(w) = hash_workspace() {
            return Some(w);
        }
        Some(load_last_workspace().unwrap_or(Workspace::Home))
    }

    #[component]
    fn App() -> Element {
        let mut current = use_signal(initial_workspace);
        // The app owns the chrome: one bar, the workspace rail, and the panel
        // rail every level publishes into (fts_chrome). Level 0 is the app's own
        // contribution — the workspace crumb and the engines/settings panels.
        let _chrome = fts_audio_ui::chrome::provide_chrome();
        let level = fts_audio_ui::chrome::use_chrome_level(0);
        let chrome = level.chrome();

        let go = use_callback(move |w: Workspace| {
            current.set(Some(w));
            store_last_workspace(w);
        });
        let here = current().unwrap_or(Workspace::Home);

        // The workspace crumb carries every other workspace as its menu, so the
        // rail's icons are never the only way to change place.
        level.crumbs(vec![
            fts_audio_ui::chrome::Crumb::here(here.label()).with_menu(
                Workspace::all()
                    .into_iter()
                    .map(|(w, label)| {
                        (
                            label.to_string(),
                            w == here,
                            Callback::new(move |_| go.call(w)),
                        )
                    })
                    .collect(),
            ),
        ]);
        level.panels(vec![
            fts_audio_ui::chrome::PanelSpec::new(
                "engines",
                "Engines",
                fts_audio_ui::chrome::Icon::Engine,
            )
            .width(300),
            fts_audio_ui::chrome::PanelSpec::new(
                "settings",
                "Settings",
                fts_audio_ui::chrome::Icon::Settings,
            )
            .width(300),
        ]);

        let rail_items: Vec<fts_audio_ui::chrome::RailItem> = Workspace::all()
            .into_iter()
            .map(|(w, label)| {
                fts_audio_ui::chrome::RailItem::new(
                    label,
                    label,
                    w.icon(),
                    current() == Some(w),
                    Callback::new(move |_| go.call(w)),
                )
            })
            .collect();
        let sub_rail = chrome.sub_rail.read().clone();

        rsx! {
            // Global reset: zero the page's default margin and paint it dark.
            document::Style { {"html,body{margin:0;padding:0;height:100%;background:#0a0a0a;overflow:hidden;}*{box-sizing:border-box;}"} }
            SessionChrome {}
            // ONE bar over two rails (fts_audio_ui::chrome::AppFrame).
            fts_audio_ui::chrome::AppFrame {
                top: rsx! {
                    fts_audio_ui::chrome::TopBar {
                        leading: rsx! {
                            span {
                                style: "font-weight: 700; letter-spacing: 1px; font-size: 12px; \
                                        color: #71717a; cursor: default; padding-right: 2px;",
                                "FTS"
                            }
                        },
                        trailing: rsx! {},
                        // The browser draws its own window: nothing to drag.
                        on_drag: move |_| {},
                        on_expand: move |_| {},
                    }
                },
                rail: rsx! {
                    fts_audio_ui::chrome::IconRail { items: rail_items, sub: sub_rail }
                },
                // App-level flyouts. Views render their own inside their layout.
                panel: rsx! {
                    fts_audio_ui::chrome::PanelHost { id: "engines".to_string(),
                        div { style: "padding: 12px;", EnginesArea {} }
                    }
                    fts_audio_ui::chrome::PanelHost { id: "settings".to_string(), SettingsPanel {} }
                },
                right: rsx! { fts_audio_ui::chrome::PanelRail {} },
                main { style: "flex: 1; min-height: 0; min-width: 0; display: flex;",
                    match current() {
                        Some(Workspace::Home) | None => rsx! {
                            HomeView { current }
                        },
                        Some(Workspace::Session) => rsx! {
                            // The browser is a remote: the same session-ui
                            // panels over SetlistService on the network
                            // engine's shared /vox router.
                            session_remote_view::SessionWorkspace {}
                        },
                    }
                }
            }
        }
    }

    // ── Home — the landing page ─────────────────────────────────────────────

    /// One workspace card on the Home page. Disabled cards are places not in
    /// the web build.
    #[component]
    fn HomeCard(
        title: &'static str,
        body: &'static str,
        target: Option<Workspace>,
        current: Signal<Option<Workspace>>,
    ) -> Element {
        let enabled = target.is_some();
        rsx! {
            button {
                style: if enabled {
                    "display: flex; flex-direction: column; align-items: flex-start; gap: 8px; width: 220px; padding: 18px 16px; border-radius: 10px; background: #111113; color: #e4e4e7; border: 1px solid #27272a; text-align: left; cursor: pointer;"
                } else {
                    "display: flex; flex-direction: column; align-items: flex-start; gap: 8px; width: 220px; padding: 18px 16px; border-radius: 10px; background: #0c0c0e; color: #52525b; border: 1px solid #1c1c1f; text-align: left;"
                },
                disabled: !enabled,
                onclick: move |_| {
                    if let Some(w) = target {
                        current.set(Some(w));
                        store_last_workspace(w);
                    }
                },
                span { style: "font-size: 16px; font-weight: 700;", "{title}" }
                span { style: "font-size: 12px; color: #a1a1aa; line-height: 1.5;", "{body}" }
                if !enabled {
                    span { style: "font-size: 11px; color: #52525b;", "coming to the web build" }
                }
            }
        }
    }

    #[component]
    fn HomeView(current: Signal<Option<Workspace>>) -> Element {
        rsx! {
            div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 24px; flex: 1;",
                div { style: "display: flex; flex-direction: column; align-items: center; gap: 6px;",
                    span { style: "font-size: 22px; font-weight: 700; letter-spacing: 2px;", "SESSION" }
                    span { style: "font-size: 12px; color: #71717a;", "Setlists, songs, and charts. Pick a surface." }
                }
                div { style: "display: flex; gap: 14px; flex-wrap: wrap; justify-content: center;",
                    HomeCard {
                        title: "Session",
                        body: "Setlists and playback — the live show: songs, transport, guide.",
                        target: Some(Workspace::Session),
                        current,
                    }
                    HomeCard {
                        title: "Charts",
                        body: "keyflow chart writing — song analysis, chord charts, arrangement.",
                        target: None,
                        current,
                    }
                }
            }
        }
    }

    // ── Engines status area (header) ────────────────────────────────────────

    /// The browser cannot supervise a process: the engines are remote.
    #[component]
    fn EnginesArea() -> Element {
        rsx! {
            div { style: "display: flex; align-items: center; gap: 6px; font-size: 12px;",
                span { style: "color: #52525b; font-size: 11px;", "engines are remote" }
            }
        }
    }

    // ── Settings (version) ──────────────────────────────────────────────────

    /// The version; the deployment updates itself — nothing to check.
    #[component]
    fn SettingsPanel() -> Element {
        rsx! {
            div {
                style: "display: flex; flex-direction: column; align-items: flex-start; gap: 12px; \
                        padding: 12px; font-size: 12px;",
                span { style: "color: #a1a1aa;", "Session v{env!(\"CARGO_PKG_VERSION\")}" }
            }
        }
    }

    /// The compiled Tailwind sheet session-ui's components style themselves
    /// with (`just tailwind`, from apps/studio/desktop/input.css, which scans
    /// every UI crate). Inlined rather than loaded as an external stylesheet
    /// so it can't go stale against a committed file.
    const APP_TAILWIND: &str = include_str!("../../assets/tailwind-signal.css");

    /// App-level chrome: the compiled Tailwind sheet, and the always-mounted
    /// event bridge — the network engine's `#[subscribe]` streams over `/vox`
    /// into session-ui's global signals.
    #[component]
    fn SessionChrome() -> Element {
        rsx! {
            document::Style { {APP_TAILWIND} }
            session_remote_view::SessionRemoteBridge {}
        }
    }
}
