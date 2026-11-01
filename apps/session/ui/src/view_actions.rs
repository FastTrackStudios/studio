//! The window's view actions both builds carry
//! (`docs/design/actions-everywhere.md`, rule 7): window state — the
//! arrangement's editing options, the mixer's racks — that a control, a
//! key or a model sets by name. The desktop registers them with the rest
//! of the window's actions (`crate::window::register`), the page on its own
//! list (`session_engine::web_actions::extend`).

use architect::action::{ActionList, ActionMeta, Effect};
use facet::Facet;

use session_engine::shared_actions::meta;

pub static OPTION: ActionMeta = meta(
    "SESSION_OPTION",
    "option",
    "Editing option",
    "Turn one of the arrangement's editing options on or off — `option`: `snap` (to the grid), `grid` (its lines), `ripple` (later items follow an edit, per track), `razor` (the razor tool), `auto_crossfade` (overlapping items crossfade), `lock` (drags scroll and move nothing); `on` true or false, the other way when left out. The main toolbar's switches. Says whether it is on now.",
    "Options",
    "",
);

/// Which option, and on or off (left out: the other way).
#[derive(Facet)]
struct OptionArg {
    option: String,
    #[facet(default)]
    on: Option<bool>,
}

pub static MIXER_FX: ActionMeta = meta(
    "SESSION_MIXER_FX",
    "mixer_fx",
    "Mixer FX racks",
    "Show or hide the FX racks over the mixer's faders (`shown`: true or false; left out, the other way): hidden, the faders have the panel's whole height. Kept across launches.",
    "View",
    "",
);

/// Shown or hidden, or (left out) the other way.
#[derive(Facet)]
struct MixerFxArg {
    #[facet(default)]
    shown: Option<bool>,
}

/// Put the view actions on `list`.
///
/// # Errors
/// The first registration the list refuses (a duplicate id).
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&OPTION, Effect::Mutating, |a: OptionArg| {
        let (name, option) = crate::options::named(&a.option).ok_or_else(|| {
            format!(
                "no option {:?}: snap, grid, ripple, razor, auto_crossfade or lock (grouping is SESSION_GROUPING)",
                a.option
            )
        })?;
        let on = a.on.unwrap_or_else(|| !option.get());
        option.set(on);
        Ok::<_, String>(format!("{name}: {}", if on { "on" } else { "off" }))
    })?;
    list.register(&MIXER_FX, Effect::Mutating, |a: MixerFxArg| {
        let on = a
            .shown
            .unwrap_or_else(|| !crate::mixer_panel::racks_shown());
        crate::mixer_panel::set_racks_shown(on);
        Ok(if on {
            "the mixer's FX racks shown"
        } else {
            "the mixer's FX racks hidden"
        }
        .to_owned())
    })?;
    Ok(())
}
