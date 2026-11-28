//! A track's FX chain, close up: every FX on it, in order, each switched
//! on or bypassed, moved up or down and taken off; an FX added from the
//! engine's own (the FTS processors, and Signal where the engine builds it)
//! or, on a desktop, any CLAP bundle by its name or path; and a generic
//! panel of any plugin's parameters — its name, value and range — for an FX
//! with no face of its own (studio#98).
//!
//! Opened by a strip's or a track row's FX button and by a rack close-up's
//! "FX chain" tab (`SESSION_CLOSEUP {chain}`), so it is the same panel on a
//! desktop, an iPad and in a browser. It reads the chain through the facade
//! ([`crate::engine::chain_of`], again every [`REREAD`] so a change made
//! elsewhere shows) and writes through the racks' own worker
//! ([`crate::tone::Shared`]), so its edits land in order with theirs:
//!
//! - **The chain** ([`Door`]): a desktop's and an iPad's window runs the
//!   engine's actions (`FX_ADD`, `FX_REMOVE`, `FX_MOVE`, `FX_ENABLE`), each
//!   one undo step; a page's Action List has none of them, so the page sends
//!   the window's own edits (`Edit::AddFx` …) inside an undo step of its own.
//! - **A parameter** is written as a rack's is ([`crate::tone::Shared::set_param`]):
//!   live as it moves, inside one undo step, and committed as `FX_SET` when
//!   the gesture ends — the path every FTS face takes.

use std::sync::{Arc, Mutex, PoisonError};

use daw_proto::fx::{FxParamRange, FxParameter};
use dioxus::prelude::*;
use fts_audio_ui::ParamHandle;
use fts_audio_ui::controls::HSlider;

use crate::closeup::{Closeup, Closeups};
use crate::engine::{ChainFx, Edit};
use crate::tone::Which;

/// How often what is shown is read from the engine again while open.
const REREAD: std::time::Duration = std::time::Duration::from_millis(500);

/// How long after the panel's own edit a read is held off: a read landing
/// under a slider being dragged would pull it back to where it was.
const OWN_EDIT: std::time::Duration = std::time::Duration::from_millis(600);

/// How often the panel sends what it made and looks for what has landed.
const TICK: std::time::Duration = std::time::Duration::from_millis(30);

/// The FTS processors a chain is offered when the engine does not say
/// what it has (an older engine, a read that failed).
const FTS: &[&str] = &[
    "FTS EQ",
    "FTS Comp",
    "FTS Saturate",
    "FTS Gate",
    "FTS Reverb",
    "FTS Delay",
    "FTS Modulation",
    "FTS Limiter",
];

/// Signal, built into a desktop's and an iPad's engine (nothing installed).
pub const SIGNAL: &str = "Signal Dev";

/// Signal's rigs, which every engine builds, a page's too (`signal_fx`):
/// the whole guitar or bass rig, the bass rig, the kit voice. Offered after
/// the processors, the whole rig first.
pub const SIGNAL_RIGS: &[&str] = &["FTS Signal Rig", "FTS Bass Rig", "FTS Kit Mic"];

// ── The chain ─────────────────────────────────────────────────────────

/// How a window changes the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Door {
    /// Through the engine's actions, each its own undo step: a desktop's
    /// and an iPad's window, whose Action List is the engine's.
    Actions,
    /// Through the window's own edits, inside an undo step: a page, whose
    /// engine in the page carries no FX actions.
    Edits,
}

/// This build's door.
pub const DOOR: Door = if cfg!(feature = "native") {
    Door::Actions
} else {
    Door::Edits
};

/// One action, as the window's worker runs it.
fn run(id: &str, args: &serde_json::Value) -> Edit {
    Edit::Run(id.to_owned(), args.to_string())
}

/// `edit` alone in an undo step named `step`.
fn stepped(step: String, edit: Edit) -> Vec<Edit> {
    vec![Edit::BeginUndo(step.clone()), edit, Edit::EndUndo(step)]
}

/// The edits that put `fx` (its name) on `track` at place `at` (0 is first).
#[must_use]
pub fn add_edits(door: Door, track: &str, fx: &str, at: usize) -> Vec<Edit> {
    let at = u32::try_from(at).unwrap_or(u32::MAX);
    match door {
        Door::Actions => vec![run(
            "FX_ADD",
            &serde_json::json!({ "track": track, "fx": fx, "at": at.saturating_add(1) }),
        )],
        Door::Edits => stepped(
            format!("Add {fx}"),
            Edit::AddFx(track.to_owned(), fx.to_owned(), at),
        ),
    }
}

/// The edits that take the FX `guid` (called `name`) off `track`.
#[must_use]
pub fn remove_edits(door: Door, track: &str, guid: &str, name: &str) -> Vec<Edit> {
    match door {
        Door::Actions => vec![run(
            "FX_REMOVE",
            &serde_json::json!({ "track": track, "fx": guid }),
        )],
        Door::Edits => stepped(
            format!("Remove {name}"),
            Edit::RemoveFx(track.to_owned(), guid.to_owned()),
        ),
    }
}

/// The edits that move the FX `guid` (called `name`) on `track` to place
/// `to` (0 is first).
#[must_use]
pub fn move_edits(door: Door, track: &str, guid: &str, name: &str, to: usize) -> Vec<Edit> {
    let to = u32::try_from(to).unwrap_or(u32::MAX);
    match door {
        Door::Actions => vec![run(
            "FX_MOVE",
            &serde_json::json!({ "track": track, "fx": guid, "to": to.saturating_add(1) }),
        )],
        Door::Edits => stepped(
            format!("Move {name}"),
            Edit::MoveFx(track.to_owned(), guid.to_owned(), to),
        ),
    }
}

/// The edits that switch the FX `guid` (called `name`) on `track` on or
/// bypass it.
#[must_use]
pub fn bypass_edits(door: Door, track: &str, guid: &str, name: &str, on: bool) -> Vec<Edit> {
    match door {
        Door::Actions => vec![run(
            "FX_ENABLE",
            &serde_json::json!({ "track": track, "fx": guid, "on": on }),
        )],
        Door::Edits => stepped(
            format!("{} {name}", if on { "Switch on" } else { "Bypass" }),
            Edit::SetFxEnabled(track.to_owned(), guid.to_owned(), on),
        ),
    }
}

/// `chain` with the one at `from` moved to `to` — what the panel shows
/// before the engine says so. Out of range, the chain as it was.
#[must_use]
pub fn moved<T: Clone>(chain: &[T], from: usize, to: usize) -> Vec<T> {
    let mut out = chain.to_vec();
    if from < out.len() {
        let one = out.remove(from);
        out.insert(to.min(out.len()), one);
    }
    out
}

/// What the panel offers to add, in the order offered: the engine's FTS
/// processors (`installed`, its list; [`FTS`] when it said nothing), then
/// Signal's rigs ([`SIGNAL_RIGS`]: what the engine lists, every one when it
/// said nothing — a page's engine builds them too, #808), then Signal's
/// plugin where the engine hosts it (`signal`, a desktop's and an iPad's).
#[must_use]
pub fn addable(installed: &[String], signal: bool) -> Vec<String> {
    let is_rig = |name: &str| SIGNAL_RIGS.iter().any(|r| r.eq_ignore_ascii_case(name));
    let mut out: Vec<String> = installed
        .iter()
        .filter(|name| name.starts_with("FTS ") && !is_rig(name))
        .cloned()
        .collect();
    let said = !installed.is_empty();
    if out.is_empty() {
        out = FTS.iter().map(|&n| n.to_owned()).collect();
    }
    out.sort();
    out.dedup();
    out.extend(
        SIGNAL_RIGS
            .iter()
            .filter(|rig| !said || installed.iter().any(|n| n.eq_ignore_ascii_case(rig)))
            .map(|&rig| rig.to_owned()),
    );
    if signal {
        out.push(SIGNAL.to_owned());
    }
    out
}

/// What a face an FX has in the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// One of the rack's panels (an FTS processor).
    Rack(Which),
    /// None of its own: the generic panel of its parameters.
    Generic,
}

/// The face the FX named `name` wears.
#[must_use]
pub fn face_of(name: &str) -> Face {
    crate::tone_read::which_of(name)
        .filter(|w| !w.name().is_empty())
        .map_or(Face::Generic, Face::Rack)
}

/// The name an FX goes by in the panel: a bundle path (`…/Room.clap`) by
/// its file's stem, any other name as it is.
#[must_use]
pub fn display_name(name: &str) -> String {
    let path = std::path::Path::new(name);
    let bundle = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("clap") || e.eq_ignore_ascii_case("vst3"));
    match path.file_stem() {
        Some(stem) if bundle => stem.to_string_lossy().into_owned(),
        _ => name.to_owned(),
    }
}

// ── A plugin's parameters ─────────────────────────────────────────────

/// What kind of control a parameter is.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// Off or on.
    Toggle,
    /// One of named choices, each at its normalized value.
    Choice(Vec<(f64, String)>),
    /// One of `n` evenly spaced values, unnamed.
    Steps(u32),
    /// Anywhere from 0 to 1, its units unknown.
    Continuous,
    /// Anywhere in a span the plugin gave in its own units (#808).
    Ranged(FxParamRange),
}

/// The control `param` is.
#[must_use]
pub fn kind_of(param: &FxParameter) -> Kind {
    if param.is_toggle {
        return Kind::Toggle;
    }
    if !param.step_labels.is_empty() {
        let mut choices = param.step_labels.clone();
        choices.sort_by(|a, b| a.0.total_cmp(&b.0));
        return Kind::Choice(choices);
    }
    match (param.step_count, &param.range) {
        (Some(n), _) if n >= 2 => Kind::Steps(n),
        (_, Some(range)) => Kind::Ranged(range.clone()),
        _ => Kind::Continuous,
    }
}

/// A parameter's range, as the panel says it: its choices, its steps, its
/// span in the plugin's units, or (units unknown) as a share of its
/// travel.
#[must_use]
pub fn range_text(kind: &Kind) -> String {
    match kind {
        Kind::Toggle => "Off / On".to_owned(),
        Kind::Choice(choices) => match (choices.first(), choices.last()) {
            (Some(first), Some(last)) if choices.len() > 2 => {
                format!("{} choices: {} … {}", choices.len(), first.1, last.1)
            }
            _ => choices
                .iter()
                .map(|(_, l)| l.as_str())
                .collect::<Vec<_>>()
                .join(" / "),
        },
        Kind::Steps(n) => format!("{n} steps"),
        Kind::Continuous => "0–100%".to_owned(),
        Kind::Ranged(range) => format!(
            "{} – {}",
            in_units(range, range.min),
            in_units(range, range.max)
        ),
    }
}

/// A plain value with its unit: `-6 dB`, `440 Hz`, `0.25`.
fn in_units(range: &FxParamRange, plain: f64) -> String {
    let number = plain_text(plain);
    match range.unit.as_str() {
        "" => number,
        "%" => format!("{number}%"),
        unit => format!("{number} {unit}"),
    }
}

/// A plain number as a person reads it: fewer places the larger it is,
/// no trailing zeros.
fn plain_text(plain: f64) -> String {
    let places = match plain.abs() {
        a if a >= 100.0 => 0,
        a if a >= 10.0 => 1,
        _ => 2,
    };
    let text = format!("{plain:.places$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    };
    if text == "-0" { "0".to_owned() } else { text }
}

/// `value` on `kind`'s own values: a toggle off or on, a choice's or a
/// step's nearest.
#[must_use]
pub fn snap(kind: &Kind, value: f64) -> f64 {
    let value = value.clamp(0.0, 1.0);
    match kind {
        Kind::Toggle => {
            if value >= 0.5 {
                1.0
            } else {
                0.0
            }
        }
        Kind::Choice(choices) => choices
            .iter()
            .map(|(v, _)| *v)
            .min_by(|a, b| (a - value).abs().total_cmp(&(b - value).abs()))
            .unwrap_or(value),
        Kind::Steps(n) => {
            let last = f64::from(n.saturating_sub(1).max(1));
            (value * last).round() / last
        }
        Kind::Continuous | Kind::Ranged(_) => value,
    }
}

/// Where a press on a toggle or a choice takes it: off to on and back, a
/// choice to the next (the last round to the first). `None` for a control
/// that is dragged, not pressed.
#[must_use]
pub fn pressed(kind: &Kind, value: f64) -> Option<f64> {
    match kind {
        Kind::Toggle => Some(if snap(kind, value) >= 0.5 { 0.0 } else { 1.0 }),
        Kind::Choice(choices) => {
            let now = snap(kind, value);
            let at = choices
                .iter()
                .position(|(v, _)| (*v - now).abs() < 1e-9)
                .unwrap_or(0);
            choices
                .get(at.saturating_add(1) % choices.len().max(1))
                .map(|(v, _)| *v)
        }
        Kind::Steps(_) | Kind::Continuous | Kind::Ranged(_) => None,
    }
}

/// A typed value for `kind` as a normalized one: `=0.42`, `42%`, a choice
/// by its name, `on`/`off` for a toggle, and a number in the plugin's
/// units (`-6 dB`, `-6`) where it gave them. Other words of the plugin's
/// are the engine's to read (FX_SET); the panel takes what it can know.
#[must_use]
pub fn parse_value(kind: &Kind, text: &str) -> Option<f64> {
    let text = text.trim();
    if let Some(raw) = text.strip_prefix('=') {
        return raw.trim().parse::<f64>().ok().map(|v| snap(kind, v));
    }
    if let Some(raw) = text.strip_suffix('%') {
        return raw
            .trim()
            .parse::<f64>()
            .ok()
            .map(|v| snap(kind, v / 100.0));
    }
    match kind {
        Kind::Toggle => match text.to_ascii_lowercase().as_str() {
            "on" | "1" | "true" => Some(1.0),
            "off" | "0" | "false" => Some(0.0),
            _ => None,
        },
        Kind::Choice(choices) => choices
            .iter()
            .find(|(_, label)| label.eq_ignore_ascii_case(text))
            .map(|(v, _)| *v),
        Kind::Ranged(range) => {
            let number = strip_unit(text, &range.unit);
            number.parse::<f64>().ok().map(|v| range.normalized(v))
        }
        Kind::Steps(_) | Kind::Continuous => text.parse::<f64>().ok().map(|v| snap(kind, v)),
    }
}

/// `text` without `unit` at its end (case aside), trimmed.
fn strip_unit<'a>(text: &'a str, unit: &str) -> &'a str {
    let text = text.trim();
    if unit.is_empty() || unit.len() > text.len() {
        return text;
    }
    let cut = text.len().saturating_sub(unit.len());
    match (text.get(..cut), text.get(cut..)) {
        (Some(number), Some(end)) if end.eq_ignore_ascii_case(unit) => number.trim(),
        _ => text,
    }
}

/// What a parameter reads: the plugin's own words while it is where the
/// plugin last said (`read`, `formatted`), else the panel's — a choice's
/// name, on/off, the value in the plugin's units, or (units unknown) a
/// share of its travel — until the engine says again.
#[must_use]
pub fn value_text(kind: &Kind, value: f64, read: (f64, &str)) -> String {
    if (value - read.0).abs() < 1e-6 && !read.1.trim().is_empty() {
        return read.1.trim().to_owned();
    }
    match kind {
        Kind::Toggle => if snap(kind, value) >= 0.5 {
            "On"
        } else {
            "Off"
        }
        .to_owned(),
        Kind::Choice(choices) => {
            let now = snap(kind, value);
            choices
                .iter()
                .find(|(v, _)| (*v - now).abs() < 1e-9)
                .map_or_else(|| percent(value), |(_, l)| l.clone())
        }
        Kind::Ranged(range) => in_units(range, range.plain(value)),
        Kind::Steps(_) | Kind::Continuous => percent(value),
    }
}

fn percent(value: f64) -> String {
    format!("{:.0}%", value.clamp(0.0, 1.0) * 100.0)
}

/// The parameters whose names hold `filter` (case aside), in the plugin's
/// order; all of them for an empty one.
#[must_use]
pub fn shown<'a>(params: &'a [FxParameter], filter: &str) -> Vec<&'a FxParameter> {
    let filter = filter.trim().to_lowercase();
    params
        .iter()
        .filter(|p| filter.is_empty() || p.name.to_lowercase().contains(&filter))
        .collect()
}

// ── What the panel reads ──────────────────────────────────────────────

/// The chain as the panel holds it: read from the engine, its own edits
/// predicted into it.
#[derive(Default)]
struct Held {
    chain: Option<Vec<ChainFx>>,
    installed: Option<Vec<String>>,
    /// A read is in flight.
    reading: bool,
    /// When the panel last predicted an edit: a read begun before it does
    /// not land over it.
    edited: Option<web_time::Instant>,
    /// Something landed since the panel last looked.
    fresh: bool,
    /// Read again as soon as what was made is sent: an FX added, which
    /// only the engine's read can show (it has the guid).
    want: bool,
    /// When an FX was last asked for: a second press before the read that
    /// shows it would add it twice.
    added: Option<web_time::Instant>,
}

/// [`Held`], shared with the reads (which are `Send` on a desktop).
#[derive(Clone, Default)]
struct Reader(Arc<Mutex<Held>>);

impl PartialEq for Reader {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Reader {
    fn with<R>(&self, f: impl FnOnce(&mut Held) -> R) -> R {
        f(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Read `track`'s chain (and once, what can be added), unless a read
    /// is in flight.
    fn read(&self, track: &str) {
        let began = web_time::Instant::now();
        let want_installed = self.with(|h| {
            if h.reading {
                return None;
            }
            h.reading = true;
            Some(h.installed.is_none())
        });
        let Some(want_installed) = want_installed else {
            return;
        };
        let (me, track) = (self.clone(), track.to_owned());
        crate::routes::spawn_on_engine("fx-chain-read", async move {
            let chain = crate::engine::chain_of(&track).await;
            let installed = if want_installed {
                Some(crate::engine::installed_fx().await)
            } else {
                None
            };
            me.with(|h| {
                h.reading = false;
                if let Some(installed) = installed {
                    h.installed = Some(installed);
                    h.fresh = true;
                }
                let stale = h.edited.is_some_and(|at| at > began);
                if let Some(chain) = chain
                    && !stale
                {
                    h.chain = Some(chain);
                    h.fresh = true;
                }
            });
        });
    }

    /// Change what is shown now, ahead of the engine.
    fn predict(&self, f: impl FnOnce(&mut Vec<ChainFx>)) {
        self.with(|h| {
            if let Some(chain) = h.chain.as_mut() {
                f(chain);
            }
            h.edited = Some(web_time::Instant::now());
            h.fresh = true;
        });
    }

    fn recent(&self) -> bool {
        self.with(|h| h.edited.is_some_and(|at| at.elapsed() < OWN_EDIT))
    }

    /// Ask for an FX to be added: false when one was just asked for and
    /// the read that shows it has not landed.
    fn ask_add(&self) -> bool {
        self.with(|h| {
            if h.added.is_some_and(|at| at.elapsed() < OWN_EDIT) {
                return false;
            }
            h.added = Some(web_time::Instant::now());
            h.edited = None;
            h.want = true;
            true
        })
    }

    fn take_want(&self) -> bool {
        self.with(|h| std::mem::take(&mut h.want))
    }

    fn take_fresh(&self) -> bool {
        self.with(|h| std::mem::take(&mut h.fresh))
    }

    fn chain(&self) -> Option<Vec<ChainFx>> {
        self.with(|h| h.chain.clone())
    }

    fn installed(&self) -> Vec<String> {
        self.with(|h| h.installed.clone().unwrap_or_default())
    }
}

/// What the panel's controls hand to the racks' worker: made in a slider's
/// closure (which must be `Send`), sent in order by the panel's own loop.
enum Intent {
    /// A parameter moved: live, in the gesture's undo step.
    Param { fx: String, index: u32, value: f64 },
    /// The gesture ended: committed (`FX_SET`) and its step closed.
    Commit,
    /// A change to the chain.
    Edits(Vec<Edit>),
}

/// What the panel made, and whether a parameter gesture it began is still
/// open (a commit is owed).
#[derive(Clone, Default)]
struct Outbox(Arc<Mutex<(Vec<Intent>, bool)>>);

impl PartialEq for Outbox {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Outbox {
    fn push(&self, intent: Intent) {
        let mut held = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match intent {
            Intent::Param { .. } => held.1 = true,
            Intent::Commit => held.1 = false,
            Intent::Edits(_) => {}
        }
        held.0.push(intent);
    }

    /// Whether this panel began a gesture it has not committed.
    fn open(&self) -> bool {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).1
    }

    /// Hand everything made to the racks' worker, in order.
    fn send(&self, track: &str, shared: &crate::tone::Shared) {
        let made = std::mem::take(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner).0);
        for intent in made {
            match intent {
                Intent::Param { fx, index, value } => shared.set_param(track, &fx, index, value),
                Intent::Commit => shared.commit(),
                Intent::Edits(edits) => shared.send_edits(edits),
            }
        }
    }
}

// ── The panel ─────────────────────────────────────────────────────────

/// One track's FX chain, or (`fx`, a guid) one FX's parameters: the whole
/// close-up layer.
#[component]
pub fn ChainCloseup(track: String, name: String, fx: Option<String>) -> Element {
    let closeups: Closeups = use_context();
    let reader = use_hook(Reader::default);
    let outbox = use_hook(Outbox::default);
    let mut tick = use_signal(|| 0_u64);
    // Closed mid-gesture: what it made is sent, and a gesture it began is
    // committed, so no undo step stays open. One it did not begin (a
    // rack's, on the same worker) is left to its own.
    {
        let (outbox, shared, track) = (
            outbox.clone(),
            std::rc::Rc::clone(&closeups.tone),
            track.clone(),
        );
        use_drop(move || {
            if outbox.open() {
                outbox.push(Intent::Commit);
            }
            outbox.send(&track, &shared);
        });
    }
    {
        let (reader, outbox, track) = (reader.clone(), outbox.clone(), track.clone());
        let shared = std::rc::Rc::clone(&closeups.tone);
        use_future(move || {
            let (reader, outbox, track) = (reader.clone(), outbox.clone(), track.clone());
            let shared = std::rc::Rc::clone(&shared);
            async move {
                reader.read(&track);
                let mut read_at = web_time::Instant::now();
                loop {
                    crate::pause(TICK).await;
                    outbox.send(&track, &shared);
                    // An add is shown by the read after it, at once.
                    if reader.take_want() {
                        reader.read(&track);
                        read_at = web_time::Instant::now();
                    } else if read_at.elapsed() >= REREAD && !reader.recent() {
                        reader.read(&track);
                        read_at = web_time::Instant::now();
                    }
                    if reader.take_fresh() {
                        tick += 1;
                    }
                }
            }
        });
    }
    let _ = tick();
    let Some(chain) = reader.chain() else {
        return rsx! {
            div { style: "padding:24px; color:#9ca3af; font-size:14px;", "Reading the chain…" }
        };
    };
    let shower = closeups.handle();
    if let Some(open) = fx {
        let Some(one) = chain.iter().find(|f| f.guid == open).cloned() else {
            return rsx! {
                div {
                    style: "padding:24px; color:#9ca3af; font-size:14px;",
                    "That FX is no longer on the track."
                }
            };
        };
        return rsx! {
            ParamsPanel { track, fx: one.guid.clone(), fx_name: display_name(&one.name), reader, outbox, tick }
        };
    }
    let count = chain.len();
    let rows = chain.iter().enumerate().map(|(at, one)| {
        let label = display_name(&one.name);
        let face = face_of(&one.name);
        let (guid, on) = (one.guid.clone(), one.enabled);
        let (reader, outbox, track, name) = (reader.clone(), outbox.clone(), track.clone(), name.clone());
        let bypass = {
            let (reader, outbox, track, guid, label) = (reader.clone(), outbox.clone(), track.clone(), guid.clone(), label.clone());
            move |_| {
                reader.predict(|c| {
                    if let Some(f) = c.iter_mut().find(|f| f.guid == guid) {
                        f.enabled = !on;
                    }
                });
                outbox.push(Intent::Edits(bypass_edits(DOOR, &track, &guid, &label, !on)));
                tick += 1;
            }
        };
        let shift = |to: usize| {
            let (reader, outbox, track, guid, label) = (reader.clone(), outbox.clone(), track.clone(), guid.clone(), label.clone());
            move |_| {
                reader.predict(|c| *c = moved(c, at, to));
                outbox.push(Intent::Edits(move_edits(DOOR, &track, &guid, &label, to)));
                tick += 1;
            }
        };
        let up = shift(at.saturating_sub(1));
        let down = shift(at.saturating_add(1));
        let remove = {
            let (reader, outbox, track, guid, label) = (reader.clone(), outbox.clone(), track.clone(), guid.clone(), label.clone());
            move |_| {
                reader.predict(|c| c.retain(|f| f.guid != guid));
                outbox.push(Intent::Edits(remove_edits(DOOR, &track, &guid, &label)));
                tick += 1;
            }
        };
        let open = {
            let (track, name, guid) = (track.clone(), name.clone(), guid.clone());
            move |_| match face {
                Face::Rack(which) => shower.show(&Closeup::Rack { guid: track.clone(), name: name.clone(), which }),
                Face::Generic => shower.show(&Closeup::Chain { guid: track.clone(), name: name.clone(), fx: Some(guid.clone()) }),
            }
        };
        let open_word = match face {
            Face::Rack(_) => "Open",
            Face::Generic => "Parameters",
        };
        let on_look = if on { "" } else { "border-color:#f59e0b; color:#f59e0b;" };
        let dim = if on { "#e5e7eb" } else { "#6b7280" };
        let place = at.saturating_add(1);
        rsx! {
            div {
                key: "{guid}",
                style: "display:flex; flex-direction:row; flex-wrap:wrap; align-items:center; gap:8px 10px; \
                        padding:10px 12px; border-radius:8px; background:#17181b; border:1px solid #2a2c31;",
                div { style: "flex:none; width:22px; font-size:12px; color:#6b7280;", "{place}" }
                div {
                    style: "flex:1; min-width:140px; font-size:13px; font-weight:600; color:{dim}; \
                            white-space:nowrap; overflow:hidden;",
                    "{label}"
                }
                button { style: "{BUTTON} {on_look}", onclick: bypass, if on { "On" } else { "Bypassed" } }
                button { style: "{BUTTON}", disabled: at == 0, onclick: up, "Up" }
                button { style: "{BUTTON}", disabled: at + 1 >= count, onclick: down, "Down" }
                button { style: "{BUTTON}", onclick: open, "{open_word}" }
                button { style: "{BUTTON}", onclick: remove, "Remove" }
            }
        }
    });
    let offered = addable(&reader.installed(), cfg!(feature = "native"));
    let add = offered.into_iter().map(|fx_name| {
        let (reader, outbox, track) = (reader.clone(), outbox.clone(), track.clone());
        rsx! {
            button {
                key: "{fx_name}",
                style: "{BUTTON}",
                onclick: move |_| {
                    // Shown by the read right after it is sent (the engine
                    // has its guid); a second press meanwhile is the same.
                    if reader.ask_add() {
                        outbox.push(Intent::Edits(add_edits(DOOR, &track, &fx_name, count)));
                        tick += 1;
                    }
                },
                "+ {fx_name}"
            }
        }
    });
    rsx! {
        div {
            style: "width:100%; height:100%; display:flex; flex-direction:column; gap:18px; \
                    padding:16px; overflow-y:scroll; box-sizing:border-box;",
            div {
                style: "display:flex; flex-direction:column; gap:8px;",
                div { style: "{HEADING}", "Chain" }
                if chain.is_empty() {
                    div { style: "font-size:12px; color:#6b7280;", "No FX on this track." }
                }
                {rows}
            }
            div {
                style: "display:flex; flex-direction:column; gap:8px;",
                div { style: "{HEADING}", "Add" }
                div { style: "display:flex; flex-direction:row; flex-wrap:wrap; gap:6px;", {add} }
                ClapAdd { track: track.clone(), at: count, reader: reader.clone(), outbox: outbox.clone(), tick }
            }
        }
    }
}

/// A CLAP bundle added by its name or path: a desktop's alone — an iPad
/// and a page host no third-party plugins.
#[component]
fn ClapAdd(track: String, at: usize, reader: Reader, outbox: Outbox, tick: Signal<u64>) -> Element {
    #[cfg(all(feature = "native", not(target_os = "ios")))]
    {
        let mut bundle = use_signal(String::new);
        let add = move |_| {
            let name = bundle.peek().trim().to_owned();
            if name.is_empty() || !reader.ask_add() {
                return;
            }
            outbox.push(Intent::Edits(add_edits(DOOR, &track, &name, at)));
            bundle.set(String::new());
            tick += 1;
        };
        rsx! {
            div {
                style: "display:flex; flex-direction:row; gap:6px; align-items:center;",
                input {
                    r#type: "text",
                    style: "{FIELD} flex:1; min-width:180px;",
                    placeholder: "A CLAP bundle: its name (Room.clap) or path",
                    value: "{bundle}",
                    oninput: move |e| bundle.set(e.value()),
                }
                button { style: "{BUTTON}", onclick: add, "+ CLAP" }
            }
        }
    }
    #[cfg(not(all(feature = "native", not(target_os = "ios"))))]
    {
        let _ = (track, at, reader, outbox, tick);
        rsx! {}
    }
}

/// One FX's parameters: each with its name, a control for its value, the
/// value as the plugin writes it, and its range; narrowed by name.
#[component]
fn ParamsPanel(
    track: String,
    fx: String,
    fx_name: String,
    reader: Reader,
    outbox: Outbox,
    tick: Signal<u64>,
) -> Element {
    let mut filter = use_signal(String::new);
    let chain = reader.chain().unwrap_or_default();
    let Some(one) = chain.iter().find(|f| f.guid == fx) else {
        return rsx! {};
    };
    let wanted = filter();
    let list = shown(&one.params, &wanted);
    let total = one.params.len();
    let rows = list.into_iter().map(|param| {
        let kind = kind_of(param);
        let range = range_text(&kind);
        let index = param.index;
        let control = match &kind {
            Kind::Toggle | Kind::Choice(_) => {
                let (reader, outbox, fx) = (reader.clone(), outbox.clone(), fx.clone());
                let (kind, value) = (kind.clone(), param.value);
                let text = value_text(&kind, value, (value, &param.formatted));
                rsx! {
                    button {
                        style: "{BUTTON} min-width:120px;",
                        onclick: move |_| {
                            if let Some(next) = pressed(&kind, value) {
                                predict_param(&reader, &fx, index, next);
                                outbox.push(Intent::Param { fx: fx.clone(), index, value: next });
                                outbox.push(Intent::Commit);
                                tick += 1;
                            }
                        },
                        "{text}"
                    }
                }
            }
            Kind::Steps(_) | Kind::Continuous | Kind::Ranged(_) => {
                let handle = slider(&reader, &outbox, (&fx, index), &kind, &param.name);
                rsx! { div { style: "flex:1; min-width:200px;", HSlider { handle, height: 26.0 } } }
            }
        };
        rsx! {
            div {
                key: "{index}",
                style: "display:flex; flex-direction:row; flex-wrap:wrap; align-items:center; gap:6px 12px; \
                        padding:8px 12px; border-radius:8px; background:#17181b; border:1px solid #2a2c31;",
                div {
                    style: "flex:none; width:180px; display:flex; flex-direction:column; gap:2px;",
                    div { style: "font-size:13px; font-weight:600; color:#e5e7eb; white-space:nowrap; overflow:hidden;", "{param.name}" }
                    div { style: "font-size:11px; color:#6b7280; white-space:nowrap; overflow:hidden;", "#{index} · {range}" }
                }
                {control}
            }
        }
    });
    rsx! {
        fts_audio_ui::drag::DragProvider { fill: true,
            div {
                style: "width:100%; height:100%; display:flex; flex-direction:column; gap:10px; \
                        padding:16px; overflow-y:scroll; box-sizing:border-box;",
                div {
                    style: "display:flex; flex-direction:row; align-items:center; gap:10px;",
                    div { style: "{HEADING} flex:none;", "{fx_name} · {total} parameters" }
                    input {
                        r#type: "text",
                        style: "{FIELD} flex:1; min-width:120px;",
                        placeholder: "Find a parameter",
                        value: "{wanted}",
                        oninput: move |e| filter.set(e.value()),
                    }
                }
                if total == 0 {
                    div { style: "font-size:12px; color:#6b7280;", "This FX has no parameters to show." }
                }
                {rows}
            }
        }
    }
}

/// `value` predicted into the parameter `index` of the FX `fx`.
fn predict_param(reader: &Reader, fx: &str, index: u32, value: f64) {
    reader.predict(|c| {
        if let Some(p) = c
            .iter_mut()
            .find(|f| f.guid == fx)
            .and_then(|f| f.params.iter_mut().find(|p| p.index == index))
        {
            p.value = value;
        }
    });
}

/// A continuous or stepped parameter as a slider's handle: read from what
/// the panel holds, predicted into it as it moves, each move a live edit in
/// the gesture's undo step, committed when let go.
fn slider(
    reader: &Reader,
    outbox: &Outbox,
    (fx, index): (&str, u32),
    kind: &Kind,
    name: &str,
) -> ParamHandle {
    let read = {
        let (reader, fx) = (reader.clone(), fx.to_owned());
        move || {
            reader.with(|h| {
                h.chain
                    .as_ref()?
                    .iter()
                    .find(|f| f.guid == fx)?
                    .params
                    .iter()
                    .find(|p| p.index == index)
                    .map(|p| (p.value, p.formatted.clone()))
            })
        }
    };
    let (r1, r2) = (read.clone(), read);
    let set = {
        let (reader, outbox, fx, kind) =
            (reader.clone(), outbox.clone(), fx.to_owned(), kind.clone());
        move |position: f32| {
            let value = snap(&kind, f64::from(position));
            predict_param(&reader, &fx, index, value);
            outbox.push(Intent::Param {
                fx: fx.clone(),
                index,
                value,
            });
        }
    };
    let end = {
        let outbox = outbox.clone();
        move || outbox.push(Intent::Commit)
    };
    // What the plugin last said this value reads, kept beside the read: a
    // moved slider says the panel's words until the engine says its own.
    let said = std::sync::Arc::new(Mutex::new(None::<(f64, String)>));
    let text = {
        let (kind, said) = (kind.clone(), std::sync::Arc::clone(&said));
        move || {
            let Some((value, formatted)) = r2() else {
                return String::new();
            };
            let mut said = said.lock().unwrap_or_else(PoisonError::into_inner);
            let read = said.get_or_insert_with(|| (value, formatted.clone()));
            // A read from the engine (its words changed) is what it says.
            if read.1 != formatted {
                *read = (value, formatted);
            }
            value_text(&kind, value, (read.0, &read.1))
        }
    };
    let parse = {
        let kind = kind.clone();
        move |text: &str| parse_value(&kind, text).map(crate::num::narrow)
    };
    let name = name.to_owned();
    let handle = ParamHandle::new(
        move || r1().map_or(0.0, |(value, _)| crate::num::narrow(value)),
        || {},
        set,
        end,
        text,
        move || name.clone(),
        parse,
    );
    match kind {
        Kind::Steps(n) => handle.with_step_count(usize::try_from(*n).ok()),
        _ => handle,
    }
}

const HEADING: &str = "font-size:11px; font-weight:650; letter-spacing:0.06em; \
     text-transform:uppercase; color:#9ca3af;";

const BUTTON: &str = "height:28px; padding:0 10px; border-radius:6px; border:1px solid #2a2c31; \
     background:#1c1e22; color:#e5e7eb; font-family:inherit; font-size:12px; \
     display:flex; align-items:center; justify-content:center; white-space:nowrap;";

const FIELD: &str = "height:28px; padding:0 8px; border-radius:6px; border:1px solid #2a2c31; \
     background:#111214; color:#e5e7eb; font-family:inherit; font-size:12px;";

#[cfg(test)]
mod tests {
    use super::*;

    fn param(index: u32, name: &str) -> FxParameter {
        FxParameter {
            index,
            name: name.to_owned(),
            value: 0.5,
            formatted: "50%".to_owned(),
            is_toggle: false,
            step_count: None,
            step_labels: Vec::new(),
            range: None,
        }
    }

    fn decibels() -> Kind {
        Kind::Ranged(FxParamRange {
            min: -60.0,
            max: 0.0,
            unit: "dB".into(),
        })
    }

    #[test]
    fn a_parameter_is_the_control_it_says_it_is() {
        let mut p = param(0, "Bypass");
        p.is_toggle = true;
        assert_eq!(kind_of(&p), Kind::Toggle);
        let mut p = param(1, "Mode");
        p.step_labels = vec![
            (1.0, "Hall".into()),
            (0.0, "Room".into()),
            (0.5, "Plate".into()),
        ];
        assert_eq!(
            kind_of(&p),
            Kind::Choice(vec![
                (0.0, "Room".into()),
                (0.5, "Plate".into()),
                (1.0, "Hall".into())
            ]),
            "choices in the order of their values"
        );
        let mut p = param(2, "Oversampling");
        p.step_count = Some(4);
        assert_eq!(kind_of(&p), Kind::Steps(4));
        assert_eq!(kind_of(&param(3, "Mix")), Kind::Continuous);
        let mut p = param(4, "Threshold");
        p.range = Some(FxParamRange {
            min: -60.0,
            max: 0.0,
            unit: "dB".into(),
        });
        assert_eq!(kind_of(&p), decibels(), "a span in the plugin's units");
    }

    #[test]
    fn a_range_says_what_the_control_can_be() {
        assert_eq!(range_text(&Kind::Toggle), "Off / On");
        let two = Kind::Choice(vec![(0.0, "Stereo".into()), (1.0, "Mono".into())]);
        assert_eq!(range_text(&two), "Stereo / Mono");
        let three = Kind::Choice(vec![
            (0.0, "A".into()),
            (0.5, "B".into()),
            (1.0, "C".into()),
        ]);
        assert_eq!(range_text(&three), "3 choices: A … C");
        assert_eq!(range_text(&Kind::Steps(4)), "4 steps");
        assert_eq!(range_text(&Kind::Continuous), "0–100%");
        assert_eq!(range_text(&decibels()), "-60 dB – 0 dB");
        let hz = Kind::Ranged(FxParamRange {
            min: 20.0,
            max: 20_000.0,
            unit: "Hz".into(),
        });
        assert_eq!(range_text(&hz), "20 Hz – 20000 Hz");
        let mix = Kind::Ranged(FxParamRange {
            min: 0.0,
            max: 100.0,
            unit: "%".into(),
        });
        assert_eq!(range_text(&mix), "0% – 100%");
    }

    #[test]
    fn a_value_lands_on_the_controls_own_values() {
        assert_eq!(snap(&Kind::Toggle, 0.7), 1.0);
        assert_eq!(snap(&Kind::Toggle, 0.2), 0.0);
        let choice = Kind::Choice(vec![
            (0.0, "A".into()),
            (0.5, "B".into()),
            (1.0, "C".into()),
        ]);
        assert_eq!(snap(&choice, 0.4), 0.5);
        assert!((snap(&Kind::Steps(4), 0.4) - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(snap(&Kind::Continuous, 1.4), 1.0);
    }

    #[test]
    fn a_press_steps_a_switch_or_a_choice_round() {
        assert_eq!(pressed(&Kind::Toggle, 0.0), Some(1.0));
        assert_eq!(pressed(&Kind::Toggle, 1.0), Some(0.0));
        let choice = Kind::Choice(vec![
            (0.0, "A".into()),
            (0.5, "B".into()),
            (1.0, "C".into()),
        ]);
        assert_eq!(pressed(&choice, 0.0), Some(0.5));
        assert_eq!(
            pressed(&choice, 1.0),
            Some(0.0),
            "the last goes round to the first"
        );
        assert_eq!(pressed(&Kind::Continuous, 0.3), None);
    }

    #[test]
    fn a_typed_value_is_read_as_the_panel_can_know_it() {
        assert_eq!(parse_value(&Kind::Continuous, "=0.25"), Some(0.25));
        assert_eq!(parse_value(&Kind::Continuous, "40%"), Some(0.4));
        assert_eq!(parse_value(&Kind::Toggle, "on"), Some(1.0));
        let choice = Kind::Choice(vec![(0.0, "Room".into()), (1.0, "Hall".into())]);
        assert_eq!(parse_value(&choice, "hall"), Some(1.0));
        assert_eq!(parse_value(&choice, "Cathedral"), None);
        // The plugin's own words are the engine's to read.
        assert_eq!(parse_value(&Kind::Continuous, "-6 dB"), None);
        // ...unless the plugin gave its units.
        let db = decibels();
        assert_eq!(parse_value(&db, "-30 dB"), Some(0.5));
        assert_eq!(parse_value(&db, "-30db"), Some(0.5));
        assert_eq!(parse_value(&db, "-15"), Some(0.75));
        assert_eq!(parse_value(&db, "=0.25"), Some(0.25));
        assert_eq!(parse_value(&db, "+12 dB"), Some(1.0), "held to the range");
        assert_eq!(parse_value(&db, "loud"), None);
    }

    #[test]
    fn a_value_reads_in_the_plugins_words_until_it_moves() {
        let choice = Kind::Choice(vec![(0.0, "Room".into()), (1.0, "Hall".into())]);
        assert_eq!(
            value_text(&Kind::Continuous, 0.5, (0.5, "-6.0 dB")),
            "-6.0 dB"
        );
        assert_eq!(value_text(&Kind::Continuous, 0.25, (0.5, "-6.0 dB")), "25%");
        assert_eq!(value_text(&choice, 1.0, (0.0, "Room")), "Hall");
        assert_eq!(value_text(&Kind::Toggle, 1.0, (0.0, "Off")), "On");
        assert_eq!(
            value_text(&decibels(), 0.25, (0.5, "-30.0 dB")),
            "-45 dB",
            "moved, in the plugin's units"
        );
        assert_eq!(value_text(&decibels(), 0.5, (0.5, "-30.0 dB")), "-30.0 dB");
    }

    #[test]
    fn a_name_narrows_the_parameters_shown() {
        let params = vec![param(0, "Decay"), param(1, "Pre Delay"), param(2, "Mix")];
        let names = |f: &str| {
            shown(&params, f)
                .into_iter()
                .map(|p| p.index)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(""), [0, 1, 2]);
        assert_eq!(names("DE"), [0, 1]);
        assert_eq!(names("width"), Vec::<u32>::new());
    }

    #[test]
    fn a_move_is_shown_before_the_engine_says_so() {
        let chain = ["EQ", "Comp", "Verb"];
        assert_eq!(moved(&chain, 2, 0), ["Verb", "EQ", "Comp"]);
        assert_eq!(moved(&chain, 0, 1), ["Comp", "EQ", "Verb"]);
        assert_eq!(
            moved(&chain, 1, 9),
            ["EQ", "Verb", "Comp"],
            "past the end is last"
        );
        assert_eq!(moved(&chain, 7, 0), chain, "nothing there, nothing moves");
    }

    #[test]
    fn a_desktop_changes_the_chain_by_action_and_a_page_by_edit() {
        // A desktop: the engine's actions, places counted from 1, by guid.
        let add = add_edits(Door::Actions, "t", "FTS EQ", 2);
        let Some(Edit::Run(id, args)) = add.first() else {
            panic!("an action: {add:?}");
        };
        assert_eq!(id, "FX_ADD");
        let args: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
        assert_eq!(
            args,
            serde_json::json!({ "track": "t", "fx": "FTS EQ", "at": 3 })
        );
        let moved = move_edits(Door::Actions, "t", "g", "Comp", 0);
        assert!(matches!(moved.as_slice(), [Edit::Run(id, _)] if id == "FX_MOVE"));
        let gone = remove_edits(Door::Actions, "t", "g", "Comp");
        assert!(matches!(gone.as_slice(), [Edit::Run(id, _)] if id == "FX_REMOVE"));
        // A page: the window's own edit, alone in an undo step.
        assert_eq!(
            bypass_edits(Door::Edits, "t", "g", "Comp", false),
            [
                Edit::BeginUndo("Bypass Comp".into()),
                Edit::SetFxEnabled("t".into(), "g".into(), false),
                Edit::EndUndo("Bypass Comp".into()),
            ]
        );
        assert_eq!(
            move_edits(Door::Edits, "t", "g", "Comp", 1).get(1),
            Some(&Edit::MoveFx("t".into(), "g".into(), 1))
        );
        assert_eq!(
            add_edits(Door::Edits, "t", "FTS EQ", 0).get(1),
            Some(&Edit::AddFx("t".into(), "FTS EQ".into(), 0))
        );
    }

    #[test]
    fn the_engines_own_fx_are_offered_and_signal_where_it_is_built() {
        let installed = vec![
            "ReaComp".to_owned(),
            "FTS Comp".to_owned(),
            "FTS EQ".to_owned(),
            "FTS Comp".to_owned(),
        ];
        assert_eq!(addable(&installed, false), ["FTS Comp", "FTS EQ"]);
        assert_eq!(
            addable(&installed, true).last().map(String::as_str),
            Some(SIGNAL)
        );
        assert_eq!(
            addable(&[], false).len(),
            FTS.len() + SIGNAL_RIGS.len(),
            "the FTS processors and Signal's rigs when the engine said nothing"
        );
        // A page's engine: Signal's rigs, after the processors, the whole
        // rig first; no Signal plugin, which only a desktop hosts.
        let page = vec![
            "FTS Kit Mic".to_owned(),
            "FTS EQ".to_owned(),
            "FTS Signal Rig".to_owned(),
            "FTS Bass Rig".to_owned(),
        ];
        assert_eq!(
            addable(&page, false),
            ["FTS EQ", "FTS Signal Rig", "FTS Bass Rig", "FTS Kit Mic"]
        );
    }

    #[test]
    fn an_fx_wears_its_face_or_the_generic_panel() {
        assert_eq!(face_of("FTS EQ"), Face::Rack(Which::Eq));
        assert_eq!(
            face_of("/Library/Audio/Plug-Ins/CLAP/Room.clap"),
            Face::Generic
        );
        assert_eq!(
            display_name("/Library/Audio/Plug-Ins/CLAP/Room.clap"),
            "Room"
        );
        assert_eq!(display_name("FTS EQ"), "FTS EQ");
    }
}
