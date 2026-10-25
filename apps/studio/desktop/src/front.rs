//! The front screen: which side of Studio a launch goes into.
//!
//! Studio is Signal (the live rig: guitar, keys, drums, e-kit, the gig
//! page) and Session (setlists, songs, charts, the DAW) in one app. The
//! window opens on the home (`home.rs`): play here, or join a session
//! happening elsewhere. What is pressed there goes into one side, which is
//! the app until it quits. A process runs ONE engine
//! (docs/design/one-engine.md), so the side not chosen never starts its
//! own; going to the other side is a relaunch.
//!
//! `FTS_STUDIO_OPEN=signal|session` skips the home (a dev run, the
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
    /// Every side this build has.
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

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Signal => "Signal",
            #[cfg(feature = "session-app")]
            Self::Session => "Session",
        }
    }
}

/// How wide a Mac's window buttons are, with their margins, in points.
#[cfg(target_os = "macos")]
pub(crate) const MAC_WINDOW_BUTTONS: u32 = 80;

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

/// The window's root: the home, then the side chosen.
#[component]
pub fn Studio() -> Element {
    // iOS: winit's window joins the app's scene first, whichever side the
    // window ends up showing — without one iOS 27 never shows it.
    #[cfg(target_os = "ios")]
    {
        let window = dioxus_native::use_window();
        use_hook({
            let window = window.clone();
            move || {
                use dioxus_native::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
                if let Ok(handle) = window.window_handle()
                    && let RawWindowHandle::UiKit(uikit) = handle.as_raw()
                {
                    app_shell::ios_scene::window_created(uikit.ui_view);
                }
            }
        });
        // The window takes the scene's size only once it has joined it,
        // after the first frame (an iPad opened sideways starts out the
        // shape of the screen held upright): watch its size, as the phone
        // shell does, and lay the doors out again when it changes.
        let mut surface = use_signal(|| (0_u32, 0_u32));
        use_future(move || {
            let window = window.clone();
            async move {
                loop {
                    let px = window.surface_size();
                    if *surface.peek() != (px.width, px.height) {
                        surface.set((px.width, px.height));
                    }
                    futures_timer::Delay::new(std::time::Duration::from_millis(300)).await;
                }
            }
        });
        let _ = surface();
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
        None => rsx! { crate::home::Home { opening: opening(), on_pick: pick } },
    }
}

/// Signal's root: the phone shell on iOS, the desk's app elsewhere — the
/// rig in its one layout (the iPad's) either way.
#[component]
fn SignalSide() -> Element {
    // A Mac keeps its window buttons, over the rig's top left (the title
    // bar is drawn through, its buttons centred in the rig's bar): the rig
    // makes room for them. Only the older Control view
    // (`FTS_FORM_FACTOR=desktop`) draws its own and takes them off.
    #[cfg(not(target_os = "ios"))]
    {
        let window = dioxus_native::use_window();
        let control_view =
            std::env::var("FTS_FORM_FACTOR").is_ok_and(|v| v.eq_ignore_ascii_case("desktop"));
        use_hook(move || {
            if control_view {
                window.set_decorations(false);
            }
        });
    }
    #[cfg(all(target_os = "macos", feature = "signal-guitar"))]
    use_context_provider(|| guitar::ui::WindowButtons(MAC_WINDOW_BUTTONS));
    #[cfg(all(feature = "signal-guitar", target_os = "ios"))]
    return rsx! { crate::mobile_view::MobileApp {} };
    #[cfg(not(all(feature = "signal-guitar", target_os = "ios")))]
    return rsx! { crate::App {} };
}
