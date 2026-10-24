//! Mix mode mutes the guide.
//!
//! The guide folder — click, count-in, cues, the prints — is for playing
//! to, not for mixing: going into Mix mode mutes a song's top-level
//! `Guide` folder, and coming out unmutes it again if Mix was what muted
//! it and it is still muted. A guide already muted is left muted, and one
//! unmuted while mixing stays as it was put.
//!
//! The song on screen, as it is shown: a song gone to in Mix mode is muted
//! then, and comes back when it is next shown in another mode.

use dioxus::prelude::*;
use session::modes::Mode;
use std::collections::HashMap;

/// One song's time in Mix mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Stint {
    /// The song was last shown in Mix mode.
    mixing: bool,
    /// Mix mode muted its guide (so leaving unmutes it).
    muted_it: bool,
}

impl Stint {
    /// The song shown in a mode (`mixing` or not) with its guide muted or
    /// not: whether to toggle the guide's mute.
    const fn shown(&mut self, mixing: bool, guide_muted: bool) -> bool {
        if mixing && !self.mixing {
            self.mixing = true;
            self.muted_it = !guide_muted;
            return !guide_muted;
        }
        if !mixing && self.mixing {
            let undo = self.muted_it && guide_muted;
            *self = Self {
                mixing: false,
                muted_it: false,
            };
            return undo;
        }
        false
    }
}

/// The song's top-level `Guide` folder: guid, and whether it is muted.
fn guide(tracks: &[daw_proto::Track]) -> Option<(String, bool)> {
    tracks
        .iter()
        .find(|t| {
            t.is_folder && t.parent_guid.is_none() && t.name.trim().eq_ignore_ascii_case("guide")
        })
        .map(|t| (t.guid.clone(), t.muted))
}

/// For a shell, over its setlist and its mode: the song on screen's guide
/// muted in Mix mode, as above.
pub fn use_mix_guide(setlist: Signal<crate::setlist::Setlist>, mode: Signal<Mode>) {
    let applier = use_hook(|| std::rc::Rc::new(crate::engine::Applier::start()));
    let stints =
        use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(HashMap::<String, Stint>::new())));
    use_effect(move || {
        let mixing = mode() == Mode::Mix;
        let Some((project, planner)) = setlist
            .read()
            .current()
            .map(|song| (song.project.clone(), song.session.planner.clone()))
        else {
            return;
        };
        let toggled = crate::widget::edit_song(&planner, |raw| {
            let (guid, muted) = guide(&raw.tracks)?;
            let toggle = stints
                .borrow_mut()
                .entry(project.clone())
                .or_default()
                .shown(mixing, muted);
            if toggle && let Some(track) = raw.tracks.iter_mut().find(|t| t.guid == guid) {
                track.muted = !track.muted;
            }
            toggle.then_some(guid)
        });
        if let Some(guid) = toggled {
            tracing::info!(
                mix.mixing = mixing,
                "mix mode: the guide folder's mute toggled"
            );
            if let Some(applier) = applier.as_ref() {
                applier.send(crate::engine::Edit::ToggleMute(guid));
            } else {
                tracing::debug!("no engine to mute the guide in");
            }
        }
    });
}
