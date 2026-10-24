//! Follow-mode: scenes are the visibility manager, and the visibility
//! manager follows the DAW mode.
//!
//! `flow.scenes.follow-mode`. Entering **Record** shows the instrument's
//! tracking scene, **Edit** its editing scene, **Mix** its mixing scene,
//! without a second choice being made. The number keys still recall
//! inside a mode, and a scene chosen by hand stays until the mode
//! changes. Modes with no default — Organize, Write, Produce, Master,
//! Live, Video, Scoring — leave the current scene alone, because there
//! is no such thing as "the scoring view of a kit".

use super::types::{Audience, Scene};

/// The instrument a window falls back to when nothing else says.
///
/// The golden session's first group, which is also the instrument a
/// session is built around most often.
pub const DEFAULT_INSTRUMENT: &str = "drums";

/// Two scenes claiming one (mode, instrument, audience) triple.
///
/// Not a warning: a triple with two answers has no default, and a window
/// that silently picked one of them would be a window whose behaviour
/// depended on table order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The mode slug both scenes claim.
    pub mode: String,
    /// The instrument both claim.
    pub instrument: String,
    /// The audience both claim.
    pub audience: Audience,
    /// The slugs that claim it, in table order.
    pub slugs: Vec<String>,
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "({}, {}, {}) is claimed by {}",
            self.mode,
            self.instrument,
            self.audience.as_str(),
            self.slugs.join(" and ")
        )
    }
}

/// Every triple claimed by more than one scene.
///
/// Empty is the only acceptable answer; the table's own test asserts it.
#[must_use]
pub fn conflicts(scenes: &[Scene]) -> Vec<Conflict> {
    let mut claims: std::collections::BTreeMap<(String, String, Audience), Vec<String>> =
        std::collections::BTreeMap::new();
    for scene in scenes {
        for mode in &scene.modes {
            claims
                .entry((mode.clone(), scene.instrument.clone(), scene.audience))
                .or_default()
                .push(scene.slug.clone());
        }
    }
    claims
        .into_iter()
        .filter(|(_, slugs)| slugs.len() > 1)
        .map(|((mode, instrument, audience), slugs)| Conflict {
            mode,
            instrument,
            audience,
            slugs,
        })
        .collect()
}

/// The default scene for a triple: the one scene declaring all three.
#[must_use]
pub fn default_for<'a>(
    scenes: &'a [Scene],
    mode: &str,
    instrument: &str,
    audience: Audience,
) -> Option<&'a Scene> {
    scenes.iter().find(|s| {
        s.audience == audience
            && s.instrument.eq_ignore_ascii_case(instrument)
            && s.modes.iter().any(|m| m.eq_ignore_ascii_case(mode))
    })
}

/// The scenes the number keys reach: this instrument's, inside this
/// mode, plus the recall-only ones — which declare no mode and are
/// therefore available in all of them.
///
/// **Scoped to the instrument**, and that is not an optimisation. With
/// four instrument sets in the table there are more scenes declaring
/// "record" than there are number keys, and reaching the bass's
/// tracking view while working on the kit is not something anyone
/// wants — the keys are for moving around the instrument in front of
/// you. Picking an instrument is what the rail and the selection do.
///
/// The bus scenes are the exception: the mix tree belongs to no
/// instrument and is reachable from all of them.
#[must_use]
pub fn in_mode<'a>(scenes: &'a [Scene], mode: &str, instrument: &str) -> Vec<&'a Scene> {
    scenes
        .iter()
        .filter(|s| s.instrument == instrument || s.instrument == BUS_INSTRUMENT)
        .filter(|s| s.modes.is_empty() || s.modes.iter().any(|m| m.eq_ignore_ascii_case(mode)))
        .collect()
}

/// The instrument the bus scenes claim: not an instrument, so they are
/// reachable whatever one is in front of you.
const BUS_INSTRUMENT: &str = "bus";

/// Which instrument a window is looking at.
///
/// The selected track's, else the shown scene's, else the default. Three
/// answers rather than one because the question has to have an answer
/// before anything is selected and before any scene is shown.
#[must_use]
pub fn instrument_for(selected: Option<&str>, shown: Option<&Scene>) -> String {
    selected
        .map(str::to_owned)
        .or_else(|| shown.map(|s| s.instrument.clone()))
        .unwrap_or_else(|| DEFAULT_INSTRUMENT.to_owned())
}

/// Which instrument a track belongs to, from its resolved taxonomy path.
///
/// The top of the path — `Drums/Drum Kit/Kick` is the drums — as the
/// word a scene's `instrument` uses. `None` for a track the template
/// never placed, which is what makes [`instrument_for`] fall through to
/// the shown scene rather than guess.
///
/// Here rather than in each caller because there are two of them — the
/// window and the REAPER applier — and an instrument that meant one
/// thing in the window and another in REAPER would put the two surfaces
/// in different scenes.
#[must_use]
pub fn instrument_of(facts: &[super::facts::Fact], guid: &str) -> Option<String> {
    let top = facts
        .iter()
        .find(|fact| fact.guid == guid)?
        .path
        .first()?
        .name
        .to_ascii_lowercase();
    // The template's group names are plural collections; a scene names
    // the instrument. Only the two that differ need saying.
    Some(match top.as_str() {
        "guitars" => "guitar".to_owned(),
        "vocals" => "vocal".to_owned(),
        _ => top,
    })
}

/// A window's scene state: which scene is shown, and why.
///
/// Small on purpose — this is the whole of follow-mode's memory, and
/// every transition in the spec is a method on it. The window owns one
/// and asks it questions; it never works the rules out itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Follow {
    /// Who the window is for. `flow.scenes.two-audiences`.
    audience: Audience,
    /// The mode the window is in.
    mode: Option<String>,
    /// The scene being shown.
    shown: Option<String>,
    /// Whether the shown scene was chosen by hand — a hand-chosen scene
    /// stays until the mode changes.
    by_hand: bool,
}

impl Follow {
    /// A window for an audience, in no mode, showing no scene.
    #[must_use]
    pub fn new(audience: Audience) -> Self {
        Self {
            audience,
            ..Self::default()
        }
    }

    /// Who this window is for.
    #[must_use]
    pub const fn audience(&self) -> Audience {
        self.audience
    }

    /// Show the window to the other audience, and re-follow the mode:
    /// the player's overview of a kit is a different scene, not the same
    /// scene drawn differently.
    pub fn set_audience(&mut self, scenes: &[Scene], audience: Audience, instrument: &str) {
        self.audience = audience;
        if let Some(mode) = self.mode.clone() {
            self.enter(scenes, &mode, instrument);
        }
    }

    /// The scene being shown, if any.
    #[must_use]
    pub fn shown(&self) -> Option<&str> {
        self.shown.as_deref()
    }

    /// Enter a mode.
    ///
    /// The mode's default for this instrument and audience becomes the
    /// shown scene, and the hand-chosen flag is cleared — that is what
    /// "a scene chosen by hand stays until the mode changes" means. A
    /// mode with no default leaves the shown scene alone.
    // r[impl flow.scenes.follow-mode]
    pub fn enter(&mut self, scenes: &[Scene], mode: &str, instrument: &str) -> Option<&str> {
        let changed = self.mode.as_deref() != Some(mode);
        self.mode = Some(mode.to_owned());
        if changed {
            self.by_hand = false;
        }
        if !self.by_hand {
            if let Some(scene) = default_for(scenes, mode, instrument, self.audience) {
                self.shown = Some(scene.slug.clone());
            }
        }
        self.shown()
    }

    /// Recall the `digit`-th scene of the current mode, 1-based.
    ///
    /// A hand choice: it survives every redraw and every reselection,
    /// and ends at the next mode change. `0` — or any digit past the
    /// end — clears the scene rather than doing nothing, so there is a
    /// key that means "back to the mode's own answer".
    // r[impl flow.scenes.follow-mode]
    pub fn recall(&mut self, scenes: &[Scene], digit: u32, instrument: &str) -> Option<&str> {
        let available = in_mode(scenes, self.mode.as_deref().unwrap_or_default(), instrument);
        let picked = digit
            .checked_sub(1)
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| available.get(i))
            .map(|scene| scene.slug.clone());
        self.by_hand = picked.is_some();
        self.shown = picked;
        self.shown()
    }

    /// Show a named scene by hand — what a rail button does.
    ///
    /// The same hand choice a number key makes, said by slug rather than
    /// by position, because a rail button knows which scene it is and
    /// asking it to count to its own index is a way to get zero. A slug
    /// nothing answers to leaves the shown scene alone: a button that
    /// named a scene the table has never heard of should do nothing, not
    /// blank the window.
    // r[impl flow.scenes.follow-mode]
    pub fn choose(&mut self, scenes: &[Scene], slug: &str) -> Option<&str> {
        if scenes.iter().any(|scene| scene.slug == slug) {
            self.shown = Some(slug.to_owned());
            self.by_hand = true;
        }
        self.shown()
    }

    /// Stop showing a scene: back to the session as the project has it.
    pub fn clear(&mut self) {
        self.shown = None;
        self.by_hand = false;
    }
}
