//! The front screen: which side of Studio a launch goes into.
//!
//! Studio is Signal (the live rig: guitar, keys, drums, e-kit, the gig
//! page) and Session (setlists, songs, charts, the DAW) in one app. For
//! now they meet only here: the window opens on two doors, and the one
//! chosen is the app until it quits. A process runs ONE engine
//! (docs/design/one-engine.md), so the side not chosen never starts its
//! own; going to the other side is a relaunch.
//!
//! `FTS_STUDIO_OPEN=signal|session` skips the doors (a dev run, the
//! simulator, a desk that is only ever one of them).

use dioxus::prelude::*;

/// One side of Studio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The live rig.
    Signal,
    /// The DAW: setlists, songs and charts.
    #[cfg(feature = "session-app")]
    Session,
}

impl Side {
    /// Every side this build has, in the order the doors show them.
    fn all() -> Vec<Self> {
        vec![
            Self::Signal,
            #[cfg(feature = "session-app")]
            Self::Session,
        ]
    }

    fn from_slug(slug: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|side| side.name().eq_ignore_ascii_case(slug.trim()))
    }

    fn name(self) -> &'static str {
        match self {
            Self::Signal => "Signal",
            #[cfg(feature = "session-app")]
            Self::Session => "Session",
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            Self::Signal => "The live rig: guitar, keys, drums and the gig.",
            #[cfg(feature = "session-app")]
            Self::Session => "The DAW: setlists, songs, charts and the band's mix.",
        }
    }

    /// The side's colour, its app icon's: Signal's green, Session's blue.
    fn accent(self) -> &'static str {
        match self {
            Self::Signal => "#2fd572",
            #[cfg(feature = "session-app")]
            Self::Session => "#3aa0ff",
        }
    }
}

/// The side asked for before launch (`FTS_STUDIO_OPEN`), if any.
fn preset() -> Option<Side> {
    let slug = std::env::var("FTS_STUDIO_OPEN").ok()?;
    let side = Side::from_slug(&slug);
    if side.is_none() {
        tracing::warn!(studio.open = %slug, "FTS_STUDIO_OPEN names no side of this build");
    }
    side
}

/// What going into `side` takes before its first frame: on a phone, the
/// rig's audio session and its in-process engine (iOS forbids the child
/// process a desktop runs it in). On the UI thread, as it was before the
/// window when Signal was its own app: CoreMIDI keeps its device list live
/// only through the thread that connected first, and that must not be a
/// thread that ends.
fn enter(side: Side) {
    tracing::info!(studio.side = side.name(), "studio: entering");
    match side {
        Side::Signal => {
            #[cfg(all(feature = "signal-guitar", target_os = "ios"))]
            {
                crate::ios_audio::configure();
                match crate::rig_engine::bootstrap_blocking() {
                    Ok(()) => tracing::info!("rig engine ready (in-process)"),
                    Err(e) => tracing::error!("rig engine failed to start: {e:?}"),
                }
            }
        }
        #[cfg(feature = "session-app")]
        Side::Session => {}
    }
}

/// The window's root: the doors, then the side chosen.
#[component]
pub fn Studio() -> Element {
    // iOS: winit's window joins the app's scene first, whichever side the
    // window ends up showing — without one iOS 27 never shows it.
    #[cfg(target_os = "ios")]
    {
        let window = dioxus_native::use_window();
        use_hook(move || {
            use dioxus_native::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = window.window_handle()
                && let RawWindowHandle::UiKit(uikit) = handle.as_raw()
            {
                app_shell::ios_scene::window_created(uikit.ui_view);
            }
        });
    }

    let mut side = use_signal(|| {
        preset().inspect(|&side| {
            enter(side);
        })
    });
    // The door pressed, lit while its side starts (a phone's rig engine
    // takes a moment), so a press is seen before the wait.
    let mut opening = use_signal(|| None::<Side>);
    let pick = move |chosen: Side| {
        if opening().is_some() {
            return;
        }
        opening.set(Some(chosen));
        spawn(async move {
            // One frame, to paint the pressed door before `enter` blocks.
            futures_timer::Delay::new(std::time::Duration::from_millis(32)).await;
            enter(chosen);
            side.set(Some(chosen));
        });
    };

    match side() {
        Some(Side::Signal) => rsx! { SignalSide {} },
        #[cfg(feature = "session-app")]
        Some(Side::Session) => rsx! { session_desktop::native::Embedded {} },
        None => rsx! { Doors { opening: opening(), on_pick: pick } },
    }
}

/// Signal's root: the phone shell on iOS, the desk's app elsewhere.
#[component]
fn SignalSide() -> Element {
    // The desk's app draws its own title bar (the header doubles as one:
    // drag surfaces, window controls); the window opened with the
    // system's, which Session's top bar sits under.
    #[cfg(not(target_os = "ios"))]
    {
        let window = dioxus_native::use_window();
        use_hook(move || window.set_decorations(false));
    }
    #[cfg(all(feature = "signal-guitar", target_os = "ios"))]
    return rsx! { crate::mobile_view::MobileApp {} };
    #[cfg(not(all(feature = "signal-guitar", target_os = "ios")))]
    return rsx! { crate::App {} };
}

/// The two doors, side by side on a wide window and stacked on a tall one.
#[component]
fn Doors(opening: Option<Side>, on_pick: EventHandler<Side>) -> Element {
    rsx! {
        div {
            style: "width: 100%; height: 100%; display: flex; flex-direction: column; \
                    align-items: center; justify-content: center; gap: 28px; padding: 32px; \
                    box-sizing: border-box; overflow: hidden; background: #0c0c0f; \
                    color: #e5e7eb; font-family: Inter, system-ui, sans-serif;",
            div {
                style: "display: flex; flex-direction: column; align-items: center; gap: 6px;",
                div { style: "font-size: 30px; font-weight: 700; letter-spacing: 0.5px;", "Studio" }
                div { style: "font-size: 14px; color: #8b9099;", "by FastTrackStudio" }
            }
            div {
                style: "display: flex; flex-wrap: wrap; justify-content: center; gap: 20px; \
                        width: 100%; max-width: 760px;",
                for side in Side::all() {
                    Door {
                        key: "{side.name()}",
                        side,
                        lit: opening == Some(side),
                        dim: opening.is_some_and(|o| o != side),
                        on_pick,
                    }
                }
            }
        }
    }
}

#[component]
fn Door(side: Side, lit: bool, dim: bool, on_pick: EventHandler<Side>) -> Element {
    let accent = side.accent();
    let border = if lit { accent } else { "#2a2c31" };
    let opacity = if dim { "0.4" } else { "1" };
    let glow = if lit {
        format!("box-shadow: 0 0 24px {accent}55;")
    } else {
        String::new()
    };
    rsx! {
        div {
            style: "flex: 1 1 280px; min-width: 0; max-width: 360px; display: flex; \
                    flex-direction: column; gap: 10px; padding: 26px 24px; border-radius: 18px; \
                    background: #17181b; border: 2px solid {border}; opacity: {opacity}; \
                    cursor: pointer; box-sizing: border-box; {glow}",
            onclick: move |_| on_pick.call(side),
            div {
                style: "width: 44px; height: 6px; border-radius: 3px; background: {accent};",
            }
            div { style: "font-size: 24px; font-weight: 700;", "{side.name()}" }
            div { style: "font-size: 15px; line-height: 1.4; color: #b4b9c2;", "{side.blurb()}" }
            div {
                style: "margin-top: 6px; font-size: 13px; font-weight: 600; color: {accent};",
                if lit { "Opening…" } else { "Open {side.name()}" }
            }
        }
    }
}
