//! The window's root: Studio's one welcome screen, and the views of the
//! project it opens into.
//!
//! The window opens on the library (`library_home.rs`): songs, setlists,
//! sessions, parts, projects, with the ways in at the top of Home (a new
//! project, join by a link, the demo, what was opened lately) and the
//! account and org in its bar. What is opened there goes straight to it:
//! a setlist or a session into the DAW view (Session's shell, its own
//! start page never shown), a part into the rig view. Back out of a song
//! comes back here.
//!
//! The keyboard menus — ⌘K (the library's commands and Session's Action
//! List), ⌘O (open by name) and `?` (every shortcut) — are one
//! [`CommandLayer`] over every view, so they are the same on the welcome
//! screen and in a song.
//!
//! A process runs ONE engine (docs/design/one-engine.md). Session's side
//! of it is prepared at launch ([`prepare_hosted`]), which serves its
//! Action List (MCP on 127.0.0.1:47811) from the welcome screen on: an
//! open asked for there (`studio open <setlist>`, a model) goes into the
//! DAW view by itself.
//!
//! `FTS_STUDIO_OPEN` skips the welcome screen for a dev run: `rig` the rig
//! view, or anything Session opens — a path, `new`, `demo` or a live link,
//! else a library setlist's title ([`Preset`]). `--guitar`/`--keys`/…
//! (`FTS_OPEN_RIG`) open the rig view too.
//!
//! [`CommandLayer`]: library::ui::CommandLayer
//! [`prepare_hosted`]: session_desktop::native::prepare_hosted

use dioxus::prelude::*;
use library::ui::{AppCommand, AppCommands, CommandLayer, LibraryData, Opener, ShowLibrary, Slots};

use crate::library_home::{Destination, destination};

/// How wide a Mac's window buttons are, with their margins, in points.
#[cfg(target_os = "macos")]
pub(crate) const MAC_WINDOW_BUTTONS: u32 = 80;

/// What the window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    /// The welcome screen: the library.
    Library,
    /// The rig view of the project: the rigs, the gig page.
    Rig,
    /// The DAW view: a song or a set open in Session's shell.
    #[cfg(feature = "session-app")]
    Daw,
}

/// What a launch asks to show first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Preset {
    /// The welcome screen.
    Welcome,
    /// The rig view.
    Rig,
    /// What Session opens, as said (`session_desktop::native::handoff::Entry::parse`).
    Open(String),
}

/// What `open` (`FTS_STUDIO_OPEN`) and a rig asked for on the command line
/// (`rig_flag`: `--guitar`, `--keys`, `FTS_OPEN_RIG`) ask to show first.
/// The old side names (`signal`, `session`) name nothing now: the welcome
/// screen, with a warning.
pub(crate) fn preset_from(open: Option<&str>, rig_flag: bool) -> Preset {
    match open.map(str::trim).filter(|o| !o.is_empty()) {
        None if rig_flag => Preset::Rig,
        None => Preset::Welcome,
        Some(o) if o.eq_ignore_ascii_case("rig") => Preset::Rig,
        Some(o) if o.eq_ignore_ascii_case("signal") || o.eq_ignore_ascii_case("session") => {
            tracing::warn!(
                studio.open = o,
                "FTS_STUDIO_OPEN: there are no sides any more — `rig`, a path, a setlist's title, `new` or a link"
            );
            Preset::Welcome
        }
        Some(o) => Preset::Open(o.to_owned()),
    }
}

/// What this launch asked for (`FTS_STUDIO_OPEN`, `FTS_OPEN_RIG`).
fn preset() -> Preset {
    let open = std::env::var("FTS_STUDIO_OPEN").ok();
    let rig_flag = std::env::var_os("FTS_OPEN_RIG").is_some()
        || std::env::var("FTS_OPEN_WORKSPACE").is_ok_and(|w| !w.trim().is_empty());
    preset_from(open.as_deref(), rig_flag)
}

/// What going into the rig view takes before its first frame: on a phone,
/// the rig's audio session and its in-process engine (iOS forbids the child
/// process a desktop runs it in). On the UI thread: CoreMIDI keeps its
/// device list live only through the thread that connected first, and that
/// must not be a thread that ends. Once per process.
fn enter_rig() {
    static ENTERED: std::sync::Once = std::sync::Once::new();
    ENTERED.call_once(|| {
        tracing::info!(studio.view = "rig", "studio: entering");
        #[cfg(all(feature = "signal-guitar", target_os = "ios"))]
        {
            crate::ios_audio::configure();
            match crate::rig_engine::bootstrap_blocking() {
                Ok(()) => tracing::info!("rig engine ready (in-process)"),
                Err(e) => tracing::error!("rig engine failed to start: {e:?}"),
            }
        }
    });
}

/// The rig a part plays on, remembered for the rig view to open on.
fn pick_rig(kind: Option<library::model::RigKind>) {
    #[cfg(any(feature = "signal", feature = "signal-guitar"))]
    if let Some(rig) = kind.and_then(|k| crate::rigs::Rig::from_slug(&k.label().to_lowercase())) {
        crate::rigs::store_last(Some(rig));
    }
    #[cfg(not(any(feature = "signal", feature = "signal-guitar")))]
    let _ = kind;
    #[cfg(feature = "signal")]
    crate::open_rig_workspace();
}

/// The window's root.
#[component]
pub fn Studio() -> Element {
    // The live rig as a panel of the DAW view's dock: Session cannot mount
    // it, so Studio hands it over (session_desktop::native::dock::HostPanels).
    #[cfg(feature = "session-app")]
    use_context_provider(|| session_desktop::native::dock::HostPanels(vec![rig_panel()]));
    // iOS: winit's window joins the app's scene first, whichever view the
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
    // The window's form factor, once for every view (the library, the rig,
    // the DAW): measured from the window and kept current.
    #[cfg_attr(not(target_os = "ios"), allow(unused_variables))]
    let screen = app_shell::ui::form::use_window_screen();
    // iOS: the window takes the scene's size only once it has joined it,
    // after the first frame (an iPad opened sideways starts out the shape
    // of the screen held upright): lay the page out again when it changes.
    #[cfg(target_os = "ios")]
    let _ = screen();

    // Session's side of the one engine, at launch: its Action List served
    // from the welcome screen on, and the system a Remote window drives
    // attached (its set is open already: straight into the DAW).
    #[cfg(feature = "session-app")]
    let prepared = use_hook(session_desktop::native::prepare_hosted);
    #[cfg(feature = "session-app")]
    let attached = prepared.attached();
    #[cfg(feature = "session-app")]
    use_context_provider(move || prepared.clone());

    // The library every view's menus read: the account's Task orgs, each
    // a space with its setlists and their songs — empty until read.
    let mut lib = use_context_provider(|| LibraryData(Signal::new(library::Library::default())));
    #[cfg(feature = "session-app")]
    let welcome = session_desktop::native::handoff::use_welcome_account();
    #[cfg(feature = "session-app")]
    use_effect(move || {
        use session_desktop::native::handoff::{ListKind, WelcomeLists};
        let orgs = match welcome.lists() {
            WelcomeLists::Ready(orgs) => orgs,
            _ => Vec::new(),
        };
        let orgs: Vec<library::task::TaskOrg> = orgs
            .into_iter()
            .map(|(slug, lists)| library::task::TaskOrg {
                slug,
                lists: lists
                    .into_iter()
                    .map(|l| library::task::TaskList {
                        id: l.id,
                        title: l.title,
                        set: l.kind == ListKind::Set,
                        songs: l
                            .songs
                            .into_iter()
                            .map(|s| library::task::TaskSong {
                                slug: s.slug,
                                title: s.title,
                                writers: s.writers,
                                key: s.key,
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect();
        let read = library::task::library(&welcome.name().unwrap_or_default(), &orgs);
        if *lib.0.peek() != read {
            lib.0.set(read);
        }
    });

    let mut view = use_signal(move || {
        #[cfg(feature = "session-app")]
        if attached {
            return View::Daw;
        }
        match preset() {
            Preset::Welcome => View::Library,
            Preset::Rig => {
                enter_rig();
                View::Rig
            }
            #[cfg(feature = "session-app")]
            Preset::Open(text) => {
                use session_desktop::native::handoff::{self, Entry};
                match Entry::parse(&text, |p| p.exists()) {
                    Some(entry) => {
                        handoff::request(entry);
                        handoff::entering();
                        View::Daw
                    }
                    None => View::Library,
                }
            }
            #[cfg(not(feature = "session-app"))]
            Preset::Open(text) => {
                tracing::warn!(studio.open = %text, "FTS_STUDIO_OPEN: this build has no DAW to open it in");
                View::Library
            }
        }
    });
    // Each going into the DAW is a fresh Session window (its start page
    // opens what was handed to it, behind its loading screen).
    #[cfg(feature = "session-app")]
    let mut daw_generation = use_signal(|| 0_u32);
    // What could not be opened, said along the foot for a moment.
    let mut notice = use_signal(|| None::<String>);
    let mut say = move |text: String| {
        notice.set(Some(text.clone()));
        spawn(async move {
            futures_timer::Delay::new(std::time::Duration::from_secs(5)).await;
            if notice.peek().as_deref() == Some(text.as_str()) {
                notice.set(None);
            }
        });
    };

    // Into the DAW with what was just handed to it, from wherever the
    // window is: a song open is left (and stopped) for it.
    #[cfg(feature = "session-app")]
    let mut into_daw = move || {
        use session_desktop::native::handoff;
        handoff::stop_for_entry();
        handoff::entering();
        daw_generation += 1;
        view.set(View::Daw);
    };
    #[cfg(feature = "session-app")]
    let enter = use_callback(move |entry: session_desktop::native::handoff::Entry| {
        session_desktop::native::handoff::request(entry);
        into_daw();
    });

    // An open asked for while the DAW is not up — the CLI's `studio open`,
    // a model over MCP, the palette's SESSION_NEW — goes into it.
    #[cfg(feature = "session-app")]
    use_future(move || async move {
        loop {
            futures_timer::Delay::new(std::time::Duration::from_millis(300)).await;
            if *view.peek() != View::Daw && session_desktop::native::handoff::pending() {
                into_daw();
            }
        }
    });

    // What a press in the library (or ⌘O) opens.
    let mut rig = move |kind| {
        pick_rig(kind);
        enter_rig();
        view.set(View::Rig);
    };
    use_context_provider(|| {
        Opener(Callback::new(move |what| {
            match destination(&lib.0.peek(), what) {
                Destination::Rig(kind) => rig(kind),
                #[cfg(feature = "session-app")]
                Destination::Setlist(title, space) => {
                    use session_desktop::native::handoff::{Entry, WelcomeLists};
                    // The org whose library holds it.
                    let org = match welcome.lists() {
                        WelcomeLists::Ready(orgs) => orgs
                            .into_iter()
                            .map(|(slug, _)| slug)
                            .find(|slug| library::task::space_of(slug) == space),
                        _ => None,
                    };
                    enter.call(Entry::Setlist {
                        org: org.or_else(session_desktop::native::handoff::welcome_org),
                        title,
                    });
                }
                #[cfg(feature = "session-app")]
                Destination::Session(title) => {
                    match session_desktop::native::handoff::session_named(&title) {
                        Some(path) => {
                            enter.call(session_desktop::native::handoff::Entry::Path(path))
                        }
                        None => {
                            tracing::warn!(open.session = %title, "open: no session of that name on this device");
                            say(format!("{title} is not on this device yet."));
                        }
                    }
                }
                #[cfg(not(feature = "session-app"))]
                Destination::Setlist(title, _) | Destination::Session(title) => {
                    say(format!("This build has no DAW to open {title} in."));
                }
                Destination::Nowhere => say("That is not in the library any more.".to_owned()),
            }
        }))
    });
    // Somewhere in the library from over another view: the library again.
    use_context_provider(|| {
        ShowLibrary(Callback::new(move |()| {
            #[cfg(feature = "session-app")]
            if *view.peek() == View::Daw {
                session_desktop::native::handoff::stop();
            }
            view.set(View::Library);
        }))
    });
    // Back out of a song, with nothing else to open: the welcome screen.
    #[cfg(feature = "session-app")]
    use_context_provider(|| session_desktop::native::handoff::Host {
        on_leave: Callback::new(move |()| view.set(View::Library)),
    });
    // ⌘K and `?`: Session's Action List beside the library's commands.
    use_context_provider(|| AppCommands {
        list: Callback::new(|()| app_commands()),
        run: Callback::new(|id: String| {
            #[cfg(feature = "session-app")]
            if let Err(error) = session_desktop::native::handoff::run_action(&id) {
                tracing::warn!(action.id = %id, %error, "palette: the action did not run");
            }
            #[cfg(not(feature = "session-app"))]
            let _ = id;
        }),
    });
    // A plain `?` typed into a song's rename or chart is the field's.
    use_context_provider(|| {
        library::ui::Typing(Callback::new(move |()| {
            #[cfg(feature = "session-app")]
            return *view.peek() == View::Daw && session_desktop::native::handoff::typing();
            #[cfg(not(feature = "session-app"))]
            false
        }))
    });
    // The account and the org in the library's bar (`welcome`, above);
    // the ways in, at the top of its Home.
    #[cfg(feature = "session-app")]
    use_context_provider(|| crate::library_home::Doors(enter));
    use_context_provider(|| Slots {
        #[cfg(feature = "session-app")]
        bar: Some(Callback::new(|()| {
            rsx! { session_desktop::native::handoff::WelcomeAccountMenu {} }
        })),
        #[cfg(feature = "session-app")]
        home: Some(Callback::new(|()| rsx! { crate::library_home::WaysIn {} })),
        ..Slots::default()
    });

    let page = match view() {
        View::Library => rsx! { crate::library_home::LibraryHome {} },
        View::Rig => rsx! { RigView {} },
        #[cfg(feature = "session-app")]
        View::Daw => rsx! { session_desktop::native::Embedded { key: "{daw_generation}" } },
    };
    rsx! {
        CommandLayer {
            window_keys: true,
            on_overlay: move |up: bool| {
                #[cfg(feature = "session-app")]
                session_desktop::native::handoff::keys_taken(up);
                #[cfg(not(feature = "session-app"))]
                let _ = up;
            },
            WindowMenuKeys { daw: is_daw(view()) }
            {page}
            if let Some(text) = notice() {
                div {
                    style: "position: absolute; left: 0; bottom: 24px; width: 100vw; display: flex; justify-content: center; \
                            pointer-events: none;",
                    span {
                        style: "padding: 10px 16px; border-radius: 10px; background: #1c1e23; border: 1px solid #2b2b31; \
                                color: #e4e4e7; font-size: 14px; box-shadow: 0 8px 24px #0008;",
                        "{text}"
                    }
                }
            }
            AccountSheet {}
        }
    }
}

/// Whether `view` is the DAW's.
const fn is_daw(view: View) -> bool {
    #[cfg(feature = "session-app")]
    return matches!(view, View::Daw);
    #[cfg(not(feature = "session-app"))]
    {
        let _ = view;
        false
    }
}

/// The menus' keys, heard from the window: ⌘K, ⌘O and `?` whatever has
/// the focus (at first nothing has, and a page's key reaches the DOM only
/// through what does), and the keys of a menu that is up. Session's own
/// window keys stand aside while a menu is up (`handoff::keys_taken`).
#[component]
fn WindowMenuKeys(daw: bool) -> Element {
    use dioxus_native::winit::event::{ElementState, WindowEvent};
    use dioxus_native::winit::keyboard::{Key, NamedKey};
    use library::ui::MenuKey;

    let menus = library::ui::use_menus();
    let mut held = use_signal(|| false);
    let mut daw_now = use_signal(|| daw);
    if *daw_now.peek() != daw {
        daw_now.set(daw);
    }
    dioxus_native::use_window_event(move |event, _| match event {
        WindowEvent::ModifiersChanged(modifiers) => {
            let state = modifiers.state();
            held.set(state.control_key() || state.meta_key());
        }
        WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
            let key = match &event.logical_key {
                Key::Named(NamedKey::Escape) => MenuKey::Escape,
                Key::Named(NamedKey::ArrowUp) => MenuKey::Up,
                Key::Named(NamedKey::ArrowDown) => MenuKey::Down,
                Key::Named(NamedKey::Enter) => MenuKey::Enter,
                Key::Named(NamedKey::Backspace) => MenuKey::Backspace,
                Key::Character(c) => MenuKey::Char(c.to_string()),
                _ => return,
            };
            #[cfg(feature = "session-app")]
            let daw_typing = *daw_now.peek() && session_desktop::native::handoff::typing();
            #[cfg(not(feature = "session-app"))]
            let daw_typing = false;
            // A field's plain keys are the field's — unless a menu is up,
            // whose query they are.
            let typing = library::ui::field_focused() || daw_typing;
            if typing && !*held.peek() && !menus.up() {
                return;
            }
            menus.key(&key, *held.peek(), typing);
        }
        _ => {}
    });
    rsx! {}
}

/// The account sheet (signing in, Account…), over every view.
#[component]
fn AccountSheet() -> Element {
    #[cfg(feature = "session-app")]
    return rsx! { session_desktop::native::handoff::WelcomeAccountSheet {} };
    #[cfg(not(feature = "session-app"))]
    rsx! {}
}

/// Session's Action List as the palette lists it.
fn app_commands() -> Vec<AppCommand> {
    #[cfg(feature = "session-app")]
    return session_desktop::native::handoff::actions()
        .into_iter()
        .map(|a| AppCommand {
            id: a.id,
            label: a.label,
            group: a.group,
            keys: a.shortcut,
            enabled: a.enabled,
        })
        .collect();
    #[cfg(not(feature = "session-app"))]
    Vec::new()
}

/// The `rig` panel kind for the Session window's dock.
#[cfg(feature = "session-app")]
fn rig_panel() -> dock::host::Kind {
    dock::host::Kind {
        kind: dock::builtin::kind::RIG,
        title: "Rig",
        short: "RIG",
        home: dock::dockspace::Place::Center,
        chrome: dock::dockspace::Chrome::Bare,
        attach: None,
        render: |_| rsx! { DockRig {} },
    }
}

/// The live rig inside the DAW view: the rig workspace (its menu, then the
/// rig picked), a client of the headless rig engine — never an engine in
/// this process, which already runs the window's.
#[cfg(feature = "session-app")]
#[component]
fn DockRig() -> Element {
    use_context_provider(|| crate::rig_view::SupervisedOnly);
    // The rig lays out by its panel, not the window (`PanelSize`): measured
    // when mounted, when the window resizes, and on redraws at most four
    // times a second — which catches a divider dragged across it.
    let mut size = use_signal(|| (0.0_f64, 0.0_f64));
    use_context_provider(|| crate::rig_view::PanelSize(size));
    let node =
        use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(None::<std::rc::Rc<MountedData>>)));
    let measure = {
        let node = std::rc::Rc::clone(&node);
        move || {
            let Some(node) = node.borrow().clone() else {
                return;
            };
            spawn(async move {
                if let Ok(r) = node.get_client_rect().await {
                    let now = (r.size.width, r.size.height);
                    let was = *size.peek();
                    if (was.0 - now.0).abs() > 1.0 || (was.1 - now.1).abs() > 1.0 {
                        size.set(now);
                    }
                }
            });
        }
    };
    #[cfg(not(target_os = "ios"))]
    {
        let measure = measure.clone();
        let last = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(None::<std::time::Instant>)));
        dioxus_native::use_window_event(move |event, _| {
            use dioxus_native::winit::event::WindowEvent;
            let due = match event {
                WindowEvent::SurfaceResized(_) => true,
                WindowEvent::RedrawRequested => last
                    .get()
                    .is_none_or(|t| t.elapsed() > std::time::Duration::from_millis(250)),
                _ => false,
            };
            if due {
                last.set(Some(std::time::Instant::now()));
                measure();
            }
        });
    }
    let on_mount = {
        let node = std::rc::Rc::clone(&node);
        move |e: MountedEvent| {
            *node.borrow_mut() = Some(e.data());
            measure();
        }
    };
    rsx! {
        div {
            style: "position:relative; flex:1; min-width:0; min-height:0; overflow:hidden;",
            onmounted: on_mount,
            div { style: "position:absolute; inset:0; display:flex; flex-direction:column;",
                crate::rig_view::SignalWorkspace {}
            }
        }
    }
}

/// The rig view: the phone shell on iOS, the desk's app elsewhere — the
/// rig in its one layout (the iPad's) either way.
#[component]
fn RigView() -> Element {
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

#[cfg(test)]
mod tests {
    use super::{Preset, preset_from};

    #[test]
    fn nothing_asked_is_the_welcome_screen() {
        assert_eq!(preset_from(None, false), Preset::Welcome);
        assert_eq!(preset_from(Some("  "), false), Preset::Welcome);
    }

    #[test]
    fn a_rig_flag_or_rig_opens_the_rig_view() {
        assert_eq!(preset_from(None, true), Preset::Rig);
        assert_eq!(preset_from(Some("rig"), false), Preset::Rig);
    }

    #[test]
    fn the_old_side_names_open_nothing() {
        assert_eq!(preset_from(Some("signal"), false), Preset::Welcome);
        assert_eq!(preset_from(Some("Session"), true), Preset::Welcome);
    }

    #[test]
    fn anything_else_is_for_the_daw() {
        assert_eq!(
            preset_from(Some("JHM Sunday"), true),
            Preset::Open("JHM Sunday".into())
        );
        assert_eq!(
            preset_from(Some("/songs/a.RPP"), false),
            Preset::Open("/songs/a.RPP".into())
        );
    }
}
