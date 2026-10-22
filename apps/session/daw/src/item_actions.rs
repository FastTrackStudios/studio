//! Items edited one by one, over MCP as by hand: what a model needs to
//! repair or reshape a lane exactly — move, trim, slip, fade, mute, put
//! on another lane, add from a recording, split, delete.
//!
//! Each call is one undo step, and says each item as it is after
//! (as `TRACK_ITEMS` does). The window reads the changed items back.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{ItemRef, Items as _, ProjectContext, TakeRef, Takes as _, TrackRef};
use daw_proto::primitives::{Duration, PositionInSeconds};
use facet::Facet;

use crate::actions::{meta, song};

static SET: ActionMeta = meta(
    "ITEMS_SET",
    "items_set",
    "Edit items",
    "Change items by GUID (from TRACK_ITEMS), each its own fields, all as one undo step: `position` (seconds; the item moves, what it plays with it), `length` (seconds, from its start), `source_offset` (seconds into its recording it starts — a slip), `lane` (fixed lane index), `muted`, `volume_db`, `fade_in_ms` / `fade_out_ms`. Only the fields given change. `label` names the undo step. Says each item after.",
    "Edit",
    "",
);

static ADD: ActionMeta = meta(
    "ITEM_ADD",
    "item_add",
    "Add an item from a recording",
    "A new item on `track` (name or GUID) — on fixed `lane` if given — at `position` for `length` seconds, playing the recording `file` (a path, or a file name the song's media holds, as TRACK_ITEMS names them) from `source_offset` seconds into it. One undo step. Says the item.",
    "Edit",
    "",
);

static DELETE: ActionMeta = meta(
    "ITEMS_DELETE",
    "items_delete",
    "Delete items",
    "Delete items by GUID (from TRACK_ITEMS), as one undo step. Says how many.",
    "Edit",
    "",
);

static SPLIT: ActionMeta = meta(
    "ITEM_SPLIT",
    "item_split",
    "Split an item",
    "Split an item (GUID) at `at` (song seconds, inside it): it keeps what is before, a new item what is after, playing on from the same place in the recording. One undo step. Says both.",
    "Edit",
    "",
);

/// One item's changes.
#[derive(Facet, Default)]
struct ItemPatch {
    guid: String,
    position: Option<f64>,
    length: Option<f64>,
    source_offset: Option<f64>,
    lane: Option<u32>,
    muted: Option<bool>,
    volume_db: Option<f64>,
    fade_in_ms: Option<f64>,
    fade_out_ms: Option<f64>,
}

#[derive(Facet)]
struct SetArg {
    items: Vec<ItemPatch>,
    label: Option<String>,
}

#[derive(Facet)]
struct AddArg {
    track: String,
    lane: Option<u32>,
    file: String,
    position: f64,
    length: f64,
    source_offset: Option<f64>,
}

#[derive(Facet)]
struct DeleteArg {
    items: Vec<String>,
}

#[derive(Facet)]
struct SplitArg {
    item: String,
    at: f64,
}

/// An item as it is now.
#[derive(Facet)]
struct ItemOut {
    guid: String,
    track: String,
    position: f64,
    length: f64,
    end: f64,
    source_offset: f64,
    source: Option<String>,
    lane: u32,
    muted: bool,
    fade_in_ms: f64,
    fade_out_ms: f64,
}

fn item_out(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    guid: &str,
) -> Result<ItemOut, String> {
    let at = ItemRef::Guid(guid.to_owned());
    let i = daw
        .get_item(project.clone(), at.clone())
        .ok_or_else(|| format!("no item {guid}"))?;
    let position = i.position.as_seconds();
    Ok(ItemOut {
        guid: i.guid.clone(),
        track: i.track_guid.clone(),
        position,
        length: i.length.as_seconds(),
        end: position + i.length.as_seconds(),
        source_offset: i.start_offset.as_seconds(),
        source: daw
            .get_active_take(project.clone(), at)
            .and_then(|t| t.source_file_path),
        lane: i.fixed_lane.unwrap_or(0),
        muted: i.muted,
        fade_in_ms: i.fade_in_length.as_seconds() * 1_000.0,
        fade_out_ms: i.fade_out_length.as_seconds() * 1_000.0,
    })
}

/// `patch` made on its item, field by field.
fn apply(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    patch: &ItemPatch,
) -> Result<(), String> {
    let at = ItemRef::Guid(patch.guid.clone());
    let item = daw
        .get_item(project.clone(), at.clone())
        .ok_or_else(|| format!("no item {}", patch.guid))?;
    let p = project.clone();
    let e = |e: daw_proto::DawError| format!("{}: {e}", patch.guid);
    if let Some(s) = patch.position {
        daw.set_position(p.clone(), at.clone(), PositionInSeconds::from_seconds(s))
            .map_err(e)?;
    }
    if let Some(s) = patch.length {
        if s <= 0.0 {
            return Err(format!("{}: a length must be more than 0", patch.guid));
        }
        daw.set_length(p.clone(), at.clone(), Duration::from_seconds(s))
            .map_err(e)?;
    }
    if let Some(s) = patch.source_offset {
        daw.set_start_offset(
            p.clone(),
            at.clone(),
            TakeRef::Active,
            Duration::from_seconds(s),
        )
        .map_err(e)?;
    }
    if let Some(lane) = patch.lane {
        daw.set_fixed_lane(p.clone(), at.clone(), lane).map_err(e)?;
    }
    if let Some(m) = patch.muted {
        daw.set_muted(p.clone(), at.clone(), m).map_err(e)?;
    }
    if let Some(db) = patch.volume_db {
        daw::service::Items::set_volume(daw, p.clone(), at.clone(), 10_f64.powf(db / 20.0))
            .map_err(e)?;
    }
    if let Some(ms) = patch.fade_in_ms {
        daw.set_fade_in(
            p.clone(),
            at.clone(),
            Duration::from_seconds(ms / 1_000.0),
            item.fade_in_shape,
        )
        .map_err(e)?;
    }
    if let Some(ms) = patch.fade_out_ms {
        daw.set_fade_out(
            p,
            at,
            Duration::from_seconds(ms / 1_000.0),
            item.fade_out_shape,
        )
        .map_err(e)?;
    }
    Ok(())
}

/// `work` as one undo step called `label`, the window told after.
fn step<T>(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    label: &str,
    work: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    daw::service::Projects::begin_undo_block(daw, project.clone(), label);
    let out = work();
    daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
    crate::studio::request_resync_items();
    crate::hit_overlay::refresh();
    out
}

/// A recording's path: `file` itself, or the song's media file of that
/// name (a file name, as TRACK_ITEMS gives it).
fn recording(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    file: &str,
) -> Result<String, String> {
    if std::path::Path::new(file).is_absolute() {
        return Ok(file.to_owned());
    }
    daw::service::Items::get_all_items(daw, project.clone())
        .into_iter()
        .find_map(|i| {
            let path = daw
                .get_active_take(project.clone(), ItemRef::Guid(i.guid))?
                .source_file_path?;
            std::path::Path::new(&path)
                .file_name()
                .is_some_and(|n| n.to_string_lossy() == file)
                .then_some(path)
        })
        .ok_or_else(|| format!("no recording called {file} in the song"))
}

pub(crate) fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&SET, Effect::Mutating, |a: SetArg| {
        let (daw, project) = song()?;
        let label = a.label.as_deref().unwrap_or("Edit items");
        step(daw, &project, label, || {
            a.items.iter().try_for_each(|p| apply(daw, &project, p))
        })?;
        a.items
            .iter()
            .map(|p| item_out(daw, &project, &p.guid))
            .collect::<Result<Vec<_>, _>>()
    })?;
    list.register(&ADD, Effect::Mutating, |a: AddArg| {
        let (daw, project) = song()?;
        let track = crate::actions::resolve(daw, &project, &a.track)?;
        let file = recording(daw, &project, &a.file)?;
        let guid = step(daw, &project, "Add an item", || {
            let guid = daw
                .add_item(
                    project.clone(),
                    TrackRef::Guid(track.clone()),
                    PositionInSeconds::from_seconds(a.position),
                    Duration::from_seconds(a.length),
                )
                .ok_or("the item would not add")?;
            let at = ItemRef::Guid(guid.clone());
            if daw.get_active_take(project.clone(), at.clone()).is_none() {
                daw.add_take(project.clone(), at.clone())
                    .ok_or("the item would take no recording")?;
            }
            daw.set_source_file(project.clone(), at.clone(), TakeRef::Active, file.clone())
                .map_err(|e| e.to_string())?;
            apply(
                daw,
                &project,
                &ItemPatch {
                    guid: guid.clone(),
                    source_offset: a.source_offset,
                    lane: a.lane,
                    length: Some(a.length),
                    ..ItemPatch::default()
                },
            )?;
            Ok(guid)
        })?;
        item_out(daw, &project, &guid)
    })?;
    list.register(&DELETE, Effect::Destructive, |a: DeleteArg| {
        let (daw, project) = song()?;
        let n = step(daw, &project, "Delete items", || {
            a.items.iter().try_for_each(|g| {
                daw.delete_item(project.clone(), ItemRef::Guid(g.clone()))
                    .map_err(|e| format!("{g}: {e}"))
            })?;
            Ok(a.items.len())
        })?;
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    })?;
    list.register(&SPLIT, Effect::Mutating, |a: SplitArg| {
        let (daw, project) = song()?;
        let before = item_out(daw, &project, &a.item)?;
        if !(a.at > before.position + crate::comp::GRAIN && a.at < before.end - crate::comp::GRAIN)
        {
            return Err(format!(
                "{} is not inside the item ({:.3}-{:.3})",
                a.at, before.position, before.end
            ));
        }
        let after = step(daw, &project, "Split an item", || {
            let copy = daw
                .duplicate_item(project.clone(), ItemRef::Guid(a.item.clone()))
                .ok_or("the item would not copy")?;
            apply(
                daw,
                &project,
                &ItemPatch {
                    guid: a.item.clone(),
                    length: Some(a.at - before.position),
                    ..ItemPatch::default()
                },
            )?;
            apply(
                daw,
                &project,
                &ItemPatch {
                    guid: copy.clone(),
                    position: Some(a.at),
                    source_offset: Some(before.source_offset + (a.at - before.position)),
                    length: Some(before.end - a.at),
                    ..ItemPatch::default()
                },
            )?;
            Ok(copy)
        })?;
        Ok(vec![
            item_out(daw, &project, &a.item)?,
            item_out(daw, &project, &after)?,
        ])
    })?;
    Ok(())
}
