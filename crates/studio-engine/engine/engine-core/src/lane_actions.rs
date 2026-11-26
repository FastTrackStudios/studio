//! What a lane's actions share between the engine and the window:
//! `TRACK_SHOW_LANES`'s arguments and its work over a song's tracks, and
//! an action asked of the window too. Moved down from studio-engine's
//! `shared_actions` and `actions` for engine-edit's comp, which registers
//! `TRACK_SHOW_LANES`; both re-export them from their old paths.

use facet::Facet;

/// Which track's lanes.
#[derive(Facet)]
pub struct ShowArg {
    /// Left out: the selected track (what a key means).
    #[facet(default)]
    pub track: Option<String>,
    pub show: Option<bool>,
    /// The tracks to open or shut, as the engine settled them: what it
    /// tells the window, which has no song of its own to look them up in
    /// when it drives an engine in another process.
    pub tracks: Option<Vec<String>>,
}

/// Whether they show.
#[derive(Facet)]
pub struct ShowOut {
    pub shown: bool,
    /// The tracks it opened or shut, the row's own first.
    pub tracks: Vec<String>,
}

/// `TRACK_SHOW_LANES`'s work over a song's tracks (`all`): every lane of
/// `guid` shown (`show`; toggled when `None`) — on a folder, its row's and
/// those of every track under it with lanes. Whether they show now, and
/// the tracks it opened or shut, the row's own first.
///
/// # Errors
/// No such track, or one lane and no track under it with more.
pub fn show_lanes(
    all: &[daw_proto::Track],
    guid: &str,
    show: Option<bool>,
) -> Result<(bool, Vec<String>), String> {
    let track = all
        .iter()
        .find(|t| t.guid == guid)
        .ok_or("the track went")?;
    let tracks = crate::lanes::opened_with_folders(all, track, &crate::lanes::shown_on(all, track));
    if tracks.is_empty() {
        return Err(format!(
            "{} has one lane, and no track under it has more",
            track.name
        ));
    }
    let shown = crate::lanes::set_expanded(&tracks, show);
    Ok((shown, tracks))
}

/// The window's in a process of its own: an action that sets what the
/// window shows (its mode, a mode's phase), run on the engine's list, is
/// asked of the window too — run there on the window's own list, where the
/// shell reads it. In the window's process no window follows it, so the
/// ask goes nowhere and nothing loops.
pub fn also_in_the_window(id: &str, args: String) {
    if crate::window_queue::engine_has_window() {
        crate::window_queue::ask(crate::window_queue::Asked::Run {
            id: id.to_owned(),
            args,
            ticket: 0,
        });
    }
}
