//! The actions both the desktop engine's list and the page's carry
//! (`docs/design/actions-everywhere.md`, rule 7): one description and one
//! argument shape per id, and the decisions that need no engine — which
//! track a name means, which folders a fold opens, which lanes a lanes
//! press shows, which phase a phase names. The desktop's handlers
//! (`crate::actions`, `crate::record`, `crate::comp`) run them on the
//! engine in this process; the page's (`crate::web_actions`) through its
//! facade.

use architect::action::ActionMeta;
use facet::Facet;

/// An action's description, as the list and its menus show it.
pub const fn meta(
    id: &'static str,
    method_name: &'static str,
    display_name: &'static str,
    description: &'static str,
    group: &'static str,
    shortcut: &'static str,
) -> ActionMeta {
    ActionMeta {
        id,
        trait_name: "Session",
        method_name,
        display_name,
        description,
        category: "Session",
        group,
        shortcut,
        toggleable: false,
        undo: false,
    }
}

pub static TRACK_SET: ActionMeta = meta(
    "TRACK_SET",
    "track_set",
    "Set track",
    "Set any of a track's fader (`volume_db`), pan (-1 left … 1 right), `mute`, `solo`, `phase` (polarity inverted), `monitor` (input monitoring: `off`, `on`, or `auto` — on except while playing) or `name` — only the fields given change. What the track panel's and the mixer's buttons and faders run (arming is TRACK_ARM).",
    "Mix",
    "",
);

pub static TRACK_ARM: ActionMeta = meta(
    "TRACK_ARM",
    "track_arm",
    "Arm a track",
    "Arm `track` (a path, name or GUID) to record, or disarm it: `on` true or false, toggled when left out. `input`: the hardware input it records, from 1 (`audio 1` in SESSION_TRACKS) — when arming a track that records nothing yet, input 1. Says what it is armed to now.",
    "Record",
    "",
);

pub static SELECT_TRACK: ActionMeta = meta(
    "SESSION_SELECT_TRACK",
    "session_select_track",
    "Select a track",
    "Select `track` (its GUID, its path — `Snare/Verb/Plate`, any tail of it — or its name) and nothing else: what the FX row, the mixer and the Editor follow. `add` true adds it to the selection instead, leaving the rest selected (a modified click on a track's name).",
    "Session",
    "",
);

pub static FOLD: ActionMeta = meta(
    "SESSION_FOLD",
    "session_fold",
    "Open or shut a folder",
    "Open (`open: true`) the folder `track` (its GUID, path or name) and every folder it is in, so its tracks show; or shut it (`open: false`). Returns the folders changed.",
    "View",
    "",
);

pub static PHASE: ActionMeta = meta(
    "SESSION_PHASE",
    "phase",
    "Phase",
    "Put a mode in one of its phases — Mix: rescue, balance, tone, polish, relational, depth, creative, overview; Organize: import, tempo-map. `mode` defaults to Edit.",
    "View",
    "",
);

pub static SHOW_LANES: ActionMeta = meta(
    "TRACK_SHOW_LANES",
    "track_show_lanes",
    "Show a track's lanes",
    "Show every lane of `track` (a path, name or GUID; left out, the selected track — Alt+L) stacked in its row — the takes, the comp and the edit side by side, to hear and comp from — or only the lane that plays: `show` true or false, toggled when left out. On a folder, its row (shut, each lane folded from the tracks under it) and every track under it with lanes. The window's view, not the song's. Says whether they show now.",
    "View",
    "Alt+L",
);

/// What to set on a track.
#[derive(Facet)]
pub struct TrackSetArg {
    pub track: String,
    pub volume_db: Option<f64>,
    pub pan: Option<f64>,
    pub mute: Option<bool>,
    pub solo: Option<bool>,
    /// Polarity inverted.
    pub phase: Option<bool>,
    /// Input monitoring: `off`, `on`, `auto` (on except while playing).
    pub monitor: Option<String>,
    pub name: Option<String>,
}

/// `TRACK_SET`'s `monitor`, as the engine's mode.
///
/// # Errors
/// A word that is none of them.
pub fn monitor_mode(word: &str) -> Result<daw_proto::track::InputMonitoringMode, String> {
    use daw_proto::track::InputMonitoringMode as M;
    match word.to_ascii_lowercase().as_str() {
        "off" => Ok(M::Off),
        "on" | "normal" => Ok(M::Normal),
        "auto" | "not_when_playing" => Ok(M::NotWhenPlaying),
        other => Err(format!("`monitor` is off, on or auto, not {other:?}")),
    }
}

/// Which track, armed or not, to what.
#[derive(Facet)]
pub struct ArmArg {
    pub track: String,
    pub on: Option<bool>,
    /// The hardware input, from 1.
    pub input: Option<u32>,
}

/// What a track is armed to now.
#[derive(Facet)]
pub struct ArmOut {
    pub track: String,
    pub armed: bool,
    /// The hardware input it records, from 1, when it records one.
    pub input: Option<u32>,
}

/// The hardware input a track records, from 1, when it records one.
#[must_use]
pub const fn input_of(input: daw_proto::track::RecordInput) -> Option<u32> {
    use daw_proto::track::RecordInput;
    match input {
        RecordInput::Audio { channel } => channel.checked_add(1),
        RecordInput::None | RecordInput::Midi { .. } | RecordInput::Raw(_) => None,
    }
}

/// Which track.
#[derive(Facet)]
pub struct SelectTrackArg {
    pub track: String,
    /// Add it to the selection rather than select it alone.
    #[facet(default)]
    pub add: bool,
}

/// A folder to open or shut.
#[derive(Facet)]
pub struct FoldArg {
    pub track: String,
    pub open: bool,
    /// The folders to open or shut, as the engine settled them: what it
    /// tells a window in another process, which has no song to look them
    /// up in.
    #[facet(default)]
    pub folders: Vec<String>,
}

/// The folders a fold of `guid` changes, of the song's `tracks`: opening,
/// it and every folder it is in (a folder open inside a shut one shows
/// nothing); shutting, it alone. Each folder's guid and name.
#[must_use]
pub fn folds_of(tracks: &[daw_proto::Track], guid: &str, open: bool) -> Vec<(String, String)> {
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let mut folders = Vec::new();
    let mut at = by_guid.get(guid).copied();
    while let Some(track) = at {
        if track.is_folder && (open || track.guid == guid) {
            folders.push((track.guid.clone(), track.name.clone()));
        }
        if !open {
            break;
        }
        at = track
            .parent_guid
            .as_deref()
            .and_then(|p| by_guid.get(p).copied());
    }
    folders
}

/// Which phase.
#[derive(Facet)]
pub struct PhaseArg {
    pub phase: String,
    pub mode: Option<String>,
}

/// The mode and phase `SESSION_PHASE` names: `mode` by name (Edit when
/// left out), `phase` by slug or name.
///
/// # Errors
/// No such mode, or no such phase in it.
pub fn pick_phase(
    mode: Option<&str>,
    phase: &str,
) -> Result<(session::modes::Mode, &'static str), String> {
    let name = mode.unwrap_or("Edit");
    let mode = session::modes::Mode::ALL
        .into_iter()
        .find(|m| m.display_name().eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| format!("no mode {name}"))?;
    let phases = session::mode_phases::of(mode);
    let found = phases
        .iter()
        .find(|p| {
            p.slug.eq_ignore_ascii_case(phase.trim()) || p.name.eq_ignore_ascii_case(phase.trim())
        })
        .ok_or_else(|| {
            format!(
                "{} has no phase {}: one of {:?}",
                mode.display_name(),
                phase,
                phases.iter().map(|p| p.slug).collect::<Vec<_>>()
            )
        })?;
    Ok((mode, found.slug))
}

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

/// The track `what` means among the song's `tracks`: its GUID, its path
/// (`Kick/Sum/Trig`, any tail of it) or its name — refused, naming the
/// candidates, when more than one fits.
///
/// # Errors
/// None fits, or more than one.
pub fn resolve_in(tracks: &[daw_proto::Track], what: &str) -> Result<String, String> {
    if let Some(t) = tracks.iter().find(|t| t.guid.eq_ignore_ascii_case(what)) {
        return Ok(t.guid.clone());
    }
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let want = what.trim().trim_matches('/').to_lowercase();
    let fits: Vec<(&daw_proto::Track, String)> = tracks
        .iter()
        .map(|t| (t, crate::gain_stage::path_of(t, &by_guid).join("/")))
        .filter(|(_, path)| {
            let path = path.to_lowercase();
            path == want || path.ends_with(&format!("/{want}"))
        })
        .collect();
    // A whole path is that track, however many others end the same way:
    // `Drums` is the top-level folder, not `REFERENCES/…/Drums` too.
    let exact: Vec<&(&daw_proto::Track, String)> = fits
        .iter()
        .filter(|(_, p)| p.to_lowercase() == want)
        .collect();
    if let [(only, _)] = exact.as_slice() {
        return Ok(only.guid.clone());
    }
    match fits.as_slice() {
        [(only, _)] => Ok(only.guid.clone()),
        [] => Err(format!("no track {what} — list them with SESSION_TRACKS")),
        many => Err(format!(
            "{what} is more than one track: {:?} — give more of its path",
            many.iter().map(|(_, p)| p.as_str()).collect::<Vec<_>>()
        )),
    }
}

pub static TOGETHER_LEAVE: ActionMeta = meta(
    "SESSION_TOGETHER_LEAVE",
    "session_together_leave",
    "Leave the live session",
    "Leave the live session (or stop sharing it). The songs keep everything edited together in their files.",
    "Set",
    "",
);

pub static TOGETHER_TRANSPORT: ActionMeta = meta(
    "SESSION_TOGETHER_TRANSPORT",
    "session_together_transport",
    "Play together or apart",
    "In a live session: `together` true plays one transport for everyone, from where this engine is; false lets everyone play on their own.",
    "Set",
    "",
);

pub static TOGETHER_LOCK: ActionMeta = meta(
    "SESSION_TOGETHER_LOCK",
    "session_together_lock",
    "Lock this device",
    "`locked` true: this device stops syncing — the others' edits wait, its own stay here. False unlocks it and merges both ways. Remembered on this device.",
    "Set",
    "",
);

/// Together or apart.
#[derive(Facet)]
pub struct TogetherArg {
    pub together: bool,
}

/// Locked or not.
#[derive(Facet)]
pub struct LockArg {
    pub locked: bool,
}

/// The live session's controls both lists carry: leave it, play together
/// or apart, lock this device (`crate::collab`, the desktop's and the
/// page's alike).
///
/// # Errors
/// The first registration the list refuses (a duplicate id).
pub fn register_together(
    list: &architect::action::ActionList,
) -> Result<(), architect::action::ActionError> {
    use architect::action::Effect;
    list.register(&TOGETHER_LEAVE, Effect::Mutating, |(): ()| {
        crate::collab::leave();
        Ok("left the live session".to_owned())
    })?;
    list.register(&TOGETHER_TRANSPORT, Effect::Mutating, |a: TogetherArg| {
        crate::collab::set_shared_transport(a.together);
        Ok(if a.together {
            "playing together"
        } else {
            "playing apart"
        }
        .to_owned())
    })?;
    list.register(&TOGETHER_LOCK, Effect::Mutating, |a: LockArg| {
        crate::collab::set_sync_locked(a.locked);
        Ok(if a.locked {
            "this device is locked"
        } else {
            "this device syncs"
        }
        .to_owned())
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(guid: &str, name: &str, parent: Option<&str>, folder: bool) -> daw_proto::Track {
        daw_proto::Track {
            guid: guid.to_owned(),
            name: name.to_owned(),
            parent_guid: parent.map(str::to_owned),
            is_folder: folder,
            ..daw_proto::Track::default()
        }
    }

    fn song() -> Vec<daw_proto::Track> {
        vec![
            track("{d}", "Drums", None, true),
            track("{k}", "Kick", Some("{d}"), true),
            track("{ki}", "In", Some("{k}"), false),
            track("{r}", "REFERENCES", None, true),
            track("{rd}", "Drums", Some("{r}"), false),
        ]
    }

    /// By GUID, by whole path, by a tail; a top-level path wins over the
    /// tails that end the same way.
    #[test]
    fn a_track_is_found_by_its_guid_its_path_or_a_tail() {
        let all = song();
        assert_eq!(resolve_in(&all, "{ki}"), Ok("{ki}".to_owned()));
        assert_eq!(resolve_in(&all, "Kick/In"), Ok("{ki}".to_owned()));
        assert_eq!(resolve_in(&all, "Drums"), Ok("{d}".to_owned()));
        assert!(resolve_in(&all, "Snare").is_err());
    }

    /// Opening opens the folders it is in; shutting shuts it alone.
    #[test]
    fn a_fold_opens_its_ancestors_and_shuts_alone() {
        let all = song();
        let open: Vec<String> = folds_of(&all, "{k}", true)
            .into_iter()
            .map(|(g, _)| g)
            .collect();
        assert_eq!(open, ["{k}", "{d}"]);
        let shut: Vec<String> = folds_of(&all, "{k}", false)
            .into_iter()
            .map(|(g, _)| g)
            .collect();
        assert_eq!(shut, ["{k}"]);
    }

    #[test]
    fn a_phase_is_named_in_its_mode() {
        assert!(pick_phase(Some("Mix"), "tone").is_ok());
        assert!(pick_phase(Some("Mix"), "no such phase").is_err());
        assert!(pick_phase(Some("Nowhere"), "tone").is_err());
        assert!(monitor_mode("auto").is_ok());
        assert!(monitor_mode("loud").is_err());
    }
}
