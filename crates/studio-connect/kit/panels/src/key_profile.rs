//! Studio's key bindings for the views: its input profile, read with
//! `keybinds` and handed to the kit (`daw_view_kit::keys::install_profile`).

use std::path::PathBuf;

use daw_view_kit::keys::Profile;

/// Hand the views Studio's bindings: once per process, at start.
pub fn install() {
    daw_view_kit::keys::install_profile(load);
}

/// The profile `FTS_INPUT_PROFILE` names (a folder with a
/// `profile.styx`), else the copy `keybinds` compiles in, else the kit's
/// built-in bindings.
fn load() -> Profile {
    profile_dir()
        .and_then(|dir| keybinds::load_profile_dir(&dir).map(|p| (p, dir.display().to_string())))
        .or_else(|| {
            keybinds::embedded::fasttrackstudio()
                .map(|p| (p, "fasttrackstudio (embedded)".to_owned()))
        })
        .map_or_else(daw_view_kit::keys::builtin, |(profile, source)| Profile {
            keymap: profile.keymap,
            labels: profile.labels,
            source,
        })
}

fn profile_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("FTS_INPUT_PROFILE")?);
    dir.join("profile.styx").exists().then_some(dir)
}
