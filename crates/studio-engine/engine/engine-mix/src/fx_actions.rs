//! A track's FX over the Action List.
//!
//! Every processor on it, every parameter of each, read and set — in the
//! plugin's own words where the plugin is one of the FTS processors
//! (`-18 dB`, `Plate`, `1/2`), or as a normalized 0..1 for any plugin.
//!
//! An FX is named by its name in the chain (`FTS Reverb`, `Pro-Q 4`, any
//! part of it, case aside) or its place (`#2`, from 1); a parameter by its
//! name (`Decay`; a name a plugin repeats — the reverb's six `Post Freq` —
//! with its count: `Post Freq 6`) or its index (`#12`, from 0).
//!
//! And their automation: a parameter's envelope, its points placed at a
//! bar (`33:2`, bar 33 beat 2), a time (`12.5s`) or a section (`Chorus 1`,
//! `Chorus 1 end`), each point's value in the same words a setting takes.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{Effects, FxChainContext, FxRef, FxTarget, ProjectContext};
use facet::Facet;

use crate::actions::{meta, resolve, song};

/// Moved down into the core (shared with edit and listen).
pub use engine_core::fx_target::target;
pub use engine_core::moment::{moment, project_guid};

static LIST: ActionMeta = meta(
    "FX_LIST",
    "fx_list",
    "A track's FX",
    "The processors on `track` (its GUID, path or name), in chain order: each one's place (`#1`…), name, whether it is on, and how many parameters it has. Read-only.",
    "FX",
    "",
);

static PARAMS: ActionMeta = meta(
    "FX_PARAMS",
    "fx_params",
    "An FX's parameters",
    "Every parameter of one FX on `track` — `fx` its name (any part) or place (`#2`) — each with its index, name, its value as the plugin writes it (`-18.0 dB`, `Plate`) and normalized (0..1), and for a choice its choices. `filter` keeps the names containing it. Read-only.",
    "FX",
    "",
);

static SET: ActionMeta = meta(
    "FX_SET",
    "fx_set",
    "Set FX parameters",
    "Set parameters of one FX on `track` (`fx` its name or `#place`). `set` maps each parameter (name — `Post Freq 6` for the sixth of a repeated one —, `#index`, or `@<clap_id>`: a CLAP parameter's id, as signal.chain/1's `clap_id` names a Signal block's slot, `@b014`) to a value: the plugin's own words for an FTS processor, a hosted CLAP (its text-to-value) or any choice (`-18 dB`, `4:1`, `Plate`, `1/2`, `On`), or `=0.42` for a normalized value on any plugin. Says what each now reads, or why not. One undo step.",
    "FX",
    "",
);

static ENABLE: ActionMeta = meta(
    "FX_ENABLE",
    "fx_enable",
    "Switch an FX on or off",
    "Bypass (`on: false`) or un-bypass (`on: true`) one FX on `track` (`fx` its name or `#place`).",
    "FX",
    "",
);

static ADD: ActionMeta = meta(
    "FX_ADD",
    "fx_add",
    "Add an FX",
    "Put an FX on `track`: one of the FTS processors (`fx`: `FTS Mic`, `FTS EQ`, `FTS Comp`, …), Signal (`Signal Dev`: built into the engine, nothing installed), or a CLAP plugin by its bundle (`fx`: a path to a `.clap`, or a bundle's name in ~/.clap or the CLAP search path — `Signal Dev`, `DragonflyRoomReverb.clap`) — at place `at` in its chain (1 is first; the end when not given) — on, or bypassed with `on: false` — and `set` its parameters as FX_SET does, in the plugin's own words (a CLAP's through its own text-to-value). Says its place and what each setting now reads. One undo step.",
    "FX",
    "",
);

static AUTOMATION: ActionMeta = meta(
    "FX_AUTOMATION",
    "fx_automation",
    "A track's plugin automation",
    "Every automated FX parameter on `track` (or only `fx`'s, its name or `#place`): the FX, the parameter, its mode, and its points — each at its bar:beat and seconds, its value normalized, and its shape. Read-only.",
    "FX",
    "",
);

static AUTOMATE: ActionMeta = meta(
    "FX_AUTOMATE",
    "fx_automate",
    "Automate an FX parameter",
    "Write points on one FX parameter's envelope: `track`, `fx` (name or `#place`), `param` (name, `Post Freq 6`, or `#index`), `points` — each `at` a bar (`33`, `33:2`, `33:2.5`), a time (`12.5s`) or a section (`Chorus 1`, `Chorus 1 end`), its `value` in FX_SET's words (`-18 dB`, `Plate`, `=0.4`), and optionally its `shape` to the next point (linear, step, smooth, fast start, fast end). The envelope's points between the first and last new one are replaced; `replace: true` clears the whole envelope first. `mode` sets how it plays (read, trim, touch, latch, write, off; read by default). Plays at once, saves with the song.",
    "FX",
    "",
);

static UNAUTOMATE: ActionMeta = meta(
    "FX_UNAUTOMATE",
    "fx_unautomate",
    "Remove FX parameter automation",
    "Delete one FX parameter's automation points on `track` (`fx`, `param` as FX_AUTOMATE) — between `from` and `to` (bars, times or sections as FX_AUTOMATE's `at`) or all of them — leaving the parameter at its set value where none are left.",
    "FX",
    "",
);

static STATE: ActionMeta = meta(
    "FX_STATE",
    "fx_state",
    "An FX's state",
    "The state one FX on `track` (`fx` its name or `#place`) holds, as the plugin saves it: `state_b64` (base64), `bytes` (its size), `plugin`. What FX_STATE_SET loads back, and what a save of the song keeps. Read-only.",
    "FX",
    "",
);

static STATE_SET: ActionMeta = meta(
    "FX_STATE_SET",
    "fx_state_set",
    "Load an FX's state",
    "Load a state into one FX on `track` (`fx` its name or `#place`): `state_file` (a file holding the plugin's state blob) or `state_b64` (the blob, base64 — FX_STATE's). The plugin is then set as the state has it (values held over it by FX_SET are let go). Kept with the song when it is saved. One undo step.",
    "FX",
    "",
);

static RELOAD: ActionMeta = meta(
    "FX_RELOAD",
    "fx_reload",
    "Reload a rebuilt CLAP",
    "When the CLAP bundle at `path` (or a bundle's name, as FX_ADD takes it) was rebuilt on disk: every instance of it on the song is swapped, in place, for one of the new build, each keeping its state, its place, its switches and its set values — while the transport keeps running. A library once loaded is never unloaded, so the new build is loaded from a copy of the bundle under a temp path (`copy`); the chain still names the bundle. Says how many were reloaded, which failed and why.",
    "FX",
    "",
);

/// One FX on a track.
#[derive(Facet)]
struct FxArg {
    track: String,
    fx: String,
}

/// A state to load into one FX: a file of it, or the blob as base64.
#[derive(Facet)]
struct StateSetArg {
    track: String,
    fx: String,
    state_file: Option<String>,
    state_b64: Option<String>,
}

/// A bundle rebuilt on disk.
#[derive(Facet)]
struct ReloadArg {
    path: String,
}

/// An FX's state, read.
#[derive(Facet)]
struct StateRow {
    fx: String,
    plugin: String,
    bytes: u32,
    state_b64: String,
}

/// A state loaded.
#[derive(Facet)]
struct StateSetRow {
    fx: String,
    bytes: u32,
    ms: f64,
}

/// A bundle reloaded under its instances.
#[derive(Facet)]
struct ReloadRow {
    path: String,
    copy: String,
    instances: u32,
    reloaded: u32,
    failed: Vec<String>,
    state_bytes: u64,
    ms: f64,
}

/// A track.
#[derive(Facet)]
struct TrackArg {
    track: String,
}

/// One FX on a track, its parameters narrowed by name.
#[derive(Facet)]
struct ParamsArg {
    track: String,
    fx: String,
    filter: Option<String>,
}

/// Parameters to set on one FX.
#[derive(Facet)]
struct SetArg {
    track: String,
    fx: String,
    set: std::collections::HashMap<String, String>,
}

/// An FX to put on a track.
#[derive(Facet)]
struct AddArg {
    track: String,
    fx: String,
    at: Option<u32>,
    on: Option<bool>,
    #[facet(default)]
    set: std::collections::HashMap<String, String>,
}

/// One FX on or off.
#[derive(Facet)]
struct EnableArg {
    track: String,
    fx: String,
    on: bool,
}

/// A track's automation, or one FX's.
#[derive(Facet)]
struct AutomationArg {
    track: String,
    fx: Option<String>,
}

/// One point to write.
#[derive(Facet)]
struct PointArg {
    at: String,
    value: String,
    shape: Option<String>,
}

/// Points to write on one parameter's envelope.
#[derive(Facet)]
struct AutomateArg {
    track: String,
    fx: String,
    param: String,
    points: Vec<PointArg>,
    replace: Option<bool>,
    mode: Option<String>,
}

/// One parameter's points to delete, between two moments or all.
#[derive(Facet)]
struct UnautomateArg {
    track: String,
    fx: String,
    param: String,
    from: Option<String>,
    to: Option<String>,
}

/// An automated parameter, as listed.
#[derive(Facet)]
struct EnvelopeRow {
    fx: String,
    param: String,
    mode: String,
    points: Vec<PointRow>,
}

/// One automation point, as listed.
#[derive(Facet)]
struct PointRow {
    bar: String,
    seconds: f64,
    value: f64,
    shape: String,
}

/// An FX in a chain, as listed.
#[derive(Facet)]
struct FxRow {
    place: String,
    name: String,
    on: bool,
    parameters: u32,
}

/// One parameter, as listed.
#[derive(Facet)]
struct ParamRow {
    index: u32,
    name: String,
    value: String,
    normalized: f64,
    choices: Vec<String>,
}

/// The parameter `which` names among `params`: `#index`, a name, or a
/// repeated name with its count (`Post Freq 6`).
pub fn param_of<'a>(
    params: &'a [daw_proto::fx::FxParameter],
    which: &str,
) -> Result<&'a daw_proto::fx::FxParameter, String> {
    let which = which.trim();
    if let Some(index) = which.strip_prefix('#') {
        let index: u32 = index
            .parse()
            .map_err(|_| format!("{which:?} is not an index"))?;
        return params
            .iter()
            .find(|p| p.index == index)
            .ok_or_else(|| format!("no parameter #{index}"));
    }
    let named: Vec<&daw_proto::fx::FxParameter> = params
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case(which))
        .collect();
    if let [one] = named.as_slice() {
        return Ok(one);
    }
    if named.len() > 1 {
        return Err(format!(
            "{which:?} is {} parameters — say which: `{which} 1`…`{which} {}`",
            named.len(),
            named.len()
        ));
    }
    // A repeated name with its count.
    if let Some((base, n)) = which.rsplit_once(' ')
        && let Ok(n) = n.parse::<usize>()
    {
        let same: Vec<&daw_proto::fx::FxParameter> = params
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case(base))
            .collect();
        if let Some(found) = n.checked_sub(1).and_then(|i| same.get(i)) {
            return Ok(found);
        }
    }
    Err(format!("no parameter {which:?} (FX_PARAMS lists them)"))
}

/// The normalized value for `text` on `param` of `fx`: `=0.42` as given;
/// a choice by its label; else the plugin's own reading of the words (an
/// FTS processor's, `builtin::engine::setting`).
pub fn value_of(fx: &str, param: &daw_proto::fx::FxParameter, text: &str) -> Result<f64, String> {
    let text = text.trim();
    if let Some(raw) = text.strip_prefix('=') {
        let v: f64 = raw
            .trim()
            .parse()
            .map_err(|_| format!("{text:?} is not a number"))?;
        return Ok(v.clamp(0.0, 1.0));
    }
    if let Some((v, _)) = param
        .step_labels
        .iter()
        .find(|(_, label)| label.eq_ignore_ascii_case(text))
    {
        return Ok(*v);
    }
    // The FTS processors read their own words; the index found by name is
    // the first of a repeated one, so the value is the same either way.
    builtin::engine::setting(fx, &param.name, text)
        .map(|(_, v)| v)
        .ok_or_else(|| {
            format!(
                "{} does not read {text:?} for {} — use its own words, or `=0..1`",
                fx, param.name
            )
        })
}

/// [`value_of`] for the FX `fx` itself: a plugin hosted from a CLAP bundle
/// reads the words with its own text-to-value; the rest as `value_of`.
pub fn value_for(
    daw: &daw::standalone::Standalone,
    fx: &daw_proto::fx::Fx,
    param: &daw_proto::fx::FxParameter,
    text: &str,
) -> Result<f64, String> {
    let words = text.trim();
    if !words.starts_with('=')
        && !param
            .step_labels
            .iter()
            .any(|(_, label)| label.eq_ignore_ascii_case(words))
        && crate::clap_fx::is_hosted_clap(daw, fx)
    {
        return crate::clap_fx::text_to_normalized(daw, &fx.guid, param.index, words).ok_or_else(
            || {
                format!(
                    "{} does not read {words:?} for {} — use its own words, or `=0..1`",
                    crate::clap_fx::display_name(fx),
                    param.name
                )
            },
        );
    }
    value_of(&fx.name, param, text)
}

/// The shape a word names.
pub fn shape_of(word: Option<&str>) -> Result<daw::service::automation::EnvelopeShape, String> {
    use daw::service::automation::EnvelopeShape;
    match word.map(|w| w.trim().to_lowercase()).as_deref() {
        None | Some("" | "linear" | "line" | "ramp") => Ok(EnvelopeShape::Linear),
        Some("step" | "square" | "jump") => Ok(EnvelopeShape::Square),
        Some("smooth" | "s-curve" | "slow start end") => Ok(EnvelopeShape::SlowStartEnd),
        Some("fast start") => Ok(EnvelopeShape::FastStart),
        Some("fast end") => Ok(EnvelopeShape::FastEnd),
        Some("bezier") => Ok(EnvelopeShape::Bezier),
        Some(other) => Err(format!(
            "no shape {other:?} — linear, step, smooth, fast start, fast end"
        )),
    }
}

/// A shape's word.
pub const fn shape_word(shape: daw::service::automation::EnvelopeShape) -> &'static str {
    use daw::service::automation::EnvelopeShape;
    match shape {
        EnvelopeShape::Linear => "linear",
        EnvelopeShape::Square => "step",
        EnvelopeShape::SlowStartEnd => "smooth",
        EnvelopeShape::FastStart => "fast start",
        EnvelopeShape::FastEnd => "fast end",
        EnvelopeShape::Bezier => "bezier",
    }
}

/// The mode a word names.
pub fn mode_of(word: &str) -> Result<daw::service::AutomationMode, String> {
    use daw::service::AutomationMode;
    match word.trim().to_lowercase().as_str() {
        "read" => Ok(AutomationMode::Read),
        "trim" | "trim/read" => Ok(AutomationMode::TrimRead),
        "touch" => Ok(AutomationMode::Touch),
        "latch" => Ok(AutomationMode::Latch),
        "latch preview" => Ok(AutomationMode::LatchPreview),
        "write" => Ok(AutomationMode::Write),
        "off" => Ok(AutomationMode::Off),
        other => Err(format!(
            "no mode {other:?} — read, trim, touch, latch, write, off"
        )),
    }
}

/// A mode's word.
pub const fn mode_word(mode: daw::service::AutomationMode) -> &'static str {
    use daw::service::AutomationMode;
    match mode {
        AutomationMode::Read => "read",
        AutomationMode::TrimRead => "trim",
        AutomationMode::Touch => "touch",
        AutomationMode::Latch => "latch",
        AutomationMode::LatchPreview => "latch preview",
        AutomationMode::Write => "write",
        AutomationMode::Off => "off",
    }
}

/// Where a parameter's envelope is.
fn envelope_at(
    target: &FxTarget,
    track: &str,
    index: u32,
) -> Result<daw::service::automation::EnvelopeLocation, String> {
    let FxRef::Guid(fx_guid) = &target.fx else {
        return Err("the FX has no guid".to_owned());
    };
    Ok(daw::service::automation::EnvelopeLocation::new(
        daw::service::TrackRef::Guid(track.to_owned()),
        daw::service::automation::EnvelopeRef::FxParam {
            fx_guid: fx_guid.clone(),
            param_index: index,
        },
    ))
}

/// The track a target's chain is on.
fn track_of(target: &FxTarget) -> Result<&str, String> {
    match &target.context {
        FxChainContext::Track(guid) => Ok(guid),
        _ => Err("not a track's chain".to_owned()),
    }
}

/// Delete `at`'s points from `from` to `to`, seconds.
fn clear(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    at: &daw::service::automation::EnvelopeLocation,
    (from, to): (f64, f64),
) {
    daw::service::Automation::delete_points_in_range(
        daw,
        project.clone(),
        at.clone(),
        daw::service::automation::TimeRangeParams {
            start: daw::service::PositionInSeconds::from_seconds(from),
            end: daw::service::PositionInSeconds::from_seconds(to),
        },
    );
}

/// `FX_AUTOMATION`: the automated parameters on a track, or on one FX.
fn automation(a: &AutomationArg) -> Result<Vec<EnvelopeRow>, String> {
    let (daw, project) = song()?;
    let guid = resolve(daw, &project, &a.track)?;
    let context = FxChainContext::Track(guid.clone());
    let chain: Vec<daw_proto::fx::Fx> = match &a.fx {
        Some(which) => vec![target(daw, &project, &a.track, which)?.1],
        None => Effects::list(daw, project.clone(), context.clone()),
    };
    let envelopes = daw::service::Automation::envelopes(
        daw,
        project.clone(),
        daw::service::TrackRef::Guid(guid.clone()),
    );
    let mut rows = Vec::new();
    for fx in &chain {
        let fx_target = FxTarget {
            context: context.clone(),
            fx: FxRef::Guid(fx.guid.clone()),
        };
        let params = Effects::parameters(daw, project.clone(), fx_target.clone());
        for envelope in &envelopes {
            let (Some(fx_guid), Some(param_index)) = (&envelope.fx_guid, &envelope.param_index)
            else {
                continue;
            };
            if fx_guid != &fx.guid {
                continue;
            }
            let at = envelope_at(&fx_target, &guid, *param_index)?;
            let points = daw::service::Automation::points(daw, project.clone(), at);
            if points.is_empty() {
                continue;
            }
            rows.push(EnvelopeRow {
                fx: fx.name.clone(),
                param: params
                    .iter()
                    .find(|p| p.index == *param_index)
                    .map_or_else(|| format!("#{param_index}"), |p| p.name.clone()),
                mode: mode_word(envelope.automation_mode).to_owned(),
                points: points
                    .iter()
                    .map(|pt| {
                        let secs = pt.time.as_seconds();
                        let (bar, beat, frac) = crate::song_bars::musical(daw, &project, secs);
                        PointRow {
                            bar: format!("{bar}:{}", f64::from(beat) + frac),
                            seconds: secs,
                            value: pt.value,
                            shape: shape_word(pt.shape).to_owned(),
                        }
                    })
                    .collect(),
            });
        }
    }
    Ok(rows)
}

/// `FX_AUTOMATE`: write points on one parameter's envelope.
fn automate(a: &AutomateArg) -> Result<String, String> {
    let (daw, project) = song()?;
    let (target, fx) = target(daw, &project, &a.track, &a.fx)?;
    let params = Effects::parameters(daw, project.clone(), target.clone());
    let param = param_of(&params, &a.param)?;
    let mut points = a
        .points
        .iter()
        .map(|p| {
            Ok((
                moment(daw, &project, &p.at)?,
                value_for(daw, &fx, param, &p.value)?,
                shape_of(p.shape.as_deref())?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    points.sort_by(|x, y| x.0.total_cmp(&y.0));
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return Err("no points".to_owned());
    };
    let mode = mode_of(a.mode.as_deref().unwrap_or("read"))?;
    let at = envelope_at(&target, track_of(&target)?, param.index)?;
    let span = if a.replace.unwrap_or(false) {
        (0.0, f64::MAX)
    } else {
        // A hair either side: a point exactly on the first or last is
        // replaced too.
        (first.0 - 1e-6, last.0 + 1e-6)
    };
    clear(daw, &project, &at, span);
    for (time, value, shape) in &points {
        let _ = daw::service::Automation::add_point(
            daw,
            project.clone(),
            at.clone(),
            daw::service::automation::AddPointParams::new(
                daw::service::PositionInSeconds::from_seconds(*time),
                *value,
                *shape,
            ),
        );
    }
    daw::service::Automation::set_automation_mode(daw, project.clone(), at.clone(), mode);
    crate::actions::list().changed("fx", &fx.guid);
    Ok(format!(
        "{} {}: {} points, {} — now {} in all",
        fx.name,
        a.param.trim(),
        points.len(),
        mode_word(mode),
        daw::service::Automation::points(daw, project, at).len()
    ))
}

/// `FX_UNAUTOMATE`: delete one parameter's points, in a span or all.
fn unautomate(a: &UnautomateArg) -> Result<String, String> {
    let (daw, project) = song()?;
    let (target, fx) = target(daw, &project, &a.track, &a.fx)?;
    let params = Effects::parameters(daw, project.clone(), target.clone());
    let param = param_of(&params, &a.param)?;
    let at = envelope_at(&target, track_of(&target)?, param.index)?;
    let from = a
        .from
        .as_deref()
        .map(|t| moment(daw, &project, t))
        .transpose()?
        .unwrap_or(0.0);
    let to =
        a.to.as_deref()
            .map(|t| moment(daw, &project, t))
            .transpose()?
            .unwrap_or(f64::MAX);
    let before = daw::service::Automation::points(daw, project.clone(), at.clone()).len();
    clear(daw, &project, &at, (from, to));
    let after = daw::service::Automation::points(daw, project, at).len();
    crate::actions::list().changed("fx", &fx.guid);
    Ok(format!(
        "{} {}: {} points removed, {after} left",
        fx.name,
        a.param.trim(),
        before.saturating_sub(after)
    ))
}

/// `set` (parameter → the plugin's words) on the FX `target` (`fx`), in
/// name order: what each now reads, or why not.
fn settle(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (target, fx): (&FxTarget, &daw_proto::fx::Fx),
    set: &std::collections::HashMap<String, String>,
) -> Vec<String> {
    let params = Effects::parameters(daw, project.clone(), target.clone());
    let mut settings: Vec<(&String, &String)> = set.iter().collect();
    settings.sort();
    settings
        .into_iter()
        .map(|(which, text)| {
            slot_named(daw, &fx.guid, which)
                .and_then(|slot| match slot {
                    Some(index) => params
                        .iter()
                        .find(|p| p.index == index)
                        .ok_or_else(|| format!("no parameter #{index}")),
                    None => param_of(&params, which),
                })
                .and_then(|param| {
                    let value = value_for(daw, fx, param, text)?;
                    Effects::set_parameter(
                        daw,
                        project.clone(),
                        daw_proto::fx::SetParameterRequest {
                            target: target.clone(),
                            index: param.index,
                            value,
                        },
                    )
                    .map_err(|e| e.to_string())?;
                    // A Signal chain kept by Studio has the old value.
                    crate::clap_host::signal::forget_chain(&fx.guid);
                    let now = Effects::parameter(daw, project.clone(), target.clone(), param.index)
                        .map_or_else(String::new, |p| p.formatted);
                    // As asked: `Post Freq 6`, not the name all six share.
                    Ok(format!("{} = {now}", which.trim()))
                })
                .unwrap_or_else(|e| format!("{which}: {e}"))
        })
        .collect()
}

/// `FX_SET`: `set` (parameter → the plugin's words) on the FX `fx` of
/// `track`; what each now reads, or why not.
///
/// # Errors
/// No such track or FX.
pub fn set(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (track, fx): (&str, &str),
    set: &std::collections::HashMap<String, String>,
) -> Result<Vec<String>, String> {
    let (target, fx) = target(daw, project, track, fx)?;
    let said = settle(daw, project, (&target, &fx), set);
    crate::actions::list().changed("fx", &fx.guid);
    Ok(said)
}

/// The slot `which` names by its CLAP id (`@b014`): the plugin's
/// parameter whose CLAP id is that (decimal, or hex as `b014` or
/// `0xb014`), or whose Signal stable id is (signal/1's
/// `param_stable_id`), or else whose name is. `None` when `which` is no
/// `@` name (a name or `#index`, for [`param_of`]).
///
/// # Errors
/// An `@` name no parameter of the plugin has.
pub fn slot_named(
    daw: &daw::standalone::Standalone,
    fx_guid: &str,
    which: &str,
) -> Result<Option<u32>, String> {
    let Some(id) = which.trim().strip_prefix('@').map(str::trim) else {
        return Ok(None);
    };
    let hex = id.strip_prefix("0x").unwrap_or(id);
    let numbers = [id.parse::<u32>().ok(), u32::from_str_radix(hex, 16).ok()];
    let found = daw.with_plugin_instance(fx_guid, |p| {
        let infos = p.params();
        let signal = p.signal();
        let by = |f: &dyn Fn(&daw_standalone::plugin::PluginParamInfo) -> bool| {
            infos.iter().position(|i| f(i))
        };
        by(&|i| numbers.contains(&Some(i.id)))
            .or_else(|| {
                by(&|i| {
                    signal
                        .as_ref()
                        .and_then(|s| s.param_stable_id(i.id))
                        .is_some_and(|s| s == id)
                })
            })
            .or_else(|| by(&|i| i.name.eq_ignore_ascii_case(id)))
    });
    match found {
        None => Err(format!(
            "{which}: the FX has no live plugin to name a CLAP id on"
        )),
        Some(None) => Err(format!("no parameter with the CLAP id {id}")),
        Some(Some(at)) => u32::try_from(at)
            .map(Some)
            .map_err(|_| format!("{which}: past the last slot")),
    }
}

/// What `fx` names to add: one of the FTS processors (by its name), a
/// CLAP compiled into this engine (Signal: `Signal Dev`, found with no
/// bundle installed), or a CLAP bundle (its path, or its name in the CLAP
/// folders).
enum Addable {
    Builtin(String),
    BuiltinClap(String),
    Clap(std::path::PathBuf),
}

fn addable(fx: &str) -> Result<Addable, String> {
    if daw_standalone::plugin::FxFactory::provides(&builtin::engine::BuiltinFx, fx) {
        return Ok(Addable::Builtin(fx.to_owned()));
    }
    // Before the folders: the engine's own build wins over an installed
    // bundle of the same plugin, as a song's FX does when it loads.
    if daw_standalone::plugin::builtin_clap(fx).is_some() {
        return Ok(Addable::BuiltinClap(fx.trim().to_owned()));
    }
    crate::clap_fx::bundle(fx).map(Addable::Clap).ok_or_else(|| {
        format!(
            "{fx} is neither one of the FTS processors (FTS Mic, FTS EQ, FTS Comp, …) nor a CLAP bundle (a path to a .clap, or a bundle's name in {})",
            crate::clap_fx::search_path()
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

/// `FX_ADD`.
fn add(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    a: &AddArg,
) -> Result<Vec<String>, String> {
    let span = tracing::info_span!(
        "FX_ADD",
        fx.kind = tracing::field::Empty,
        fx.added = tracing::field::Empty,
        fx.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let what = addable(&a.fx)?;
    let guid = resolve(daw, project, &a.track)?;
    let context = FxChainContext::Track(guid);
    let index = a.at.unwrap_or(u32::MAX).saturating_sub(1);
    let label = format!("Add {}", a.fx);
    daw::service::Projects::begin_undo_block(daw, project.clone(), &label);
    let added = match &what {
        Addable::Builtin(name) => {
            span.record("fx.kind", "builtin");
            Effects::add_at(
                daw,
                project.clone(),
                daw_proto::fx::AddFxAtRequest {
                    context: context.clone(),
                    name: name.clone(),
                    index,
                },
            )
            .ok_or_else(|| format!("{} could not be added", a.fx))
        }
        Addable::BuiltinClap(name) => {
            span.record("fx.kind", "builtin-clap");
            crate::clap_fx::add_builtin(daw, project, &context, name, index)
        }
        Addable::Clap(bundle) => {
            span.record("fx.kind", "clap");
            crate::clap_fx::add(daw, project, &context, bundle, index)
        }
    };
    let out = added.and_then(|fx_guid| {
        let target = FxTarget {
            context: context.clone(),
            fx: FxRef::Guid(fx_guid.clone()),
        };
        if !a.on.unwrap_or(true) {
            Effects::set_enabled(daw, project.clone(), target.clone(), false)
                .map_err(|e| e.to_string())?;
        }
        let fx = Effects::get(daw, project.clone(), target.clone())
            .ok_or_else(|| format!("{} is not on the track", a.fx))?;
        let mut said = settle(daw, project, (&target, &fx), &a.set);
        let place = Effects::list(daw, project.clone(), context.clone())
            .iter()
            .position(|f| f.guid == fx_guid)
            .map_or(0, |i| i.saturating_add(1));
        said.insert(
            0,
            format!(
                "{} at #{place}{}",
                crate::clap_fx::display_name(&fx),
                if a.on.unwrap_or(true) {
                    ""
                } else {
                    ", bypassed"
                }
            ),
        );
        crate::actions::list().changed("fx", &fx_guid);
        Ok(said)
    });
    daw::service::Projects::end_undo_block(daw, project.clone(), &label, None);
    span.record("fx.added", out.is_ok());
    span.record("fx.ms", began.elapsed().as_secs_f64() * 1e3);
    out
}

/// The size of a blob, as a row says it.
fn size_of(bytes: &[u8]) -> u32 {
    u32::try_from(bytes.len()).unwrap_or(u32::MAX)
}

/// `FX_STATE`.
fn state(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    a: &FxArg,
) -> Result<StateRow, String> {
    use base64::Engine as _;
    let span = tracing::info_span!("FX_STATE", fx.state_bytes = tracing::field::Empty);
    let _in = span.enter();
    let (target, fx) = target(daw, project, &a.track, &a.fx)?;
    let bytes = match daw.save_plugin_state(&fx.guid) {
        Ok(bytes) => bytes,
        Err(_) => Effects::state_chunk(daw, project.clone(), target).unwrap_or_default(),
    };
    span.record("fx.state_bytes", bytes.len());
    Ok(StateRow {
        fx: crate::clap_fx::display_name(&fx),
        plugin: fx.plugin_name.clone(),
        bytes: size_of(&bytes),
        state_b64: base64::engine::general_purpose::STANDARD.encode(&bytes),
    })
}

/// The blob a state set names: its file's bytes, or its base64 decoded.
fn state_bytes(file: Option<&str>, b64: Option<&str>) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    match (file, b64) {
        (Some(file), None) => std::fs::read(file).map_err(|e| format!("{file}: {e}")),
        (None, Some(b64)) => base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| format!("state_b64 is not base64: {e}")),
        (Some(_), Some(_)) => Err("give `state_file` or `state_b64`, not both".to_owned()),
        (None, None) => Err("give `state_file` or `state_b64`".to_owned()),
    }
}

/// `FX_STATE_SET`.
fn state_set(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    a: &StateSetArg,
) -> Result<StateSetRow, String> {
    let span = tracing::info_span!(
        "FX_STATE_SET",
        fx.state_bytes = tracing::field::Empty,
        fx.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let song = project_guid(project)?;
    let bytes = state_bytes(a.state_file.as_deref(), a.state_b64.as_deref())?;
    span.record("fx.state_bytes", bytes.len());
    let (_, fx) = target(daw, project, &a.track, &a.fx)?;
    let name = crate::clap_fx::display_name(&fx);
    let label = format!("Load {name} state");
    daw::service::Projects::begin_undo_block(daw, project.clone(), &label);
    let loaded = crate::clap_fx::load_state(daw, &song, &fx.guid, &bytes);
    daw::service::Projects::end_undo_block(daw, project.clone(), &label, None);
    loaded.map_err(|e| format!("{name}: {e}"))?;
    crate::actions::list().changed("fx", &fx.guid);
    let ms = began.elapsed().as_secs_f64() * 1e3;
    span.record("fx.ms", ms);
    Ok(StateSetRow {
        fx: name,
        bytes: size_of(&bytes),
        ms,
    })
}

/// Whether the FX named `name` in its chain is the bundle at `bundle`.
pub fn is_bundle(name: &str, bundle: &std::path::Path) -> bool {
    let path = std::path::Path::new(name);
    path == bundle || std::fs::canonicalize(path).is_ok_and(|p| p == bundle)
}

/// `FX_RELOAD`: [`crate::clap_fx::swap_instance`] under every instance of
/// the bundle on the song.
fn reload(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    a: &ReloadArg,
) -> Result<ReloadRow, String> {
    let span = tracing::info_span!(
        "FX_RELOAD",
        fx.instances = tracing::field::Empty,
        fx.reloaded = tracing::field::Empty,
        fx.failed = tracing::field::Empty,
        fx.state_bytes = tracing::field::Empty,
        fx.ms = tracing::field::Empty,
    );
    let _in = span.enter();
    let began = web_time::Instant::now();
    let song = project_guid(project)?;
    let bundle = crate::clap_fx::bundle(&a.path)
        .ok_or_else(|| format!("{} is not a CLAP bundle", a.path))?;
    let instances: Vec<(String, String)> = daw
        .read_project(&song, |p| {
            p.fx_chains
                .values()
                .flatten()
                .filter(|e| is_bundle(&e.fx.name, &bundle))
                .map(|e| (e.fx.guid.clone(), e.fx.plugin_name.clone()))
                .collect()
        })
        .unwrap_or_default();
    span.record("fx.instances", instances.len());
    if instances.is_empty() {
        return Err(format!("no instance of {} on the song", bundle.display()));
    }
    let copy = crate::clap_fx::fresh_copy(&bundle)?;
    let mut row = ReloadRow {
        path: bundle.display().to_string(),
        copy: copy.display().to_string(),
        instances: u32::try_from(instances.len()).unwrap_or(u32::MAX),
        reloaded: 0,
        failed: Vec::new(),
        state_bytes: 0,
        ms: 0.0,
    };
    for (fx_guid, plugin) in &instances {
        match crate::clap_fx::swap_instance(daw, fx_guid, &copy) {
            Ok(bytes) => {
                row.reloaded = row.reloaded.saturating_add(1);
                row.state_bytes = row
                    .state_bytes
                    .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
                crate::actions::list().changed("fx", fx_guid);
            }
            Err(e) => row.failed.push(format!("{plugin} ({fx_guid}): {e}")),
        }
    }
    row.ms = began.elapsed().as_secs_f64() * 1e3;
    span.record("fx.reloaded", row.reloaded);
    span.record("fx.failed", row.failed.len());
    span.record("fx.state_bytes", row.state_bytes);
    span.record("fx.ms", row.ms);
    if !row.failed.is_empty() {
        tracing::warn!(
            fx.failed = row.failed.len(),
            "some instances kept the old build"
        );
    }
    Ok(row)
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&LIST, Effect::ReadOnly, |a: TrackArg| {
        let (daw, project) = song()?;
        let guid = resolve(daw, &project, &a.track)?;
        let context = FxChainContext::Track(guid);
        Ok(Effects::list(daw, project.clone(), context.clone())
            .into_iter()
            .enumerate()
            .map(|(i, fx)| FxRow {
                place: format!("#{}", i.saturating_add(1)),
                parameters: u32::try_from(
                    Effects::parameters(
                        daw,
                        project.clone(),
                        FxTarget {
                            context: context.clone(),
                            fx: FxRef::Guid(fx.guid.clone()),
                        },
                    )
                    .len(),
                )
                .unwrap_or(u32::MAX),
                name: crate::clap_fx::display_name(&fx),
                on: fx.enabled,
            })
            .collect::<Vec<_>>())
    })?;
    list.register(&PARAMS, Effect::ReadOnly, |a: ParamsArg| {
        let (daw, project) = song()?;
        let (target, _) = target(daw, &project, &a.track, &a.fx)?;
        let filter = a.filter.map(|f| f.to_lowercase());
        Ok(Effects::parameters(daw, project, target)
            .into_iter()
            .filter(|p| {
                filter
                    .as_ref()
                    .is_none_or(|f| p.name.to_lowercase().contains(f))
            })
            .map(|p| ParamRow {
                index: p.index,
                choices: p.step_labels.iter().map(|(_, l)| l.clone()).collect(),
                name: p.name,
                value: p.formatted,
                normalized: p.value,
            })
            .collect::<Vec<_>>())
    })?;
    list.register(&SET, Effect::Mutating, |a: SetArg| {
        let (daw, project) = song()?;
        set(daw, &project, (&a.track, &a.fx), &a.set)
    })?;
    list.register(&ADD, Effect::Mutating, |a: AddArg| {
        let (daw, project) = song()?;
        add(daw, &project, &a)
    })?;
    list.register(&STATE, Effect::ReadOnly, |a: FxArg| {
        let (daw, project) = song()?;
        state(daw, &project, &a)
    })?;
    list.register(&STATE_SET, Effect::Mutating, |a: StateSetArg| {
        let (daw, project) = song()?;
        state_set(daw, &project, &a)
    })?;
    list.register(&RELOAD, Effect::Mutating, |a: ReloadArg| {
        let (daw, project) = song()?;
        reload(daw, &project, &a)
    })?;
    list.register(&AUTOMATION, Effect::ReadOnly, |a: AutomationArg| {
        automation(&a)
    })?;
    list.register(&AUTOMATE, Effect::Mutating, |a: AutomateArg| automate(&a))?;
    list.register(&UNAUTOMATE, Effect::Mutating, |a: UnautomateArg| {
        unautomate(&a)
    })?;
    list.register(&ENABLE, Effect::Mutating, |a: EnableArg| {
        let (daw, project) = song()?;
        let (target, fx) = target(daw, &project, &a.track, &a.fx)?;
        Effects::set_enabled(daw, project, target, a.on).map_err(|e| e.to_string())?;
        Ok(format!(
            "{} {}",
            fx.name,
            if a.on { "on" } else { "bypassed" }
        ))
    })?;
    Ok(())
}

#[cfg(test)]
mod clap_tests;
