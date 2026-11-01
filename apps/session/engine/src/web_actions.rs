//! The page's Action List (`docs/design/actions-everywhere.md`, rule 7).
//!
//! The browser build runs its own engine in the page (`crate::web_engine`:
//! daw-standalone on wasm, reached through the `daw` facade), with no link
//! to a desktop engine. Its controls run named actions all the same, on
//! this list: the actions both lists carry (`crate::shared_actions` — one
//! description and one argument shape per id), carried out here through
//! the facade, and the arrangement's gestures (`crate::arrange_actions`).
//! The window adds its own view actions (`session_ui::view_actions`) with
//! [`extend`].
//!
//! A control runs one with [`press`] (in the background: the page has one
//! thread, and the facade answers in time), or queues it among its edits
//! as `Edit::Run`, which the page's edit worker runs in order with them
//! ([`run`]).

use architect::action::{ActionList, Effect};
use daw_control::{Project, TrackHandle};

use crate::shared_actions::{
    ArmArg, ArmOut, FOLD, FoldArg, PHASE, PhaseArg, SELECT_TRACK, SHOW_LANES, SelectTrackArg,
    ShowArg, ShowOut, TRACK_ARM, TRACK_SET, TrackSetArg, folds_of, input_of, monitor_mode,
    pick_phase, resolve_in, show_lanes,
};

thread_local! {
    /// The page's one list, built the first time it is asked for. The page
    /// has one thread; the list lives as long as it does.
    static LIST: &'static ActionList = {
        let list: &'static ActionList = Box::leak(Box::default());
        if let Err(error) = register(list) {
            tracing::error!(%error, "page: its actions stopped registering");
        }
        list
    };
}

/// The page's Action List.
#[must_use]
pub fn list() -> &'static ActionList {
    LIST.with(|list| *list)
}

/// Add actions to the page's list: what the window registers of its own.
///
/// # Errors
/// The first registration the list refuses (a duplicate id).
pub fn extend(
    add: impl FnOnce(&ActionList) -> Result<(), architect::action::ActionError>,
) -> Result<(), architect::action::ActionError> {
    add(list())
}

/// Run `id` with `args` (JSON; empty for none) on the page's list,
/// awaited: what it said (JSON), or why not.
///
/// # Errors
/// No such action, arguments it cannot take, or it refused.
pub async fn run(id: &str, args: &[u8]) -> Result<Vec<u8>, String> {
    list()
        .run_json_async(id, args)
        .await
        .map_err(|e| e.to_string())
}

/// Run `id` in the background, as a control does; a refusal said once.
pub fn press(id: &str, args: String) {
    let id = id.to_owned();
    wasm_bindgen_futures::spawn_local(async move {
        let _span = tracing::info_span!("page.press", page.press_action = %id).entered();
        if let Err(e) = run(&id, args.as_bytes()).await {
            tracing::warn!(action.id = %id, error = %e, "control action refused");
        }
    });
}

/// The song open in the page, through its facade.
async fn song() -> Result<Project, String> {
    let daw = daw::rpc::Daw::try_get().ok_or("no engine in the page")?;
    daw.current_project().await.map_err(|e| e.to_string())
}

/// The song's tracks.
async fn tracks(song: &Project) -> Result<Vec<daw_proto::Track>, String> {
    song.tracks().all().await.map_err(|e| e.to_string())
}

/// The track `guid`, to drive.
async fn track(song: &Project, guid: &str) -> Result<TrackHandle, String> {
    song.tracks()
        .by_guid(guid)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "the track went".to_owned())
}

async fn track_set(a: TrackSetArg) -> Result<(), String> {
    let song = song().await?;
    let guid = resolve_in(&tracks(&song).await?, &a.track)?;
    let t = track(&song, &guid).await?;
    let failed = |e: daw_control::Error| e.to_string();
    if let Some(db) = a.volume_db {
        t.set_volume(10_f64.powf(db / 20.0)).await.map_err(failed)?;
    }
    if let Some(pan) = a.pan {
        t.set_pan(pan.clamp(-1.0, 1.0)).await.map_err(failed)?;
    }
    match a.mute {
        Some(true) => t.mute().await.map_err(failed)?,
        Some(false) => t.unmute().await.map_err(failed)?,
        None => {}
    }
    match a.solo {
        Some(true) => t.solo().await.map_err(failed)?,
        Some(false) => t.unsolo().await.map_err(failed)?,
        None => {}
    }
    if let Some(monitor) = a.monitor.as_deref() {
        t.set_input_monitor(monitor_mode(monitor)?)
            .await
            .map_err(failed)?;
    }
    if let Some(phase) = a.phase {
        t.set_phase_inverted(phase).await.map_err(failed)?;
    }
    if let Some(name) = a.name.as_deref() {
        t.rename(name).await.map_err(failed)?;
    }
    Ok(())
}

async fn track_arm(a: ArmArg) -> Result<ArmOut, String> {
    use daw_proto::track::RecordInput;
    let song = song().await?;
    let all = tracks(&song).await?;
    let guid = resolve_in(&all, &a.track)?;
    let before = all
        .iter()
        .find(|t| t.guid == guid)
        .ok_or("the track went")?;
    let t = track(&song, &guid).await?;
    let failed = |e: daw_control::Error| e.to_string();
    let on = a.on.unwrap_or(!before.armed);
    // An armed track with no input records input 1, as the desktop's does.
    let input = a
        .input
        .or_else(|| (on && input_of(before.record_input).is_none()).then_some(1));
    if let Some(input) = input {
        let channel = input
            .checked_sub(1)
            .ok_or("inputs count from 1: `input` 1 is the first")?;
        t.set_record_input(RecordInput::Audio { channel })
            .await
            .map_err(failed)?;
    }
    if on {
        t.arm().await.map_err(failed)?;
    } else {
        t.disarm().await.map_err(failed)?;
    }
    let now = t.info().await.map_err(failed)?;
    Ok(ArmOut {
        track: now.name,
        armed: now.armed,
        input: input_of(now.record_input),
    })
}

async fn select_track(a: SelectTrackArg) -> Result<String, String> {
    let song = song().await?;
    let guid = resolve_in(&tracks(&song).await?, &a.track)?;
    let t = track(&song, &guid).await?;
    if a.add {
        t.select().await
    } else {
        t.select_exclusive().await
    }
    .map_err(|e| e.to_string())?;
    Ok(guid)
}

async fn fold(a: FoldArg) -> Result<Vec<String>, String> {
    if !a.folders.is_empty() {
        for folder in &a.folders {
            crate::folds::toggle(folder, !a.open);
        }
        return Ok(a.folders);
    }
    let song = song().await?;
    let all = tracks(&song).await?;
    let guid = resolve_in(&all, &a.track)?;
    let changed = folds_of(&all, &guid, a.open);
    for (folder, _) in &changed {
        crate::folds::toggle(folder, !a.open);
    }
    Ok(changed.into_iter().map(|(_, name)| name).collect())
}

async fn show(a: ShowArg) -> Result<ShowOut, String> {
    if let Some(settled) = a.tracks.filter(|t| !t.is_empty()) {
        let shown = crate::lanes::set_expanded(&settled, a.show);
        return Ok(ShowOut {
            shown,
            tracks: settled,
        });
    }
    let song = song().await?;
    let all = tracks(&song).await?;
    let guid = match a.track.as_deref() {
        Some(track) => resolve_in(&all, track)?,
        None => all
            .iter()
            .find(|t| t.selected)
            .map(|t| t.guid.clone())
            .ok_or("no track given, and none selected")?,
    };
    let (shown, tracks) = show_lanes(&all, &guid, a.show)?;
    Ok(ShowOut { shown, tracks })
}

/// Put the page's actions on `list`.
fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register_async(&TRACK_SET, Effect::Mutating, track_set)?;
    list.register_async(&TRACK_ARM, Effect::Mutating, track_arm)?;
    list.register_async(&SELECT_TRACK, Effect::Mutating, select_track)?;
    list.register_async(&FOLD, Effect::Mutating, fold)?;
    list.register_async(&SHOW_LANES, Effect::Mutating, show)?;
    list.register(&PHASE, Effect::Mutating, |a: PhaseArg| {
        let (mode, slug) = pick_phase(a.mode.as_deref(), &a.phase)?;
        session::mode_phases::choose(mode, slug);
        Ok(())
    })?;
    crate::shared_actions::register_together(list)?;
    crate::arrange_actions::register(list)
}
