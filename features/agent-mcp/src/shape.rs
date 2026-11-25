//! Compact views of Studio's answers: what an agent reads by default.
//!
//! Each action answers everything it knows (`MIX_STATE` of a folder lists
//! every parameter of every Signal block, each with its choices). These
//! keep what a mixing decision is made from — paths, faders, FX names,
//! the blocks and the parameters that differ from their defaults — and
//! round the numbers. `detail: "full"` (or `view: "full"`) is the
//! answer as Studio gave it, paged.

use std::fmt::Write as _;

use serde_json::{Map, Value, json};

/// How much of an answer to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    /// What a decision is made from (the default).
    Summary,
    /// The summary and every parameter, as text, without choices.
    Params,
    /// The answer as Studio gave it.
    Full,
}

impl Detail {
    /// `detail` (or `view`) of a tool's arguments.
    ///
    /// # Errors
    ///
    /// A value it is not, with the ones it may be.
    pub fn of(args: &Value, default: Self) -> Result<Self, String> {
        let given = args
            .get("detail")
            .or_else(|| args.get("view"))
            .and_then(Value::as_str);
        match given {
            None => Ok(default),
            Some("summary" | "tree") => Ok(Self::Summary),
            Some("params" | "list") => Ok(Self::Params),
            Some("full" | "raw") => Ok(Self::Full),
            Some(other) => Err(format!(
                "detail `{other}` is not one: `summary` (default), `params`, or `full`"
            )),
        }
    }
}

/// Every number rounded to `places` decimals (integers kept).
#[must_use]
pub fn round(value: &Value, places: i32) -> Value {
    match value {
        Value::Number(n) if n.is_f64() => n.as_f64().map_or(Value::Null, |f| {
            let k = 10f64.powi(places);
            json!((f * k).round() / k)
        }),
        Value::Array(a) => Value::Array(a.iter().map(|v| round(v, places)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), round(v, places)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn num(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn truthy(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// A dB figure as text: `-3.2 dB`.
fn db(x: f64) -> String {
    format!("{:.1} dB", (x * 10.0).round() / 10.0)
}

// ── Signal ──────────────────────────────────────────────────────────────

/// Whether a block parameter sits away from its default.
fn changed(p: &Value) -> bool {
    match (num(p, "value"), num(p, "default")) {
        (Some(v), Some(d)) => (v - d).abs() > 1e-4 * d.abs().max(1.0),
        _ => false,
    }
}

/// One block parameter as a line: `threshold = -40.0 dB (default -30)`.
fn param_line(p: &Value) -> String {
    let mut s = format!("{} = {}", text(p, "id"), text(p, "text"));
    if changed(p)
        && let Some(d) = num(p, "default")
    {
        let _ = write!(s, " (default {})", trim_num(d));
    }
    s
}

fn trim_num(x: f64) -> String {
    let r = (x * 100.0).round() / 100.0;
    if (r - r.round()).abs() < f64::EPSILON {
        format!("{r:.0}")
    } else {
        format!("{r}")
    }
}

/// One Signal block: its id, kind, bypass, key, and its parameters —
/// those off their defaults (`set`) in a summary, all of them with
/// [`Detail::Params`], everything with [`Detail::Full`].
#[must_use]
pub fn block(b: &Value, detail: Detail) -> Value {
    if detail == Detail::Full {
        return b.clone();
    }
    let params = b.get("params").and_then(Value::as_array);
    let mut out = Map::new();
    out.insert("id".into(), json!(text(b, "id")));
    out.insert("kind".into(), json!(text(b, "kind")));
    if truthy(b, "bypass") {
        out.insert("bypass".into(), json!(true));
    }
    if let Some(from) = b
        .get("key")
        .and_then(|k| k.get("from"))
        .and_then(Value::as_str)
    {
        out.insert("key".into(), json!(from));
    }
    let status = text(b, "status");
    if !status.is_empty() && status != "ok" {
        out.insert("status".into(), json!(status));
    }
    match detail {
        Detail::Summary => {
            let set: Map<String, Value> = params
                .into_iter()
                .flatten()
                .filter(|p| changed(p))
                .map(|p| (text(p, "id").to_owned(), json!(text(p, "text"))))
                .collect();
            out.insert("set".into(), Value::Object(set));
            out.insert("params".into(), json!(params.map_or(0, Vec::len)));
        }
        Detail::Params | Detail::Full => {
            let all: Vec<Value> = params
                .into_iter()
                .flatten()
                .map(|p| json!(param_line(p)))
                .collect();
            out.insert("params".into(), Value::Array(all));
        }
    }
    Value::Object(out)
}

/// A Signal chain (`signal.chain/1`): its phases and their blocks.
#[must_use]
pub fn chain(c: &Value, detail: Detail) -> Value {
    if detail == Detail::Full {
        return c.clone();
    }
    let phases: Vec<Value> = c
        .get("phases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|ph| {
            let mut out = Map::new();
            out.insert("phase".into(), json!(text(ph, "id")));
            if let Some(m) = num(ph, "makeup_db") {
                out.insert("makeup_db".into(), round(&json!(m), 2));
            }
            if truthy(ph, "bypass") {
                out.insert("bypass".into(), json!(true));
            }
            let blocks: Vec<Value> = ph
                .get("blocks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|b| block(b, detail))
                .collect();
            out.insert("blocks".into(), Value::Array(blocks));
            Value::Object(out)
        })
        .collect();
    let mut out = Map::new();
    let mode = text(c, "mode");
    if !mode.is_empty() {
        out.insert("mode".into(), json!(mode));
    }
    out.insert("phases".into(), Value::Array(phases));
    Value::Object(out)
}

/// The block `id` of a chain: by its chain id (`tone.eq`) or, when only
/// one block has it, its own id after the phase (`eq`).
#[must_use]
pub fn find_block<'a>(c: &'a Value, id: &str) -> Option<&'a Value> {
    let blocks: Vec<&Value> = c
        .get("phases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|ph| {
            ph.get("blocks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .collect();
    if let Some(b) = blocks.iter().find(|b| text(b, "id") == id) {
        return Some(b);
    }
    let tail: Vec<&&Value> = blocks
        .iter()
        .filter(|b| text(b, "id").rsplit('.').next() == Some(id) || text(b, "kind") == id)
        .collect();
    match tail.as_slice() {
        [one] => Some(one),
        _ => None,
    }
}

/// Every block id of a chain, for an error that says which there are.
#[must_use]
pub fn block_ids(c: &Value) -> Vec<String> {
    c.get("phases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|ph| {
            ph.get("blocks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .map(|b| text(b, "id").to_owned())
        .collect()
}

// ── MIX_STATE ───────────────────────────────────────────────────────────

fn send_line(s: &Value, arrow: &str) -> String {
    let mut line = format!(
        "{arrow} {} {} {}",
        text(s, "track"),
        num(s, "db").map(db).unwrap_or_default(),
        text(s, "tap")
    );
    if let Some(p) = num(s, "pan")
        && p.abs() > 1e-3
    {
        let _ = write!(line, " pan {}", trim_num(p));
    }
    if truthy(s, "mute") {
        line.push_str(" (muted)");
    }
    line
}

fn fx_line(f: &Value) -> String {
    let mut s = format!("{} {}", text(f, "place"), text(f, "name"));
    if !truthy(f, "on") {
        s.push_str(" (off)");
    }
    if let Some(l) = f.get("latency").and_then(Value::as_u64)
        && l > 0
    {
        let _ = write!(s, " [{l} smp]");
    }
    s
}

/// One track of `MIX_STATE`.
fn mix_track(t: &Value, detail: Detail) -> Value {
    let mut out = Map::new();
    out.insert("path".into(), json!(text(t, "path")));
    out.insert("kind".into(), json!(text(t, "kind")));
    if let Some(l) = t.get("level") {
        out.insert("level".into(), l.clone());
    }
    if let Some(v) = num(t, "volume_db") {
        out.insert("volume_db".into(), round(&json!(v), 2));
    }
    if let Some(p) = num(t, "pan") {
        out.insert("pan".into(), round(&json!(p), 2));
    }
    out.insert("mute".into(), json!(truthy(t, "mute")));
    out.insert("solo".into(), json!(truthy(t, "solo")));
    if truthy(t, "phase") {
        out.insert("phase".into(), json!(true));
    }
    if t.get("to_parent").and_then(Value::as_bool) == Some(false) {
        out.insert("to_parent".into(), json!(false));
    }
    if let Some(l) = t.get("latency").and_then(Value::as_u64)
        && l > 0
    {
        out.insert("latency".into(), json!(l));
    }
    let lines = |key: &str, arrow: &str| -> Vec<Value> {
        t.get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|s| json!(send_line(s, arrow)))
            .collect()
    };
    let sends = lines("sends", "→");
    if !sends.is_empty() {
        out.insert("sends".into(), Value::Array(sends));
    }
    let receives = lines("receives", "←");
    if !receives.is_empty() {
        out.insert("receives".into(), Value::Array(receives));
    }
    let mut fx = Vec::new();
    let mut signal = Vec::new();
    for f in t.get("fx").and_then(Value::as_array).into_iter().flatten() {
        if detail == Detail::Params
            && let Some(params) = f
                .get("params")
                .filter(|p| p.as_array().is_some_and(|a| !a.is_empty()))
        {
            fx.push(json!({ "fx": fx_line(f), "params": params }));
        } else {
            fx.push(json!(fx_line(f)));
        }
        if let Some(c) = f.get("signal").filter(|c| c.is_object()) {
            let mut sc = chain(c, detail);
            if let Some(o) = sc.as_object_mut() {
                o.insert("fx".into(), json!(text(f, "place")));
            }
            signal.push(sc);
        }
    }
    out.insert("fx".into(), Value::Array(fx));
    match signal.len() {
        0 => {}
        1 => {
            out.insert("signal".into(), signal.swap_remove(0));
        }
        _ => {
            out.insert("signal".into(), Value::Array(signal));
        }
    }
    let automation: Vec<Value> = t
        .get("automation")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|a| {
            let points = a
                .get("points")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            json!(format!(
                "{} ({}, {points} points)",
                text(a, "lane"),
                text(a, "mode")
            ))
        })
        .collect();
    if !automation.is_empty() {
        out.insert("automation".into(), Value::Array(automation));
    }
    Value::Object(out)
}

/// `MIX_STATE`, compact.
///
/// Each track's path, fader, pan, mute/solo, sends, FX by name and its
/// Signal chain with only the parameters off their defaults
/// ([`Detail::Summary`]), or every parameter as a line ([`Detail::Params`]).
#[must_use]
pub fn mix_state(v: &Value, detail: Detail) -> Value {
    if detail == Detail::Full {
        return v.clone();
    }
    let tracks: Vec<Value> = v
        .get("tracks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|t| mix_track(t, detail))
        .collect();
    let mut out = Map::new();
    for key in ["track", "from", "to"] {
        if let Some(x) = v.get(key).filter(|x| !x.is_null()) {
            out.insert(key.into(), x.clone());
        }
    }
    out.insert("tracks".into(), Value::Array(tracks));
    Value::Object(out)
}

// ── SESSION_TRACKS ──────────────────────────────────────────────────────

/// One track as a line: `Drums/Kick/Sum/In  -3.2 dB  pan 0.25  muted  fx: Signal Dev`.
fn track_line(t: &Value) -> String {
    let folder = truthy(t, "folder");
    let mut s = text(t, "path").to_owned();
    if folder {
        s.push('/');
    }
    if let Some(v) = num(t, "volume_db") {
        s.push_str("  ");
        s.push_str(&db(v));
    }
    if let Some(p) = num(t, "pan")
        && p.abs() > 1e-3
    {
        let _ = write!(s, "  pan {}", trim_num(p));
    }
    for (key, word) in [
        ("muted", "muted"),
        ("soloed", "solo"),
        ("inverted", "ø"),
        ("armed", "armed"),
    ] {
        if truthy(t, key) {
            s.push_str("  ");
            s.push_str(word);
        }
    }
    let fx: Vec<&str> = t
        .get("fx")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !fx.is_empty() {
        s.push_str("  fx: ");
        s.push_str(&fx.join(", "));
    }
    if !folder && let Some(n) = t.get("items").and_then(Value::as_u64) {
        let _ = write!(s, "  items {n}");
    }
    s
}

/// The fields `view: "list"` keeps unless `fields` names others.
pub const LIST_FIELDS: &[&str] = &[
    "path",
    "folder",
    "volume_db",
    "pan",
    "muted",
    "soloed",
    "fx",
];

/// `SESSION_TRACKS` as a tree of lines ([`Detail::Summary`]), a list of
/// objects with `fields` ([`Detail::Params`]), or as it is.
#[must_use]
pub fn session_tracks(
    v: &Value,
    detail: Detail,
    fields: Option<&[String]>,
    under: Option<&str>,
) -> Value {
    let tracks: Vec<&Value> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| under.is_none_or(|u| path_under(text(t, "path"), u)))
        .collect();
    match detail {
        Detail::Full => Value::Array(tracks.into_iter().cloned().collect()),
        Detail::Summary => json!({
            "tracks": tracks.len(),
            "tree": tracks.iter().map(|t| json!(track_line(t))).collect::<Vec<_>>(),
        }),
        Detail::Params => {
            let keep: Vec<&str> = fields.map_or_else(
                || LIST_FIELDS.to_vec(),
                |f| f.iter().map(String::as_str).collect(),
            );
            Value::Array(
                tracks
                    .iter()
                    .map(|t| {
                        let o: Map<String, Value> = keep
                            .iter()
                            .filter_map(|k| t.get(*k).map(|x| ((*k).to_owned(), round(x, 2))))
                            .collect();
                        Value::Object(o)
                    })
                    .collect(),
            )
        }
    }
}

/// How many levels `path` is below `under` (below the top with none):
/// `Drums/Kick` is 1 below `Drums`, `Drums` is 1 below the top.
#[must_use]
pub fn depth_below(path: &str, under: Option<&str>) -> usize {
    let n = |p: &str| {
        p.trim_end_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .count()
    };
    n(path).saturating_sub(under.map_or(0, n))
}

/// Whether `path` is `under` or inside it (`Drums` holds `Drums/Kick`).
#[must_use]
pub fn path_under(path: &str, under: &str) -> bool {
    let under = under.trim_end_matches('/');
    path == under
        || path
            .strip_prefix(under)
            .is_some_and(|rest| rest.starts_with('/'))
}

// ── LISTEN, LISTEN_AB, MIX_MEASURE ──────────────────────────────────────

/// LISTEN's figures without the per-second and per-hop series.
fn figures(f: &Value) -> Value {
    let mut f = f.clone();
    if let Some(o) = f.as_object_mut() {
        o.remove("short_term");
    }
    round(&f, 2)
}

fn dynamics(d: &Value) -> Value {
    let list: Vec<Value> = d
        .as_array()
        .into_iter()
        .flatten()
        .map(|x| {
            let mut x = x.clone();
            if let Some(o) = x.as_object_mut() {
                o.remove("gr");
            }
            round(&x, 2)
        })
        .collect();
    Value::Array(list)
}

fn rendered(r: &Value) -> Value {
    let mut r = r.clone();
    if let Some(o) = r.as_object_mut() {
        for key in ["tracks", "replayed"] {
            if let Some(n) = o.get(key).and_then(Value::as_array).map(Vec::len) {
                o.insert(key.into(), json!(n));
            }
        }
    }
    round(&r, 0)
}

/// LISTEN, compact: its figures, dynamics without the per-hop series,
/// and what it rendered by count.
#[must_use]
pub fn listen(v: &Value, detail: Detail) -> Value {
    if detail == Detail::Full {
        return v.clone();
    }
    let mut out = v.as_object().cloned().unwrap_or_default();
    if let Some(f) = v.get("figures") {
        out.insert("figures".into(), figures(f));
    }
    if let Some(d) = v.get("dynamics") {
        out.insert("dynamics".into(), dynamics(d));
    }
    if let Some(r) = v.get("rendered") {
        out.insert("rendered".into(), rendered(r));
    }
    {
        let key = "bands_hz";
        if let Some(x) = out.get(key).map(|x| round(x, 0)) {
            out.insert(key.into(), x);
        }
    }
    round(&Value::Object(out), 2)
}

/// `LISTEN_AB`, compact, with the difference level-matched.
///
/// `matched` takes the loudness change out (`gain_db`), so what is left
/// in `bands`, `true_peak` and `peak` is what the change did beyond
/// making it louder or quieter — the comparison a level-matched A/B hears.
#[must_use]
pub fn listen_ab(v: &Value, detail: Detail) -> Value {
    let mut out = if detail == Detail::Full {
        v.as_object().cloned().unwrap_or_default()
    } else {
        let mut o = v.as_object().cloned().unwrap_or_default();
        for key in ["a", "b"] {
            if let Some(f) = v.get(key) {
                o.insert(key.into(), figures(f));
            }
        }
        if let Some(d) = v.get("difference") {
            o.insert("difference".into(), figures(d));
        }
        o
    };
    if let Some(d) = v.get("difference") {
        out.insert("matched".into(), matched(d));
    }
    if detail == Detail::Full {
        Value::Object(out)
    } else {
        round(&Value::Object(out), 2)
    }
}

/// A b − a difference with its loudness change taken out.
#[must_use]
pub fn matched(d: &Value) -> Value {
    let gain = num(d, "lufs").unwrap_or(0.0);
    let less = |key: &str| num(d, key).map(|x| x - gain);
    let bands: Vec<Value> = d
        .get("bands")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|b| b.as_f64().map_or(Value::Null, |x| json!(x - gain)))
        .collect();
    round(
        &json!({
            "gain_db": gain,
            "true_peak": less("true_peak"),
            "peak": less("peak"),
            "crest": num(d, "crest"),
            "width": num(d, "width"),
            "bands": bands,
            "note": "b − a with the loudness change (gain_db) taken out: what remains is tone and dynamics. Give b −gain_db to hear it level-matched.",
        }),
        2,
    )
}

/// The figures a block's pre/post comparison reads.
const KEY_FIGURES: &[&str] = &[
    "lufs",
    "short_max",
    "true_peak",
    "peak",
    "rms",
    "crest",
    "width",
];

/// What one block did: its input and output figures and the difference
/// (`gain_db` its loudness change, `crest_db` how much it squeezed or
/// opened the transients), with the instance's dynamics after it.
#[must_use]
pub fn block_diff(block: &str, pre: &Value, post: &Value, with_bands: bool) -> Value {
    let pick = |v: &Value| -> Value {
        let f = v.get("figures").cloned().unwrap_or(Value::Null);
        let o: Map<String, Value> = KEY_FIGURES
            .iter()
            .filter_map(|k| f.get(*k).map(|x| ((*k).to_owned(), x.clone())))
            .collect();
        Value::Object(o)
    };
    let fp = pre.get("figures").cloned().unwrap_or(Value::Null);
    let fq = post.get("figures").cloned().unwrap_or(Value::Null);
    let diff: Map<String, Value> = KEY_FIGURES
        .iter()
        .filter_map(|k| Some(((*k).to_owned(), json!(num(&fq, k)? - num(&fp, k)?))))
        .collect();
    let mut out = json!({
        "block": block,
        "pre": pick(pre),
        "post": pick(post),
        "difference": diff,
        "gain_db": num(&fq, "lufs").zip(num(&fp, "lufs")).map(|(b, a)| b - a),
        "crest_db": num(&fq, "crest").zip(num(&fp, "crest")).map(|(b, a)| b - a),
        "dynamics": post.get("dynamics").map(dynamics),
    });
    if with_bands && let Some(o) = out.as_object_mut() {
        let a = fp.get("bands").and_then(Value::as_array);
        let b = fq.get("bands").and_then(Value::as_array);
        if let (Some(a), Some(b)) = (a, b) {
            let d: Vec<Value> = a
                .iter()
                .zip(b)
                .map(|(x, y)| json!(y.as_f64().unwrap_or(0.0) - x.as_f64().unwrap_or(0.0)))
                .collect();
            o.insert("bands".into(), Value::Array(d));
            o.insert(
                "bands_hz".into(),
                round(post.get("bands_hz").unwrap_or(&Value::Null), 0),
            );
        }
    }
    round(&out, 2)
}

/// One `MIX_MEASURE` track, its `numbers` (and `raw`) flattened in.
fn measured(t: &Value, bands: bool) -> Value {
    let mut out = Map::new();
    for key in ["path", "kind", "parent"] {
        if let Some(x) = t.get(key).filter(|x| !x.is_null()) {
            out.insert(key.into(), x.clone());
        }
    }
    let put_numbers = |out: &mut Map<String, Value>, n: &Value, prefix: &str| {
        for (k, x) in n.as_object().into_iter().flatten() {
            if k == "bands" && !bands {
                continue;
            }
            out.insert(format!("{prefix}{k}"), x.clone());
        }
    };
    if let Some(n) = t.get("numbers") {
        put_numbers(&mut out, n, "");
    }
    for (k, x) in t.as_object().into_iter().flatten() {
        if matches!(k.as_str(), "path" | "kind" | "parent" | "numbers" | "raw") {
            continue;
        }
        if k == "latency" && x.as_u64() == Some(0) {
            continue;
        }
        if x.is_object() {
            continue;
        }
        out.insert(k.clone(), x.clone());
    }
    if let Some(raw) = t
        .get("raw")
        .and_then(|r| r.get("numbers").or(Some(r)))
        .filter(|r| r.is_object())
    {
        put_numbers(&mut out, raw, "raw_");
    }
    out.retain(|_, x| !x.is_null());
    round(&Value::Object(out), 2)
}

/// `MIX_MEASURE`, compact: one flat row a track, bands only when asked.
#[must_use]
pub fn measure(v: &Value, detail: Detail, bands: bool) -> Value {
    if detail == Detail::Full {
        return v.clone();
    }
    let mut out = v.as_object().cloned().unwrap_or_default();
    let tracks: Vec<Value> = v
        .get("tracks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|t| measured(t, bands))
        .collect();
    out.insert("tracks".into(), Value::Array(tracks));
    if !bands {
        out.remove("bands_hz");
    }
    round(&Value::Object(out), 2)
}

/// A balance read off `MIX_MEASURE`: each track's loudness against
/// `reference`'s (`rel_db`), loudest first.
///
/// # Errors
///
/// The reference was not among what was measured.
pub fn balance(v: &Value, reference: &str) -> Result<Value, String> {
    let rows: Vec<Value> = v
        .get("tracks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|t| measured(t, false))
        .collect();
    let is_ref = |r: &Value| {
        let p = text(r, "path");
        p == reference || p.ends_with(&format!("/{reference}"))
    };
    let reference_row = rows.iter().find(|r| is_ref(r)).ok_or_else(|| {
        format!(
            "the reference `{reference}` was not measured; measured: {}",
            rows.iter()
                .map(|r| text(r, "path"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    let ref_lufs = num(reference_row, "lufs");
    let mut out: Vec<Value> = rows
        .iter()
        .map(|r| {
            let lufs = num(r, "lufs");
            json!({
                "path": text(r, "path"),
                "kind": text(r, "kind"),
                "lufs": lufs,
                "rel_db": lufs.zip(ref_lufs).map(|(a, b)| a - b),
                "short_max": num(r, "short_max"),
                "true_peak": num(r, "true_peak"),
                "share_db": num(r, "share_db"),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        let x = num(a, "lufs").unwrap_or(f64::NEG_INFINITY);
        let y = num(b, "lufs").unwrap_or(f64::NEG_INFINITY);
        y.total_cmp(&x)
    });
    Ok(round(
        &json!({
            "reference": text(reference_row, "path"),
            "reference_lufs": ref_lufs,
            "from": v.get("from"),
            "to": v.get("to"),
            "rows": out,
            "ms": v.get("ms"),
        }),
        2,
    ))
}

// ── actions, FX_PARAMS ──────────────────────────────────────────────────

/// The Action List as lines: `MIX_STATE — Everything about a track`,
/// with `[mutating]` / `[destructive]` where it is not read-only.
#[must_use]
pub fn actions(v: &Value) -> Value {
    let list = v
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let lines: Vec<Value> = list
        .iter()
        .map(|a| {
            let mut s = format!("{} — {}", text(a, "id"), text(a, "name"));
            match text(a, "effect") {
                "read-only" | "" => {}
                e => {
                    let _ = write!(s, " [{e}]");
                }
            }
            if a.get("enabled").and_then(Value::as_bool) == Some(false) {
                let _ = write!(s, " (disabled: {})", text(a, "disabled_reason"));
            }
            json!(s)
        })
        .collect();
    json!({
        "count": lines.len(),
        "actions": lines,
        "next": "describe {id} says what one takes and does; run {id, args} runs it",
    })
}

/// `FX_PARAMS` as lines: `#3 Threshold = -18.0 dB (0.62)`, choices only
/// when asked.
#[must_use]
pub fn fx_params(v: &Value, choices: bool) -> Value {
    Value::Array(
        v.as_array()
            .into_iter()
            .flatten()
            .map(|p| {
                let line = format!(
                    "#{} {} = {} ({})",
                    p.get("index").and_then(Value::as_u64).unwrap_or(0),
                    text(p, "name"),
                    text(p, "value"),
                    num(p, "normalized")
                        .map(|n| format!("{n:.3}"))
                        .unwrap_or_default()
                );
                let cs = p
                    .get("choices")
                    .and_then(Value::as_array)
                    .filter(|c| !c.is_empty());
                match cs {
                    Some(cs) if choices => json!({ "param": line, "choices": cs }),
                    _ => json!(line),
                }
            })
            .collect(),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn eq_block() -> Value {
        let mut params: Vec<Value> = (0..170)
            .map(|i| {
                json!({"id": format!("band.{i}.gain"), "name": format!("Gain {i}"), "default": 0.0, "value": 0.0,
                       "text": "0.0 dB", "choices": ["Bell", "Shelf", "Cut", "Notch"], "min": -18.0, "max": 18.0})
            })
            .collect();
        params[7]["value"] = json!(-6.0);
        params[7]["text"] = json!("-6.0 dB");
        json!({"id": "tone.eq", "kind": "eq", "bypass": false, "status": "ok", "params": params,
               "key": null, "meters": [], "backend": "linked"})
    }

    fn state() -> Value {
        json!({"track": "Drums", "from": null, "to": null, "ms": 3.0, "tracks": [
            {"path": "Drums", "level": 0, "kind": "folder", "volume_db": -9.240_000_1, "pan": 0.0,
             "mute": false, "solo": false, "phase": false, "to_parent": false, "latency": 3840,
             "meter": {"peak": [-150.0, -150.0]}, "receives": [], "automation": [],
             "sends": [{"track": "MIX BUS/DRUM BUS", "db": 0.0, "pan": 0.0, "mute": false, "tap": "post-fader"}],
             "fx": [
                {"place": "#1", "name": "FTS EQ", "on": false, "latency": 0, "parameters": 2258},
                {"place": "#2", "name": "Signal Dev", "on": true, "latency": 0, "parameters": 268,
                 "params": ["#0 Macro 1 = 0.0 (0.0)"],
                 "signal": {"mode": "per-track", "schema": "signal.chain/1", "phases": [
                    {"id": "tone", "makeup_db": 1.879_999_6, "bypass": false, "blocks": [eq_block()]}]}}
             ]}
        ]})
    }

    #[test]
    fn a_mix_state_summary_keeps_only_what_differs() {
        let full = state();
        let s = mix_state(&full, Detail::Summary);
        let t = &s["tracks"][0];
        assert_eq!(t["volume_db"], json!(-9.24));
        assert_eq!(t["fx"], json!(["#1 FTS EQ (off)", "#2 Signal Dev"]));
        assert_eq!(t["sends"], json!(["→ MIX BUS/DRUM BUS 0.0 dB post-fader"]));
        let b = &t["signal"]["phases"][0]["blocks"][0];
        assert_eq!(b["set"], json!({"band.7.gain": "-6.0 dB"}));
        assert_eq!(b["params"], json!(170));
        assert_eq!(t["signal"]["phases"][0]["makeup_db"], json!(1.88));
        let full_len = full.to_string().len();
        let small = s.to_string().len();
        assert!(small * 20 < full_len, "{small} vs {full_len}");
    }

    #[test]
    fn params_detail_lists_every_parameter_without_choices() {
        let s = mix_state(&state(), Detail::Params);
        let b = &s["tracks"][0]["signal"]["phases"][0]["blocks"][0];
        assert_eq!(b["params"].as_array().unwrap().len(), 170);
        assert_eq!(b["params"][7], json!("band.7.gain = -6.0 dB (default 0)"));
        assert!(!s.to_string().contains("Notch"));
        assert_eq!(
            s["tracks"][0]["fx"][1]["params"],
            json!(["#0 Macro 1 = 0.0 (0.0)"])
        );
    }

    #[test]
    fn full_detail_is_the_answer_itself() {
        assert_eq!(mix_state(&state(), Detail::Full), state());
    }

    #[test]
    fn blocks_are_found_by_chain_id_or_their_own() {
        let c = &state()["tracks"][0]["fx"][1]["signal"];
        assert!(find_block(c, "tone.eq").is_some());
        assert!(find_block(c, "eq").is_some());
        assert!(find_block(c, "gate").is_none());
        assert_eq!(block_ids(c), vec!["tone.eq".to_owned()]);
    }

    fn tracks() -> Value {
        json!([
            {"guid": "{A}", "name": "Drums", "path": "Drums", "folder": true, "muted": false, "soloed": false,
             "inverted": false, "volume_db": -9.24, "pan": 0.0, "items": 0, "fx": ["Signal Dev"], "lanes": [],
             "armed": false, "record_input": "none", "open": true},
            {"guid": "{B}", "name": "In", "path": "Drums/Kick/In", "folder": false, "muted": true, "soloed": false,
             "inverted": true, "volume_db": 0.0, "pan": 0.25, "items": 8, "fx": [], "lanes": [{"lane": 0, "name": "1", "items": 8, "plays": true}],
             "armed": false, "record_input": "none", "open": null},
            {"guid": "{C}", "name": "Bass", "path": "Bass", "folder": false, "muted": false, "soloed": false,
             "inverted": false, "volume_db": -3.0, "pan": 0.0, "items": 1, "fx": [], "lanes": [],
             "armed": false, "record_input": "none", "open": null}
        ])
    }

    #[test]
    fn the_session_is_a_tree_of_lines() {
        let s = session_tracks(&tracks(), Detail::Summary, None, None);
        assert_eq!(
            s["tree"],
            json!([
                "Drums/  -9.2 dB  fx: Signal Dev",
                "Drums/Kick/In  0.0 dB  pan 0.25  muted  ø  items 8",
                "Bass  -3.0 dB  items 1"
            ])
        );
        let under = session_tracks(&tracks(), Detail::Summary, None, Some("Drums"));
        assert_eq!(under["tracks"], json!(2));
    }

    #[test]
    fn a_list_keeps_the_fields_asked_for() {
        let fields = vec!["path".to_owned(), "volume_db".to_owned()];
        let s = session_tracks(&tracks(), Detail::Params, Some(&fields), None);
        assert_eq!(s[1], json!({"path": "Drums/Kick/In", "volume_db": 0.0}));
    }

    #[test]
    fn depth_counts_levels_below() {
        assert_eq!(depth_below("Drums/Kick", Some("Drums")), 1);
        assert_eq!(depth_below("Drums/Kick/Sum", Some("Drums/")), 2);
        assert_eq!(depth_below("Drums", None), 1);
    }

    #[test]
    fn a_path_is_under_its_folder_not_a_sibling_with_the_prefix() {
        assert!(path_under("Drums/Kick", "Drums"));
        assert!(path_under("Drums", "Drums/"));
        assert!(!path_under("Drumsy", "Drums"));
    }

    #[test]
    fn listen_drops_the_series() {
        let v = json!({"figures": {"lufs": -19.891, "short_term": [-21.2, -20.0], "bands": [-43.681]},
                       "dynamics": [{"fx": "#5 Signal Dev", "gr": [-1.2, -2.1], "max_gr_db": -84.554}],
                       "rendered": {"tracks": ["a", "b"], "replayed": [], "plan_ms": 5303.14}});
        let s = listen(&v, Detail::Summary);
        assert_eq!(s["figures"], json!({"lufs": -19.89, "bands": [-43.68]}));
        assert_eq!(
            s["dynamics"],
            json!([{"fx": "#5 Signal Dev", "max_gr_db": -84.55}])
        );
        assert_eq!(s["rendered"]["tracks"], json!(2));
    }

    #[test]
    fn an_ab_is_level_matched() {
        let v = json!({"a": {"lufs": -20.0}, "b": {"lufs": -17.0},
                       "difference": {"lufs": 3.0, "true_peak": 1.0, "bands": [3.0, 5.0, 1.0], "crest": -2.0}});
        let s = listen_ab(&v, Detail::Summary);
        assert_eq!(s["matched"]["gain_db"], json!(3.0));
        assert_eq!(s["matched"]["true_peak"], json!(-2.0));
        assert_eq!(s["matched"]["bands"], json!([0.0, 2.0, -2.0]));
    }

    #[test]
    fn a_blocks_pre_and_post_say_what_it_did() {
        let pre =
            json!({"figures": {"lufs": -20.0, "crest": 18.0, "peak": -2.0}, "bands_hz": [100.0]});
        let post = json!({"figures": {"lufs": -21.5, "crest": 14.0, "peak": -6.0},
                          "dynamics": [{"fx": "#5 Signal Dev", "gr": [1.0], "max_gr_db": -6.0}], "bands_hz": [100.0]});
        let d = block_diff("tone.comp", &pre, &post, false);
        assert_eq!(d["gain_db"], json!(-1.5));
        assert_eq!(d["crest_db"], json!(-4.0));
        assert_eq!(d["difference"]["peak"], json!(-4.0));
        assert_eq!(
            d["dynamics"],
            json!([{"fx": "#5 Signal Dev", "max_gr_db": -6.0}])
        );
    }

    fn measured_mix() -> Value {
        json!({"from": 73.5, "to": 85.5, "ms": 1200.0, "bands_hz": [31.5], "tracks": [
            {"path": "Drums/Kick", "kind": "folder", "parent": "Drums", "latency": 0,
             "numbers": {"lufs": -18.0, "short_max": -16.0, "true_peak": -1.0, "bands": [-30.0]}, "share_db": -3.0},
            {"path": "Drums/Snare", "kind": "folder", "parent": "Drums", "latency": 0,
             "numbers": {"lufs": -20.5, "short_max": -18.0, "true_peak": -2.0, "bands": [-40.0]}, "share_db": -5.0},
            {"path": "Bass", "kind": "track", "latency": 64,
             "numbers": {"lufs": -16.0, "short_max": -14.0, "true_peak": -3.0, "bands": [-20.0]}}
        ]})
    }

    #[test]
    fn a_measure_is_one_flat_row_a_track() {
        let m = measure(&measured_mix(), Detail::Summary, false);
        assert_eq!(
            m["tracks"][0],
            json!({"path": "Drums/Kick", "kind": "folder", "parent": "Drums", "lufs": -18.0,
                   "short_max": -16.0, "true_peak": -1.0, "share_db": -3.0})
        );
        assert_eq!(m["tracks"][2]["latency"], json!(64));
        assert!(m.get("bands_hz").is_none());
    }

    #[test]
    fn a_balance_is_against_the_reference() {
        let b = balance(&measured_mix(), "Kick").unwrap();
        assert_eq!(b["reference"], json!("Drums/Kick"));
        let rows = b["rows"].as_array().unwrap();
        assert_eq!(rows[0]["path"], json!("Bass"));
        assert_eq!(rows[0]["rel_db"], json!(2.0));
        assert_eq!(rows[2]["rel_db"], json!(-2.5));
        let missing = balance(&measured_mix(), "Vox").unwrap_err();
        assert!(missing.contains("Drums/Snare"), "{missing}");
    }

    #[test]
    fn actions_are_lines() {
        let v = json!({"actions": [
            {"id": "MIX_STATE", "name": "Everything about a track", "effect": "read-only", "enabled": true},
            {"id": "TRACK_SET", "name": "Set a track", "effect": "mutating", "enabled": true}]});
        assert_eq!(
            actions(&v)["actions"],
            json!([
                "MIX_STATE — Everything about a track",
                "TRACK_SET — Set a track [mutating]"
            ])
        );
    }

    #[test]
    fn detail_names_are_checked() {
        assert_eq!(Detail::of(&json!({}), Detail::Summary), Ok(Detail::Summary));
        assert_eq!(
            Detail::of(&json!({"view": "full"}), Detail::Summary),
            Ok(Detail::Full)
        );
        assert!(Detail::of(&json!({"detail": "most"}), Detail::Summary).is_err());
    }
}
