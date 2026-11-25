//! Studio, as the views' kit asks of the app a window is in
//! (`daw_view_kit::app`, `daw_view_kit::keys`): its key bindings, its
//! dock, the mode it shows.

use daw_view_kit::app::{App, DockAsk};
use session_engine_client::actions::{self, DockRequest};

/// Hand the views Studio's answers: once per process, at start.
pub fn install() {
    crate::key_profile::install();
    // The Editor's keys and wheel, and daw-ui's profiles: Studio's keymaps.
    editor::paint::keymap::install(editor::paint::keymap::Keymaps {
        defaults: || keybinds::load_default_config().unwrap_or_default(),
        user: user_keymap,
    });
    daw_ui::keymaps::install(daw_ui::keymaps::Profiles {
        list: || {
            keybinds::embedded::list()
                .into_iter()
                .map(|(slug, cfg)| daw_ui::keymaps::ProfileInfo {
                    slug,
                    name: cfg.name,
                    description: cfg.description,
                })
                .collect()
        },
        keymap: |slug| keybinds::embedded::profile(slug).map(|p| p.keymap),
    });
    #[cfg(feature = "native")]
    if let Err(error) = register(daw_view_kit::actions::list()) {
        tracing::error!(%error, "studio: the views' actions stopped registering");
    }
    daw_view_kit::app::install(App {
        docked: actions::docked,
        dock,
        shown,
    });
    mixer_view::app::install(mixer_view::app::App {
        modes,
        scenes,
        scene,
        phase,
        choose_phase,
        is_pair_half: dynamic_template::scenes::is_pair_half,
    });
}

/// Ask Studio's dock for what the window asked.
fn dock(ask: DockAsk) {
    actions::request_dock(match ask {
        DockAsk::TogglePanel(kind) => DockRequest::TogglePanel(kind.to_owned()),
        DockAsk::ToggleDocks => DockRequest::ToggleDocks,
    });
}

/// The mode Studio's shell shows, and its phase.
fn shown() -> (Option<String>, Option<String>) {
    let mode = actions::shown_mode();
    (
        mode.map(|m| m.display_name().to_owned()),
        mode.and_then(session::mode_phases::current)
            .map(|p| p.slug.to_owned()),
    )
}

/// Gain stage the open song, or clear it (`{"clear": true}`): what the
/// arrangement's Shift+G and Alt+G run. Says what it did.
#[cfg(feature = "native")]
static GAIN_STAGE_RUN: architect::action::ActionMeta = daw_view_kit::actions::meta(
    "SESSION_GAIN_STAGE_RUN",
    "gain_stage_run",
    "Gain stage the song",
    "Set every recorded track's item volume to the reference kit's levels (`clear`: every staged item back at 1). Says what it did.",
    "Mix",
    "",
);

/// What `SESSION_GAIN_STAGE_RUN` takes.
#[cfg(feature = "native")]
#[derive(facet::Facet, Default)]
struct StageArg {
    #[facet(default)]
    clear: bool,
}

/// Studio's own actions the views run by id.
#[cfg(feature = "native")]
fn register(list: &architect::action::ActionList) -> Result<(), architect::action::ActionError> {
    list.register(
        &GAIN_STAGE_RUN,
        architect::action::Effect::Mutating,
        |a: StageArg| {
            daw_view_kit::open::block_on_engine(gain_stage(a.clear))
                .ok_or_else(|| "no engine runtime".to_owned())?
        },
    )
}

/// Gain stage (or clear) the open song through the facade.
#[cfg(feature = "native")]
async fn gain_stage(clear: bool) -> Result<String, String> {
    let daw = daw::rpc::Daw::try_get().ok_or("the daw facade is not up")?;
    let project = daw.current_project().await.map_err(|e| e.to_string())?;
    if clear {
        let cleared = session_engine_client::gain_stage::clear_live(&project)
            .await
            .map_err(|e| e.to_string())?;
        tracing::info!(stage.cleared = cleared.len(), "gain staging cleared");
        return Ok("gain staging cleared".to_owned());
    }
    let report = session_engine_client::gain_stage::stage_live(&project)
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!(
        stage.staged = report.staged.len(),
        stage.silent = report.silent.len(),
        stage.silent_tracks = %report.silent.join(", "),
        stage.levels = %report
            .staged
            .iter()
            .map(|s| format!("{} {:.1}→{:+.1}", s.name, s.level, s.gain_db))
            .collect::<Vec<_>>()
            .join(", "),
        "gain staged"
    );
    Ok("gain staged".to_owned())
}

/// Studio's modes, each its slug and name.
fn modes() -> &'static [(&'static str, &'static str)] {
    static MODES: std::sync::OnceLock<Vec<(&'static str, &'static str)>> =
        std::sync::OnceLock::new();
    MODES.get_or_init(|| {
        session::modes::Mode::ALL
            .iter()
            .map(|m| (m.slug(), m.display_name()))
            .collect()
    })
}

/// A template scene, as the mixer's rails offer it.
fn rail_scene(s: &'static dynamic_template::scenes::Scene) -> mixer_view::app::Scene {
    mixer_view::app::Scene {
        slug: s.slug.as_str(),
        short: s.short.as_str(),
    }
}

/// The template's scenes `mode` offers `instrument`.
fn scenes(mode: &str, instrument: &str) -> Vec<mixer_view::app::Scene> {
    dynamic_template::scenes::follow::in_mode(dynamic_template::scenes::scenes(), mode, instrument)
        .into_iter()
        .map(rail_scene)
        .collect()
}

/// The template's scene `slug`.
fn scene(slug: &str) -> Option<mixer_view::app::Scene> {
    dynamic_template::scenes::scene(slug).map(rail_scene)
}

/// Mix mode's phase, as Studio's shell last chose it, and its counter.
fn phase() -> (Option<&'static str>, u64) {
    (
        session::mode_phases::current(session::modes::Mode::Mix).map(|p| p.slug),
        session::mode_phases::version(),
    )
}

/// Choose Mix mode's phase.
fn choose_phase(slug: &str) {
    session::mode_phases::choose(session::modes::Mode::Mix, slug);
}

/// The user's own keymap file, natively (a page reads no files).
fn user_keymap() -> Option<architect_input::KeymapConfig> {
    #[cfg(not(target_arch = "wasm32"))]
    return keybinds::load_user_config().ok().flatten();
    #[cfg(target_arch = "wasm32")]
    None
}
