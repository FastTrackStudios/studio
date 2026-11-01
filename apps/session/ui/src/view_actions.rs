//! The window's view actions both builds carry
//! (`docs/design/actions-everywhere.md`, rule 7): window state — the
//! arrangement's editing options, the mixer's racks, rails, scroll and
//! folds, the FX view's, a close-up — that a control, a
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

pub static MIXER_VIEW: ActionMeta = meta(
    "MIXER_VIEW",
    "mixer_view",
    "Mixer view",
    "Set what the painted mixer shows — any of: `scene` (a scene's slug, as its rail names it), `focus_selected` (selecting a track opens it wide), `take_focus_width` (a focused track's width comes out of its neighbours), `audience` (`engineer` or `player`), `scroll` (how far along the strips, px), `rack_scroll` (how far down the racks, px), `rack_fold` (`{track, phase}`: that phase's section of the track's rack folded or opened, the other way). The mixer's rails, its scroll and its racks' folds; only what is given changes.",
    "View",
    "",
);

/// A rack's phase section to fold or open.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct RackFold {
    pub track: String,
    pub phase: String,
}

/// What `MIXER_VIEW` changes.
#[derive(Facet, Clone, Debug, Default, PartialEq)]
pub struct MixerView {
    #[facet(default)]
    pub scene: Option<String>,
    #[facet(default)]
    pub focus_selected: Option<bool>,
    #[facet(default)]
    pub take_focus_width: Option<bool>,
    #[facet(default)]
    pub audience: Option<String>,
    #[facet(default)]
    pub scroll: Option<f64>,
    #[facet(default)]
    pub rack_scroll: Option<f64>,
    #[facet(default)]
    pub rack_fold: Option<RackFold>,
}

pub static FX_VIEW: ActionMeta = meta(
    "FX_VIEW",
    "fx_view",
    "FX view",
    "Set what the FX view shows — any of: `fold` (`{track, group, solo?, every_lane?}`: a group of a lane — a phase slug, a rig section `input`/`pre`/`core`/`mod`/`time`, or `returns` — folded or opened, the other way; `solo` opens it alone, shutting the lane's others; `every_lane` does it on every lane in view), `scroll` (`{track, at}`: how far along that lane's chain, px), `down` (the first lane shown, by its place from the top). Only what is given changes.",
    "View",
    "",
);

/// A group of an FX lane to fold or open.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct FxFold {
    pub track: String,
    pub group: String,
    #[facet(default)]
    pub solo: bool,
    #[facet(default)]
    pub every_lane: bool,
}

/// An FX lane's scroll.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct FxScroll {
    pub track: String,
    pub at: f64,
}

/// What `FX_VIEW` changes.
#[derive(Facet, Clone, Debug, Default, PartialEq)]
pub struct FxView {
    #[facet(default)]
    pub fold: Option<FxFold>,
    #[facet(default)]
    pub scroll: Option<FxScroll>,
    #[facet(default)]
    pub down: Option<u32>,
}

pub static CLOSEUP: ActionMeta = meta(
    "SESSION_CLOSEUP",
    "closeup",
    "Close-up",
    "Show one panel of a track's rack over the whole window — `track` (its GUID), `panel` (`eq`, `comp`, `sat`, `gate`, `rescue_eq`, `reverb`, `delay`, …), `name` the title (the track's name) — or `close` it. A rack's zoom button, the close-up's tabs and its Back.",
    "View",
    "",
);

/// What `SESSION_CLOSEUP` shows, or that it closes.
#[derive(Facet, Clone, Debug, Default, PartialEq)]
pub struct CloseupArg {
    #[facet(default)]
    pub track: Option<String>,
    #[facet(default)]
    pub panel: Option<String>,
    #[facet(default)]
    pub name: Option<String>,
    #[facet(default)]
    pub close: bool,
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
    const fn new() -> Self {
        Self {
            next: 1,
            kept: std::collections::VecDeque::new(),
        }
    }
}

/// How many asks are kept for a surface that has not looked yet.
const KEPT: usize = 64;

fn post<T>(asks: &std::sync::Mutex<Asks<T>>, ask: T) {
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

/// `MIXER_VIEW`'s asks.
pub static MIXER_ASKS: std::sync::Mutex<Asks<MixerView>> = std::sync::Mutex::new(Asks::new());
/// `FX_VIEW`'s asks.
pub static FX_ASKS: std::sync::Mutex<Asks<FxView>> = std::sync::Mutex::new(Asks::new());
/// `SESSION_CLOSEUP`'s asks: the close-up to show (`None`: close).
pub static CLOSEUP_ASKS: std::sync::Mutex<Asks<Option<crate::closeup::Closeup>>> =
    std::sync::Mutex::new(Asks::new());

fn mixer_view(a: MixerView) -> Result<String, String> {
    if let Some(scene) = &a.scene
        && !dynamic_template::scenes::scenes()
            .iter()
            .any(|s| s.slug.as_str() == scene)
    {
        return Err(format!("no scene {scene:?}"));
    }
    if let Some(audience) = &a.audience
        && !["engineer", "player"].contains(&audience.to_ascii_lowercase().as_str())
    {
        return Err(format!(
            "`audience` is engineer or player, not {audience:?}"
        ));
    }
    if let Some(fold) = &a.rack_fold
        && session::mix_phases::MixPhase::from_slug(&fold.phase).is_none()
    {
        return Err(format!("no mix phase {:?}", fold.phase));
    }
    if [a.scroll, a.rack_scroll]
        .into_iter()
        .flatten()
        .any(|v| !v.is_finite())
    {
        return Err("a scroll is not a number".to_owned());
    }
    post(&MIXER_ASKS, a);
    Ok("the mixer shows it on its next frame".to_owned())
}

fn fx_view(a: FxView) -> Result<String, String> {
    if let Some(fold) = &a.fold
        && crate::fx_view::group_named(&fold.group).is_none()
    {
        return Err(format!(
            "no group {:?}: a phase slug, input, pre, core, mod, time or returns",
            fold.group
        ));
    }
    if a.scroll.as_ref().is_some_and(|s| !s.at.is_finite()) {
        return Err("`scroll.at` is not a number".to_owned());
    }
    post(&FX_ASKS, a);
    Ok("the FX view shows it on its next frame".to_owned())
}

fn closeup(a: CloseupArg) -> Result<String, String> {
    if a.close {
        post(&CLOSEUP_ASKS, None);
        return Ok("closed".to_owned());
    }
    let track = a.track.ok_or("`track` (its GUID), or `close`")?;
    let panel = a.panel.as_deref().unwrap_or("eq");
    let which = crate::tone::Which::from_slug(panel).ok_or_else(|| {
        let all: Vec<String> = crate::tone::Which::ALL.iter().map(|w| w.slug()).collect();
        format!("no panel {panel:?}: one of {all:?}")
    })?;
    let name = a.name.unwrap_or_else(|| track.clone());
    post(
        &CLOSEUP_ASKS,
        Some(crate::closeup::Closeup::Rack {
            guid: track,
            name,
            which,
        }),
    );
    Ok(format!("{} close up", which.slug()))
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
    list.register(&MIXER_VIEW, Effect::Mutating, mixer_view)?;
    list.register(&FX_VIEW, Effect::Mutating, fx_view)?;
    list.register(&CLOSEUP, Effect::Mutating, closeup)?;
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

    /// Names a model or the CLI gets wrong are refused, saying what fits.
    #[test]
    fn the_view_actions_refuse_what_they_cannot_show() {
        assert!(
            closeup(CloseupArg {
                track: Some("{t}".into()),
                panel: Some("comp".into()),
                ..CloseupArg::default()
            })
            .is_ok()
        );
        assert!(
            closeup(CloseupArg {
                track: Some("{t}".into()),
                panel: Some("kazoo".into()),
                ..CloseupArg::default()
            })
            .is_err()
        );
        assert!(closeup(CloseupArg::default()).is_err());
        assert!(
            fx_view(FxView {
                fold: Some(FxFold {
                    track: "{t}".into(),
                    group: "core".into(),
                    solo: false,
                    every_lane: false,
                }),
                ..FxView::default()
            })
            .is_ok()
        );
        assert!(
            fx_view(FxView {
                fold: Some(FxFold {
                    track: "{t}".into(),
                    group: "nowhere".into(),
                    solo: false,
                    every_lane: false,
                }),
                ..FxView::default()
            })
            .is_err()
        );
        assert!(
            mixer_view(MixerView {
                audience: Some("crowd".into()),
                ..MixerView::default()
            })
            .is_err()
        );
        assert!(
            mixer_view(MixerView {
                rack_fold: Some(RackFold {
                    track: "{t}".into(),
                    phase: "tone".into(),
                }),
                ..MixerView::default()
            })
            .is_ok()
        );
    }

    /// Every panel's name comes back as that panel.
    #[test]
    fn every_rack_panel_is_named() {
        for which in crate::tone::Which::ALL {
            assert_eq!(crate::tone::Which::from_slug(&which.slug()), Some(which));
        }
    }
}
