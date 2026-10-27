//! Placing the song's structure — a marker, a section, a time signature —
//! on the engine, at the edit cursor or over the time selection
//! (`crate::edit_state`): what Organize mode's toolbar (`crate::organize`)
//! and `SECTION_INSERT` / `MARKER_INSERT` on the Action List both run.

use session::section_kinds::{KeyflowAction, MarkerKind, SectionKind};

/// One button of the toolbar — what [`insert`] puts on the timeline.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Button {
    Marker(MarkerKind),
    Section(SectionKind),
    TimeSig(i32, i32),
}

/// Insert `button`'s marker, section or time signature, then bring the
/// guide up to date.
///
/// On the engine's current song at `edit` (the edit cursor and time selection, handed to
/// the engine first; the engine's own when `None`), then bring the guide
/// up to date — the toolbar's press, and `SECTION_INSERT` /
/// `MARKER_INSERT` on the Action List, one path.
///
/// A section past the song's end moves SONGEND out to it, so the guide
/// runs over it. The engine's cursor and selection afterwards (a section
/// moves the cursor to its end and uses up the selection) go back to the
/// arrangement.
pub fn insert(
    daw: &daw::standalone::Standalone,
    button: Button,
    shift: bool,
    edit: Option<crate::edit_state::Edit>,
) {
    use daw::service::ProjectContext;
    use daw::service::transport::service::Transport as _;
    let project = ProjectContext::Current;
    if let Some(edit) = edit {
        let handed = match edit.selection {
            Some(span) => daw.set_time_selection(project.clone(), span.start, span.end),
            None => daw.clear_time_selection(project.clone()),
        };
        if let Err(e) = handed.and_then(|()| daw.set_position(project.clone(), edit.at)) {
            tracing::warn!(error = %e, "organize: the edit cursor could not be handed to the engine");
        }
    }
    match button {
        Button::Marker(kind) => {
            session::keyflow::actions::dispatch(daw, KeyflowAction::InsertMarker(kind));
        }
        Button::Section(kind) => {
            session::keyflow::actions::dispatch(daw, KeyflowAction::InsertSection(kind));
            reach_song_end(daw, &project);
        }
        Button::TimeSig(num, den) => {
            if let Err(e) =
                session::keyflow::time_signature::insert_time_signature(daw, num, den, shift)
            {
                tracing::warn!(error = %e, num, den, "organize: the time signature was refused");
            }
        }
    }
    crate::edit_state::ask(engine_edit(daw, &project));
    // The cues follow the sections; the click follows the meter.
    if let Err(e) = session::guide::Guide::new(daw.clone())
        .with_instrument(crate::prepare::GUIDE_INSTRUMENT)
        .generate(session::guide::GuideScope::All)
    {
        tracing::debug!(error = %e, "organize: no guide to regenerate yet");
    }
}

/// The engine's edit cursor and time selection on `project`, as the
/// arrangement keeps them.
#[must_use]
pub fn engine_edit(
    daw: &daw::standalone::Standalone,
    project: &daw::service::ProjectContext,
) -> crate::edit_state::Edit {
    use daw::service::transport::service::Transport as _;
    let at = daw
        .get_state(project.clone())
        .edit_position
        .time
        .map_or_else(
            || daw.get_position(project.clone()),
            |position| position.as_seconds(),
        );
    let selection = daw
        .get_time_selection(project.clone())
        .map(|span| crate::edit_state::Span::between(span.start_seconds, span.end_seconds))
        .filter(|span| span.is_meaningful());
    crate::edit_state::Edit { at, selection }
}

/// The song's end moved out to its last section's, when a section now
/// runs past it: SONGEND, the `=END` after it and the SONG region. The
/// guide is laid over the song to SONGEND, so a section inserted after it
/// would otherwise have no click.
pub(crate) fn reach_song_end(daw: &daw::standalone::Standalone, project: &daw::service::ProjectContext) {
    use daw::service::{Markers, Regions};
    let song_lane = session::ruler_lanes::CoreLane::Song.lane_index();
    let last = crate::actions::sections(daw, project)
        .iter()
        .map(|s| s.end)
        .fold(0.0_f64, f64::max);
    let short = |at: f64| at < last - 1e-6;
    for marker in Markers::all(daw, project.clone()) {
        let (Some(id), Some(at)) = (marker.id, marker.position.seconds()) else {
            continue;
        };
        let name = marker.name.trim();
        if (name.eq_ignore_ascii_case("SONGEND") || name == "=END")
            && short(at)
            && let Err(e) = Markers::set_position(daw, project.clone(), id, last)
        {
            tracing::warn!(error = %e, marker = name, "organize: the song's end did not move");
        }
    }
    for region in Regions::all(daw, project.clone()) {
        let (Some(id), Some(lane)) = (region.id, region.lane) else {
            continue;
        };
        let (start, end) = (
            region.time_range.start_seconds(),
            region.time_range.end_seconds(),
        );
        if lane == song_lane
            && short(end)
            && let Err(e) = Regions::set_bounds(daw, project.clone(), id, start, last)
        {
            tracing::warn!(error = %e, "organize: the SONG region did not grow");
        }
    }
}
