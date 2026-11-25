//! `SONG_KEY`: change the song's key.
//!
//! The plan is `session::key_change`; this applies it through the facade
//! (so it runs the same in a window driving an engine in another process):
//! take pitch on every item of the pitched tracks, the KEY track's
//! changes moved, the chart rewritten.

use architect::action::{ActionError, ActionList, Effect};
use daw::service::{ItemRef, Items, TakeRef, Tracks};
use facet::Facet;
use session::key::{format_key, key_changes, key_from_name, set_key_at};
use session::key_change::{PlanTrack, plan, transpose_chart};

static SONG_KEY: architect::action::ActionMeta = crate::actions::meta(
    "SONG_KEY",
    "song_key",
    "Change the song's key",
    "Change the song to the key on `root` (e.g. `Eb`), in the mode it is in now: every item on a non-percussion track gets the interval as its take pitch (the nearer way, −5…+6, added to what it has), the KEY track's changes and the chart's chords and key line follow. Drums and percussion (the template's Drums and Percussion folders, then names), click, count and guide are left alone. Says the interval and the tracks pitched.",
    "Song",
    "",
);

/// The key to change to.
#[derive(Facet)]
struct KeyArg {
    /// The root: `C`, `Eb`, `F#`. The song keeps its mode.
    root: String,
}

/// What changing the key did.
#[derive(Facet)]
struct KeyChanged {
    /// The key now, as the KEY track writes it.
    key: String,
    /// The interval, in semitones.
    semitones: i32,
    /// Items given a take pitch, on this many tracks.
    items: u32,
    tracks: u32,
    /// Whether the chart was rewritten.
    chart: bool,
}

/// `name` moved by `semitones`, spelled with sharps when `sharps`.
fn shifted_root(root: u8, semitones: i32, sharps: bool) -> &'static str {
    const SHARP: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    const FLAT: [&str; 12] = [
        "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
    ];
    let at = (i32::from(root) + semitones).rem_euclid(12) as usize;
    if sharps { SHARP[at] } else { FLAT[at] }
}

pub fn register(list: &ActionList) -> Result<(), ActionError> {
    list.register(&SONG_KEY, Effect::Mutating, |a: KeyArg| {
        let (daw, project) = crate::actions::song()?;
        let changes = key_changes(daw, &project);
        let from = changes
            .first()
            .map(|c| c.key.clone())
            .ok_or("the song has no key on its KEY track to change from")?;
        // The interval is the root's alone, so the mode stays the song's.
        let major = !format_key(&from).ends_with("minor");
        let to = key_from_name(&a.root, major).ok_or_else(|| format!("no key {}", a.root))?;

        let all = Tracks::all(daw, project.clone());
        let plan_tracks: Vec<PlanTrack> = all
            .iter()
            .map(|t| PlanTrack {
                guid: t.guid.clone(),
                name: t.name.clone(),
                parent: t.parent_guid.clone(),
                is_folder: t.is_folder,
            })
            .collect();
        let plan = plan(&plan_tracks, &from, &to);
        let sharps = a.root.contains('#');

        // One undo step, and a failure partway leaves the tracks to undo
        // rather than half-pitched.
        let label = "Change the song's key";
        daw::service::Projects::begin_undo_block(daw, project.clone(), label);
        let done = (|| -> Result<(u32, u32, bool), String> {
            let mut items = 0_u32;
            let mut tracks = 0_u32;
            if plan.semitones != 0 {
                for guid in &plan.pitched {
                    let mut any = false;
                    for item in Items::get_items(
                        daw,
                        project.clone(),
                        daw::service::TrackRef::Guid(guid.clone()),
                    ) {
                        let at = ItemRef::Guid(item.guid.clone());
                        let Some(take) =
                            daw_proto::Takes::get_active_take(daw, project.clone(), at.clone())
                        else {
                            continue;
                        };
                        daw_proto::Takes::set_pitch(
                            daw,
                            project.clone(),
                            at,
                            TakeRef::Active,
                            take.pitch + f64::from(plan.semitones),
                        )
                        .map_err(|e| e.to_string())?;
                        items += 1;
                        any = true;
                    }
                    tracks += u32::from(any);
                }
            }

            // The KEY track: every change moved by the interval, the first
            // the key picked.
            for (i, change) in changes.iter().enumerate() {
                let key = if i == 0 {
                    to.clone()
                } else {
                    let root = shifted_root(change.key.root.semitone % 12, plan.semitones, sharps);
                    let major = format_key(&change.key).ends_with("major");
                    key_from_name(root, major).unwrap_or_else(|| change.key.clone())
                };
                set_key_at(daw, project.clone(), change.seconds, &key)
                    .map_err(|e| e.to_string())?;
            }

            // The chart, shared with collaborators through the session doc.
            let chart = crate::collab::chart_context()
                .map(|(doc, _, _)| doc.read().chart)
                .filter(|text| !text.trim().is_empty())
                .map(|text| transpose_chart(&text, &to))
                .inspect(|moved| crate::collab::local_chart(moved))
                .is_some();
            Ok((items, tracks, chart))
        })();
        daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
        let (items, tracks, chart) = done?;

        Ok(KeyChanged {
            key: plan.key_name,
            semitones: plan.semitones,
            items,
            tracks,
            chart,
        })
    })
}
