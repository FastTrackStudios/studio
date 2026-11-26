//! A song's sends over the Action List: every send listed (`SENDS_LIST`),
//! and one added, set or taken off (`SEND_ADD`, `SEND_SET`,
//! `SEND_REMOVE`).
//!
//! A send is named by its two ends — `from` and `to`, each a track's path,
//! name or GUID — and, when a track sends to the same track twice, by its
//! place among them (`index`, from 0). Its level is in dB, its pan from
//! −1 (left) to 1 (right), and where it taps its track one of `post-fader`
//! (after the fader: the default), `pre-fader` (after the FX, before the
//! fader) or `pre-fx` (the track's input). The engine renders every send
//! it lists, aligned for its plugins' delay (daw-standalone's `pdc`).

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{ProjectContext, RouteLocation, RouteRef, RouteType, Routing, TrackRef, Tracks};
use daw_proto::routing::{SendMode, TrackRoute};
use daw_standalone::sync::Standalone;
use facet::Facet;

use crate::actions::{meta, resolve, song};

static LIST: ActionMeta = meta(
    "SENDS_LIST",
    "sends_list",
    "Sends",
    "Every send on the song — or, with `track` (a path, name or GUID), the ones from it and into it: each send's `from` and `to` (paths), `index` (its place among `from`'s sends), `level_db`, `pan` (-1 left … 1 right), `tap` (`post-fader`, `pre-fader` — after the FX, before the fader — or `pre-fx`), `pre_fader` (whether it is taken before the fader), `mute`, `phase`, `mono` and `channels` (`1/2→1/2`; `→3/4` is a destination's sidechain). Read-only.",
    "Mix",
    "",
);

static ADD: ActionMeta = meta(
    "SEND_ADD",
    "send_add",
    "Add a send",
    "Send `from` (a track's path, name or GUID) to `to`: at `level_db` (0 unless given), taken `pre_fader` (after the FX, before the fader: true) or after the fader (false, the default) — or `tap: pre-fx`, the track's input. A send that already goes from `from` to `to` is left as it is unless `again: true`. Refused when it would feed back (`to` feeds `from`, or is inside it). Says the send as it now is. One undo step.",
    "Mix",
    "",
);

static SET: ActionMeta = meta(
    "SEND_SET",
    "send_set",
    "Set a send",
    "Set any of the send from `from` to `to`'s `level_db`, `pan` (-1 … 1), `mute`, `pre_fader` (true: after the FX, before the fader; false: after the fader) or `tap` (`post-fader`, `pre-fader`, `pre-fx`). `index` picks among several sends between the same two tracks (from 0). Only the fields given change. Says the send as it now is. One undo step.",
    "Mix",
    "",
);

static REMOVE: ActionMeta = meta(
    "SEND_REMOVE",
    "send_remove",
    "Remove a send",
    "Take off the send from `from` to `to` (`index` among several, from 0; `all: true` every one between them). Says how many went. One undo step.",
    "Mix",
    "",
);

/// Whose sends.
#[derive(Facet)]
struct ListArg {
    track: Option<String>,
}

/// A send to add.
#[derive(Facet)]
pub struct AddArg {
    pub from: String,
    pub to: String,
    pub level_db: Option<f64>,
    pub pre_fader: Option<bool>,
    pub tap: Option<String>,
    pub again: Option<bool>,
}

/// What to set on a send.
#[derive(Facet, Default)]
pub struct SetArg {
    pub from: String,
    pub to: String,
    pub index: Option<u32>,
    pub level_db: Option<f64>,
    pub pan: Option<f64>,
    pub mute: Option<bool>,
    pub pre_fader: Option<bool>,
    pub tap: Option<String>,
}

/// A send to take off.
#[derive(Facet)]
struct RemoveArg {
    from: String,
    to: String,
    index: Option<u32>,
    all: Option<bool>,
}

/// One send, as SENDS_LIST says it.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct SendRow {
    pub from: String,
    pub to: String,
    pub index: u32,
    pub level_db: f64,
    pub pan: f64,
    pub tap: String,
    pub pre_fader: bool,
    pub mute: bool,
    pub phase: bool,
    pub mono: bool,
    pub channels: String,
}

/// The song's sends.
#[derive(Facet)]
pub struct ListRow {
    pub sends: Vec<SendRow>,
    pub count: u32,
    pub ms: f64,
}

/// A send added (or found there already).
#[derive(Facet)]
pub struct AddRow {
    /// `added`, or `exists` (left as it was).
    pub status: String,
    pub send: SendRow,
    pub ms: f64,
}

/// Sends taken off.
#[derive(Facet)]
pub struct RemoveRow {
    pub removed: u32,
    pub ms: f64,
}

/// Linear gain as dB (−150 for silence).
#[must_use]
pub fn db(gain: f64) -> f64 {
    if gain <= 1e-8 {
        -150.0
    } else {
        20.0 * gain.log10()
    }
}

/// dB as linear gain.
#[must_use]
pub fn gain(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// A tap's word.
#[must_use]
pub const fn tap_word(mode: SendMode) -> &'static str {
    match mode {
        SendMode::PostFader => "post-fader",
        SendMode::PostFx => "pre-fader",
        SendMode::PreFx => "pre-fx",
    }
}

/// The tap `tap` (a word) or `pre_fader` names; `None` when neither is
/// given.
///
/// # Errors
/// A word that is no tap.
pub fn tap_of(tap: Option<&str>, pre_fader: Option<bool>) -> Result<Option<SendMode>, String> {
    if let Some(word) = tap {
        return match word.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "post-fader" | "post" => Ok(Some(SendMode::PostFader)),
            "pre-fader" | "post-fx" | "pre" => Ok(Some(SendMode::PostFx)),
            "pre-fx" | "input" => Ok(Some(SendMode::PreFx)),
            other => Err(format!(
                "`tap` is post-fader, pre-fader or pre-fx, not {other:?}"
            )),
        };
    }
    Ok(pre_fader.map(|pre| {
        if pre {
            SendMode::PostFx
        } else {
            SendMode::PostFader
        }
    }))
}

/// Every track's path, by GUID.
#[must_use]
pub fn paths(tracks: &[daw_proto::Track]) -> std::collections::HashMap<String, String> {
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    tracks
        .iter()
        .map(|t| {
            (
                t.guid.clone(),
                crate::gain_stage::path_of(t, &by_guid).join("/"),
            )
        })
        .collect()
}

/// One route as a row.
fn row(r: &TrackRoute, paths: &std::collections::HashMap<String, String>) -> SendRow {
    let path = |guid: &str| paths.get(guid).cloned().unwrap_or_else(|| guid.to_owned());
    let pair = |c: &daw_proto::routing::ChannelMapping| {
        let from = c.start_channel.saturating_add(1);
        let n = c.num_channels.max(2);
        format!("{from}/{}", from.saturating_add(n).saturating_sub(1))
    };
    SendRow {
        from: path(&r.source_track_guid),
        to: r.dest_track_guid.as_deref().map(path).unwrap_or_default(),
        index: r.index,
        level_db: (db(r.volume) * 100.0).round() / 100.0,
        pan: r.pan,
        tap: tap_word(r.send_mode).to_owned(),
        pre_fader: r.send_mode != SendMode::PostFader,
        mute: r.muted,
        phase: r.phase_inverted,
        mono: r.mono,
        channels: format!("{}→{}", pair(&r.source_channels), pair(&r.dest_channels)),
    }
}

/// `SENDS_LIST`: every send on the song, or from and into `track`.
///
/// # Errors
/// No such track.
pub fn list(
    daw: &Standalone,
    project: &ProjectContext,
    track: Option<&str>,
) -> Result<ListRow, String> {
    let span = tracing::info_span!(
        "SENDS_LIST",
        mix.tracks = tracing::field::Empty,
        mix.sends = tracing::field::Empty,
        mix.ms = tracing::field::Empty,
        mix.one = track.is_some(),
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let tracks = Tracks::all(daw, project.clone());
    let paths = paths(&tracks);
    let only = track.map(|t| resolve(daw, project, t)).transpose()?;
    let mut sends: Vec<SendRow> = Vec::new();
    for t in &tracks {
        let routes = Routing::sends(daw, project.clone(), TrackRef::Guid(t.guid.clone()));
        for r in routes.iter().filter(|r| {
            only.as_deref()
                .is_none_or(|o| r.source_track_guid == o || r.dest_track_guid.as_deref() == Some(o))
        }) {
            sends.push(row(r, &paths));
        }
    }
    let out = ListRow {
        count: u32::try_from(sends.len()).unwrap_or(u32::MAX),
        sends,
        ms: began.elapsed().as_secs_f64() * 1e3,
    };
    span.record("mix.tracks", tracks.len());
    span.record("mix.sends", out.count);
    span.record("mix.ms", out.ms);
    Ok(out)
}

/// The sends from `from` to `to`, by their index among `from`'s.
pub fn between(
    daw: &Standalone,
    project: &ProjectContext,
    from: &str,
    to: &str,
) -> Vec<TrackRoute> {
    Routing::sends(daw, project.clone(), TrackRef::Guid(from.to_owned()))
        .into_iter()
        .filter(|r| r.dest_track_guid.as_deref() == Some(to))
        .collect()
}

/// Whether a send from `from` to `to` would feed back: `to` is `from`, is
/// inside it, or already reaches it (through its folders and sends).
pub fn feeds_back(daw: &Standalone, project: &ProjectContext, from: &str, to: &str) -> bool {
    let tracks = Tracks::all(daw, project.clone());
    let parent: std::collections::HashMap<&str, Option<&str>> = tracks
        .iter()
        .map(|t| (t.guid.as_str(), t.parent_guid.as_deref()))
        .collect();
    // Where each track's signal goes: its folder, its sends.
    let onward = |guid: &str| -> Vec<String> {
        let mut next: Vec<String> = parent
            .get(guid)
            .copied()
            .flatten()
            .map(str::to_owned)
            .into_iter()
            .collect();
        next.extend(
            Routing::sends(daw, project.clone(), TrackRef::Guid(guid.to_owned()))
                .into_iter()
                .filter(|r| !r.muted)
                .filter_map(|r| r.dest_track_guid),
        );
        next
    };
    if from == to {
        return true;
    }
    // `to` reaches `from`: the new send closes a loop.
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![to.to_owned()];
    while let Some(at) = stack.pop() {
        if at == from {
            return true;
        }
        if seen.insert(at.clone()) {
            stack.extend(onward(&at));
        }
    }
    false
}

/// `SEND_ADD`.
///
/// # Errors
/// No such track, a bad tap, it would feed back, or the engine refused.
pub fn add(daw: &Standalone, project: &ProjectContext, a: &AddArg) -> Result<AddRow, String> {
    let span = tracing::info_span!(
        "SEND_ADD",
        mix.status = tracing::field::Empty,
        mix.level_db = a.level_db.unwrap_or(0.0),
        mix.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let from = resolve(daw, project, &a.from)?;
    let to = resolve(daw, project, &a.to)?;
    let mode = tap_of(a.tap.as_deref(), a.pre_fader)?.unwrap_or(SendMode::PostFader);
    let paths = paths(&Tracks::all(daw, project.clone()));
    if !a.again.unwrap_or(false)
        && let Some(there) = between(daw, project, &from, &to).first()
    {
        span.record("mix.status", "exists");
        return Ok(AddRow {
            status: "exists".into(),
            send: row(there, &paths),
            ms: began.elapsed().as_secs_f64() * 1e3,
        });
    }
    if feeds_back(daw, project, &from, &to) {
        tracing::warn!(
            mix.refused = "feedback",
            "SEND_ADD refused: the send would feed back"
        );
        return Err(format!(
            "a send from {} to {} would feed back: {} already reaches {}",
            a.from, a.to, a.to, a.from
        ));
    }
    let label = "Add send";
    daw::service::Projects::begin_undo_block(daw, project.clone(), label);
    let made = add_send(daw, project, (&from, &to), a.level_db.unwrap_or(0.0), mode);
    daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
    let index = made?;
    let send = Routing::sends(daw, project.clone(), TrackRef::Guid(from.clone()))
        .into_iter()
        .find(|r| r.index == index)
        .ok_or("the send went missing")?;
    crate::actions::list().changed("track", &from);
    span.record("mix.status", "added");
    span.record("mix.ms", began.elapsed().as_secs_f64() * 1e3);
    Ok(AddRow {
        status: "added".into(),
        send: row(&send, &paths),
        ms: began.elapsed().as_secs_f64() * 1e3,
    })
}

/// A send from `from` to `to` (GUIDs) at `level_db`, tapped at `mode`: its
/// index among `from`'s sends.
///
/// # Errors
/// The engine refused it.
pub fn add_send(
    daw: &Standalone,
    project: &ProjectContext,
    (from, to): (&str, &str),
    level_db: f64,
    mode: SendMode,
) -> Result<u32, String> {
    let index = Routing::add_send(
        daw,
        project.clone(),
        TrackRef::Guid(from.to_owned()),
        TrackRef::Guid(to.to_owned()),
    )
    .ok_or("the engine would not add the send")?;
    let at = RouteLocation::send(TrackRef::Guid(from.to_owned()), RouteRef::Index(index));
    Routing::set_volume(daw, project.clone(), at.clone(), gain(level_db))
        .map_err(|e| e.to_string())?;
    if mode != SendMode::PostFader {
        Routing::set_send_mode(
            daw,
            project.clone(),
            TrackRef::Guid(from.to_owned()),
            RouteRef::Index(index),
            mode,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(index)
}

/// The one send `index` picks between `from` and `to` (the only one when
/// `index` is left out).
fn pick(
    daw: &Standalone,
    project: &ProjectContext,
    (from, to): (&str, &str),
    index: Option<u32>,
    names: (&str, &str),
) -> Result<TrackRoute, String> {
    let sends = between(daw, project, from, to);
    match (index, sends.as_slice()) {
        (_, []) => Err(format!(
            "no send from {} to {} — SENDS_LIST",
            names.0, names.1
        )),
        (None, [one]) => Ok(one.clone()),
        (None, many) => Err(format!(
            "{} sends go from {} to {}: give `index` (0 … {})",
            many.len(),
            names.0,
            names.1,
            many.len().saturating_sub(1)
        )),
        (Some(i), many) => many
            .get(usize::try_from(i).unwrap_or(usize::MAX))
            .cloned()
            .ok_or_else(|| {
                format!(
                    "there are {} sends from {} to {}",
                    many.len(),
                    names.0,
                    names.1
                )
            }),
    }
}

/// `SEND_SET`.
///
/// # Errors
/// No such send, a bad value, or the engine refused.
pub fn set(daw: &Standalone, project: &ProjectContext, a: &SetArg) -> Result<SendRow, String> {
    let span = tracing::info_span!(
        "SEND_SET",
        mix.level = a.level_db.is_some(),
        mix.pan = a.pan.is_some(),
        mix.mute = a.mute.is_some(),
        mix.tap = a.tap.is_some() || a.pre_fader.is_some(),
        mix.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let from = resolve(daw, project, &a.from)?;
    let to = resolve(daw, project, &a.to)?;
    let send = pick(daw, project, (&from, &to), a.index, (&a.from, &a.to))?;
    let mode = tap_of(a.tap.as_deref(), a.pre_fader)?;
    if let Some(pan) = a.pan
        && !(-1.0..=1.0).contains(&pan)
    {
        return Err(format!("`pan` is -1 … 1, not {pan}"));
    }
    let at = RouteLocation::new(
        TrackRef::Guid(from.clone()),
        RouteType::Send,
        RouteRef::Index(send.index),
    );
    let label = "Set send";
    daw::service::Projects::begin_undo_block(daw, project.clone(), label);
    let done = (|| -> Result<(), String> {
        if let Some(level) = a.level_db {
            Routing::set_volume(daw, project.clone(), at.clone(), gain(level))
                .map_err(|e| e.to_string())?;
        }
        if let Some(pan) = a.pan {
            Routing::set_pan(daw, project.clone(), at.clone(), pan).map_err(|e| e.to_string())?;
        }
        if let Some(mute) = a.mute {
            Routing::set_muted(daw, project.clone(), at.clone(), mute)
                .map_err(|e| e.to_string())?;
        }
        if let Some(mode) = mode {
            Routing::set_send_mode(
                daw,
                project.clone(),
                TrackRef::Guid(from.clone()),
                RouteRef::Index(send.index),
                mode,
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
    done?;
    crate::actions::list().changed("track", &from);
    let now = Routing::get_route(daw, project.clone(), at).ok_or("the send went missing")?;
    span.record("mix.ms", began.elapsed().as_secs_f64() * 1e3);
    Ok(row(&now, &paths(&Tracks::all(daw, project.clone()))))
}

/// `SEND_REMOVE`.
///
/// # Errors
/// No such send, or the engine refused.
fn remove(daw: &Standalone, project: &ProjectContext, a: &RemoveArg) -> Result<RemoveRow, String> {
    let span = tracing::info_span!(
        "SEND_REMOVE",
        mix.removed = tracing::field::Empty,
        mix.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let from = resolve(daw, project, &a.from)?;
    let to = resolve(daw, project, &a.to)?;
    let mut doomed: Vec<u32> = if a.all.unwrap_or(false) {
        between(daw, project, &from, &to)
            .iter()
            .map(|r| r.index)
            .collect()
    } else {
        vec![pick(daw, project, (&from, &to), a.index, (&a.from, &a.to))?.index]
    };
    if doomed.is_empty() {
        return Err(format!("no send from {} to {}", a.from, a.to));
    }
    // Last first: taking one off renumbers the ones after it.
    doomed.sort_unstable_by(|a, b| b.cmp(a));
    let label = "Remove send";
    daw::service::Projects::begin_undo_block(daw, project.clone(), label);
    let mut removed = 0u32;
    let mut failed = None;
    for index in doomed {
        let at = RouteLocation::send(TrackRef::Guid(from.clone()), RouteRef::Index(index));
        match Routing::remove_route(daw, project.clone(), at) {
            Ok(()) => removed += 1,
            Err(e) => failed = Some(e.to_string()),
        }
    }
    daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
    crate::actions::list().changed("track", &from);
    span.record("mix.removed", removed);
    span.record("mix.ms", began.elapsed().as_secs_f64() * 1e3);
    if let Some(why) = failed.filter(|_| removed == 0) {
        return Err(why);
    }
    Ok(RemoveRow {
        removed,
        ms: began.elapsed().as_secs_f64() * 1e3,
    })
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&LIST, Effect::ReadOnly, |a: ListArg| {
        let (daw, project) = song()?;
        self::list(daw, &project, a.track.as_deref())
    })?;
    list.register(&ADD, Effect::Mutating, |a: AddArg| {
        let (daw, project) = song()?;
        add(daw, &project, &a)
    })?;
    list.register(&SET, Effect::Mutating, |a: SetArg| {
        let (daw, project) = song()?;
        set(daw, &project, &a)
    })?;
    list.register(&REMOVE, Effect::Mutating, |a: RemoveArg| {
        let (daw, project) = song()?;
        remove(daw, &project, &a)
    })?;
    Ok(())
}
