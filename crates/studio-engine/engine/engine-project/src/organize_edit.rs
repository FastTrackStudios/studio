//! Placing the song's structure — a marker, a section, a time signature —
//! on the engine, at the edit cursor or over the time selection
//! (`crate::edit_state`): what Organize mode's toolbar (`crate::organize`)
//! and `SECTION_INSERT` / `MARKER_INSERT` on the Action List both run.

use fts_session::section_kinds::KeyflowAction;

pub use engine_runtime::client::organize_edit::*;

static PRESS: architect::action::ActionMeta = crate::actions::meta(
    "ORGANIZE_PRESS",
    "organize_press",
    "Press an Organize button",
    "What a press on the Organize toolbar does, at the edit cursor or over the time selection: `button` a marker (`Marker(SongEnd)`), a section (`Section(Chorus)`) or a time signature (`TimeSig(3, 4)`; `shift` for one bar only). The guide is generated again.",
    "Structure",
    "",
);

/// A toolbar press.
#[derive(facet::Facet)]
struct PressArg {
    button: String,
    shift: Option<bool>,
}

/// `ORGANIZE_PRESS` onto `list`.
///
/// # Errors
///
/// The list refused it (a duplicate id).
pub fn register(
    list: &architect::action::ActionList,
) -> Result<(), architect::action::ActionError> {
    list.register(
        &PRESS,
        architect::action::Effect::Mutating,
        |a: PressArg| {
            let button =
                Button::from_key(&a.button).ok_or_else(|| format!("no button {}", a.button))?;
            let daw = crate::open::local_engine().ok_or("no song open in the engine")?;
            insert(
                daw,
                button,
                a.shift.unwrap_or(false),
                crate::edit_state::current(),
            );
            crate::song_model::request_resync();
            Ok(format!("{} pressed", a.button))
        },
    )?;
    Ok(())
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
            engine_domain::keyflow::actions::dispatch(daw, KeyflowAction::InsertMarker(kind));
        }
        Button::Section(kind) => {
            engine_domain::keyflow::actions::dispatch(daw, KeyflowAction::InsertSection(kind));
            reach_song_end(daw, &project);
        }
        Button::TimeSig(num, den) => {
            if let Err(e) =
                engine_domain::keyflow::time_signature::insert_time_signature(daw, num, den, shift)
            {
                tracing::warn!(error = %e, num, den, "organize: the time signature was refused");
            }
        }
    }
    crate::edit_state::ask(engine_edit(daw, &project));
    // The cues follow the sections; the click follows the meter.
    if let Err(e) = engine_domain::guide::Guide::new(daw.clone())
        .with_instrument(crate::prepare::GUIDE_INSTRUMENT)
        .generate(engine_domain::guide::GuideScope::All)
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
pub fn reach_song_end(daw: &daw::standalone::Standalone, project: &daw::service::ProjectContext) {
    use daw::service::{Markers, Regions};
    let song_lane = engine_domain::ruler_lanes::CoreLane::Song.lane_index();
    let last = last_section_end(&Regions::all(daw, project.clone()));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_names_every_button_by_a_key_the_engine_reads_back() {
        for button in Button::NAMED {
            assert_eq!(Button::from_key(&button.key()), Some(button), "{button:?}");
        }
        assert_eq!(
            Button::from_key("TimeSig(7, 8)"),
            Some(Button::TimeSig(7, 8))
        );
        assert_eq!(Button::from_key("Section(Polka)"), None);
    }
}

/// Where the last section ends (0 with none): the latest end of a region on
/// the sections lane (a region on no lane is on it), not the song's own
/// region nor an edit zone.
fn last_section_end(regions: &[daw_proto::Region]) -> f64 {
    let sections = engine_domain::ruler_lanes::CoreLane::Sections.lane_index();
    regions
        .iter()
        .filter(|r| r.lane.is_none_or(|l| l == sections))
        .map(|r| r.time_range.end_seconds())
        .fold(0.0_f64, f64::max)
}
