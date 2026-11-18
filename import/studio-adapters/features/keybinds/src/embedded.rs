//! The keybind profiles, compiled in.
//!
//! An app installed away from a daw checkout has no profile directory to
//! read, and a keymap found relative to the working directory is found
//! only when the app happens to be started from the right place. So the
//! profile's files are embedded here, in the repo that owns them: a
//! consumer in another repo cannot `include_str!` across the boundary
//! (a git dependency has no stable path on disk), so it asks this crate
//! for the bytes instead.

use crate::loader::{Profile, load_profile_from};
use crate::proto::ProfileConfig;

/// Every file of `reaper-input/config/config/ableton`, by name.
pub const ABLETON: &[(&str, &str)] = &[
    (
        "profile.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/ableton/profile.styx"),
    ),
    (
        "transport.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/ableton/transport.styx"),
    ),
];

/// Every file of `reaper-input/config/config/fasttrackstudio`, by name.
pub const FASTTRACKSTUDIO: &[(&str, &str)] = &[
    (
        "profile.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/profile.styx"
        ),
    ),
    (
        "automation.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/automation.styx"
        ),
    ),
    (
        "editing.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/editing.styx"
        ),
    ),
    (
        "expression-editor.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/expression-editor.styx"
        ),
    ),
    (
        "fx.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/fasttrackstudio/fx.styx"),
    ),
    (
        "grid.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/fasttrackstudio/grid.styx"),
    ),
    (
        "lanes-takes.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/lanes-takes.styx"
        ),
    ),
    (
        "markers.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/markers.styx"
        ),
    ),
    (
        "midi-modes.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/midi-modes.styx"
        ),
    ),
    (
        "midi.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/fasttrackstudio/midi.styx"),
    ),
    (
        "modes.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/fasttrackstudio/modes.styx"),
    ),
    (
        "mouse-profile.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/mouse-profile.styx"
        ),
    ),
    (
        "mouse.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/fasttrackstudio/mouse.styx"),
    ),
    (
        "navigation.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/navigation.styx"
        ),
    ),
    (
        "options.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/options.styx"
        ),
    ),
    (
        "scrolling.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/scrolling.styx"
        ),
    ),
    (
        "tracks.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/tracks.styx"
        ),
    ),
    (
        "transport.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/transport.styx"
        ),
    ),
    (
        "utility.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/utility.styx"
        ),
    ),
    (
        "views.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/fasttrackstudio/views.styx"),
    ),
    (
        "visibility.styx",
        include_str!(
            "../../adapters/reaper/reaper-input/config/config/fasttrackstudio/visibility.styx"
        ),
    ),
    (
        "zoom.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/fasttrackstudio/zoom.styx"),
    ),
];

/// Every file of `reaper-input/config/config/logic`, by name.
pub const LOGIC: &[(&str, &str)] = &[
    (
        "profile.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/logic/profile.styx"),
    ),
    (
        "editing.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/logic/editing.styx"),
    ),
    (
        "midi.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/logic/midi.styx"),
    ),
    (
        "mouse-profile.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/logic/mouse-profile.styx"),
    ),
    (
        "mouse.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/logic/mouse.styx"),
    ),
    (
        "navigation.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/logic/navigation.styx"),
    ),
    (
        "scrolling.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/logic/scrolling.styx"),
    ),
    (
        "transport.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/logic/transport.styx"),
    ),
    (
        "views.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/logic/views.styx"),
    ),
];

/// Every file of `reaper-input/config/config/pro-tools`, by name.
pub const PRO_TOOLS: &[(&str, &str)] = &[
    (
        "profile.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/pro-tools/profile.styx"),
    ),
    (
        "transport.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/pro-tools/transport.styx"),
    ),
];

/// Every file of `reaper-input/config/config/reaper`, by name.
pub const REAPER: &[(&str, &str)] = &[
    (
        "profile.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/reaper/profile.styx"),
    ),
    (
        "editing.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/reaper/editing.styx"),
    ),
    (
        "midi.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/reaper/midi.styx"),
    ),
    (
        "mouse-profile.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/reaper/mouse-profile.styx"),
    ),
    (
        "mouse.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/reaper/mouse.styx"),
    ),
    (
        "navigation.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/reaper/navigation.styx"),
    ),
    (
        "scrolling.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/reaper/scrolling.styx"),
    ),
    (
        "transport.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/reaper/transport.styx"),
    ),
    (
        "views.styx",
        include_str!("../../adapters/reaper/reaper-input/config/config/reaper/views.styx"),
    ),
];

/// Every profile of `reaper-input/config/config`, by slug (its directory's
/// name there): the profiles a host offers, with or without a checkout.
pub const PROFILES: &[(&str, &[(&str, &str)])] = &[
    ("ableton", ABLETON),
    ("fasttrackstudio", FASTTRACKSTUDIO),
    ("logic", LOGIC),
    ("pro-tools", PRO_TOOLS),
    ("reaper", REAPER),
];

/// One profile's files, by slug.
fn files(slug: &str) -> Option<&'static [(&'static str, &'static str)]> {
    PROFILES.iter().find(|(s, _)| *s == slug).map(|(_, f)| *f)
}

/// The profile `slug`, from the embedded copy.
#[must_use]
pub fn profile(slug: &str) -> Option<Profile> {
    let files = files(slug)?;
    load_profile_from(|name| {
        files
            .iter()
            .find(|(file, _)| *file == name)
            .map(|(_, text)| (*text).to_owned())
    })
}

/// Every embedded profile's slug and header, sorted by name (as
/// [`crate::list_profiles`] lists a directory).
#[must_use]
pub fn list() -> Vec<(String, ProfileConfig)> {
    let mut profiles: Vec<(String, ProfileConfig)> = PROFILES
        .iter()
        .filter_map(|(slug, files)| {
            let text = files.iter().find(|(f, _)| *f == "profile.styx")?.1;
            let config = facet_styx::from_str::<ProfileConfig>(text).ok()?;
            Some(((*slug).to_owned(), config))
        })
        .collect();
    profiles.sort_by(|a, b| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()));
    profiles
}

/// The FastTrackStudio profile, from the embedded copy.
#[must_use]
pub fn fasttrackstudio() -> Option<Profile> {
    profile("fasttrackstudio")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_profile_loads_with_bindings() {
        for (slug, _) in PROFILES {
            let profile = profile(slug).unwrap_or_else(|| panic!("{slug} does not load"));
            assert!(!profile.keymap.keymap.is_empty(), "{slug} has no bindings");
        }
    }

    #[test]
    fn the_list_names_every_profile_once_sorted_by_name() {
        let listed = list();
        assert_eq!(listed.len(), PROFILES.len());
        let names: Vec<String> = listed.iter().map(|(_, c)| c.name.to_lowercase()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }
}
