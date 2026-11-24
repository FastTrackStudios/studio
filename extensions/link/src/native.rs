//! The native process: a Studio window attached to the engine the `daw`
//! extension runs, over its socket.

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use anyrender::recording::RenderCommand;
use architect_scene::paint::PaintMount;
use architect_scene::paint::{Layer, LayerKey, Scene};
use daw_view_kit::song_model::StudioSession;
use dioxus::prelude::*;
use serde_json::Value as Json;
use session_engine_client::studio_engine::link;

use crate::Stats;

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
    // On a phone the engine is in this process (the `daw` extension links
    // the views to it, `session_engine_client::open::link_window_here`): nothing
    // to find.
    if cfg!(target_os = "ios") {
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

/// The app attaches the views itself ([`attach_at`]): a view's own
/// [`attach`] does nothing from now on, so no view links to another
/// engine first.
pub fn attach_by_host() {
    ATTACHING.store(true, Ordering::Release);
}

/// Attach this process to the engine the app itself found (and checked:
/// Studio's engine discovery makes sure the pid owns its port), rather
/// than the newest that answers: an app that drives the engine over its
/// Action List views the same one. Kept linked after, as [`attach`] is.
///
/// # Errors
///
/// The engine's socket refused the link.
pub fn attach_at(pid: u32, socket: std::path::PathBuf, mcp: String) -> Result<(), String> {
    ATTACHING.store(true, Ordering::Release);
    let found = link::Found {
        pid,
        socket,
        mcp,
        ws: None,
        build: String::new(),
    };
    attach_to(&found)?;
    set_problem(None);
    link::keep_linked(link::Mode::Attach, attach_to);
    Ok(())
}

fn attach_to(found: &link::Found) -> Result<(), String> {
    let target = session_engine_client::open::RemoteTarget::Engine {
        socket: found.socket.clone(),
    };
    session_engine_client::open::set_mode(session_engine_client::open::ModeState::remote(
        target.clone(),
        false,
    ));
    session_engine_client::open::attach(&target).map_err(|e| e.to_string())?;
    link::set_linked(found.clone());
    Ok(())
}

/// The song the engine has current, as the window lays it out, with what
/// names it (a new song or a new set mounts a view again).
#[must_use]
pub fn current() -> Option<(String, StudioSession)> {
    session_engine_client::view_client::peek_set(|view| {
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

/// The mode this view shows: its host's (the client's, which every one
/// of its windows shares and `SESSION_MODE` sets), else one of its own from
/// [`mode_from_env`] for a view hosted alone. A view never makes its own
/// beside its host's: two would disagree about what is drawn and which
/// keys do what.
pub fn use_client_mode() -> dioxus::prelude::Signal<session::modes::Mode> {
    use dioxus::prelude::*;
    use_hook(|| {
        try_consume_context::<Signal<session::modes::Mode>>()
            .unwrap_or_else(|| provide_context(Signal::new(mode_from_env())))
    })
}

/// Whether the `daw` extension's value table carries the tracks' mute and
/// solo ([`session_engine_client::engine::track_value`]).
///
/// Then a pane draws their lit state from it, apart, and lights them in
/// the same frame as every other pane. One `stat` a call.
#[must_use]
pub fn engine_lights_buttons() -> bool {
    let Ok(name) =
        architect_scene::anim::Name::new(&session_engine_client::engine::track_value(0, "mute"))
    else {
        return false;
    };
    architect_scene::engine::shared::open_engine("daw").is_some_and(|t| t.slot(&name).is_some())
}

/// [`engine_lights_buttons`], for a pane that the host moves by engine
/// values: never when this window is compiled into its host app, which
/// composes a linked pane's layers itself and binds none of them to an
/// engine's values (they would all draw lit). The pane then draws its
/// buttons' state in its own picture.
#[must_use]
pub fn engine_lights_buttons_here() -> bool {
    engine_lights_buttons()
}

/// `mount` reading the engine's values, when this window is compiled into
/// its host app.
///
/// A process copy's layers are bound to the engine's table by the host
/// (`architect-extensions`' remote view). A linked pane's are composed in
/// the app's own document, which binds a table only when the mount brings
/// one: without it every part a value shows (a lit mute or solo, a meter)
/// draws as if its value were full. The table is the `daw` extension's
/// shared one, as the song has it now (a pane mounts again for another
/// song).
#[must_use]
pub fn with_engine_values(mount: PaintMount) -> PaintMount {
    if session_engine_client::actions::is_in_host()
        && let Some(table) = architect_scene::engine::shared::open_engine("daw")
    {
        return mount.with_engine(table);
    }
    mount
}

/// An action of the engine's Action List, run over this window's link.
///
/// # Errors
///
/// The engine refused it, or is not linked.
pub fn run(id: &str, args: &Json) -> Result<Json, String> {
    let out = session_engine_client::view_client::run(id, args.to_string().as_bytes())?;
    Ok(serde_json::from_str(&out).unwrap_or(Json::String(out)))
}

/// [`run`], awaited: the one shape a view's action takes on every target
/// (in a browser the page's list answers in its own time).
///
/// # Errors
///
/// As [`run`].
#[expect(
    clippy::unused_async,
    reason = "the browser's is awaited; one shape for both"
)]
pub async fn call(id: &str, args: &Json) -> Result<Json, String> {
    run(id, args)
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

/// A part's move as the host's animation of it.
#[must_use]
pub fn animation(m: &daw_view_kit::overlay::ValueMove) -> Option<architect_scene::anim::Animation> {
    use architect_scene::anim::{Animation, Input, Linear, Name, Property};
    use daw_view_kit::overlay::ValueProperty;
    let name = Name::new(&m.name).ok()?;
    let property = match m.property {
        ValueProperty::Y => Property::Y,
        ValueProperty::Opacity => Property::Opacity,
        ValueProperty::Visible => Property::Visible,
    };
    let mut linear = Linear::new(m.scale, m.offset);
    linear.min = m.min;
    linear.max = m.max;
    Some(Animation::linear(property, Input::Value(name), linear))
}

/// Parts an engine's values move ([`daw_view_kit::overlay::ValueLayer`]) as a
/// pane's layers, keyed from `first`: a part drawn as last frame keeps its
/// content, so nothing of it crosses.
#[derive(Default)]
pub struct ValueParts {
    sent: std::collections::HashMap<LayerKey, Arc<Vec<RenderCommand>>>,
}

impl ValueParts {
    /// `parts` as layers keyed `first`, `first + 1`, …, each at `z` (or
    /// `over_z` for one over the caps), placed by `transform` unless the
    /// part has its own.
    pub fn layers(
        &mut self,
        parts: &[daw_view_kit::overlay::ValueLayer],
        first: LayerKey,
        (z, over_z): (i32, i32),
        transform: architect_scene::paint::peniko::kurbo::Affine,
    ) -> Vec<(LayerKey, Layer)> {
        let mut out = Vec::with_capacity(parts.len());
        for (i, part) in parts.iter().enumerate() {
            let key = LayerKey::try_from(i)
                .ok()
                .and_then(|i| first.checked_add(i))
                .unwrap_or(LayerKey::MAX);
            let content = match self.sent.get(&key) {
                Some(was) if was.as_slice() == part.scene.commands.as_slice() => Arc::clone(was),
                _ => {
                    let now = Arc::new(part.scene.commands.clone());
                    self.sent.insert(key, Arc::clone(&now));
                    now
                }
            };
            let mut layer = Layer::new(content)
                .animated(part.moves.iter().filter_map(animation).collect::<Vec<_>>());
            let at = part.transform.unwrap_or(transform);
            layer.transform = at;
            layer.clip = part.clip.map(|clip| at.transform_rect_bbox(clip));
            layer.z = if part.over { over_z } else { z };
            out.push((key, layer));
        }
        let keep: Vec<LayerKey> = out.iter().map(|(k, _)| *k).collect();
        self.sent.retain(|key, _| keep.contains(key));
        out
    }
}

/// A pane's size in whole logical pixels.
#[must_use]
pub const fn pane_size(ctx: &architect_scene::view::ViewCtx) -> (u32, u32) {
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
