//! What a Studio view extension's process is: a Studio window on Studio's
//! engine.
//!
//! It attaches to the engine the `daw` extension runs exactly as
//! `studio-desktop`'s window attaches to a `studio engine`
//! (`session_engine::studio_engine::link`, `session_engine::open::attach`)
//! ([`attach`]), reads the song through the window protocol's replicas
//! (`session_engine::view_client`) ([`use_song`]), runs the engine's
//! actions over that link ([`run`]), and hands Architect's pane input on
//! as the Blitz events Studio's widgets take ([`input`]). A view's pane
//! paints its widget as one layer that crosses only when it changed
//! ([`SceneLayer`]).
//!
//! `studio.arrangement` and `studio.mixer` both build on it; neither
//! needs the other installed.

pub mod input;

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use anyrender::recording::RenderCommand;
use architect_workspace::paint::{Layer, LayerKey, Scene};
use dioxus::prelude::*;
use serde_json::Value as Json;
use session_engine::studio_engine::link;
use session_ui::song_model::StudioSession;

/// How long an attach waits for an engine before it says so (it keeps
/// trying after).
const WAIT: std::time::Duration = std::time::Duration::from_secs(5);

static ATTACHING: AtomicBool = AtomicBool::new(false);
static PROBLEM: Mutex<Option<String>> = Mutex::new(None);

fn set_problem(why: Option<String>) {
    *PROBLEM.lock().unwrap_or_else(PoisonError::into_inner) = why;
}

/// Why there is no song yet, when something went wrong.
#[must_use]
pub fn problem() -> Option<String> {
    PROBLEM
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Attach this process, a Studio window, to the engine.
///
/// What `studio-desktop`'s window does with `FTS_ENGINE=attach`
/// (`session_desktop::native::attach_engine`). Once per process, on a
/// thread of its own; it keeps the link after (an engine that restarts is
/// attached to again).
pub fn attach() {
    if ATTACHING.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("studio-attach".to_owned())
        .spawn(|| {
            loop {
                match link::choose(link::Mode::Attach, WAIT).and_then(|found| attach_to(&found)) {
                    Ok(()) => {
                        set_problem(None);
                        link::keep_linked(link::Mode::Attach, attach_to);
                        return;
                    }
                    Err(e) => set_problem(Some(format!("waiting for Studio's engine: {e}"))),
                }
            }
        });
}

fn attach_to(found: &link::Found) -> Result<(), String> {
    let target = session_engine::open::RemoteTarget::Engine {
        socket: found.socket.clone(),
    };
    session_engine::open::set_mode(session_engine::open::ModeState::remote(
        target.clone(),
        false,
    ));
    session_engine::open::attach(&target).map_err(|e| e.to_string())?;
    link::set_linked(found.clone());
    Ok(())
}

/// The song the engine has current, as the window lays it out, with what
/// names it (a new song or a new set mounts a view again).
#[must_use]
pub fn current() -> Option<(String, StudioSession)> {
    session_engine::view_client::peek_set(|view| {
        let set = view.setlist.as_ref()?;
        let song = set.songs.get(set.at)?;
        Some((
            format!("{}:{}", view.epoch, song.project),
            song.session.clone(),
        ))
    })
}

/// The song, as a signal a view renders from: attached on first use,
/// looked at a few times a second, changed only when another song or set
/// is current.
pub fn use_song() -> Signal<Option<(String, StudioSession)>> {
    attach();
    let mut song = use_signal(current);
    use_future(move || async move {
        loop {
            futures_timer::Delay::new(std::time::Duration::from_millis(200)).await;
            let now = current();
            if now.as_ref().map(|(k, _)| k) != song.peek().as_ref().map(|(k, _)| k) {
                song.set(now);
            }
        }
    });
    song
}

/// What a pane shows until there is a song: what it waits for.
///
/// # Errors
///
/// None: it renders.
pub fn waiting() -> Element {
    let surface = surface();
    let why = problem().unwrap_or_else(|| "Waiting for Studio's engine to open a song…".to_owned());
    rsx! {
        div { style: "padding:16px; font-size:13px; opacity:0.7; color:#ccc; background:{surface}; height:100%;",
            "{why}"
        }
    }
}

/// The panels' background, Studio's dark theme's (the session window is
/// dark only).
#[must_use]
pub fn surface() -> String {
    daw_ui::studio::lanes::Colors::from_theme(&daw_ui::theming::Theme::dark()).surface
}

/// The class a view's root takes for Studio's box sizing.
///
/// Studio's window sizes every box by its border (`*{box-sizing:
/// border-box}`, `studio-desktop`'s root style). A view's root takes this
/// class, and [`studio_style`] gives it the rule: the view's own, not the
/// Architect window's around it.
pub const ROOT_CLASS: &str = "studio-view";

/// The rule [`ROOT_CLASS`] names, to render once in a view.
///
/// A `style` element in the view's own tree. A process view is a Blitz
/// sub-document
/// of the host's (`architect_extensions::remote`), which takes a `style`
/// element as a sheet; `document::Style` goes to a document provider the
/// guest has none of.
///
/// # Errors
///
/// None: it renders.
pub fn studio_style() -> Element {
    rsx! {
        style { {".studio-view, .studio-view * { box-sizing: border-box; }"} }
    }
}

/// The window's mode as Studio's shell has it: `FTS_SESSION_MODE`, else
/// Live (`session_desktop::native::shell`).
#[must_use]
pub fn mode_from_env() -> session::modes::Mode {
    std::env::var("FTS_SESSION_MODE")
        .ok()
        .and_then(|name| {
            session::modes::Mode::ALL
                .into_iter()
                .find(|m| m.display_name().eq_ignore_ascii_case(&name))
        })
        .unwrap_or(session::modes::Mode::Live)
}

/// The window actions the mixer pane carries out (`studio.mixer` claims
/// them from the engine; a window showing the arrangement hands them
/// over): what `studio-desktop`'s one window runs itself.
pub const MIXER_ACTIONS: [&str; 3] = ["MIXER_VIEW", "SESSION_MIXER_FX", "SESSION_TOGGLE_MIXER"];

/// An action of the engine's Action List, run over this window's link.
///
/// # Errors
///
/// The engine refused it, or is not linked.
pub fn run(id: &str, args: &Json) -> Result<Json, String> {
    let out = session_engine::view_client::run(id, args.to_string().as_bytes())?;
    Ok(serde_json::from_str(&out).unwrap_or(Json::String(out)))
}

/// What a pane's paint view needs to run a panel's code inside the view's
/// Dioxus runtime (its signals are set there, as on the other hosts).
#[derive(Clone)]
pub struct InScope {
    pub runtime: Rc<dioxus::core::Runtime>,
    pub scope: dioxus::core::ScopeId,
}

impl InScope {
    /// The scope this is called in (from a component).
    #[must_use]
    pub fn here() -> Self {
        Self {
            runtime: dioxus::core::Runtime::current(),
            scope: dioxus::core::current_scope_id(),
        }
    }

    pub fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        self.runtime.in_scope(self.scope, f)
    }
}

/// How a pane has been painting.
#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub frames: u64,
    /// Frames whose scene differed from the last (and so crossed).
    pub changed: u64,
    pub size: (u32, u32),
    pub commands: usize,
    /// The view's work a frame (the panel's frame and the widget's
    /// paint), the latest few hundred, in milliseconds.
    pub work_ms: std::collections::VecDeque<f64>,
}

impl Stats {
    /// As an action answers it.
    #[must_use]
    pub fn json(&self) -> Json {
        let pct = |p: usize| {
            let mut s: Vec<f64> = self.work_ms.iter().copied().collect();
            s.sort_by(f64::total_cmp);
            s.get(
                s.len()
                    .saturating_sub(1)
                    .saturating_mul(p)
                    .checked_div(100)
                    .unwrap_or(0),
            )
            .copied()
            .unwrap_or(0.0)
        };
        serde_json::json!({
            "count": self.frames,
            "changed": self.changed,
            "size": [self.size.0, self.size.1],
            "commands": self.commands,
            "work_p50_ms": pct(50),
            "work_p99_ms": pct(99),
        })
    }
}

/// A widget's scene as a pane's one layer: a frame that paints the same
/// scene keeps the layer's content, so nothing crosses (the web host's
/// rule).
pub struct SceneLayer {
    key: LayerKey,
    last: Option<Scene>,
    content: Arc<Vec<RenderCommand>>,
    stats: Arc<Mutex<Stats>>,
}

impl SceneLayer {
    #[must_use]
    pub fn new(key: LayerKey, stats: Arc<Mutex<Stats>>) -> Self {
        Self {
            key,
            last: None,
            content: Arc::new(Vec::new()),
            stats,
        }
    }

    /// The scene last painted: the widget's own picture, before it
    /// crossed.
    #[must_use]
    pub const fn last(&self) -> Option<&Scene> {
        self.last.as_ref()
    }

    /// The layer for `scene`, painted at `size` after `began`.
    pub fn frame(
        &mut self,
        scene: Scene,
        size: (u32, u32),
        began: web_time::Instant,
    ) -> Vec<(LayerKey, Layer)> {
        let changed = self.last.as_ref() != Some(&scene);
        let commands = scene.commands.len();
        if changed {
            self.content = Arc::new(scene.commands.clone());
            self.last = Some(scene);
        }
        let work = began.elapsed().as_secs_f64() * 1000.0;
        {
            let mut stats = self.stats.lock().unwrap_or_else(PoisonError::into_inner);
            stats.frames = stats.frames.saturating_add(1);
            stats.size = size;
            if changed {
                stats.changed = stats.changed.saturating_add(1);
                stats.commands = commands;
            }
            stats.work_ms.push_back(work);
            if stats.work_ms.len() > 600 {
                stats.work_ms.pop_front();
            }
        }
        vec![(self.key, Layer::new(Arc::clone(&self.content)))]
    }
}

/// A pane's size in whole logical pixels.
#[must_use]
pub const fn pane_size(ctx: &architect_workspace::ViewCtx) -> (u32, u32) {
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a pane's size in logical pixels"
    )]
    (
        ctx.geometry.width.max(1.0) as u32,
        ctx.geometry.height.max(1.0) as u32,
    )
}
