//! The base actions: the transport, the song's sections, tracks and
//! items, the window's mode, docks and controls, saving, undo and the
//! song's settings. They go with `engine-core`.

#[allow(
    clippy::wildcard_imports,
    reason = "split out of actions.rs: the parent's helpers and imports, as before"
)]
use super::*;

/// A time in the song, seconds from its start.
#[derive(Facet)]
pub(crate) struct AtArg {
    at: f64,
}

/// The transport's actions: the engine's, not the window's — they run on
/// it at once, the window or none (headless, hidden, another monitor), and
/// a window shows what the engine does.
pub(crate) fn register_transport(list: &ActionList) -> Result<(), architect::action::ActionError> {
    for (meta, command) in [
        (&PLAY_STOP, crate::engine::Move::PlayStop),
        (&PLAY_PAUSE, crate::engine::Move::PlayStop),
    ] {
        list.register(meta, Effect::Mutating, move |(): ()| {
            crate::engine::transport_here(command, 0.0);
            if crate::engine::buffering::waiting() {
                let waiting = crate::open::current_song()
                    .and_then(|s| crate::stream_set::readiness(&s))
                    .map_or(0, |r| r.waiting_on);
                return Ok(format!(
                    "waiting: {waiting} recordings are not here yet — it plays by itself once they are (press again to call it off)"
                ));
            }
            Ok(format!("{}: done", meta.display_name))
        })?;
    }
    // Home is the engine's too; the window, when there is one, also takes
    // its edit cursor home (idempotent: it asks the engine again).
    list.register(&GO_TO_START, Effect::Mutating, |(): ()| {
        crate::engine::transport_here(crate::engine::Move::Home, 0.0);
        crate::window_queue::ask(crate::window_queue::Asked::GoToStart);
        Ok(format!("{}: done", GO_TO_START.display_name))
    })?;
    for (meta, command) in [
        (&STOP, crate::engine::Move::Stop),
        (&GO_TO_END, crate::engine::Move::End),
        (&LOOP, crate::engine::Move::ToggleLoop),
    ] {
        list.register(meta, Effect::Mutating, move |(): ()| {
            crate::engine::transport_here(command, 0.0);
            Ok(format!("{}: done", meta.display_name))
        })?;
    }
    for (meta, command) in [
        (&SEEK, crate::engine::Move::Seek),
        (&PLAY_FROM, crate::engine::Move::PlayFrom),
    ] {
        list.register(meta, Effect::Mutating, move |a: AtArg| {
            if !a.at.is_finite() || a.at < 0.0 {
                return Err("`at` is seconds from the start, 0 or more".to_owned());
            }
            crate::engine::transport_here(command, a.at);
            Ok(format!("{}: {:.3} s", meta.display_name, a.at))
        })?;
    }
    Ok(())
}

pub(crate) static TRACKS: ActionMeta = ActionMeta {
    category: "Session",
    ..meta(
        "SESSION_TRACKS",
        "tracks",
        "Tracks",
        "Every track in the open song: GUID, name, its folder path, folder or not, mute, solo, fader (dB), pan, how many items, its FX, its lanes, whether it is armed and what it records from (`record_input`: none, `audio N`, midi), and for a folder whether the window shows it open (`open`; false: folded to its row). `filter` keeps the tracks whose path contains it.",
        "Data",
        "",
    )
};

/// Which tracks.
#[derive(Facet)]
pub(crate) struct TracksArg {
    /// Keep the tracks whose folder path contains this (case aside).
    filter: Option<String>,
}

/// A track, as the list describes it.
#[derive(Facet)]
pub(crate) struct TrackRow {
    guid: String,
    name: String,
    /// The folders it is in and its name: `Drums/Drum Kit/Kick/Sum/In`.
    path: String,
    folder: bool,
    muted: bool,
    soloed: bool,
    /// Its polarity is flipped.
    inverted: bool,
    volume_db: f64,
    pan: f64,
    items: u32,
    fx: Vec<String>,
    /// Its fixed lanes, when it has them, named lanes and the playing
    /// one first — then the rest only by count.
    lanes: Vec<LaneRow>,
    /// Armed to record.
    armed: bool,
    /// What it records from: `none`, `audio 1` (an input, from 1), `midi`
    /// or `raw N`.
    record_input: String,
    /// A folder: whether the window shows what is in it (`false`: folded
    /// to its own row). None for a track.
    open: Option<bool>,
}

/// What `input` says, as [`TrackRow::record_input`] reads.
pub(crate) fn record_input_of(input: daw_proto::track::RecordInput) -> String {
    use daw_proto::track::RecordInput;
    match input {
        RecordInput::None => "none".to_owned(),
        RecordInput::Audio { channel } => format!("audio {}", channel.saturating_add(1)),
        RecordInput::Midi { .. } => "midi".to_owned(),
        RecordInput::Raw(raw) => format!("raw {raw}"),
    }
}

/// Which folders the window shows open, by GUID: every track planned as
/// the window plans its rows ([`crate::folds::apply`]), a folder open
/// when the row after it is one of its own.
pub(crate) fn open_folders(tracks: &[daw_proto::Track]) -> std::collections::HashSet<String> {
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let depth = |track: &daw_proto::Track| {
        let mut depth = 0_u32;
        let mut at = track.parent_guid.as_deref();
        while let Some(parent) = at.and_then(|p| by_guid.get(p)) {
            depth = depth.saturating_add(1);
            at = parent.parent_guid.as_deref();
        }
        depth
    };
    let rows: Vec<(daw_proto::Track, u32)> = tracks.iter().map(|t| (t.clone(), depth(t))).collect();
    let shown = crate::folds::apply(rows);
    shown
        .iter()
        .enumerate()
        .filter(|(i, (track, _))| track.is_folder && !crate::folds::shut(&shown, *i))
        .map(|(_, (track, _))| track.guid.clone())
        .collect()
}

/// One fixed lane.
#[derive(Facet)]
pub(crate) struct LaneRow {
    lane: u32,
    name: String,
    items: u32,
    plays: bool,
}

/// `track`'s lanes worth listing: those named something other than their
/// number, and those playing.
pub(crate) fn lane_rows(track: &daw_proto::Track, items: &[daw_proto::Item]) -> Vec<LaneRow> {
    (0..track.lane_count)
        .filter_map(|lane| {
            let name = usize::try_from(lane)
                .ok()
                .and_then(|i| track.lane_names.get(i))
                .cloned()
                .unwrap_or_default();
            let plays = lane < 64 && track.lane_play_mask & (1_u64 << lane) != 0;
            let numbered = name.trim().parse::<u32>().is_ok() || name.trim().is_empty();
            (plays || !numbered).then(|| LaneRow {
                lane,
                items: u32::try_from(
                    items
                        .iter()
                        .filter(|i| i.fixed_lane.unwrap_or(0) == lane)
                        .count(),
                )
                .unwrap_or(u32::MAX),
                name,
                plays,
            })
        })
        .collect()
}

/// Which mode.
#[derive(Facet)]
pub(crate) struct ModeArg {
    mode: String,
}

pub(crate) static SECTIONS: ActionMeta = meta(
    "SESSION_SECTIONS",
    "sections",
    "Sections",
    "The song's sections (its regions), in order: name, start and end seconds, and the bars they span — what SESSION_VIEW's `section` names.",
    "View",
    "",
);

/// Moved down into the core (`fx_actions::moment` reads them).
pub use engine_core::moment::{SectionOut, sections};

pub(crate) static CHART_AT: ActionMeta = meta(
    "SESSION_CHART_AT",
    "chart_at",
    "Where the chart is",
    "Where the current song's chart is at `at` seconds (left out: the playhead), as a chart following the playhead shows it: the `bar` as played (1 the first after the count-in; a repeat's second pass is the next bars along), the `written` bar it plays and its `pass`, its `section`, and the bar's `start` and `end`. From the song's Keyflow chart (the `.kf` beside it: `source` chart); a song with no `.kf` follows the chart its session shows (`source` session, no `written` or `pass`): its bars counted from the tempo map and its sections its markers', as the chart drawn in the session's bars numbers and heads them, the one Keyflow generates from the markers when nobody has charted it (SESSION_CHART shows which). No `bar` in a count-in or past the end. Read-only.",
    "View",
    "",
);

/// When, seconds; left out, the playhead.
#[derive(Facet)]
pub(crate) struct ChartAtArg {
    at: Option<f64>,
}

/// Where the chart is, as `SESSION_CHART_AT` says it.
#[derive(Facet)]
pub(crate) struct ChartAtOut {
    at: f64,
    source: String,
    bar: Option<u32>,
    written: Option<u32>,
    pass: Option<u32>,
    section: Option<String>,
    start: Option<f64>,
    end: Option<f64>,
}

/// Where the open song's chart is at `at` (the playhead when `None`): the
/// reads first (the song, the playhead, its chart file), then the answer
/// from the chart or from the markers.
fn chart_at(at: Option<f64>) -> Result<ChartAtOut, String> {
    let (daw, project) = song()?;
    let at = match at {
        Some(at) => at,
        None => playhead().ok_or("the song has no playhead")?,
    };
    let guid = engine_core::moment::project_guid(&project)?;
    let chart = daw
        .read_project(&guid, |p| p.info.path.clone())
        .and_then(|path| chart_beside(std::path::Path::new(&path)));
    let sections = sections(daw, &project);
    match chart {
        Some(file) => {
            let text =
                std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            Ok(from_chart(
                at,
                engine_core::chart_at::chart_bar(&text, at)?,
                &sections,
            ))
        }
        None => {
            let bar = crate::song_bars::musical(daw, &project, at).0;
            let span = (
                crate::song_bars::time(daw, &project, bar, 1, 0.0),
                crate::song_bars::time(daw, &project, bar.saturating_add(1), 1, 0.0),
            );
            Ok(from_markers(at, bar, span, &sections))
        }
    }
}

/// The current song's playhead, seconds.
fn playhead() -> Option<f64> {
    let song = crate::open::current_song()?;
    crate::stream_set::transport_of(&song).map(|(position, _)| position)
}

/// The song's Keyflow chart: the one `.kf` beside its file.
fn chart_beside(song: &std::path::Path) -> Option<std::path::PathBuf> {
    use crate::folder::Folder as _;
    crate::folder::Disk.only_with_extension(song.parent()?, "kf")
}

/// The section of `sections` that `at` lies in.
fn section_at(sections: &[SectionOut], at: f64) -> Option<&SectionOut> {
    sections.iter().find(|s| s.start <= at && at < s.end)
}

/// The answer from the chart's bar at `at`, its section named as the song
/// names it: its region there (`Verse 1`), the chart's own name (`Verse`)
/// where it has none.
fn from_chart(
    at: f64,
    bar: Option<engine_core::chart_at::ChartBar>,
    sections: &[SectionOut],
) -> ChartAtOut {
    let Some(bar) = bar else {
        return ChartAtOut {
            at,
            source: "chart".to_owned(),
            bar: None,
            written: None,
            pass: None,
            section: None,
            start: None,
            end: None,
        };
    };
    let region = section_at(sections, f64::midpoint(bar.start, bar.end)).map(|s| s.name.clone());
    ChartAtOut {
        at,
        source: "chart".to_owned(),
        bar: Some(bar.bar),
        written: Some(bar.written),
        pass: Some(bar.pass),
        section: Some(region.unwrap_or(bar.section)),
        start: Some(bar.start),
        end: Some(bar.end),
    }
}

/// The answer for a song with no chart file, on the chart its session
/// shows (generated from its markers when nobody has charted it,
/// studio#693): `bar` the one the tempo map counts at `at` (`span` its
/// start and end), the section the regions name, as that chart numbers and
/// heads them; nothing past the last section.
fn from_markers(at: f64, bar: i32, span: (f64, f64), sections: &[SectionOut]) -> ChartAtOut {
    let inside = at < sections.iter().map(|s| s.end).fold(0.0, f64::max);
    ChartAtOut {
        at,
        source: "session".to_owned(),
        bar: u32::try_from(bar).ok().filter(|b| inside && *b > 0),
        written: None,
        pass: None,
        section: section_at(sections, at).map(|s| s.name.clone()),
        start: inside.then_some(span.0),
        end: inside.then_some(span.1),
    }
}

pub(crate) fn register_sections(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&SECTIONS, Effect::ReadOnly, |(): ()| {
        let (daw, project) = song()?;
        Ok(sections(daw, &project))
    })?;
    list.register(&CHART_AT, Effect::ReadOnly, |a: ChartAtArg| chart_at(a.at))?;
    Ok(())
}

pub(crate) fn register_phase(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&PHASE, Effect::Mutating, |a: PhaseArg| {
        let (mode, slug) = crate::shared_actions::pick_phase(a.mode.as_deref(), &a.phase)?;
        engine_domain::mode_phases::choose(mode, slug);
        also_in_the_window(
            PHASE.id,
            format!(r#"{{"phase":"{}","mode":"{}"}}"#, slug, mode.display_name()),
        );
        Ok(())
    })?;
    Ok(())
}

pub(crate) static WINDOW_TARGET: ActionMeta = meta(
    "SESSION_WINDOW_TARGET",
    "window_target",
    "Which window the actions drive",
    "With more than one window following the engine, which one its window actions (SESSION_VIEW, SESSION_ZOOM, SESSION_FRAMES, SESSION_WINDOW, SESSION_MODE…) go to: `window` by name (FTS_WINDOW_NAME, `main` by default); empty or left out: whichever takes them first. Says the windows seen and the one now named.",
    "View",
    "",
);

/// Which window, by name.
#[derive(Facet)]
pub(crate) struct WindowTargetArg {
    window: Option<String>,
}

/// What [`WINDOW_TARGET`] answers.
#[derive(Facet)]
pub(crate) struct WindowTarget {
    /// The window named now; `None`: whichever takes them first.
    target: Option<String>,
    /// The windows that have taken the engine's asks, newest last.
    seen: Vec<String>,
}

pub(crate) fn register_window_target(
    list: &ActionList,
) -> Result<(), architect::action::ActionError> {
    list.register(&WINDOW_TARGET, Effect::Mutating, |a: WindowTargetArg| {
        if let Some(window) = a.window {
            crate::window_queue::set_target(Some(window));
        }
        Ok(WindowTarget {
            target: crate::window_queue::target(),
            seen: crate::window_queue::seen_windows(),
        })
    })?;
    Ok(())
}

pub(crate) fn register_mode(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&MODE, Effect::Mutating, |a: ModeArg| {
        let mode = engine_domain::modes::Mode::ALL
            .into_iter()
            .find(|m| m.display_name().eq_ignore_ascii_case(a.mode.trim()))
            .ok_or_else(|| {
                let names: Vec<&str> = engine_domain::modes::Mode::ALL
                    .iter()
                    .map(|m| m.display_name())
                    .collect();
                format!("no mode {}: one of {names:?}", a.mode)
            })?;
        request_mode(mode);
        also_in_the_window(MODE.id, format!(r#"{{"mode":"{}"}}"#, mode.display_name()));
        Ok(())
    })?;
    Ok(())
}

pub static SCREENSET: ActionMeta = meta(
    "SESSION_SCREENSET",
    "screenset",
    "Screenset",
    "Recall a screenset — a saved layout of the window's panels — by its number or name: 1 Performance, 2 Overview, 3 Session, 4 Mix, 5 Edit, 6 Charts, 7 Signal, or one saved since.",
    "View",
    "",
);

pub static SCREENSET_SAVE: ActionMeta = meta(
    "SESSION_SCREENSET_SAVE",
    "save_screenset",
    "Save Screenset",
    "Keep the window's layout in the screenset on show, for this kind of screen (phone, tablet or desk, upright or sideways).",
    "View",
    "",
);

pub static SCREENSET_SAVE_AS: ActionMeta = meta(
    "SESSION_SCREENSET_SAVE_AS",
    "save_screenset_as",
    "Save Screenset As",
    "Keep the window's layout as a new screenset, named `name` (else numbered), and show it.",
    "View",
    "",
);

pub static ARRANGE: ActionMeta = meta(
    "SESSION_ARRANGE",
    "arrange",
    "Arrange Panels",
    "Turn arranging on or off: every panel shows its name and can be dragged to an edge, beside another, or onto it as a tab (holding W does the same while held).",
    "View",
    "",
);

pub static PANEL: ActionMeta = meta(
    "SESSION_PANEL",
    "panel",
    "Show or Hide a Panel",
    "Open a panel of `kind` where that kind lives when the window shows none, or close every one it shows: arrangement, mixer, editor, fx-row, transport, progress, chart, chart-editor, lyrics, set, setlist, browser, inspector, versions, collab, rig, switches, navigator, now, gig-chart, tones.",
    "View",
    "",
);

/// Which panel kind.
#[derive(Facet)]
pub(crate) struct PanelArg {
    kind: String,
}

/// Which screenset: its number key or its name.
#[derive(Facet)]
pub(crate) struct ScreensetArg {
    screenset: String,
}

/// A name for a new screenset, if any.
#[derive(Facet)]
pub(crate) struct SaveAsArg {
    #[facet(default)]
    name: Option<String>,
}

/// The dock actions' one argument.
#[derive(Facet)]
pub(crate) struct DockIntentArg {
    intent: dock_model::DockIntent,
}

/// The action a dock intent runs and its arguments (JSON) — for the layout's
/// intents; `None` for a screenset's or Arrange's, which run
/// `SESSION_SCREENSET*` / `SESSION_ARRANGE` by name.
#[must_use]
pub fn dock_action(intent: &dock_model::DockIntent) -> Option<(&'static str, String)> {
    let id = dock_intent_id(intent)?;
    let arg = DockIntentArg {
        intent: intent.clone(),
    };
    facet_json::to_string(&arg).ok().map(|json| (id, json))
}

/// Which dock action names `intent`.
pub(crate) fn dock_intent_id(intent: &dock_model::DockIntent) -> Option<&'static str> {
    use dock_model::DockIntent as I;
    Some(match intent {
        I::Move { .. } | I::Attach { .. } => PANEL_MOVE.id,
        I::Open { .. } | I::Duplicate { .. } => PANEL_OPEN.id,
        I::Close { .. } => PANEL_CLOSE.id,
        I::Show { .. } => PANEL_SHOW.id,
        I::Maximize { .. } => PANEL_MAXIMIZE.id,
        I::Chrome { .. } => PANEL_TABS.id,
        I::Share { .. } => DOCK_SPLIT.id,
        I::EdgeSize { .. } | I::EdgeShown { .. } => DOCK_EDGE.id,
        I::FloatBounds { .. } => DOCK_FLOAT.id,
        I::Recall { .. } | I::Save | I::SaveAs { .. } | I::Arrange => return None,
    })
}

pub(crate) fn register_dock(list: &ActionList) -> Result<(), architect::action::ActionError> {
    for meta in [
        &PANEL_MOVE,
        &PANEL_OPEN,
        &PANEL_CLOSE,
        &PANEL_SHOW,
        &PANEL_MAXIMIZE,
        &PANEL_TABS,
        &DOCK_SPLIT,
        &DOCK_EDGE,
        &DOCK_FLOAT,
    ] {
        list.register(meta, Effect::Mutating, move |a: DockIntentArg| {
            if dock_intent_id(&a.intent) != Some(meta.id) {
                return Err(format!("{} does not take that intent", meta.id));
            }
            let args = facet_json::to_string(&a).map_err(|e| e.to_string())?;
            request_dock(DockRequest::Intent(a.intent));
            also_in_the_window(meta.id, args);
            Ok(())
        })?;
    }
    list.register(&SCREENSET, Effect::Mutating, |a: ScreensetArg| {
        let which = a.screenset.trim().to_owned();
        if which.is_empty() {
            return Err("name a screenset: its number or its name".to_owned());
        }
        request_dock(DockRequest::Recall(which.clone()));
        let quoted = facet_json::to_string(&which).unwrap_or_default();
        also_in_the_window(SCREENSET.id, format!(r#"{{"screenset":{quoted}}}"#));
        Ok(())
    })?;
    list.register(&SCREENSET_SAVE, Effect::Mutating, |(): ()| {
        request_dock(DockRequest::Save);
        also_in_the_window(SCREENSET_SAVE.id, "{}".to_owned());
        Ok(())
    })?;
    list.register(&SCREENSET_SAVE_AS, Effect::Mutating, |a: SaveAsArg| {
        let name = a
            .name
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty());
        request_dock(DockRequest::SaveAs(name.clone()));
        let args = name.map_or_else(
            || "{}".to_owned(),
            |n| {
                format!(
                    r#"{{"name":{}}}"#,
                    facet_json::to_string(&n).unwrap_or_default()
                )
            },
        );
        also_in_the_window(SCREENSET_SAVE_AS.id, args);
        Ok(())
    })?;
    list.register(&PANEL, Effect::Mutating, |a: PanelArg| {
        let kind = a.kind.trim().to_owned();
        if kind.is_empty() {
            return Err("name a panel kind".to_owned());
        }
        let quoted = facet_json::to_string(&kind).unwrap_or_default();
        request_dock(DockRequest::TogglePanel(kind));
        also_in_the_window(PANEL.id, format!(r#"{{"kind":{quoted}}}"#));
        Ok(())
    })?;
    list.register(&DOCKS, Effect::Mutating, |(): ()| {
        request_dock(DockRequest::ToggleDocks);
        also_in_the_window(DOCKS.id, "{}".to_owned());
        Ok(())
    })?;
    list.register(&ARRANGE, Effect::Mutating, |(): ()| {
        request_dock(DockRequest::Arrange);
        also_in_the_window(ARRANGE.id, "{}".to_owned());
        Ok(())
    })?;
    Ok(())
}

pub(crate) static EDIT_LANES: ActionMeta = meta(
    "DRUMS_EDIT_LANES",
    "drums_edit_lanes",
    "Comp and edit lanes",
    "Set the kit up to be edited, every drum track alike: its comping lane (the comp that plays) named COMP, and a new lane EDIT holding a copy of it, playing — so edits land on EDIT and COMP stays the comp they were made from (DRUMS_PLAY_LANE to hear either). The takes and their comp areas are untouched: EDIT is added after the last lane. A track already set up is left — unless `reset`, which starts the edit over: each EDIT lane's slices dropped and its COMP copied onto it again (one undo step). `dry_run` says what it would do. Sets a mark on the song first, in its history (`Before DRUMS_EDIT_LANES <date time>`, named in the result's `checkpoint`; SESSION_HISTORY_RESTORE `mark` goes back to it), and is refused if one cannot be made. `mark` (any mutating drum action takes one): a mark of that name set on the song first, a restore point SESSION_HISTORY_RESTORE `mark` goes back to; every call is a step of its own in the history, never merged with the one before.",
    "Drums",
    "",
);

pub(crate) static PLAY_LANE: ActionMeta = meta(
    "DRUMS_PLAY_LANE",
    "drums_play_lane",
    "Play a lane",
    "Play the kit's lane named `lane` (COMP, EDIT, or any lane's name) on every drum track at once — the edited drums against the comp they were made from.",
    "Drums",
    "",
);

/// Whether to only say.
#[derive(Facet)]
pub(crate) struct LanesArg {
    /// A mark of this name set on the song first (`history_marks`).
    mark: Option<String>,
    dry_run: Option<bool>,
    /// Start the edit over: each EDIT lane's slices dropped and its COMP
    /// copied onto it again.
    reset: Option<bool>,
}

/// What setting the lanes up did, track by track.
#[derive(Facet)]
pub(crate) struct LanesOut {
    /// `path: comp lane → EDIT lane (n items copied)`, or why it was left.
    tracks: Vec<String>,
    applied: bool,
    /// The checkpoint forked before (`safety_net`); none for a dry run.
    checkpoint: Option<String>,
}

/// A lane to play.
#[derive(Facet)]
pub(crate) struct PlayLaneArg {
    lane: String,
}

/// Where to save, if not the song's own `.session`. Only `to`: a
/// misnamed argument (`dir`) would otherwise save over the song's own.
#[cfg(feature = "native")]
#[derive(Facet)]
#[facet(deny_unknown_fields)]
pub(crate) struct SaveArg {
    to: Option<String>,
}

#[cfg(feature = "native")]
pub(crate) static CHECK_SAVED: ActionMeta = meta(
    "SESSION_CHECK_SAVED",
    "session_check_saved",
    "Check a saved song",
    "Reopen a saved project file (`dir`: a `Song.session`, or an alternative `Song.session/Organized`) as a second song, as a reopen would, and set it beside the song open now as the project doc reads both: every track with its FX and what they hold, sends, automation, lanes and groups; every item with its take, stretch markers and notes; markers, regions and tempo. Says `the same` or what differs. The second song is closed again.",
    "Session",
    "",
);

/// The folder to check.
#[cfg(feature = "native")]
#[derive(Facet)]
pub(crate) struct CheckSavedArg {
    dir: String,
}

#[cfg(feature = "native")]
pub(crate) static MARK_KINDS: ActionMeta = meta(
    "SESSION_MARK_KINDS",
    "session_mark_kinds",
    "Mark the kit's kinds",
    "Write on every track of the kit what the golden template says it is — piece, Sum, mic, trigger, sub, sample, reverb — matched by name, for a song prepared before kinds were kept on the track. The scenes (Edit mode's rows above all) read them there. One undo step. Says how many tracks were marked.",
    "Session",
    "",
);

pub(crate) static CHECKPOINT: ActionMeta = meta(
    "SESSION_CHECKPOINT",
    "session_checkpoint",
    "Save a checkpoint",
    "Save the song and fork a named alternative from it as it is now (`name`: \"Organized\", \"Mix A\") — a branch of the song's own project file (`Song.session`), sharing everything it had and keeping only what is done to it after, the way Logic keeps alternatives. The song stays open, under its own name. Open one later from the start screen's Recent or SESSION_OPEN.",
    "Session",
    "",
);

/// The checkpoint's name.
#[cfg(feature = "native")]
#[derive(Facet)]
pub(crate) struct CheckpointArg {
    name: String,
}

#[cfg(feature = "native")]
pub(crate) static ALTERNATIVES: ActionMeta = meta(
    "SESSION_ALTERNATIVES",
    "session_alternatives",
    "The song's alternatives",
    "The open song's own version and its alternatives (checkpoints saved with SESSION_CHECKPOINT), the newest first: name, folder, when saved. Read-only.",
    "Session",
    "",
);

#[cfg(feature = "native")]
pub(crate) static STREAMS: ActionMeta = meta(
    "SESSION_STREAMS",
    "session_streams",
    "What is streaming",
    "Every song streamed in from elsewhere (a Task library, a share link, another engine) and how far along it is: `state` local (every recording it plays is here, kept on disk), arriving, or streaming (nothing yet); its recordings, each with its bytes and how many have arrived, the least arrived first; where they are kept. `song` (the engine project) narrows it to one; `top` caps the recordings listed (all when left out). Read-only.",
    "Session",
    "",
);

#[cfg(feature = "native")]
pub(crate) static STREAM_QUALITY: ActionMeta = meta(
    "SESSION_STREAM_QUALITY",
    "session_stream_quality",
    "Streaming quality",
    "What streamed songs are heard from, on this device: `proxies` (the compressed proxies only; an original is never fetched) or `full` (the proxies first, then — once every song's proxies are here — the original recordings, fetched one at a time behind everything else, each take moving to its original the next time its song is stopped). Left out, says what it is. Kept for every set.",
    "Session",
    "",
);

/// The quality to stream at, or nothing to ask what it is.
#[cfg(feature = "native")]
#[derive(Facet)]
pub(crate) struct StreamQualityArg {
    quality: Option<String>,
}

/// Which song, and how many of its recordings to list.
#[cfg(feature = "native")]
#[derive(Facet)]
pub(crate) struct StreamsArg {
    song: Option<String>,
    top: Option<u32>,
}

#[cfg(feature = "native")]
pub(crate) static RECENT: ActionMeta = meta(
    "SESSION_RECENT",
    "session_recent",
    "Songs opened lately",
    "The songs and setlists opened lately, the newest first: name, the version that was open (the song, or which alternative), when, and the song's alternatives. Read-only.",
    "Session",
    "",
);

/// Whether the open song has changes it was not saved with; `None` when
/// that is not known.
#[cfg(feature = "native")]
#[must_use]
pub(crate) fn open_song_unsaved() -> Option<bool> {
    let (daw, ProjectContext::Project(guid)) = song().ok()? else {
        return None;
    };
    crate::projects::unsaved(daw, &guid)
}

/// The open song's versions, for the top bar's Versions.
///
/// The song's own (`"The song"`) and its checkpoints, the newest first —
/// each its name, when it was saved, where it is, and whether it is the
/// one open.
#[cfg(feature = "native")]
#[must_use]
pub(crate) fn open_song_versions() -> Vec<crate::projects::Version> {
    let Ok((daw, ProjectContext::Project(guid))) = song() else {
        return Vec::new();
    };
    let Some(open) = daw.read_project(&guid, |p| std::path::PathBuf::from(&p.info.path)) else {
        return Vec::new();
    };
    let Ok(base) = song_base(daw, &guid) else {
        return Vec::new();
    };
    let same = |a: &std::path::Path, b: &std::path::Path| {
        std::path::absolute(a).ok() == std::path::absolute(b).ok()
    };
    let mut out: Vec<crate::projects::Version> = crate::projects::main_of(&base)
        .map(|path| crate::projects::Version {
            name: "The song".to_owned(),
            when: String::new(),
            open: crate::projects::alternative_name(&open).is_none(),
            path,
        })
        .into_iter()
        .collect();
    out.extend(crate::projects::alternatives(&base).into_iter().map(|alt| {
        crate::projects::Version {
            open: same(&alt.path, &open),
            name: alt.name,
            when: crate::projects::ago(alt.saved),
            path: alt.path,
        }
    }));
    out
}

/// The open song's base ([`crate::projects::base_of`]): what its
/// alternatives are kept beside.
#[cfg(feature = "native")]
pub(crate) fn song_base(
    daw: &daw::standalone::Standalone,
    guid: &str,
) -> Result<std::path::PathBuf, String> {
    let path = daw
        .read_project(guid, |p| p.info.path.clone())
        .ok_or("no song open")?;
    crate::projects::base_of(std::path::Path::new(&path))
        .ok_or_else(|| format!("{path} is not a song's file: nowhere to keep alternatives"))
}

#[cfg(feature = "native")]

pub(crate) static JOBS: ActionMeta = meta(
    "SESSION_JOBS",
    "session_jobs",
    "Work in the background",
    "What is being done off the window's thread right now — reading waveforms, slicing them, opening or saving a project file, an offline render (`Measuring the mix`, `Rendering the mix`, `Listening`) — each with its `id`, how far along (`done` of `total`; a total of 0 is work that does not know its length), and whether SESSION_JOB_CANCEL can stop it (`cancellable`). What the bottom bar shows. Empty when nothing is running. Read-only.",
    "View",
    "",
);

/// One job, as listed.
#[derive(Facet)]
pub(crate) struct JobRow {
    id: u64,
    label: String,
    done: u64,
    total: u64,
    cancellable: bool,
}

pub(crate) static JOB_CANCEL: ActionMeta = meta(
    "SESSION_JOB_CANCEL",
    "session_job_cancel",
    "Stop work in the background",
    "Stop the background job `id` (a SESSION_JOBS row's): an offline render (MIX_MEASURE, MIX_RENDER, LISTEN) stops at its next block and answers that it was cancelled, rendering nothing more. Answers `cancelled` (asked to stop), `not_cancellable` (work that cannot stop part way) or `not_running` (no such job now; it may just have finished). Touches neither the song nor playback.",
    "View",
    "",
);

/// The job to stop.
#[derive(Facet)]
pub(crate) struct JobCancelArg {
    id: u64,
}

pub(crate) static ITEM_STYLE: ActionMeta = meta(
    "SESSION_ITEM_STYLE",
    "session_item_style",
    "Item style",
    "How items are drawn: `outlined: true` (the default) a faint body in the item's colour with the waveform standing out over it in the full colour and a line round each item; `false` the half-strength block of colour the waveform is read against.",
    "View",
    "",
);

/// Outlined or not.
#[derive(Facet)]
pub(crate) struct ItemStyleArg {
    outlined: bool,
}

pub(crate) static STOP_BEHAVIOUR: ActionMeta = meta(
    "SESSION_STOP_BEHAVIOUR",
    "session_stop_behaviour",
    "Stop behaviour",
    "What Stop does with the cursor: `behaviour: \"return\"` (the default) puts it back where play started, `\"stay\"` leaves it where it stopped. Any other value, or an empty one, only reads the setting.",
    "Transport",
    "",
);

/// The stop behaviour to set, or nothing to read it.
#[derive(Facet)]
pub(crate) struct StopBehaviourArg {
    behaviour: String,
}

pub(crate) fn register_style(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&STOP_BEHAVIOUR, Effect::Mutating, |a: StopBehaviourArg| {
        if let Some(stop) = crate::play_return::Stop::from_slug(&a.behaviour) {
            crate::play_return::set(stop);
        }
        Ok(crate::play_return::get().slug())
    })?;
    list.register(&JOBS, Effect::ReadOnly, |(): ()| {
        Ok(crate::jobs::running()
            .into_iter()
            .map(
                |crate::jobs::Running {
                     id,
                     label,
                     done,
                     total,
                 }| JobRow {
                    id,
                    label,
                    done: u64::try_from(done).unwrap_or(u64::MAX),
                    total: u64::try_from(total).unwrap_or(u64::MAX),
                    cancellable: crate::jobs::cancellable(id),
                },
            )
            .collect::<Vec<_>>())
    })?;
    list.register(&JOB_CANCEL, Effect::Mutating, |a: JobCancelArg| {
        Ok::<_, String>(match crate::jobs::cancel(a.id) {
            crate::jobs::Cancelled::Asked => "cancelled",
            crate::jobs::Cancelled::NotCancellable => "not_cancellable",
            crate::jobs::Cancelled::NotRunning => "not_running",
        })
    })?;
    list.register(&ITEM_STYLE, Effect::Mutating, |a: ItemStyleArg| {
        crate::options::OUTLINED.set(a.outlined);
        // The lanes are recorded with their bodies: read back, drawn anew.
        crate::song_model::request_resync();
        Ok(if a.outlined { "outlined" } else { "filled" })
    })?;
    Ok(())
}

pub(crate) static MULTITRACKS: ActionMeta = meta(
    "SESSION_MULTITRACKS",
    "session_multitracks",
    "Load the multitracks",
    "Hear every streamed song by its stems: they all come in now, and each song moves over from its stereo reference as they arrive. Says how the songs are heard (`stems`, and `referenced` while any still plays its reference).",
    "Streaming",
    "",
);

pub(crate) static GROUPING: ActionMeta = meta(
    "SESSION_GROUPING",
    "session_grouping",
    "Grouping",
    "Whether a fader, mute or solo moved on a grouped track moves its group (`on`). Says what it is now.",
    "Mix",
    "",
);

/// Grouping on or off.
#[cfg(feature = "native")]
#[derive(Facet)]
pub(crate) struct GroupingArg {
    on: bool,
}

pub(crate) static ACTIONS: ActionMeta = meta(
    "SESSION_ACTIONS",
    "session_actions",
    "Actions a palette runs",
    "The actions that take nothing and run at once — what a command palette offers — each its `id`, `label`, `group`, `shortcut`, `description` and whether it can run now (`enabled`). Read-only.",
    "View",
    "",
);

/// An action a palette runs.
#[cfg(feature = "native")]
#[derive(Facet)]
pub struct PaletteRow {
    pub id: String,
    pub label: String,
    pub group: String,
    pub shortcut: String,
    pub description: String,
    pub enabled: bool,
}

/// The actions a palette offers, as the engine lists them
/// (`SESSION_ACTIONS`): none when it cannot say.
#[cfg(feature = "native")]
#[must_use]
pub fn palette() -> Vec<PaletteRow> {
    run(ACTIONS.id, b"")
        .ok()
        .and_then(|bytes| facet_json::from_slice::<Vec<PaletteRow>>(&bytes).ok())
        .unwrap_or_default()
}

/// What a window's controls ask of the engine: the multitracks, grouping,
/// the song's versions and the palette's actions — so a window in another
/// process (and MCP, and the CLI) reads and sets them where they live.
#[cfg(feature = "native")]
pub(crate) fn register_window_controls(
    list: &ActionList,
) -> Result<(), architect::action::ActionError> {
    list.register(&MULTITRACKS, Effect::Mutating, |(): ()| {
        crate::reference_play::load_multitracks();
        Ok::<_, String>(crate::view::Listening {
            stems: crate::reference_play::multitracks(),
            referenced: crate::reference_play::any(),
        })
    })?;
    list.register(&GROUPING, Effect::Mutating, |a: GroupingArg| {
        crate::options::GROUPING.set(a.on);
        crate::options::grouping_changed(a.on);
        Ok::<_, String>(a.on)
    })?;
    list.register(&VERSIONS, Effect::ReadOnly, |(): ()| {
        Ok::<_, String>(VersionsRead {
            versions: open_song_versions()
                .into_iter()
                .map(|v| VersionRow {
                    name: v.name,
                    when: v.when,
                    path: v.path.to_string_lossy().into_owned(),
                    open: v.open,
                })
                .collect(),
            unsaved: open_song_unsaved(),
        })
    })?;
    list.register(&ACTIONS, Effect::ReadOnly, |(): ()| {
        let all = crate::actions::list();
        Ok::<_, String>(
            all.infos()
                .into_iter()
                .filter(|a| a.params_schema.is_none() && !all.is_async(&a.id))
                .map(|a| PaletteRow {
                    label: if a.display_name.is_empty() {
                        a.id.clone()
                    } else {
                        a.display_name
                    },
                    id: a.id,
                    group: if a.group.is_empty() {
                        a.category
                    } else {
                        a.group
                    },
                    shortcut: a.shortcut,
                    description: a.description,
                    enabled: a.enabled,
                })
                .collect::<Vec<_>>(),
        )
    })?;
    Ok(())
}

#[cfg(feature = "native")]
pub(crate) fn register_undo(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&FOLD, Effect::Mutating, |a: FoldArg| {
        // Told by the engine which folders it changed: set here as settled.
        if !a.folders.is_empty() {
            for folder in &a.folders {
                crate::folds::toggle(folder, !a.open);
            }
            return Ok(a.folders);
        }
        let (daw, project) = song()?;
        let tracks = daw::service::Tracks::all(daw, project);
        let guid = crate::shared_actions::resolve_in(&tracks, &a.track)?;
        let changed = crate::shared_actions::folds_of(&tracks, &guid, a.open);
        for (folder, _) in &changed {
            crate::folds::toggle(folder, !a.open);
        }
        let (folders, opened): (Vec<String>, Vec<String>) = changed.into_iter().unzip();
        // The folds are the window's view: run here, the window is asked
        // too, told the folders settled so the two agree.
        also_in_the_window(
            FOLD.id,
            serde_json::json!({ "track": guid, "open": a.open, "folders": folders }).to_string(),
        );
        Ok(opened)
    })?;
    list.register(&UNDO, Effect::Mutating, |_: NoArg| {
        let (daw, project) = song()?;
        // A song open from its file undoes through its doc: only this
        // peer's steps, a collaborator's edits left standing.
        if let Some(said) = undo_through_doc(daw, &project, true)? {
            return Ok(said);
        }
        let label = daw::service::Projects::last_undo_label(daw, project.clone());
        if !daw::service::Projects::undo(daw, project.clone()) {
            stepped();
            return Ok("nothing to undo".to_owned());
        }
        stepped();
        samples_follow(daw, &project, crate::hits_midi::Mode::Follow);
        crate::changes::announce(crate::changes::Change::Marks);
        crate::song_model::request_resync();
        crate::actions::list().changed("items", "undo");
        Ok(format!(
            "undid {}",
            label.unwrap_or_else(|| "a step".to_owned())
        ))
    })?;
    list.register(&REDO, Effect::Mutating, |_: NoArg| {
        let (daw, project) = song()?;
        if let Some(said) = undo_through_doc(daw, &project, false)? {
            return Ok(said);
        }
        let label = daw::service::Projects::last_redo_label(daw, project.clone());
        if !daw::service::Projects::redo(daw, project.clone()) {
            stepped();
            return Ok("nothing to redo".to_owned());
        }
        stepped();
        samples_follow(daw, &project, crate::hits_midi::Mode::Follow);
        crate::changes::announce(crate::changes::Change::Marks);
        crate::song_model::request_resync();
        crate::actions::list().changed("items", "redo");
        Ok(format!(
            "redid {}",
            label.unwrap_or_else(|| "a step".to_owned())
        ))
    })?;
    Ok(())
}

pub(crate) fn stepped() {
    undone_there();
    // A window in another process counts it too.
    #[cfg(feature = "native")]
    crate::studio_engine::view_host::changed(None, crate::view::Change::Undone);
}

/// Undo (`back`) or redo through the song's doc, when it is open from its
/// project file: what to say, or `None` for the engine's own undo.
#[cfg(feature = "native")]
pub(crate) fn undo_through_doc(
    daw: &daw::standalone::Standalone,
    project: &daw::service::ProjectContext,
    back: bool,
) -> Result<Option<String>, String> {
    use crate::project_file::Undone;
    let daw::service::ProjectContext::Project(guid) = project else {
        return Ok(None);
    };
    let done = if back {
        crate::project_file::undo(daw, guid)?
    } else {
        crate::project_file::redo(daw, guid)?
    };
    let (verb, none) = if back {
        ("undid", "nothing to undo")
    } else {
        ("redid", "nothing to redo")
    };
    Ok(match done {
        Undone::NotOpen => None,
        Undone::Nothing => {
            stepped();
            Some(none.to_owned())
        }
        Undone::Step(name) => {
            stepped();
            samples_follow(daw, project, crate::hits_midi::Mode::Follow);
            crate::changes::announce(crate::changes::Change::Marks);
            crate::song_model::request_resync();
            crate::actions::list().changed("items", if back { "undo" } else { "redo" });
            Some(format!("{verb} {name}"))
        }
    })
}

#[cfg(feature = "native")]
pub(crate) fn register_save(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&SELECT_TRACK, Effect::Mutating, |a: SelectTrackArg| {
        let (daw, project) = song()?;
        let guid = resolve(daw, &project, &a.track)?;
        let at = daw::service::TrackRef::Guid(guid.clone());
        if a.add {
            daw::service::Tracks::set_selected(daw, project, at, true)
        } else {
            daw::service::Tracks::select_exclusive(daw, project, at)
        }
        .map_err(|e| e.to_string())?;
        crate::actions::list().changed("tracks", &guid);
        Ok(guid)
    })?;
    list.register(&MARK_KINDS, Effect::Mutating, |_: NoArg| {
        let (daw, project) = song()?;
        daw::service::Projects::begin_undo_block(daw, project.clone(), "Mark the kit's kinds");
        let marked =
            dynamic_template::apply::DawTarget::on(daw.clone(), project.clone()).mark_kit_kinds();
        daw::service::Projects::end_undo_block(daw, project, "Mark the kit's kinds", None);
        let marked = marked.map_err(|e| e.to_string())?;
        crate::song_model::request_resync();
        Ok(format!("{marked} tracks marked"))
    })?;
    list.register(&CHECKPOINT, Effect::Mutating, |a: CheckpointArg| {
        let (daw, project) = song()?;
        let ProjectContext::Project(guid) = &project else {
            return Err("no song open".to_owned());
        };
        let from = std::path::PathBuf::from(
            daw.read_project(guid, |p| p.info.path.clone())
                .ok_or("no song open")?,
        );
        let file = crate::project_file::fork(daw, guid, &from, &a.name)?;
        // The song was saved on the way: nothing unsaved now.
        crate::projects::mark_saved(daw, guid);
        Ok(format!(
            "saved the song and forked {} from it in {}; the song stays open",
            a.name.trim(),
            file.display()
        ))
    })?;
    list.register(&STREAMS, Effect::ReadOnly, |a: StreamsArg| {
        let mut songs = crate::stream_set::report();
        for s in &mut songs {
            s.readiness = crate::stream_set::readiness(&s.project);
        }
        if let Some(song) = &a.song {
            songs.retain(|s| &s.project == song);
        }
        if let Some(top) = a.top {
            for s in &mut songs {
                s.recordings.truncate(top as usize);
            }
        }
        Ok(songs)
    })?;
    list.register(&STREAM_QUALITY, Effect::Mutating, |a: StreamQualityArg| {
        use crate::stream_quality::{Quality, current, set};
        if let Some(word) = a.quality.as_deref() {
            let quality = Quality::parse(word)
                .ok_or_else(|| format!("`quality`: `proxies` or `full`, not {word:?}"))?;
            set(quality);
        }
        Ok(current().key().to_owned())
    })?;
    list.register(&ALTERNATIVES, Effect::ReadOnly, |(): ()| {
        let (daw, project) = song()?;
        let ProjectContext::Project(guid) = &project else {
            return Err("no song open".to_owned());
        };
        let base = song_base(daw, guid)?;
        let unsaved = match crate::projects::unsaved(daw, guid) {
            Some(true) => " (unsaved changes)",
            Some(false) => " (saved)",
            None => "",
        };
        let mut out = vec![format!(
            "the song{unsaved}: {}",
            crate::projects::main_of(&base)
                .map_or_else(|| "(not saved yet)".to_owned(), |p| p.display().to_string())
        )];
        out.extend(crate::projects::alternatives(&base).into_iter().map(|alt| {
            format!(
                "{}: {} (saved {})",
                alt.name,
                alt.path.display(),
                crate::projects::ago(alt.saved)
            )
        }));
        Ok(out)
    })?;
    list.register(&RECENT, Effect::ReadOnly, |(): ()| {
        Ok(crate::projects::recent()
            .into_iter()
            .map(|r| {
                let version = r
                    .opened_alternative()
                    .map_or_else(String::new, |alt| format!(" as {alt}"));
                let alternatives: Vec<String> = crate::projects::alternatives(&r.base)
                    .into_iter()
                    .map(|a| a.name)
                    .collect();
                format!(
                    "{}{version}, {} — {}{}",
                    r.name(),
                    crate::projects::ago(r.at),
                    r.opened.display(),
                    if alternatives.is_empty() {
                        String::new()
                    } else {
                        format!(" (alternatives: {})", alternatives.join(", "))
                    }
                )
            })
            .collect::<Vec<_>>())
    })?;
    list.register(&SAVE, Effect::Mutating, |a: SaveArg| {
        let traced =
            crate::edit_trace::Traced::enter(crate::edit_trace::edit_span!("SESSION_SAVE"));
        let out = save(a);
        traced.outcome(&out);
        out
    })?;
    register_check_saved(list)
}

/// `SESSION_SAVE`: see its description.
pub(crate) fn save(a: SaveArg) -> Result<String, String> {
    {
        let (daw, project) = song()?;
        let ProjectContext::Project(guid) = &project else {
            return Err("no song open".to_owned());
        };
        // A new song has no file yet: its first save names it.
        let untitled = crate::project_new::untitled(daw, guid);
        let dir = if let Some(to) = a.to.clone() {
            std::path::PathBuf::from(to)
        } else if untitled {
            return Err(
                "the song is untitled: save it with `to`, the file it goes in — its name becomes the song's"
                    .to_owned(),
            );
        } else {
            let from = std::path::PathBuf::from(
                daw.read_project(guid, |p| p.info.path.clone())
                    .ok_or("no song open")?,
            );
            if from.extension().is_some_and(|e| e == "session") {
                from
            } else {
                from.with_extension("session")
            }
        };
        let own = a.to.is_none() || untitled;
        let stage = tracing::info_span!("save.write").entered();
        let at = crate::session_file::save_session(daw, guid, &dir)?;
        drop(stage);
        if untitled {
            crate::project_new::named(daw, guid, &dir, &at);
        }
        if own {
            // The song itself, saved: nothing unsaved now. (A copy saved
            // elsewhere leaves the song as unsaved as it was.)
            crate::projects::mark_saved(daw, guid);
        }
        Ok(format!("saved to {}", at.display()))
    }
}

/// `SESSION_CHECK_SAVED`.
pub(crate) fn register_check_saved(
    list: &ActionList,
) -> Result<(), architect::action::ActionError> {
    list.register(&CHECK_SAVED, Effect::ReadOnly, |a: CheckSavedArg| {
        let (daw, project) = song()?;
        let ProjectContext::Project(guid) = &project else {
            return Err("no song open".to_owned());
        };
        let apart = crate::project_file::check(daw, guid, std::path::Path::new(&a.dir))?;
        Ok(if apart.is_empty() {
            vec!["the same".to_owned()]
        } else {
            apart
        })
    })?;
    Ok(())
}

pub(crate) static ITEMS: ActionMeta = meta(
    "TRACK_ITEMS",
    "track_items",
    "A track's items",
    "A track's items from `from` to `to` (seconds; all, unless given): each one's GUID, position, length and end, how far into its source it starts and the source's file, its lane (and whether that plays), fades — to see what an edit made: the slices, where they sit, what they play. 500 at most.",
    "Data",
    "",
);

/// Which items.
#[derive(Facet)]
pub(crate) struct ItemsArg {
    track: String,
    from: Option<f64>,
    to: Option<f64>,
}

/// One item.
#[derive(Facet)]
pub(crate) struct ItemRow {
    guid: String,
    position: f64,
    length: f64,
    end: f64,
    source_offset: f64,
    /// The recording it plays: its file's name.
    source: Option<String>,
    lane: u32,
    plays: bool,
    fade_in_ms: f64,
    fade_out_ms: f64,
    /// Drawn out: the slowest rate its stretch markers play any of it at
    /// (1 as recorded, 0.5 half speed — an octave down, as the engine
    /// stretches by rate). `None`: not stretched.
    slowest: Option<f64>,
}

/// The slowest rate between `markers`' neighbours; `None` without two.
pub(crate) fn slowest_rate(markers: &[daw_proto::StretchMarker]) -> Option<f64> {
    markers
        .windows(2)
        .filter_map(|pair| {
            let (a, b) = (pair.first()?, pair.get(1)?);
            let played = b.position - a.position;
            (played > 1e-9).then(|| (b.source_position - a.source_position) / played)
        })
        .reduce(f64::min)
}

pub(crate) fn register_items(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&ITEMS, Effect::ReadOnly, |a: ItemsArg| {
        let (daw, project) = song()?;
        let guid = resolve(daw, &project, &a.track)?;
        let track = Tracks::get(daw, project.clone(), TrackRef::Guid(guid.clone()))
            .ok_or("the track went")?;
        let (from, to) = (
            a.from.unwrap_or(f64::NEG_INFINITY),
            a.to.unwrap_or(f64::INFINITY),
        );
        let mut rows: Vec<ItemRow> =
            daw::service::Items::get_items(daw, project.clone(), TrackRef::Guid(guid))
                .into_iter()
                .filter_map(|i| {
                    let position = i.position.as_seconds();
                    let end = position + i.length.as_seconds();
                    (end >= from && position <= to).then(|| ItemRow {
                        source: daw_proto::Takes::get_active_take(
                            daw,
                            project.clone(),
                            daw_proto::ItemRef::Guid(i.guid.clone()),
                        )
                        .and_then(|t| t.source_file_path)
                        .and_then(|p| {
                            std::path::Path::new(&p)
                                .file_name()
                                .map(|f| f.to_string_lossy().into_owned())
                        }),
                        lane: i.fixed_lane.unwrap_or(0),
                        plays: daw_view::song::project::on_playing_lane(&track, &i),
                        guid: i.guid.clone(),
                        position,
                        length: i.length.as_seconds(),
                        end,
                        source_offset: i.start_offset.as_seconds(),
                        fade_in_ms: i.fade_in_length.as_seconds() * 1_000.0,
                        fade_out_ms: i.fade_out_length.as_seconds() * 1_000.0,
                        slowest: slowest_rate(&daw_proto::StretchMarkers::get_stretch_markers(
                            daw,
                            project.clone(),
                            daw_proto::ItemRef::Guid(i.guid.clone()),
                            daw_proto::TakeRef::Active,
                        )),
                    })
                })
                .collect();
        rows.sort_by(|a, b| a.position.total_cmp(&b.position));
        rows.truncate(500);
        Ok(rows)
    })?;
    Ok(())
}

pub(crate) fn register_lanes(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&EDIT_LANES, Effect::Mutating, |a: LanesArg| {
        let span = crate::edit_trace::edit_span!("DRUMS_EDIT_LANES");
        let mark = a.mark.clone();
        crate::edit_trace::run_edit(span, (EDIT_LANES.id, mark.as_deref()), |traced| {
            let (daw, project) = song()?;
            let dry = a.dry_run.unwrap_or(false);
            traced.record("edit.dry_run", dry);
            let stage = tracing::info_span!("edit.checkpoint").entered();
            let net = crate::safety_net::before(
                EDIT_LANES.id,
                if dry {
                    crate::safety_net::Reach::Nothing
                } else {
                    crate::safety_net::Reach::Song
                },
            )?;
            drop(stage);
            let stage = crate::edit_trace::stage_span!("edit.lanes").entered();
            let tracks = if a.reset == Some(true) {
                crate::drum_lanes::reset_edit(daw, &project, dry)?
            } else {
                crate::drum_lanes::set_up(daw, &project, dry)?
            };
            drop(stage);
            traced.count("edit.tracks", tracks.len());
            Ok(LanesOut {
                tracks,
                applied: !dry,
                checkpoint: net.name(),
            })
        })
    })?;
    list.register(&PLAY_LANE, Effect::Mutating, |a: PlayLaneArg| {
        let (daw, project) = song()?;
        crate::drum_lanes::play(daw, &project, &a.lane)
    })?;
    Ok(())
}

pub(crate) fn register_tracks(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&TRACKS, Effect::ReadOnly, |a: TracksArg| {
        let (daw, project) = song()?;
        let tracks = Tracks::all(daw, project.clone());
        let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
            tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
        let filter = a.filter.map(|f| f.to_lowercase());
        let open = open_folders(&tracks);
        Ok(tracks
            .iter()
            .map(|t| (t, crate::gain_stage::path_of(t, &by_guid).join("/")))
            .filter(|(_, path)| {
                filter
                    .as_deref()
                    .is_none_or(|f| path.to_lowercase().contains(f))
            })
            .map(|(t, path)| {
                let items = daw::service::Items::get_items(
                    daw,
                    project.clone(),
                    TrackRef::Guid(t.guid.clone()),
                );
                (t, path, items)
            })
            .map(|(t, path, items)| TrackRow {
                lanes: lane_rows(t, &items),
                armed: t.armed,
                record_input: record_input_of(t.record_input),
                open: t.is_folder.then(|| open.contains(&t.guid)),
                guid: t.guid.clone(),
                name: t.name.clone(),
                path,
                folder: t.is_folder,
                muted: t.muted,
                soloed: t.soloed,
                inverted: t.phase_inverted,
                volume_db: 20.0 * t.volume.max(1e-9).log10(),
                pan: t.pan,
                items: u32::try_from(items.len()).unwrap_or(u32::MAX),
                fx: daw::service::Effects::list(
                    daw,
                    project.clone(),
                    daw::service::FxChainContext::Track(t.guid.clone()),
                )
                .into_iter()
                .map(|f| {
                    if f.enabled {
                        f.plugin_name
                    } else {
                        format!("{} (bypassed)", f.plugin_name)
                    }
                })
                .collect(),
            })
            .collect::<Vec<_>>())
    })?;
    Ok(())
}

pub(crate) fn register_track_set(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&TRACK_SET, Effect::Mutating, |a: TrackSetArg| {
        let (daw, project) = song()?;
        let guid = resolve(daw, &project, &a.track)?;
        let at = || TrackRef::Guid(guid.clone());
        let failed = |e: daw_proto::DawError| e.to_string();
        // One undo step: what makes the set a step of the song (its doc,
        // its file, its history, a render worker's copy), as REAPER's
        // fader moves are.
        let label = "Set track";
        daw::service::Projects::begin_undo_block(daw, project.clone(), label);
        let set = (|| {
            if let Some(db) = a.volume_db {
                Tracks::set_volume(daw, project.clone(), at(), 10_f64.powf(db / 20.0))
                    .map_err(failed)?;
            }
            if let Some(pan) = a.pan {
                Tracks::set_pan(daw, project.clone(), at(), pan.clamp(-1.0, 1.0))
                    .map_err(failed)?;
            }
            if let Some(mute) = a.mute {
                Tracks::set_muted(daw, project.clone(), at(), mute).map_err(failed)?;
            }
            if let Some(solo) = a.solo {
                Tracks::set_soloed(daw, project.clone(), at(), solo).map_err(failed)?;
            }
            if let Some(monitor) = a.monitor.as_deref() {
                let mode = crate::shared_actions::monitor_mode(monitor)?;
                Tracks::set_input_monitor(daw, project.clone(), at(), mode).map_err(failed)?;
            }
            if let Some(phase) = a.phase {
                Tracks::set_phase_inverted(daw, project.clone(), at(), phase).map_err(failed)?;
            }
            if let Some(name) = a.name.as_deref() {
                Tracks::rename(daw, project.clone(), at(), name).map_err(failed)?;
            }
            Ok::<(), String>(())
        })();
        daw::service::Projects::end_undo_block(daw, project, label, None);
        set?;
        crate::actions::list().changed("tracks", &guid);
        Ok(())
    })?;
    Ok(())
}

/// The core's actions and those of the modules that stay with the engine:
/// the transport, the window, the song's tracks and items, saving and
/// undo, the set, the overlays.
///
/// # Errors
///
/// The first registration the list refuses.
pub(crate) fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    register_transport(list)?;
    register_mode(list)?;
    register_dock(list)?;
    register_window_target(list)?;
    register_phase(list)?;
    register_sections(list)?;
    register_tracks(list)?;
    register_track_set(list)?;
    register_items(list)?;
    register_lanes(list)?;
    #[cfg(feature = "native")]
    register_save(list)?;
    #[cfg(feature = "native")]
    register_undo(list)?;
    register_style(list)?;
    #[cfg(feature = "native")]
    register_window_controls(list)?;
    crate::studio_engine::set_actions::register(list)?;
    crate::arrange_actions::register(list)?;
    crate::notes_actions::register(list)?;
    #[cfg(feature = "native")]
    crate::record::register(list)?;
    #[cfg(feature = "native")]
    crate::render_ahead::register(list)?;
    crate::audio_device::register(list)?;
    crate::timeline_actions::register(list)?;
    crate::page::register(list)?;
    crate::prepare::register(list)?;
    crate::perf::register(list)?;
    crate::perf_meter::register(list)?;
    crate::history::register(list)?;
    crate::backup::register(list)?;
    crate::safety_net::register(list)?;
    crate::poi::register(list)?;
    crate::track_regions::register(list)?;
    Ok(())
}
