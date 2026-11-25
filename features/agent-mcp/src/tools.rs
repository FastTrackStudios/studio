//! The tools an agent sees.
//!
//! A curated set named for what they do, each one action of Studio's (its arguments' schema taken from Studio, so it
//! never drifts) or a few run together, plus the generic five over the
//! whole Action List and the bridge's own (`job`, `jobs`, `job_cancel`,
//! `more`).

use serde_json::{Map, Value, json};

use crate::jobs::Job;
use crate::shape::{self, Detail};
use crate::upstream::{self, Failure, Upstream, failure};

/// What a tool runs on: Studio and the job it is.
pub struct Ctx<'a> {
    pub up: &'a dyn Upstream,
    pub job: &'a Job,
}

impl Ctx<'_> {
    /// Run action `id`, saying so as the job's step.
    ///
    /// # Errors
    ///
    /// Studio's failure.
    pub fn run(&self, id: &str, args: &Value) -> Result<Value, Failure> {
        self.job.step(id);
        upstream::run(self.up, id, args, &self.job.cancel)
    }

    /// A protocol method (`actions`, `describe`, `batch`, `watch`).
    ///
    /// # Errors
    ///
    /// Studio's failure.
    pub fn method(&self, method: &str, params: &Value) -> Result<Value, Failure> {
        self.job.step(method);
        self.up.call(method, params, &self.job.cancel)
    }
}

/// How a tool runs.
#[derive(Clone, Copy, Debug)]
pub enum Exec {
    /// One action, its arguments prepared and its answer shaped by id.
    Action(&'static str),
    /// Several, run together by the bridge.
    Composite(fn(&Ctx<'_>, &Value) -> Result<Value, Failure>),
}

/// One tool.
#[derive(Clone, Copy, Debug)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub exec: Exec,
    /// The schema of a composite (an action's comes from Studio).
    pub schema: Option<fn() -> Value>,
}

const T: &str =
    "`track`: a path (`Drums/Kick/Sum/In`), a tail of it (`Kick/Sum/In`), a name or a GUID.";

macro_rules! tool {
    ($name:literal, $id:literal, $desc:expr) => {
        Tool {
            name: $name,
            description: $desc,
            exec: Exec::Action($id),
            schema: None,
        }
    };
}

/// The curated tools, in the order a client lists them.
#[must_use]
#[allow(clippy::too_many_lines)] // One entry a tool: the list is the catalog.
pub fn curated() -> Vec<Tool> {
    vec![
        // ── where things are ──
        tool!(
            "session_tree",
            "SESSION_TRACKS",
            "The open song's tracks as a compact tree: one line each — path (folders end in `/`), fader, pan, muted/solo/ø, FX names, items. Start here to learn the paths every other tool takes. `under: \"Drums\"` keeps one folder; `filter` any path containing it. `view: \"list\"` gives objects with `fields` (default path, folder, volume_db, pan, muted, soloed, fx); `view: \"full\"` everything (GUIDs, lanes)."
        ),
        tool!(
            "sections",
            "SESSION_SECTIONS",
            "The song's sections in order: name, start/end seconds, bars. Their names work as `from`/`to` in every listen, measure and state tool (`CH 1`, `CH 1 end`)."
        ),
        tool!(
            "mix_state",
            "MIX_STATE",
            "One read of a track and everything inside it (`depth` levels; 0 the track alone): fader, pan, mute/solo, sends, latency, FX by name (`#3 Signal Dev`, `(off)` when off) and its Signal chain — phases, their make-up, each block's id, kind, key and the parameters off their defaults (`set`). Compact by default; `detail: \"params\"` adds every FX and block parameter as a line, `detail: \"full\"` the raw answer (paged). Read-only."
        ),
        tool!(
            "signal_block",
            "SIGNAL_CHAIN",
            "A Signal block addressed by track + block id (`tone.comp`, `rescue.gate`, or just `comp` when only one): every parameter as `id = text (default …)`, its bypass and key. Without `block`: the whole chain compact (phases, blocks, what differs). Read-only. Set it with signal_block_set."
        ),
        // ── listening and measuring ──
        tool!(
            "listen",
            "LISTEN",
            "Render only what feeds one point of a track, offline, and measure it: LUFS, short-term max, true peak, peak, RMS, crest, third-octave bands, width, correlation, and per compressor/gate (or Signal instance) its make-up and gain reduction. `at`: `post-fader` (default), `pre-fx`, `post-fx`, `input`, `fx:<n>:in|out`, `block:<id>:pre|post`. Fast on repeats (unchanged inputs replay). Series are dropped by default; `detail: \"full\"` keeps them. Read-only."
        ),
        Tool {
            name: "block_measure",
            description: "What one Signal block does, in one call: LISTEN at `block:<id>:pre` and `:post` on `track` over `from`..`to`, answered as the input and output figures and their difference — `gain_db` (its loudness change), `crest_db` (transients squeezed < 0 or opened > 0), peak and width changes, the instance's gain reduction — and with `bands: true` the band-by-band EQ curve it applied. Read-only.",
            exec: Exec::Composite(block_measure),
            schema: Some(block_measure_schema),
        },
        tool!(
            "listen_ab",
            "LISTEN_AB",
            "Before/after a change, level-matched: listens at one point, runs `change` (a list of `{action, args}`, e.g. `{\"action\": \"SIGNAL_BLOCK_SET\", \"args\": {…}}`) as one undo step, listens again, and puts the mix back unless `keep: true`. Answers `a`, `b`, `difference` and `matched` — the difference with the loudness change (`gain_db`) taken out, so what is left is tone and dynamics."
        ),
        tool!(
            "listen_hits",
            "LISTEN_HITS",
            "A drum piece's hits cut out of the mix where they play now (`piece`: Kick, Snare, Tom 1…), averaged: peak, RMS, attack, decay, head/tail spectra, spread, and per mic its offset, polarity, correlation and share of the sum. Read-only."
        ),
        tool!(
            "listen_align",
            "LISTEN_ALIGN",
            "How each mic of a drum piece lines up with its reference mic on the attacks — offset (samples/ms), polarity, correlation — and what aligning would gain; see `describe LISTEN_ALIGN` for applying it."
        ),
        tool!(
            "mix_measure",
            "MIX_MEASURE",
            "Render a span offline through the whole mix and measure each of `tracks` (default: the Drums folder and everything in it) and the master: LUFS, short max, true peak, peak, RMS, crest, width, balance, `share_db` of its folder; `bypass_signal: true` adds raw numbers and Signal's gain per track. One flat row a track; bands only with `with_bands: true`. A whole song takes minutes: it becomes a job (see `job`). Read-only."
        ),
        Tool {
            name: "balance",
            description: "The balance in one call: every folder's (or each of `tracks`') LUFS against a `reference` track over `from`..`to`, loudest first — `rel_db` is how far above or below the reference it sits, `share_db` its part of its parent. `under: \"Drums\", depth: 1` reads the kit's pieces. Renders the mix offline (MIX_MEASURE); long spans become a job. Read-only.",
            exec: Exec::Composite(balance),
            schema: Some(balance_schema),
        },
        // ── changing the mix ──
        tool!(
            "track_set",
            "TRACK_SET",
            "Set a track's fader (`volume_db`), pan (-1…1), `mute`, `solo`, `phase` (polarity), monitoring. One undo step."
        ),
        tool!(
            "fx_params",
            "FX_PARAMS",
            "One FX's parameters as lines: `#index name = value (normalized)`; `filter` keeps names containing it, `choices: true` adds each choice list. Read-only. For a Signal block use signal_block."
        ),
        tool!(
            "fx_set",
            "FX_SET",
            "Set parameters of one FX on a track (`fx` its name or `#place`): `set` maps each parameter (name, `#index`) to a value — its text (`-6 dB`, `Bell`), a number in its units, or normalized. One undo step."
        ),
        tool!(
            "signal_block_set",
            "SIGNAL_BLOCK_SET",
            "Set one Signal block's parameters by id, whether or not an FX slot shows them: `track`, `block` (`tone.eq`, `comp`), `params` `{\"band.7.freq\": 450, \"band.7.gain\": -6, \"makeup\": \"2 dB\"}` (numbers in their units, booleans, or text) and/or `bypass`. Lands at once, playing or stopped; one undo step. Answers the block as it is now (compact)."
        ),
        tool!(
            "signal_run",
            "SIGNAL_RUN",
            "Run one of Signal's own actions on a track's Signal instance: `action` (`SIGNAL_SET`, `SIGNAL_BLOCK_ADD`, `SIGNAL_BLOCK_REMOVE`, `SIGNAL_BLOCK_MOVE`, `SIGNAL_PRESET_LOAD`; `list` lists them), `args` an object. E.g. `{\"track\": \"Kick/Sum/In\", \"action\": \"SIGNAL_SET\", \"args\": {\"phase\": \"tone\", \"makeup_db\": 0.5}}`."
        ),
        tool!(
            "mix_snapshot",
            "MIX_SNAPSHOT",
            "Keep the mix as it stands under `name` (faders, pans, mutes, solos, polarity, sends, FX switches and states; a `scope` of tracks or some categories only) to come back to or compare."
        ),
        tool!(
            "mix_restore",
            "MIX_RESTORE",
            "Put the mix back as snapshot `name` has it (`group:variant` for a mix variant). One undo step."
        ),
        tool!(
            "mix_diff",
            "MIX_DIFF",
            "What differs between snapshot `a` and `b` (`current` is the mix as it stands, the default `b`). Read-only."
        ),
        // ── drums ──
        tool!(
            "drums_level",
            "DRUMS_LEVEL",
            "Even each drum's strokes toward its median level before editing (the kick most of the way, the snare and toms less), from the hit markers. `dry_run` first to see what it would do."
        ),
        tool!(
            "drums_fundamental",
            "DRUMS_FUNDAMENTAL",
            "A drum's fundamental (Hz) read off its mics after each trigger hit — what its Fund/Sub band-pass is tuned to. Read-only."
        ),
        tool!(
            "hits_list",
            "HITS_LIST",
            "The hit markers: id, piece, time recorded and where it plays now, level, approval/ghost/bleed flags. Long lists page (`more`). Read-only."
        ),
        tool!(
            "hits_mark",
            "HITS_MARK",
            "Set what is known about hits `ids`: `approve`, `ghost`, `bleed`, `note`, `grace_of`."
        ),
        // ── the app ──
        tool!(
            "session_mode",
            "SESSION_MODE",
            "Put Studio's windows in a mode: Organize, Write, Produce, Record, Edit, Mix, Master, Live, Video or Scoring."
        ),
        tool!(
            "session_open",
            "STUDIO_SESSION_OPEN",
            "Open a set as the session (the whole window): its setlist bar, the song's view."
        ),
        tool!(
            "engine_status",
            "STUDIO_ENGINE",
            "Studio's engine: off, linking, opening, ready (pid) or failed (why), and its transport. Read-only."
        ),
    ]
}

/// The generic tools, over the whole Action List.
#[must_use]
pub fn generic() -> Vec<Tool> {
    vec![
        Tool {
            name: "actions",
            description: "Every action Studio has (~580), one line each: `ID — name`, `[mutating]` where it changes something. `filter` matches ids, names and descriptions (`filter: \"variant\"`), `category`, `effect` (`read-only`, `mutating`). Then `describe` one, `run` it.",
            exec: Exec::Composite(actions),
            schema: Some(actions_schema),
        },
        Tool {
            name: "describe",
            description: "One action in full: what it does, what it takes (JSON Schema) and gives. The curated tools' descriptions are short; this is the whole text.",
            exec: Exec::Composite(describe),
            schema: Some(describe_schema),
        },
        Tool {
            name: "run",
            description: "Run any action by `id` with `args` (an object). Answers known to be big (MIX_STATE, SESSION_TRACKS, LISTEN, MIX_MEASURE, FX_PARAMS) are compacted as their curated tools do; `detail: \"full\"` for the raw answer. Big answers page (`more`); long calls become jobs (`job`).",
            exec: Exec::Composite(run),
            schema: Some(run_schema),
        },
        Tool {
            name: "batch",
            description: "Run several actions in order (`steps`: each `{id, args}`), stopping at the first failure unless `stop_on_error: false`. At most 64; not atomic.",
            exec: Exec::Composite(batch),
            schema: Some(batch_schema),
        },
        Tool {
            name: "watch",
            description: "What happened in Studio since `since` (a point from an earlier watch; omit for the latest): actions run and state changed, waiting up to `wait_ms` (30 s at most) for the next when nothing has yet.",
            exec: Exec::Composite(watch),
            schema: Some(watch_schema),
        },
    ]
}

/// The bridge's own tools' schemas: `(name, description, schema)`.
#[must_use]
pub fn bridge_tools() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        (
            "job",
            "A job's result: a call that outlived its wait answered `{job: \"j3\", state: \"running\"}`. This waits up to `wait` seconds (20 by default, 600 at most) for it and answers its result when done, or how it stands (elapsed, step) while it runs. Call again to keep waiting.",
            json!({"type": "object", "properties": {"job": {"type": "string"}, "wait": {"type": "number", "minimum": 0, "maximum": 600}}, "required": ["job"]}),
        ),
        (
            "jobs",
            "The calls running (or done) in the background, newest first: id, tool, state, elapsed, step.",
            json!({"type": "object", "properties": {}}),
        ),
        (
            "job_cancel",
            "Stop waiting for a job and drop its connection to Studio. A render Studio already started runs to its end there and its answer is thrown away.",
            json!({"type": "object", "properties": {"job": {"type": "string"}}, "required": ["job"]}),
        ),
        (
            "more",
            "The next page of a big answer: one that did not fit says `page: {result: \"r2\", next: 40, more: …}`. `offset` where to start (its `next`), `limit` items at most.",
            json!({"type": "object", "properties": {"result": {"type": "string"}, "offset": {"type": "integer", "minimum": 0}, "limit": {"type": "integer", "minimum": 1}}, "required": ["result", "offset"]}),
        ),
    ]
}

/// Every tool that runs on Studio: the curated, then the generic.
#[must_use]
pub fn all() -> Vec<Tool> {
    let mut t = curated();
    t.extend(generic());
    t
}

/// The bridge's own arguments of an action tool (stripped before Studio
/// sees them), as schema properties.
#[must_use]
pub fn extras(id: &str) -> Value {
    let detail = json!({"type": "string", "enum": ["summary", "params", "full"], "description": "How much: `summary` (default), `params` (every parameter as a line), `full` (the raw answer, paged)."});
    let mut p = Map::new();
    match id {
        "MIX_STATE" | "SIGNAL_CHAIN" => {
            p.insert("detail".into(), detail);
        }
        "SESSION_TRACKS" => {
            p.insert("view".into(), json!({"type": "string", "enum": ["tree", "list", "full"], "description": "`tree` (default): a line a track; `list`: objects with `fields`; `full`: everything."}));
            p.insert("fields".into(), json!({"type": "array", "items": {"type": "string"}, "description": "With `view: \"list\"`: which fields (guid, name, path, folder, muted, soloed, inverted, volume_db, pan, items, fx, lanes, armed, record_input, open)."}));
            p.insert("under".into(), json!({"type": "string", "description": "Only this folder and what is in it (`Drums`)."}));
        }
        "SIGNAL_BLOCK_SET" => {
            p.insert("params".into(), json!({"type": "object", "description": "`{\"<param id>\": value}`: ids as signal_block lists them; a number in its units, a boolean, or its text."}));
        }
        "FX_PARAMS" => {
            p.insert(
                "choices".into(),
                json!({"type": "boolean", "description": "Add each choice parameter's choices."}),
            );
        }
        "LISTEN" | "LISTEN_AB" => {
            p.insert("detail".into(), json!({"type": "string", "enum": ["summary", "full"], "description": "`full` keeps the per-second and per-hop series."}));
        }
        "MIX_MEASURE" => {
            p.insert("with_bands".into(), json!({"type": "boolean", "description": "Keep each track's bands (dropped by default)."}));
            p.insert(
                "detail".into(),
                json!({"type": "string", "enum": ["summary", "full"]}),
            );
        }
        _ => {}
    }
    if id == "SIGNAL_CHAIN" {
        p.insert("block".into(), json!({"type": "string", "description": "A block's id (`tone.comp`, or `comp` when only one block is one)."}));
    }
    Value::Object(p)
}

/// Fix what an action's own schema says loosely (a JSON string where an
/// object is meant).
pub fn fix_schema(id: &str, takes: &mut Value) {
    let Some(props) = takes.get_mut("properties").and_then(Value::as_object_mut) else {
        return;
    };
    match id {
        "LISTEN_AB" => {
            props.insert("change".into(), json!({"type": "array", "description": "The calls to make between a and b, in order.", "items": {"type": "object", "properties": {"action": {"type": "string"}, "args": {"type": "object"}}, "required": ["action"]}}));
        }
        "SIGNAL_RUN" => {
            props.insert(
                "args".into(),
                json!({"type": "object", "description": "The Signal action's arguments."}),
            );
        }
        _ => {}
    }
}

/// The bridge's own arguments, stripped before an action sees them.
const BRIDGE_ARGS: &[&str] = &["detail", "view", "fields", "under", "choices", "with_bands"];

fn strip(args: &Value, keep_for: &str) -> Value {
    let mut out = args.as_object().cloned().unwrap_or_default();
    for k in BRIDGE_ARGS {
        out.remove(*k);
    }
    if keep_for == "SIGNAL_CHAIN" {
        out.remove("block");
    }
    Value::Object(out)
}

fn bad(e: impl Into<String>) -> Failure {
    failure("bad_args", e)
}

/// Run action `id` the way its curated tool does: its arguments
/// prepared, its answer made compact.
///
/// # Errors
///
/// Studio's failure, or bad bridge arguments.
pub fn run_action(ctx: &Ctx<'_>, id: &str, args: &Value) -> Result<Value, Failure> {
    let default = match id {
        "SIGNAL_CHAIN" if args.get("block").is_some() => Detail::Params,
        _ => Detail::Summary,
    };
    let detail = Detail::of(args, default).map_err(bad)?;
    let mut up = strip(args, id);
    if id == "MIX_STATE"
        && detail == Detail::Summary
        && let Some(o) = up.as_object_mut()
    {
        o.entry("params").or_insert(json!(false));
    }
    let answer = ctx.run(id, &up)?;
    Ok(match id {
        "MIX_STATE" => shape::mix_state(&answer, detail),
        "SESSION_TRACKS" => {
            let fields: Option<Vec<String>> =
                args.get("fields").and_then(Value::as_array).map(|f| {
                    f.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                });
            shape::session_tracks(
                &answer,
                detail,
                fields.as_deref(),
                args.get("under").and_then(Value::as_str),
            )
        }
        "LISTEN" => shape::listen(&answer, detail),
        "LISTEN_AB" => shape::listen_ab(&answer, detail),
        "MIX_MEASURE" => {
            let bands = args
                .get("with_bands")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || args.get("bands").is_some_and(Value::is_string);
            shape::measure(&answer, detail, bands)
        }
        "FX_PARAMS" if detail != Detail::Full => shape::fx_params(
            &answer,
            args.get("choices")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ),
        "SIGNAL_CHAIN" => signal_chain(&answer, args, detail)?,
        "SIGNAL_BLOCK_SET" if detail != Detail::Full => {
            let mut a = answer.clone();
            if let Some(b) = answer.get("block").filter(|b| b.is_object())
                && let Some(o) = a.as_object_mut()
            {
                o.insert("block".into(), shape::block(b, Detail::Summary));
            }
            a
        }
        _ => answer,
    })
}

fn signal_chain(answer: &Value, args: &Value, detail: Detail) -> Result<Value, Failure> {
    let chain = answer.get("chain").unwrap_or(answer);
    let track = answer.get("track").cloned().unwrap_or(Value::Null);
    let Some(id) = args.get("block").and_then(Value::as_str) else {
        return Ok(json!({ "track": track, "chain": shape::chain(chain, detail) }));
    };
    let Some(b) = shape::find_block(chain, id) else {
        return Err(bad(format!(
            "no block `{id}` on {track}; its blocks: {}",
            shape::block_ids(chain).join(", ")
        )));
    };
    Ok(json!({ "track": track, "block": shape::block(b, detail) }))
}

// ── composites ──────────────────────────────────────────────────────────

fn need<'a>(args: &'a Value, key: &str) -> Result<&'a str, Failure> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad(format!("`{key}` is missing")))
}

fn span_props() -> Value {
    json!({
        "from": {"type": "string", "description": "Bars (`33`, `33:2`), seconds (`12.5s`) or a section (`CH 1`)."},
        "to": {"type": "string", "description": "As `from` (`CH 1 end`)."},
    })
}

fn merged(mut a: Value, b: &Value) -> Value {
    if let (Some(x), Some(y)) = (a.as_object_mut(), b.as_object()) {
        for (k, v) in y {
            x.insert(k.clone(), v.clone());
        }
    }
    a
}

fn block_measure_schema() -> Value {
    json!({"type": "object", "properties": merged(json!({
        "track": {"type": "string", "description": T},
        "block": {"type": "string", "description": "The block's chain id (`tone.comp`), as signal_block lists them."},
        "solo": {"type": "array", "items": {"type": "string"}},
        "exact": {"type": "boolean", "description": "Render in one piece (differences under ~0.3 dB call for it)."},
        "bands": {"type": "boolean", "description": "Add the band-by-band difference (the EQ curve it applied)."},
    }), &span_props()), "required": ["track", "block", "from", "to"]})
}

fn block_measure(ctx: &Ctx<'_>, args: &Value) -> Result<Value, Failure> {
    let track = need(args, "track")?;
    let block = need(args, "block")?;
    let mut base = Map::new();
    base.insert("track".into(), json!(track));
    for key in ["from", "to", "solo", "exact"] {
        if let Some(v) = args.get(key) {
            base.insert(key.into(), v.clone());
        }
    }
    let at = |side: &str| {
        let mut a = base.clone();
        a.insert("at".into(), json!(format!("block:{block}:{side}")));
        Value::Object(a)
    };
    ctx.job.step(format!("1/2 LISTEN block:{block}:pre"));
    let pre = upstream::run(ctx.up, "LISTEN", &at("pre"), &ctx.job.cancel)?;
    ctx.job.step(format!("2/2 LISTEN block:{block}:post"));
    let post = upstream::run(ctx.up, "LISTEN", &at("post"), &ctx.job.cancel)?;
    let bands = args.get("bands").and_then(Value::as_bool).unwrap_or(false);
    let mut out = shape::block_diff(block, &pre, &post, bands);
    if let Some(o) = out.as_object_mut() {
        o.insert(
            "track".into(),
            pre.get("track").cloned().unwrap_or_else(|| json!(track)),
        );
        o.insert(
            "from".into(),
            pre.get("from").cloned().unwrap_or(Value::Null),
        );
        o.insert("to".into(), pre.get("to").cloned().unwrap_or(Value::Null));
    }
    Ok(out)
}

fn balance_schema() -> Value {
    json!({"type": "object", "properties": merged(json!({
        "reference": {"type": "string", "description": "The track the others are read against (`Drums/Kick`, `Kick`)."},
        "tracks": {"type": "array", "items": {"type": "string"}, "description": "What to read (default: every folder, or every folder `under`)."},
        "under": {"type": "string", "description": "Only the folders in this one (`Drums`)."},
        "depth": {"type": "integer", "minimum": 1, "description": "Folders at most this many levels below `under` (or the top): 1 is `Drums/Kick`, `Drums/Snare`… but not `Drums/Kick/Sum`."},
        "post_fader": {"type": "boolean", "description": "false: after the FX, before the fader."},
    }), &span_props()), "required": ["reference", "from", "to"]})
}

fn balance(ctx: &Ctx<'_>, args: &Value) -> Result<Value, Failure> {
    let reference = need(args, "reference")?;
    let mut tracks: Vec<String> = args
        .get("tracks")
        .and_then(Value::as_array)
        .map(|t| {
            t.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if tracks.is_empty() {
        ctx.job.step("1/2 SESSION_TRACKS");
        let all = upstream::run(ctx.up, "SESSION_TRACKS", &json!({}), &ctx.job.cancel)?;
        let under = args.get("under").and_then(Value::as_str);
        let depth = args
            .get("depth")
            .and_then(Value::as_u64)
            .and_then(|d| usize::try_from(d).ok());
        tracks = all
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| t.get("folder").and_then(Value::as_bool) == Some(true))
            .filter_map(|t| t.get("path").and_then(Value::as_str))
            .filter(|p| {
                under.is_none_or(|u| {
                    shape::path_under(p, u) && p.trim_end_matches('/') != u.trim_end_matches('/')
                })
            })
            .filter(|p| depth.is_none_or(|d| shape::depth_below(p, under) <= d))
            .map(str::to_owned)
            .collect();
    }
    if !tracks
        .iter()
        .any(|t| t == reference || t.ends_with(&format!("/{reference}")))
    {
        tracks.push(reference.to_owned());
    }
    let mut m = json!({ "tracks": tracks });
    if let Some(o) = m.as_object_mut() {
        for key in ["from", "to", "post_fader"] {
            if let Some(v) = args.get(key) {
                o.insert(key.into(), v.clone());
            }
        }
    }
    ctx.job.step("2/2 MIX_MEASURE");
    let measured = upstream::run(ctx.up, "MIX_MEASURE", &m, &ctx.job.cancel)?;
    shape::balance(&measured, reference).map_err(|e| failure("failed", e))
}

fn actions_schema() -> Value {
    json!({"type": "object", "properties": {
        "filter": {"type": "string"}, "category": {"type": "string"},
        "effect": {"type": "string", "enum": ["read-only", "mutating", "destructive"]},
    }})
}

fn actions(ctx: &Ctx<'_>, args: &Value) -> Result<Value, Failure> {
    Ok(shape::actions(&ctx.method("actions", args)?))
}

fn describe_schema() -> Value {
    json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]})
}

fn describe(ctx: &Ctx<'_>, args: &Value) -> Result<Value, Failure> {
    ctx.method("describe", &json!({ "id": need(args, "id")? }))
}

fn run_schema() -> Value {
    json!({"type": "object", "properties": {
        "id": {"type": "string", "description": "The action's id (`MIX_STATE`)."},
        "args": {"type": "object", "description": "Its arguments, as `describe` says."},
        "ticket": {"type": "string", "description": "A person's approval, retrying a call that asked for one."},
        "detail": {"type": "string", "enum": ["summary", "params", "full"]},
    }, "required": ["id"]})
}

fn run(ctx: &Ctx<'_>, args: &Value) -> Result<Value, Failure> {
    let id = need(args, "id")?;
    let mut inner = args.get("args").cloned().unwrap_or_else(|| json!({}));
    if let Some(d) = args.get("detail")
        && let Some(o) = inner.as_object_mut()
    {
        o.insert("detail".into(), d.clone());
    }
    if let Some(ticket) = args.get("ticket").and_then(Value::as_str) {
        ctx.job.step(id);
        let a = strip(&inner, id);
        return ctx.up.call(
            "run",
            &json!({"id": id, "args": a, "ticket": ticket}),
            &ctx.job.cancel,
        );
    }
    run_action(ctx, id, &inner)
}

fn batch_schema() -> Value {
    json!({"type": "object", "properties": {
        "steps": {"type": "array", "maxItems": 64, "items": {"type": "object", "properties": {"id": {"type": "string"}, "args": {"type": "object"}}, "required": ["id"]}},
        "stop_on_error": {"type": "boolean"},
    }, "required": ["steps"]})
}

fn batch(ctx: &Ctx<'_>, args: &Value) -> Result<Value, Failure> {
    ctx.method("batch", args)
}

fn watch_schema() -> Value {
    json!({"type": "object", "properties": {
        "since": {"type": "integer", "minimum": 0}, "max": {"type": "integer", "minimum": 1},
        "wait_ms": {"type": "integer", "minimum": 0, "maximum": 30000},
    }})
}

fn watch(ctx: &Ctx<'_>, args: &Value) -> Result<Value, Failure> {
    ctx.method("watch", args)
}

/// What Studio is, for the model on connecting.
pub const INSTRUCTIONS: &str = "Studio is the DAW the person mixes in, with the song they have open. \
These tools are its Action List, made for an agent: the curated ones (session_tree, mix_state, \
signal_block, listen, block_measure, listen_ab, mix_measure, balance, track_set, fx_set, \
signal_block_set, mix_snapshot/restore/diff, drums_*, hits_*) cover a mixing session; `actions`, \
`describe` and `run` reach every other action (~580: MIX_VARIANT*, SENDS_*, QUANTIZE, ZONES_*, …). \
\n\nAddressing: a track is its path (`Drums/Kick/Sum/In`), any tail of it (`Kick/Sum/In`), a name or \
a GUID — session_tree lists the paths. Spans (`from`/`to`) are bars (`33`, `33:2`), seconds \
(`12.5s`) or section names (`CH 1`, `CH 1 end`; sections lists them). A Signal block is track + \
block id (`tone.comp`). The kit follows the golden template: each piece (Kick, Snare, Tom 1…) has \
its close mics, a `Trig` (keys its gate, never heard), a `Sample`, and a `Fund`/`Sub` band-pass. \
\n\nAnswers are compact by default (`detail: \"full\"` for everything); one too big pages — call \
`more` with the `result` and `next` it gives. A call still running after its `wait` (20 s unless \
given) answers `{job, state: \"running\"}`: call `job {job}` to wait for its result (renders of a \
whole song take minutes). Mix by measurement: listen/block_measure for one point, \
mix_measure/balance across the mix, listen_ab to try a change level-matched and put it back, \
mix_snapshot before a series of changes. Changes are undo steps in the person's session: say what \
you change.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_are_unique_and_short() {
        let mut names: Vec<&str> = all().iter().map(|t| t.name).collect();
        names.extend(bridge_tools().iter().map(|t| t.0));
        let n = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), n);
        assert!(
            names
                .iter()
                .all(|n| n.len() <= 32 && n.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        );
    }

    #[test]
    fn descriptions_stay_small_enough_for_a_tool_list() {
        let total: usize = all().iter().map(|t| t.description.len()).sum();
        assert!(total < 14_000, "{total}");
    }

    #[test]
    fn bridge_arguments_never_reach_studio() {
        let a = strip(
            &json!({"track": "Kick", "detail": "full", "under": "Drums", "block": "x"}),
            "SIGNAL_CHAIN",
        );
        assert_eq!(a, json!({"track": "Kick"}));
    }
}
