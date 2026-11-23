//! What a Studio rig view extension's pane is: a vox client of the rig
//! engine (the `studio.rig` extension), as every Signal GUI is.
//!
//! - [`discover`] finds the engine this app started.
//! - [`RigPane`] is a pane's root: it waits for the connection, puts the
//!   rig's clients in context and draws Signal's panel providers
//!   (`guitar::ui::panels::RigProviders`) around the view, mounted afresh
//!   when the engine is lost and found again.
//! - [`bus`] is what panes share past the engine's own state: the routing
//!   grid's selection, the browser's focus, the part being built. Each
//!   pane mirrors it into its own context.
//!
//! The view extensions (`studio.rig.switches`, `.setlist`, `.browser`,
//! `.grid`, `.macros`, `.status`, `studio.audio-settings`, `studio.keys`)
//! build on it; none needs another installed.

pub mod bus;
pub mod discover;

use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use architect::connect::{Endpoint, pool};
use architect_extensions::hostcall::HostRequest;
use architect_extensions::linked::Handle;
use dioxus::prelude::*;
use guitar::proto::audio::AudioSettingsClient;
use guitar::proto::rig::{RigClient, RigStreamClient};
use guitar::ui::panels::{BrowserFocus, BuildPick, PanelHost, RoutingSel};
use keys::proto::keys::{KeysRigClient, KeysRigStreamClient};
use serde_json::{Value as Json, json};
use signal_rigs_proto::perform::service::{RigPerformClient, RigPerformStreamClient};
use signal_tone3000::tone3000::{Tone3000Client, Tone3000StreamClient};

/// How often a pane without an engine looks for one.
const RETRY: Duration = Duration::from_millis(800);
/// How often a connected pane asks the engine whether it is there.
const PING: Duration = Duration::from_millis(1500);

/// Every client a rig pane's views read. The rig's own three are always
/// there; the rest are `None` on an engine that does not serve them.
#[derive(Clone)]
pub struct Clients {
    pub url: String,
    pub rig: RigClient,
    pub stream: RigStreamClient,
    pub settings: AudioSettingsClient,
    pub tones: Option<Tone3000Client>,
    pub tones_stream: Option<Tone3000StreamClient>,
    pub perform: Option<RigPerformClient>,
    pub perform_stream: Option<RigPerformStreamClient>,
    pub keys: Option<KeysRigClient>,
    pub keys_stream: Option<KeysRigStreamClient>,
}

/// The runtime the connection lives on: a process's panes come and go
/// (each development-copy view has a runtime of its own), the connection
/// stays.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        #[expect(
            clippy::expect_used,
            reason = "a process that cannot start a thread cannot draw a pane either"
        )]
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("studio-rig-link")
            .enable_all()
            .build()
            .expect("the rig link's runtime")
    })
}

/// Gives this thread a tokio runtime to make timers and tasks on, when it
/// has none.
///
/// Signal's views sleep and spawn through `architect::platform`, which is
/// tokio on native: its own app draws inside a tokio runtime. Architect's
/// window does not, so a rig pane's thread enters the link's runtime, once,
/// for good (a timer made there wakes the view's task as any waker does).
/// A development copy's view thread already runs one and keeps it.
fn give_this_thread_a_runtime() {
    thread_local! {
        static ENTERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if ENTERED.replace(true) || tokio::runtime::Handle::try_current().is_ok() {
        return;
    }
    // Never left: the thread is the window's, and draws rig panes until
    // the app ends.
    std::mem::forget(runtime().enter());
}

/// A typed client of the engine at `url`, over the process's one
/// connection to it.
async fn establish<C: vox_core::FromVoxLane + Send + 'static>(url: &str) -> Option<C> {
    let endpoint = Endpoint::url(url.to_owned());
    let dialled = runtime()
        .spawn(async move { pool().caller(&endpoint).await })
        .await
        .ok()?;
    match dialled {
        Ok(caller) => Some(C::from_vox_lane(caller, None)),
        Err(error) => {
            tracing::debug!(%error, rig.url = url, "the rig engine did not answer");
            None
        }
    }
}

/// Every client of the engine at `url`, when it serves the rig.
async fn connect(url: &str) -> Option<Clients> {
    let rig: RigClient = establish(url).await?;
    // The first call says whether there is an engine behind the socket.
    rig.status().await.ok()?;
    let guitar = signal_rigs_proto::Rig::Guitar.slug();
    let perform: Option<RigPerformClient> = establish(url).await;
    let perform_stream: Option<RigPerformStreamClient> = establish(url).await;
    Some(Clients {
        url: url.to_owned(),
        rig,
        stream: establish(url).await?,
        settings: establish(url).await?,
        tones: establish(url).await,
        tones_stream: establish(url).await,
        perform: perform.map(|c| architect::scope_client!(c, guitar)),
        perform_stream: perform_stream.map(|c| architect::scope_client!(c, guitar)),
        keys: establish(url).await,
        keys_stream: establish(url).await,
    })
}

/// Why a pane has no rig yet.
static PROBLEM: Mutex<Option<String>> = Mutex::new(None);

fn set_problem(why: Option<String>) {
    *PROBLEM.lock().unwrap_or_else(PoisonError::into_inner) = why;
}

/// Why there is no rig yet, when something is known.
#[must_use]
pub fn problem() -> Option<String> {
    PROBLEM
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// How many of this process's panes hold a connection now.
static CONNECTED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// One connected pane, counted while it lives.
struct Counted;

impl Counted {
    fn new() -> Self {
        CONNECTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        CONNECTED.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// This process's side of the rig, as a view extension's `…_STATE`
/// action answers it: the engine it would attach to, how many of its
/// panes are connected, why none is when something is known, and what
/// the panes share.
#[must_use]
pub fn state() -> Json {
    use guitar::ui::panels::Selected;
    let shared = bus::get();
    let selection = shared.selection.map(|selected| match selected {
        Selected::Block(id) => json!({ "block": id }),
        Selected::Module(name, blocks) => json!({ "module": name, "blocks": blocks }),
    });
    let focus = shared.focus.map(|f| {
        json!({ "kind": f.kind, "preset": f.preset, "variation": f.variation, "search": f.search })
    });
    json!({
        "engine": discover::url(),
        "panes_connected": CONNECTED.load(std::sync::atomic::Ordering::Relaxed),
        "problem": problem(),
        "selection": selection,
        "focus": focus,
        "part": shared.part.map(|p| p.saturating_add(1)),
    })
}

/// A pane's connection: the clients, and which connection they are of
/// (it moves when the engine is lost and found again).
#[derive(Clone, Copy)]
pub struct Link {
    clients: Signal<Option<Clients>>,
    pub generation: Signal<u32>,
}

impl Link {
    /// The clients, while the engine is there.
    #[must_use]
    pub fn current(&self) -> Option<Clients> {
        self.clients.read().clone()
    }
}

/// Connects this pane to the app's rig engine and keeps it so: it looks
/// until one answers, asks it whether it is there every 1.5 s, and after
/// two silences looks again.
pub fn use_link() -> Link {
    let mut clients = use_signal(|| None::<Clients>);
    let mut generation = use_signal(|| 0u32);
    use_future(move || async move {
        loop {
            let found = match discover::url() {
                Some(url) => connect(&url).await,
                None => None,
            };
            let Some(found) = found else {
                set_problem(Some(match discover::url() {
                    Some(url) => format!("Waiting for the rig engine at {url}…"),
                    None => "Waiting for the rig engine (the studio.rig extension)…".to_owned(),
                }));
                futures_timer::Delay::new(RETRY).await;
                continue;
            };
            set_problem(None);
            let rig = found.rig.clone();
            generation += 1;
            clients.set(Some(found));
            // Counted until the engine is lost or the pane closes (the
            // future is dropped with it).
            let counted = Counted::new();
            let mut silences = 0u32;
            while silences < 2 {
                futures_timer::Delay::new(PING).await;
                silences = if rig.status().await.is_ok() {
                    0
                } else {
                    silences.saturating_add(1)
                };
            }
            tracing::warn!(
                rig.outcome = "lost",
                "the rig engine went away; looking again"
            );
            drop(counted);
            clients.set(None);
        }
    });
    Link {
        clients,
        generation,
    }
}

/// The extension's way to the host, kept for what a pane asks of it
/// (bring another view up). Each view extension gives its own.
static HANDLES: Mutex<Vec<(String, Handle)>> = Mutex::new(Vec::new());

/// Keeps `handle` as extension `id`'s way to the host.
pub fn set_handle(id: &str, handle: Handle) {
    let mut all = HANDLES.lock().unwrap_or_else(PoisonError::into_inner);
    all.retain(|(known, _)| known != id);
    all.push((id.to_owned(), handle));
}

fn handle(id: &str) -> Option<Handle> {
    HANDLES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|(known, _)| known == id)
        .map(|(_, handle)| handle.clone())
}

/// Runs the app's action `action` as extension `id` (which must declare
/// it under `[capabilities] actions`).
///
/// # Errors
///
/// The extension is not running, or the host refused or failed the action.
pub async fn run(id: &str, action: &str, args: Json) -> Result<Json, String> {
    let handle = handle(id).ok_or_else(|| format!("{id} is not running"))?;
    handle
        .host(HostRequest::Run {
            action: action.to_owned(),
            args,
            ticket: None,
        })
        .await
        .map_err(|e| format!("{action}: {e}"))
}

/// Extension `id`'s own setting `key` (`<id>.<name>`), as the settings
/// registry has it: the stored value, else its manifest's default.
///
/// # Errors
///
/// The extension is not running, or the host refused.
pub async fn setting(id: &str, key: &str) -> Result<Json, String> {
    let handle = handle(id).ok_or_else(|| format!("{id} is not running"))?;
    handle
        .host(HostRequest::Setting {
            key: key.to_owned(),
        })
        .await
        .map_err(|e| format!("{key}: {e}"))
}

/// Sets extension `id`'s own setting `key`.
///
/// # Errors
///
/// The extension is not running, or the host refused.
pub async fn set_setting(id: &str, key: &str, value: Json) -> Result<Json, String> {
    let handle = handle(id).ok_or_else(|| format!("{id} is not running"))?;
    handle
        .host(HostRequest::SetSetting {
            key: key.to_owned(),
            value,
        })
        .await
        .map_err(|e| format!("{key}: {e}"))
}

/// The clients of this app's rig engine, for an action (a pane has its
/// own, [`RigPane`]'s): `None` while no engine answers.
pub async fn clients() -> Option<Clients> {
    connect(&discover::url()?).await
}

/// Which extension a pane belongs to: what it asks of the host, it asks
/// as that extension.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Owner(pub &'static str);

/// Whether a pane's view is the guitar rig's (Signal's panel providers
/// around it) or brings its own state (the keys').
#[derive(Clone, Copy, PartialEq, Eq)]
struct Bare(bool);

/// A pane's root for a view that brings its own state (the keys rig's):
/// `children` under the engine's clients once it answers, with no
/// guitar providers around them.
#[component]
pub fn EnginePane(owner: Owner, children: Element) -> Element {
    use_context_provider(|| Bare(true));
    rsx! {
        RigPane { owner, {children} }
    }
}

/// A rig pane's root: `children` under the rig's clients and Signal's
/// panel providers once the engine answers, a line saying what it waits
/// for until then.
#[component]
pub fn RigPane(owner: Owner, children: Element) -> Element {
    give_this_thread_a_runtime();
    let link = use_link();
    use_context_provider(|| link);
    use_context_provider(|| owner);
    let generation = (link.generation)();
    if link.current().is_none() {
        return waiting();
    }
    rsx! {
        div { style: "position:relative; width:100%; height:100%; min-width:0; min-height:0; \
                      display:flex; overflow:hidden; background:#0f0f12; color:#e4e4e7; \
                      font-family: Inter, -apple-system, sans-serif;",
            // A list of one, so its key is diffed: another connection
            // mounts the view afresh (its client reads are taken at mount).
            for key in std::iter::once(generation) {
                Connected { key: "{key}", {children.clone()} }
            }
        }
    }
}

/// What a pane shows until the engine answers.
fn waiting() -> Element {
    let why = problem().unwrap_or_else(|| "Connecting to the rig engine…".to_owned());
    rsx! {
        div { style: "width:100%; height:100%; display:flex; align-items:center; justify-content:center; \
                      padding:16px; box-sizing:border-box; font-size:13px; color:#8e8e98; \
                      background:#0f0f12; font-family: Inter, -apple-system, sans-serif; text-align:center;",
            "{why}"
        }
    }
}

/// Opens view `view` of extension `extension` beside what is open, as
/// `owner` asks it of the host.
fn open_view(owner: Owner, extension: &'static str, view: String) {
    spawn(async move {
        let asked = run(
            owner.0,
            "EXT_VIEW_OPEN",
            json!({ "extension": extension, "view": view, "placement": "split" }),
        )
        .await;
        if let Err(error) = asked {
            tracing::debug!(%error, ext.view = %view, "a rig pane's view was not opened");
        }
    });
}

/// The view under one connection: the clients in context, then Signal's
/// providers, then what panes share.
#[component]
fn Connected(children: Element) -> Element {
    let link = use_context::<Link>();
    let owner = use_context::<Owner>();
    use_hook(|| {
        if let Some(clients) = link.clients.peek().clone() {
            provide(clients);
        }
    });
    use_context_provider(|| PanelHost {
        open_browser: Callback::new(move |()| {
            open_view(owner, "studio.rig.browser", "browser".to_owned());
        }),
        open_setup: Callback::new(move |tab: &'static str| {
            let view = if tab == "guitar" { "instrument" } else { tab };
            open_view(owner, "studio.audio-settings", view.to_owned());
        }),
    });
    if try_use_context::<Bare>().is_some_and(|bare| bare.0) {
        return rsx! {
            div { style: "position:relative; flex:1; min-width:0; min-height:0; display:flex;", {children} }
        };
    }
    rsx! {
        guitar::ui::panels::RigProviders {
            Shared {}
            {children}
        }
    }
}

/// Puts a connection's clients in context for the views below.
fn provide(clients: Clients) {
    let _ = provide_context(clients.rig.clone());
    let _ = provide_context(clients.stream.clone());
    let _ = provide_context(clients.settings.clone());
    let _ = provide_context(signal_rig_perform_ui::PerformClients {
        client: clients.perform.clone(),
        stream: clients.perform_stream.clone(),
    });
    if let Some(tones) = clients.tones.clone() {
        let _ = provide_context(tones);
    }
    if let Some(stream) = clients.tones_stream.clone() {
        let _ = provide_context(stream);
    }
    if let Some(keys) = clients.keys.clone() {
        let _ = provide_context(keys);
    }
    if let Some(stream) = clients.keys_stream.clone() {
        let _ = provide_context(stream);
    }
    let _ = provide_context(signal_tone3000::ui::UrlOpener::new(open_externally));
    let _ = provide_context(clients);
}

/// Opens `url` in the person's browser (TONE3000's sign-in).
fn open_externally(url: String) {
    if url.is_empty() {
        return;
    }
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(error) = std::process::Command::new(program).arg(&url).spawn() {
        tracing::warn!(%error, program, "the system browser did not open");
    }
}

/// Keeps this pane's shared signals (the routing's selection, the
/// browser's focus, the part being built) in step with the process's
/// [`bus`]: a change here goes there, a change there comes here.
#[component]
fn Shared() -> Element {
    let mut selection = use_context::<RoutingSel>().0;
    let mut focus = use_context::<BrowserFocus>().0;
    let mut part = use_context::<BuildPick>().part;
    // Seeded from the bus: a pane opened after a selection shows it.
    use_hook(|| {
        let now = bus::get();
        selection.set(now.selection);
        part.set(now.part);
    });
    use_effect(move || bus::put(|b| b.selection = selection()));
    use_effect(move || {
        // A focus is an ask, taken once: it goes to the bus and on to
        // every pane, whose browser opens on it.
        if let Some(asked) = focus() {
            bus::put(|b| b.focus = Some(asked));
        }
    });
    use_effect(move || bus::put(|b| b.part = part()));
    use_future(move || async move {
        let mut changes = bus::subscribe();
        loop {
            if changes.changed().await.is_err() {
                return;
            }
            let now = bus::get();
            if *selection.peek() != now.selection {
                selection.set(now.selection);
            }
            if now.focus.is_some() && *focus.peek() != now.focus {
                focus.set(now.focus);
            }
            if *part.peek() != now.part {
                part.set(now.part);
            }
        }
    });
    rsx! {}
}
