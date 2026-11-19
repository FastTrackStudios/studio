//! Organize mode's instruments: the tracks a song starts with, added from
//! the instrument column on the left of the arrangement — or by
//! `TRACK_ADD_INSTRUMENT`, which is what the column's buttons run.
//!
//! - **Drums** is the golden template's whole kit
//!   (`dynamic_template::golden_session::shape::drums`): every piece with
//!   its Sum, close mics, `Trig`, `Sample` and `Fund` (the kick's `Sub`),
//!   the toms, cymbals and rooms, the Process chains (in the mixer only),
//!   and a `Drums/MIDI` track first in it, where the part is drawn. Each
//!   track is marked with what the template says it is, the reverb banks
//!   and the one-note tracks fed as the template feeds them, the triggers
//!   muted — then the folder is shut: one row, even with nothing recorded.
//! - **Bass** is one `Bass` track and **Guitar** one `Guitar` track: a
//!   level is a folder only when it has more than one member; the grow
//!   actions make it one.
//!
//! Each goes to its bus as the organizer routes it (DRUM BUS, BASS BUS,
//! ELECTRIC BUS), the buses made when the song has none, and the top
//! level put back in the template's order: the Guide first, the
//! instruments, the buses last. One undo step each.
//!
//! `TRACK_RENAME` renames a track, as a double-click on its name does.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{ProjectContext, Projects, Routing, TrackRef, Tracks};
use dynamic_template::golden_session::shape::Node;
use dynamic_template::golden_session::{GROUP_KEY, KIND_KEY, Kind};
use facet::Facet;

pub static ADD: ActionMeta = crate::actions::meta(
    "TRACK_ADD_INSTRUMENT",
    "track_add_instrument",
    "Add an instrument",
    "Add an `instrument`'s tracks at the top level, routed to its bus: `Drums` the golden template's whole kit (each piece's Sum, close mics, Trig, Sample and Fund, the toms, cymbals, rooms and Process) with a `Drums/MIDI` track first in it for the drawn part, folded to one row; `Bass` one `Bass` track; `Guitar` one `Guitar` track. Refused when the song has a top-level track of that name already. One undo step. Says what it made and where it goes.",
    "Tracks",
    "",
);

static RENAME: ActionMeta = crate::actions::meta(
    "TRACK_RENAME",
    "track_rename",
    "Rename a track",
    "Give `track` (its GUID, path or a tail of its path) the name `name`, as a double-click on its name does. Its GUID stays. One undo step. Returns the GUID.",
    "Tracks",
    "",
);

static MOVE: ActionMeta = crate::actions::meta(
    "TRACK_MOVE",
    "track_move",
    "Move a track into a folder",
    "Move `track` (its GUID, path or a tail of its path; a track with no tracks under it) to the end of the top-level folder `into` (its name; made at the end of the song when there is none, `muted` then setting whether it starts muted). The track keeps its GUID, items, FX and sends. The folder it leaves stays, with what else it holds. One undo step. `every_song`: the same in every song of the open set at once, each song's own track of that name, without going to any of them (says what happened in each). Returns the folder's and the track's GUIDs.",
    "Tracks",
    "",
);

static HEAR: ActionMeta = crate::actions::meta(
    "SESSION_HEAR",
    "session_hear",
    "Hear the set by its references or its stems",
    "How every song of the open set is heard, all at once and without going to any: `by` `reference` plays each song's stereo reference (its top-level `Reference` folder on, every instrument folder muted: far less to render, the transport, sections, chart and lights as before), `stems` the multitracks (the instrument folders on, the reference muted). The Guide folder and the buses are left as they are. With no `by`, says which it is now. One undo step a song.",
    "Tracks",
    "",
);

/// How the set is heard.
#[derive(Facet)]
struct HearArg {
    #[facet(default)]
    by: Option<String>,
}

/// The top-level folder a song's stereo reference is in.
const REFERENCE_FOLDER: &str = "Reference";

/// Whether top-level track `name` is neither an instrument folder nor the
/// reference: the guide and the buses, which both ways of hearing leave.
fn left_alone(name: &str) -> bool {
    let n = name.trim().to_ascii_uppercase();
    n == "GUIDE" || n.ends_with(" BUS") || n == "KEYFLOW"
}

/// Whether song `project` is heard by its reference now: its Reference
/// folder is on. `None` for a song with no such folder.
fn heard_by_reference(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Option<bool> {
    Tracks::all(daw, project.clone())
        .iter()
        .find(|t| t.parent_guid.is_none() && t.name.trim().eq_ignore_ascii_case(REFERENCE_FOLDER))
        .map(|t| !t.muted)
}

/// Song `project` heard by its reference (`reference`) or its stems.
/// Whether it has a reference to hear.
fn hear(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    reference: bool,
) -> Result<bool, String> {
    let top: Vec<_> = Tracks::all(daw, project.clone())
        .into_iter()
        .filter(|t| t.parent_guid.is_none())
        .collect();
    let has = top
        .iter()
        .any(|t| t.name.trim().eq_ignore_ascii_case(REFERENCE_FOLDER));
    if reference && !has {
        // Nothing to hear it by: left on its stems.
        return Ok(false);
    }
    for track in top.iter().filter(|t| !left_alone(&t.name)) {
        let is_reference = track.name.trim().eq_ignore_ascii_case(REFERENCE_FOLDER);
        let mute = is_reference != reference;
        if track.muted != mute {
            Tracks::set_muted(
                daw,
                project.clone(),
                TrackRef::Guid(track.guid.clone()),
                mute,
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(has)
}

/// A track, and the top-level folder it goes into.
#[derive(Facet)]
struct MoveArg {
    track: String,
    into: String,
    /// Whether a folder made for it starts muted.
    #[facet(default)]
    muted: Option<bool>,
    /// In every song of the open set, not only the one showing: each
    /// song's own track of that name, in one call, no song gone to.
    #[facet(default)]
    every_song: Option<bool>,
}

/// `track` moved to the end of top-level folder `into`, made when the
/// song has none. `(folder, track)` GUIDs.
fn move_into(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    into: &str,
    muted: Option<bool>,
) -> Result<(String, String), String> {
    use daw::service::TrackTree;
    let depth = |guid: &str, to: i32| {
        Tracks::set_folder_depth(daw, project.clone(), TrackRef::Guid(guid.to_owned()), to)
            .map_err(|e| e.to_string())
    };
    let guid = crate::actions::resolve(daw, project, track)?;
    let tree = TrackTree::new(Tracks::all(daw, project.clone()));
    let moved = tree.get(&guid).ok_or("the track is gone")?.clone();
    if tree
        .parent_of(&moved)
        .is_some_and(|p| p.parent_guid.is_none() && p.name.trim().eq_ignore_ascii_case(into.trim()))
    {
        return Ok((moved.parent_guid.clone().unwrap_or_default(), guid));
    }
    if moved.folder_depth > 0 {
        return Err(format!(
            "{track} is a folder: only a track with nothing under it moves (move its tracks one by one)"
        ));
    }
    // Out of where it is: what it closed, the track before it closes.
    if moved.folder_depth < 0 {
        let before = tree
            .at_index(moved.index.saturating_sub(1))
            .ok_or("a track that closes a folder has one before it")?;
        depth(&before.guid, before.folder_depth + moved.folder_depth)?;
        depth(&guid, 0)?;
    }
    let last = u32::try_from(tree_len(daw, project).saturating_sub(1)).unwrap_or(u32::MAX);
    Tracks::move_to(daw, project.clone(), TrackRef::Guid(guid.clone()), last)
        .map_err(|e| e.to_string())?;
    // The folder: the top-level track of that name, else a new one, last.
    let all = Tracks::all(daw, project.clone());
    let folder = match all.iter().find(|t| {
        t.parent_guid.is_none() && t.guid != guid && t.name.trim().eq_ignore_ascii_case(into.trim())
    }) {
        Some(found) => found.guid.clone(),
        None => {
            let made =
                Tracks::add(daw, project.clone(), into.trim(), None).map_err(|e| e.to_string())?;
            if muted == Some(true) {
                Tracks::set_muted(daw, project.clone(), TrackRef::Guid(made.clone()), true)
                    .map_err(|e| e.to_string())?;
            }
            made
        }
    };
    // Into it, as its last track.
    let tree = TrackTree::new(Tracks::all(daw, project.clone()));
    let parent = tree.get(&folder).ok_or("the folder is gone")?.clone();
    let plan = tree.plan_append(&folder).ok_or("no place in the folder")?;
    if let Some((terminator, to)) = &plan.retarget {
        depth(terminator, *to)?;
    }
    if parent.folder_depth <= 0 {
        // A track with nothing under it becomes the folder.
        depth(&folder, 1)?;
    }
    let here = tree.get(&guid).map_or(u32::MAX, |t| t.index);
    // Taken from after the folder's end, the place is the plan's; from
    // before it, everything between moves up one.
    let index = if here < plan.index {
        plan.index.saturating_sub(1)
    } else {
        plan.index
    };
    Tracks::move_to(daw, project.clone(), TrackRef::Guid(guid.clone()), index)
        .map_err(|e| e.to_string())?;
    depth(&guid, -plan.closes)?;
    Ok((folder, guid))
}

/// How many tracks the song has.
fn tree_len(daw: &daw::standalone::Standalone, project: &ProjectContext) -> usize {
    Tracks::all(daw, project.clone()).len()
}

/// Which instrument.
#[derive(Facet)]
struct AddArg {
    instrument: String,
}

/// A track and its new name.
#[derive(Facet)]
struct RenameArg {
    track: String,
    name: String,
}

/// What Organize mode's instrument column adds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Instrument {
    Drums,
    Bass,
    Guitar,
}

impl Instrument {
    /// The column's buttons, top to bottom.
    pub const ALL: [Self; 3] = [Self::Drums, Self::Bass, Self::Guitar];

    /// Its name, and the top-level track's.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Drums => "Drums",
            Self::Bass => "Bass",
            Self::Guitar => "Guitar",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Drums => "Add the drum kit — the golden template's, folded to one row",
            Self::Bass => "Add a Bass track, to the bass bus",
            Self::Guitar => "Add a Guitar track, to the electric bus",
        }
    }

    /// The instrument `text` names, loosely: `drums`, `kit`, `Guitars`.
    fn named(text: &str) -> Option<Self> {
        let text = text.trim();
        let plural = |name: &str| {
            text.strip_suffix(['s', 'S'])
                .is_some_and(|one| one.eq_ignore_ascii_case(name))
        };
        Self::ALL
            .into_iter()
            .find(|i| text.eq_ignore_ascii_case(i.name()) || plural(i.name()))
            .or_else(|| {
                ["kit", "drum", "drum kit"]
                    .iter()
                    .any(|k| text.eq_ignore_ascii_case(k))
                    .then_some(Self::Drums)
            })
    }
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&ADD, Effect::Mutating, |a: AddArg| {
        let instrument = Instrument::named(&a.instrument).ok_or_else(|| {
            format!(
                "no instrument {}: one of {:?}",
                a.instrument,
                Instrument::ALL.map(Instrument::name)
            )
        })?;
        let (daw, project) = crate::actions::song()?;
        let said = add(daw, &project, instrument)?;
        crate::song_model::request_resync();
        crate::actions::list().changed("tracks", instrument.name());
        Ok(said)
    })?;
    list.register(&HEAR, Effect::Mutating, |a: HearArg| {
        let (daw, current) = crate::actions::song()?;
        let songs: Vec<(String, ProjectContext)> = crate::studio_engine::set::lineup().map_or_else(
            || vec![("the song".to_owned(), current.clone())],
            |l| {
                l.songs
                    .into_iter()
                    .map(|(title, guid)| (title, ProjectContext::Project(guid)))
                    .collect()
            },
        );
        let Some(by) = a.by.as_deref().map(str::trim).filter(|b| !b.is_empty()) else {
            let on = songs
                .iter()
                .filter(|(_, p)| heard_by_reference(daw, p) == Some(true))
                .count();
            return Ok(format!(
                "{{\"by\":\"{}\",\"songs\":{},\"by_reference\":{on}}}",
                if on > 0 && on == songs.len() {
                    "reference"
                } else if on == 0 {
                    "stems"
                } else {
                    "mixed"
                },
                songs.len()
            ));
        };
        let reference = match by.to_ascii_lowercase().as_str() {
            "reference" | "references" => true,
            "stems" | "multitracks" => false,
            other => {
                return Err(format!(
                    "hear the set by `reference` or `stems`, not {other}"
                ));
            }
        };
        let label = if reference {
            "Hear the references"
        } else {
            "Hear the stems"
        };
        let mut without = Vec::new();
        for (title, project) in &songs {
            Projects::begin_undo_block(daw, project.clone(), label);
            let heard = hear(daw, project, reference);
            Projects::end_undo_block(daw, project.clone(), label, None);
            if !heard? {
                without.push(title.clone());
            }
        }
        crate::song_model::request_resync();
        crate::actions::list().changed("tracks", by);
        Ok(format!(
            "{{\"by\":\"{}\",\"songs\":{},\"without_reference\":{}}}",
            if reference { "reference" } else { "stems" },
            songs.len(),
            serde_json_list(&without)
        ))
    })?;
    list.register(&MOVE, Effect::Mutating, |a: MoveArg| {
        if a.into.trim().is_empty() {
            return Err("a folder's name cannot be empty".to_owned());
        }
        let (daw, project) = crate::actions::song()?;
        let label = "Move a track into a folder";
        if a.every_song == Some(true) {
            // Every song of the set is a project of the same engine: each
            // is edited where it is, the window staying on its song.
            let lineup = crate::studio_engine::set::lineup().ok_or("no set is open")?;
            let mut said = Vec::with_capacity(lineup.songs.len());
            for (title, guid) in &lineup.songs {
                let song = ProjectContext::Project(guid.clone());
                Projects::begin_undo_block(daw, song.clone(), label);
                let moved = move_into(daw, &song, &a.track, &a.into, a.muted);
                Projects::end_undo_block(daw, song, label, None);
                said.push(match moved {
                    Ok(_) => format!("{title}: moved"),
                    Err(e) => format!("{title}: not moved ({e})"),
                });
            }
            crate::song_model::request_resync();
            crate::actions::list().changed("tracks", &a.into);
            return Ok(said.join("; "));
        }
        Projects::begin_undo_block(daw, project.clone(), label);
        let moved = move_into(daw, &project, &a.track, &a.into, a.muted);
        Projects::end_undo_block(daw, project, label, None);
        let (folder, track) = moved.map_err(|e| format!("{} was not moved: {e}", a.track))?;
        crate::song_model::request_resync();
        crate::actions::list().changed("tracks", &track);
        Ok(format!("{{\"folder\":\"{folder}\",\"track\":\"{track}\"}}"))
    })?;
    list.register(&RENAME, Effect::Mutating, |a: RenameArg| {
        let name = a.name.trim();
        if name.is_empty() {
            return Err("a track's name cannot be empty".to_owned());
        }
        let (daw, project) = crate::actions::song()?;
        let guid = crate::actions::resolve(daw, &project, &a.track)?;
        let label = "Rename a track";
        Projects::begin_undo_block(daw, project.clone(), label);
        let renamed = Tracks::rename(daw, project.clone(), TrackRef::Guid(guid.clone()), name);
        Projects::end_undo_block(daw, project, label, None);
        renamed.map_err(|e| format!("{} was not renamed: {e}", a.track))?;
        crate::song_model::request_resync();
        crate::actions::list().changed("tracks", &guid);
        Ok(guid)
    })?;
    Ok(())
}

/// Add `instrument`'s tracks to the song — one undo step — and say what
/// was made.
///
/// # Errors
///
/// The song has a top-level track of that name already, or the engine
/// refused a step.
pub fn add(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    instrument: Instrument,
) -> Result<String, String> {
    let name = instrument.name();
    if Tracks::all(daw, project.clone())
        .iter()
        .any(|t| t.parent_guid.is_none() && t.name.trim().eq_ignore_ascii_case(name))
    {
        return Err(format!(
            "the song has a {name} already — TRACK_RENAME it first to add another"
        ));
    }
    let label = format!("Add {name}");
    Projects::begin_undo_block(daw, project.clone(), &label);
    let done = build(daw, project, instrument);
    Projects::end_undo_block(daw, project.clone(), &label, None);
    let (made, routed) = done?;
    tracing::info!(
        track.instrument = name,
        track.made = made,
        track.routed = ?routed,
        "an instrument was added"
    );
    Ok(if routed.is_empty() {
        format!("{name}: {made} tracks made")
    } else {
        format!("{name}: {made} tracks made, {}", routed.join(", "))
    })
}

/// The tracks made, wired and placed: how many, and where each top-level
/// one goes.
fn build(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    instrument: Instrument,
) -> Result<(usize, Vec<String>), String> {
    let (top, made) = match instrument {
        Instrument::Drums => add_kit(daw, project)?,
        Instrument::Bass | Instrument::Guitar => (
            Tracks::add(daw, project.clone(), instrument.name(), None)
                .map_err(|e| e.to_string())?,
            1,
        ),
    };
    let routed = place(daw, project, std::slice::from_ref(&top))?;
    Ok((made, routed))
}

/// The golden kit appended, nested, marked and wired, its triggers muted,
/// its `MIDI` track taking MIDI, folded: the Drums folder's GUID, and how
/// many tracks it is.
fn add_kit(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
) -> Result<(String, usize), String> {
    let mut rows = Vec::new();
    let made = make(daw, project, &kit(), 0, &mut rows)?;
    nest(daw, project, &rows)?;
    wire_kit(daw, project, &made)?;
    dynamic_template::apply::DawTarget::on(daw.clone(), project.clone())
        .mute_triggers()
        .map_err(|e| e.to_string())?;
    if let Some(midi) = made.children.iter().find(|c| c.name == MIDI) {
        Tracks::set_record_input(
            daw,
            project.clone(),
            TrackRef::Guid(midi.guid.clone()),
            daw_proto::track::RecordInput::Midi {
                device_id: None,
                channel: None,
            },
        )
        .map_err(|e| e.to_string())?;
    }
    // Folded: the kit is one row until it is opened.
    crate::folds::toggle(&made.guid, true);
    Ok((made.guid, rows.len()))
}

/// The drawn part's track, first in the kit (`Drums/MIDI`).
const MIDI: &str = "MIDI";

/// The golden kit, with the `MIDI` track first in it.
fn kit() -> Node {
    let mut kit = dynamic_template::golden_session::shape::drums();
    let midi = Node::new(MIDI, kit.colour, Kind::Source).midi();
    kit.children.insert(0, midi);
    kit
}

/// A node of the template made as tracks: what is needed to wire it.
struct Made {
    guid: String,
    name: String,
    kind: Kind,
    children: Vec<Self>,
}

impl Made {
    const fn is_folder(&self) -> bool {
        !self.children.is_empty()
    }

    /// Every track with nothing in it, below this one.
    fn leaves(&self, out: &mut Vec<String>) {
        for child in &self.children {
            if child.is_folder() {
                child.leaves(out);
            } else {
                out.push(child.guid.clone());
            }
        }
    }

    /// This track and every one below it.
    fn all(&self, out: &mut Vec<String>) {
        out.push(self.guid.clone());
        for child in &self.children {
            child.all(out);
        }
    }
}

/// `node` and everything in it appended as tracks, each marked with its
/// kind (and the template group it stands for), coloured, a stereo pair
/// on two channels; `rows` gets each track and how deep it sits, in
/// order.
fn make(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    node: &Node,
    level: u32,
    rows: &mut Vec<(String, u32)>,
) -> Result<Made, String> {
    let guid = Tracks::add(daw, project.clone(), &node.name, None).map_err(|e| e.to_string())?;
    rows.push((guid.clone(), level));
    let track = || TrackRef::Guid(guid.clone());
    Tracks::set_ext_state(daw, project.clone(), track(), KIND_KEY, node.kind.as_str())
        .map_err(|e| e.to_string())?;
    if let Some(group) = node.template {
        Tracks::set_ext_state(daw, project.clone(), track(), GROUP_KEY, &group.join("/"))
            .map_err(|e| e.to_string())?;
    }
    if node.colour != 0 {
        Tracks::set_color(daw, project.clone(), track(), native_colour(node.colour))
            .map_err(|e| e.to_string())?;
    }
    if node.stereo {
        Tracks::set_num_channels(daw, project.clone(), track(), 2).map_err(|e| e.to_string())?;
    }
    let children = node
        .children
        .iter()
        .map(|child| make(daw, project, child, level.saturating_add(1), rows))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Made {
        guid,
        name: node.name.clone(),
        kind: node.kind,
        children,
    })
}

/// `0xRRGGBB` as the engine's native colour, `0x01BBGGRR` (REAPER's, with
/// its "set" flag).
const fn native_colour(rgb: u32) -> u32 {
    let [_, r, g, b] = rgb.to_be_bytes();
    u32::from_be_bytes([1, b, g, r])
}

/// The folder depths that make `rows` (appended last, in order, each with
/// its level) the tree they were made as.
fn nest(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    rows: &[(String, u32)],
) -> Result<(), String> {
    let level = |l: u32| i32::try_from(l).unwrap_or(i32::MAX);
    let next = rows.iter().skip(1).map(|(_, l)| level(*l)).chain([0]);
    for ((guid, at), next) in rows.iter().zip(next) {
        let depth = next.saturating_sub(level(*at));
        Tracks::set_folder_depth(daw, project.clone(), TrackRef::Guid(guid.clone()), depth)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The kit fed as the template feeds it (`DawTarget::apply_golden_kit`
/// and `add_drum_process` do the same to a recorded kit): a piece's
/// reverb bank and `Sub` by its Sum, the toms' bank by every tom, each
/// `Fund` by the mics beside it, and every Process chain by every piece.
/// A bank's returns and the Process chains start muted but for a bank's
/// first: a palette, not all of it at once. Process is in the mixer only.
fn wire_kit(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    kit: &Made,
) -> Result<(), String> {
    let mut feeds: Vec<(String, String)> = Vec::new();
    let mut quiet: Vec<String> = Vec::new();
    feeds_in(kit, &mut feeds, &mut quiet);
    let mut hidden: Vec<String> = Vec::new();
    if let Some(process) = kit.children.iter().find(|c| c.kind == Kind::Process) {
        let mut chains = Vec::new();
        process.leaves(&mut chains);
        let pieces = kit
            .children
            .iter()
            .filter(|c| c.kind != Kind::Process && c.name != MIDI);
        for piece in pieces {
            feeds.extend(chains.iter().map(|c| (piece.guid.clone(), c.clone())));
        }
        quiet.extend(chains);
        process.all(&mut hidden);
    }
    for (from, to) in &feeds {
        Routing::add_send(
            daw,
            project.clone(),
            TrackRef::Guid(from.clone()),
            TrackRef::Guid(to.clone()),
        )
        .ok_or_else(|| format!("the engine refused a send in the kit ({from} to {to})"))?;
    }
    for guid in &quiet {
        Tracks::set_muted(daw, project.clone(), TrackRef::Guid(guid.clone()), true)
            .map_err(|e| e.to_string())?;
    }
    for guid in &hidden {
        Tracks::set_visibility(
            daw,
            project.clone(),
            TrackRef::Guid(guid.clone()),
            false,
            true,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The sends inside `node` and below it, and the returns that start muted.
fn feeds_in(node: &Made, feeds: &mut Vec<(String, String)>, quiet: &mut Vec<String>) {
    let sum = node.children.iter().find(|c| c.kind == Kind::Sum);
    let pieces: Vec<String> = node
        .children
        .iter()
        .filter(|c| c.kind == Kind::Piece)
        .map(|c| c.guid.clone())
        .collect();
    // What feeds a bank or a one-note track beside it: the Sum, in a
    // piece; every piece, in the toms.
    let from: Vec<String> = sum.map_or(pieces, |sum| vec![sum.guid.clone()]);
    let mics: Vec<String> = node
        .children
        .iter()
        .filter(|c| c.kind == Kind::Source && !c.is_folder())
        .map(|c| c.guid.clone())
        .collect();
    for child in &node.children {
        match child.kind {
            Kind::Verb if child.is_folder() => {
                for (i, ret) in child.children.iter().enumerate() {
                    feeds.extend(from.iter().map(|f| (f.clone(), ret.guid.clone())));
                    if i > 0 {
                        quiet.push(ret.guid.clone());
                    }
                }
            }
            Kind::Sub => {
                feeds.extend(from.iter().map(|f| (f.clone(), child.guid.clone())));
            }
            Kind::Fundamental => {
                feeds.extend(mics.iter().map(|m| (m.clone(), child.guid.clone())));
            }
            _ => {}
        }
        if child.kind != Kind::Process {
            feeds_in(child, feeds, quiet);
        }
    }
}

/// `top` (top-level tracks just made) routed to their buses as the
/// organizer files them — the buses made where the song has none — and
/// the top level put in the template's order: the Guide, the
/// instruments, the buses. Says where each went.
fn place(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    top: &[String],
) -> Result<Vec<String>, String> {
    use dynamic_template::apply::{DawTarget, apply_buses, classified_paths, route_to_bus};
    let mut target = DawTarget::on(daw.clone(), project.clone());
    let entries: Vec<_> = classified_paths(&target)
        .into_iter()
        .filter(|e| top.contains(&e.track))
        .collect();
    let buses = dynamic_template::buses_for_paths(entries.iter().map(|e| e.path.as_slice()));
    let applied = apply_buses(&mut target, &buses).map_err(|e| e.to_string())?;
    let mut routed = Vec::new();
    for entry in &entries {
        let Some(bus) = dynamic_template::bus_for_path(&entry.path) else {
            continue;
        };
        let Some(id) = applied.by_name.get(bus) else {
            continue;
        };
        route_to_bus(&mut target, &entry.track, id).map_err(|e| e.to_string())?;
        routed.push(format!("{} to {bus}", entry.name));
    }
    let mut last: Vec<&str> = vec![
        dynamic_template::apply::REFERENCES_FOLDER,
        "CLICK + GUIDE BUS",
        "MIX BUS",
        "UNSORTED",
    ];
    for spec in dynamic_template::BUS_TREE {
        if !last.iter().any(|n| n.eq_ignore_ascii_case(spec.name)) {
            last.push(spec.name);
        }
    }
    target
        .order_top_level(&["Guide", "Keyflow"], &last)
        .map_err(|e| e.to_string())?;
    Ok(routed)
}

/// `names` as a JSON array of strings.
fn serde_json_list(names: &[String]) -> String {
    let quoted: Vec<String> = names
        .iter()
        .map(|n| format!("\"{}\"", n.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    format!("[{}]", quoted.join(","))
}
