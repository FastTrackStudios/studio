//! The window's view actions both builds carry
//! (`docs/design/actions-everywhere.md`, rule 7): window state — the
//! arrangement's editing options, the mixer's racks, rails, scroll and
//! folds, the FX view's, a close-up — that a control, a
//! key or a model sets by name. The desktop registers them with the rest
//! of the window's actions (`crate::window::register`), the page on its own
//! list (`studio_engine::web_actions::extend`).

use architect::action::{ActionList, ActionMeta, Effect};
use facet::Facet;

use daw_view::actions::meta;

pub static OPTION: ActionMeta = meta(
    "SESSION_OPTION",
    "option",
    "Editing option",
    "Turn one of the arrangement's editing options on or off — `option`: `snap` (to the grid), `grid` (its lines), `ripple` (later items follow an edit, per track), `razor` (the razor tool), `auto_crossfade` (overlapping items crossfade), `lock` (drags scroll and move nothing), `tempo_lane` (the ruler's TEMPO lane on a touchscreen's slim ruler, where the tempo and meter are set; a full ruler always has it); `on` true or false, the other way when left out. The main toolbar's switches. Says whether it is on now.",
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

/// What view actions asked, numbered, for every surface that shows that
/// view to take up on its next frame ([`since`]): the actions run where
/// the window's state is not (the CLI, a model, the page's list in the
/// background), so they leave it here.
pub struct Asks<T> {
    next: u64,
    kept: std::collections::VecDeque<(u64, T)>,
}

impl<T> Asks<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next: 1,
            kept: std::collections::VecDeque::new(),
        }
    }
}

/// How many asks are kept for a surface that has not looked yet.
const KEPT: usize = 64;

pub fn post<T>(asks: &std::sync::Mutex<Asks<T>>, ask: T) {
    let mut asks = asks
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let number = asks.next;
    asks.next = asks.next.saturating_add(1);
    asks.kept.push_back((number, ask));
    while asks.kept.len() > KEPT {
        asks.kept.pop_front();
    }
}

/// The asks after `seen`, in order; `seen` moved past them.
pub fn since<T: Clone>(asks: &std::sync::Mutex<Asks<T>>, seen: &mut u64) -> Vec<T> {
    let asks = asks
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let out = asks
        .kept
        .iter()
        .filter(|(n, _)| *n > *seen)
        .map(|(_, a)| a.clone())
        .collect();
    *seen = asks.next.saturating_sub(1);
    out
}

/// Whether there are asks after `seen`.
pub fn pending<T>(asks: &std::sync::Mutex<Asks<T>>, seen: u64) -> bool {
    asks.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .next
        .saturating_sub(1)
        > seen
}

/// Put the view actions on `list`.
///
/// # Errors
/// The first registration the list refuses (a duplicate id).
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&OPTION, Effect::Mutating, |a: OptionArg| {
        if a.option == "tempo_lane" {
            let on = a.on.unwrap_or_else(|| !crate::ruler_layout::tempo_shown());
            crate::ruler_layout::show_tempo(on);
            return Ok(format!("tempo_lane: {}", if on { "on" } else { "off" }));
        }
        let (name, option) = crate::options::named(&a.option).ok_or_else(|| {
            format!(
                "no option {:?}: snap, grid, ripple, razor, auto_crossfade, lock or tempo_lane (grouping is SESSION_GROUPING)",
                a.option
            )
        })?;
        let on = a.on.unwrap_or_else(|| !option.get());
        option.set(on);
        Ok::<_, String>(format!("{name}: {}", if on { "on" } else { "off" }))
    })?;
    // The mixer's own, when a mixer is installed (`crate::dock`).
    if let Some(mixer) = crate::dock::mixer_views() {
        (mixer.register)(list)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each surface takes every ask once, in order, however many look.
    #[test]
    fn every_surface_takes_each_ask_once() {
        static ASKS: std::sync::Mutex<Asks<u32>> = std::sync::Mutex::new(Asks::new());
        let (mut one, mut two) = (0, 0);
        post(&ASKS, 1);
        post(&ASKS, 2);
        assert!(pending(&ASKS, one));
        assert_eq!(since(&ASKS, &mut one), [1, 2]);
        assert!(!pending(&ASKS, one));
        post(&ASKS, 3);
        assert_eq!(since(&ASKS, &mut one), [3]);
        assert_eq!(since(&ASKS, &mut two), [1, 2, 3]);
    }
}
pub static MIXER_FX: ActionMeta = meta(
    "SESSION_MIXER_FX",
    "mixer_fx",
    "Mixer FX racks",
    "Show or hide the FX racks over the mixer's faders (`shown`: true or false; left out, the other way): hidden, the faders have the panel's whole height. Kept across launches.",
    "View",
    "",
);
