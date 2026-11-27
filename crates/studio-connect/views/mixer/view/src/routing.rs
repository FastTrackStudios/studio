//! A track's routing, close up: every send it makes, every send it
//! receives and every hardware output it plays to, each with its level,
//! pan and mute, a send's tap, and a way to take it off; and a send added
//! to any other track (studio#98).
//!
//! Opened by the strip's routing widget (`SESSION_CLOSEUP {routing}`) and
//! drawn in the close-up layer, so it is the same panel on a desktop, an
//! iPad and in a browser. It reads through [`crate::routes::Routes`] (the
//! engine's routes for the one track, read again every [`REREAD`] so a
//! change made elsewhere shows) and writes the window's routing edits
//! (`Edit::SetRouteVolume` …) through the engine's [`Applier`], each
//! predicted into what it reads so a slider does not wait on the engine.
//!
//! A receive is editable here because it is the send feeding it: turning
//! one down turns the other down. Adding a hardware output is not offered:
//! the engine's facade cannot yet say which outputs the audio device has,
//! so a picker would offer outputs that may not exist.
//!
//! [`Applier`]: crate::engine::Applier

use std::sync::{Arc, Mutex, PoisonError};

use daw_proto::routing::{RouteType, SendMode, TrackRoute};
use dioxus::prelude::*;
use fts_audio_ui::ParamHandle;
use fts_audio_ui::controls::HSlider;

use crate::engine::Edit;
use crate::routes::{At, RouteTrack, Routes, Wiring};

/// How often what is shown is read from the engine again while open.
const REREAD: std::time::Duration = std::time::Duration::from_millis(500);

/// How long after the panel's own edit a read is held off: a read landing
/// under a slider being dragged would pull it back to where it was.
const OWN_EDIT: std::time::Duration = std::time::Duration::from_millis(400);

/// How often the panel looks for what has landed and sends what it made.
const TICK: std::time::Duration = std::time::Duration::from_millis(30);

/// How long a send asked for is kept off the "send to" list: long enough
/// for the reads after it to show it.
const PENDING: std::time::Duration = std::time::Duration::from_secs(2);

/// The lowest level the slider reaches above silence, in dB.
pub const FLOOR_DB: f64 = -60.0;
/// The highest, in dB.
pub const CEILING_DB: f64 = 12.0;

/// The undo step one slider gesture makes.
const STEP: &str = "Edit routing";

/// A gain (1.0 = 0 dB) as the slider's position, 0..1: [`FLOOR_DB`] to
/// [`CEILING_DB`] in dB, and silence at 0.
#[must_use]
pub fn level_position(gain: f64) -> f32 {
    if gain <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * gain.log10();
    narrow(((db - FLOOR_DB) / (CEILING_DB - FLOOR_DB)).clamp(0.0, 1.0))
}

/// The gain at a slider position ([`level_position`]'s inverse): 0 is
/// silence.
#[must_use]
pub fn level_gain(position: f32) -> f64 {
    let at = f64::from(position).clamp(0.0, 1.0);
    if at <= 0.0 {
        return 0.0;
    }
    crate::engine::db_to_gain(FLOOR_DB + at * (CEILING_DB - FLOOR_DB))
}

/// A gain as words: `-6.0 dB`, `-inf dB`.
#[must_use]
pub fn level_text(gain: f64) -> String {
    if gain <= 0.0 {
        return "-inf dB".to_owned();
    }
    format!("{:.1} dB", 20.0 * gain.log10())
}

/// A typed level (`-6`, `-6 dB`, `-inf`) as a slider position.
#[must_use]
pub fn parse_level(text: &str) -> Option<f32> {
    let words = text.trim().trim_end_matches("dB").trim();
    if words.eq_ignore_ascii_case("-inf") {
        return Some(0.0);
    }
    let db: f64 = words.parse().ok()?;
    Some(level_position(crate::engine::db_to_gain(db)))
}

/// A pan (-1 left … 1 right) as the slider's position, 0..1.
#[must_use]
pub fn pan_position(pan: f64) -> f32 {
    narrow(((pan.clamp(-1.0, 1.0) + 1.0) / 2.0).clamp(0.0, 1.0))
}

/// The pan at a slider position.
#[must_use]
pub fn pan_at(position: f32) -> f64 {
    f64::from(position).clamp(0.0, 1.0).mul_add(2.0, -1.0)
}

/// A pan as words: `C`, `L30`, `R100`.
#[must_use]
pub fn pan_text(pan: f64) -> String {
    let percent = (pan.clamp(-1.0, 1.0) * 100.0).round();
    if percent.abs() < 0.5 {
        "C".to_owned()
    } else if percent < 0.0 {
        format!("L{:.0}", -percent)
    } else {
        format!("R{percent:.0}")
    }
}

/// A typed pan (`C`, `L30`, `R100`, `-0.3`) as a slider position.
#[must_use]
pub fn parse_pan(text: &str) -> Option<f32> {
    let words = text.trim();
    if words.eq_ignore_ascii_case("c") {
        return Some(pan_position(0.0));
    }
    let side = |rest: &str, sign: f64| -> Option<f32> {
        let percent: f64 = rest.trim().parse().ok()?;
        Some(pan_position(sign * percent / 100.0))
    };
    if let Some(rest) = words.strip_prefix(['L', 'l']) {
        return side(rest, -1.0);
    }
    if let Some(rest) = words.strip_prefix(['R', 'r']) {
        return side(rest, 1.0);
    }
    words.parse::<f64>().ok().map(pan_position)
}

/// Where a send is taken from, as the panel says it.
#[must_use]
pub const fn tap_word(mode: SendMode) -> &'static str {
    match mode {
        SendMode::PostFader => "Post-fader",
        SendMode::PostFx => "Pre-fader",
        SendMode::PreFx => "Pre-FX",
    }
}

/// The tap a press on a send's tap moves to: post-fader, pre-fader
/// (after the FX), pre-FX, round again.
#[must_use]
pub const fn next_tap(mode: SendMode) -> SendMode {
    match mode {
        SendMode::PostFader => SendMode::PostFx,
        SendMode::PostFx => SendMode::PreFx,
        SendMode::PreFx => SendMode::PostFader,
    }
}

/// What a route is called on `mine`'s panel: the track at its other end,
/// by name where `tracks` knows it, or the hardware output it plays to.
#[must_use]
pub fn route_name(route: &TrackRoute, mine: &str, tracks: &[RouteTrack]) -> String {
    if route.route_type == RouteType::HardwareOutput {
        return route.hw_output_name.clone().unwrap_or_else(|| {
            let first = route.hw_output_index.unwrap_or(0).saturating_mul(2) + 1;
            format!("Out {first}/{}", first + 1)
        });
    }
    let Some(far) = crate::routes::partner(route, mine) else {
        return "?".to_owned();
    };
    tracks
        .iter()
        .find(|t| t.guid == far)
        .map(|t| t.name.clone())
        .or_else(|| {
            (route.route_type == RouteType::Send)
                .then(|| route.dest_track_name.clone())
                .flatten()
        })
        .unwrap_or_else(|| far.to_owned())
}

/// The tracks `mine` can send to: every other track, in the song's order,
/// less the ones it already sends to (`pending` too: asked for, not yet
/// read back) and the ones that feed it — a track that sends to it, and
/// the tracks inside it, which a folder sums — since a send to one of
/// those feeds back. (A second send to the same track is the engine's to
/// make by action; the panel offers one.)
#[must_use]
pub fn send_targets<'a>(
    tracks: &'a [RouteTrack],
    mine: &str,
    wiring: &Wiring,
    pending: &std::collections::HashSet<String>,
) -> Vec<&'a RouteTrack> {
    let inside = |guid: &str| {
        // Walk up from `guid`: `mine` above it means `guid` is inside.
        let mut at = tracks.iter().find(|t| t.guid == guid);
        let mut steps = 0;
        while let Some(t) = at {
            match t.parent.as_deref() {
                Some(up) if up == mine => return true,
                Some(up) if steps < tracks.len() => {
                    at = tracks.iter().find(|p| p.guid == up);
                    steps += 1;
                }
                _ => return false,
            }
        }
        false
    };
    tracks
        .iter()
        .filter(|t| t.guid != mine && !pending.contains(&t.guid))
        .filter(|t| {
            !wiring
                .sends
                .iter()
                .any(|s| s.dest_track_guid.as_deref() == Some(t.guid.as_str()))
        })
        .filter(|t| {
            !wiring
                .receives
                .iter()
                .any(|r| crate::routes::partner(r, mine) == Some(t.guid.as_str()))
        })
        .filter(|t| !inside(&t.guid))
        .collect()
}

/// The edit that adds a send from `from` to `to`. A desktop's or an iPad's
/// runs `SEND_ADD`, which the engine refuses where the send would feed back
/// by a longer way round than [`send_targets`] can see; a page's list has
/// no `SEND_ADD`, so the page sends the window's own edit.
fn add_send(from: &str, to: &str) -> Edit {
    #[cfg(feature = "native")]
    {
        let args = serde_json::json!({ "from": from, "to": to });
        Edit::Run("SEND_ADD".to_owned(), args.to_string())
    }
    #[cfg(not(feature = "native"))]
    {
        Edit::AddSend(from.to_owned(), to.to_owned())
    }
}

/// A position as the slider takes it.
#[allow(
    clippy::cast_possible_truncation,
    reason = "a slider position is 0..1, well inside f32"
)]
const fn narrow(v: f64) -> f32 {
    v as f32
}

/// What the panel's controls hand to the engine: edits made in a handle's
/// closure (which must be `Send`), sent in order by the panel's own loop
/// through the window's applier, and when the last one was made.
#[derive(Clone, Default)]
struct Outbox(Arc<Mutex<(Vec<Edit>, Option<web_time::Instant>)>>);

impl Outbox {
    fn push(&self, edit: Edit) {
        let mut held = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        held.0.push(edit);
        held.1 = Some(web_time::Instant::now());
    }

    fn take(&self) -> Vec<Edit> {
        std::mem::take(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner).0)
    }

    /// The panel edited within [`OWN_EDIT`].
    fn recent(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .1
            .is_some_and(|at| at.elapsed() < OWN_EDIT)
    }
}

/// One track's routing, the whole close-up layer.
#[component]
pub fn RoutingCloseup(track: String) -> Element {
    let routes = use_hook(Routes::default);
    let outbox = use_hook(Outbox::default);
    let applier = use_hook(|| std::rc::Rc::new(crate::engine::Applier::start()));
    let mut pending = use_signal(std::collections::HashMap::<String, web_time::Instant>::new);
    let mut tick = use_signal(|| 0_u64);
    // Closed with edits still queued (a typed level's end among them):
    // sent, so no undo step is left open.
    {
        let (outbox, applier) = (outbox.clone(), std::rc::Rc::clone(&applier));
        use_drop(move || send_all(&outbox, &applier));
    }
    {
        let (routes, outbox, track) = (routes.clone(), outbox.clone(), track.clone());
        let applier = std::rc::Rc::clone(&applier);
        use_future(move || {
            let (routes, outbox, track) = (routes.clone(), outbox.clone(), track.clone());
            let applier = std::rc::Rc::clone(&applier);
            async move {
                routes.fetch(vec![track.clone()]);
                routes.fetch_tracks();
                let mut read_at = web_time::Instant::now();
                loop {
                    crate::pause(TICK).await;
                    send_all(&outbox, &applier);
                    if read_at.elapsed() >= REREAD && !outbox.recent() {
                        routes.reread(vec![track.clone()]);
                        routes.fetch_tracks();
                        read_at = web_time::Instant::now();
                    }
                    if routes.take_fresh() {
                        tick += 1;
                    }
                }
            }
        });
    }
    let _ = tick();
    let Some(wiring) = routes.get(&track) else {
        return rsx! {
            div { style: "padding:24px; color:#9ca3af; font-size:14px;", "Reading the routing…" }
        };
    };
    let tracks = routes.tracks().unwrap_or_default();
    let section = |title: &'static str, kind: RouteType, empty: &'static str| {
        let list = wiring.list(kind);
        let rows = list.iter().map(|route| {
            let at = At::new(kind, route.index);
            let name = route_name(route, &track, &tracks);
            rsx! {
                RouteRow {
                    key: "{kind:?}-{route.index}",
                    track: track.clone(),
                    at,
                    name,
                    muted: route.muted,
                    mode: route.send_mode,
                    routes: RoutesProp(routes.clone()),
                    outbox: OutboxProp(outbox.clone()),
                    tick,
                }
            }
        });
        rsx! {
            div {
                style: "display:flex; flex-direction:column; gap:8px;",
                div { style: "{HEADING}", "{title}" }
                if list.is_empty() {
                    div { style: "font-size:12px; color:#6b7280;", "{empty}" }
                }
                {rows}
            }
        }
    };
    // A send asked for within PENDING is not offered again; past it, the
    // reads that followed say whether it was made.
    let asked: std::collections::HashSet<String> = pending
        .peek()
        .iter()
        .filter(|(_, at)| at.elapsed() < PENDING)
        .map(|(guid, _)| guid.clone())
        .collect();
    let targets: Vec<(String, String)> = send_targets(&tracks, &track, &wiring, &asked)
        .into_iter()
        .map(|t| (t.guid.clone(), t.name.clone()))
        .collect();
    let add = {
        let (outbox, track) = (outbox.clone(), track.clone());
        targets.into_iter().map(move |(guid, name)| {
            let (outbox, track) = (outbox.clone(), track.clone());
            rsx! {
                button {
                    key: "{guid}",
                    style: "{BUTTON}",
                    onclick: move |_| {
                        // Offered once: taken off the list until the read
                        // that shows the send lands.
                        let fresh = pending
                            .peek()
                            .get(&guid)
                            .is_none_or(|at| at.elapsed() >= PENDING);
                        if fresh {
                            pending.write().insert(guid.clone(), web_time::Instant::now());
                            outbox.push(add_send(&track, &guid));
                        }
                    },
                    "+ {name}"
                }
            }
        })
    };
    rsx! {
        fts_audio_ui::drag::DragProvider { fill: true,
            div {
                style: "width:100%; height:100%; display:flex; flex-direction:column; gap:18px; \
                        padding:16px; overflow-y:scroll; box-sizing:border-box;",
                {section("Sends", RouteType::Send, "This track sends nowhere.")}
                {section("Receives", RouteType::Receive, "Nothing sends to this track.")}
                {section("Hardware outputs", RouteType::HardwareOutput, "No hardware outputs.")}
                div {
                    style: "display:flex; flex-direction:column; gap:8px;",
                    div { style: "{HEADING}", "Send to" }
                    div { style: "display:flex; flex-direction:row; flex-wrap:wrap; gap:6px;", {add} }
                }
            }
        }
    }
}

/// Send what the panel's controls made, in order.
fn send_all(outbox: &Outbox, applier: &Option<crate::engine::Applier>) {
    let edits = outbox.take();
    if let Some(applier) = applier {
        for edit in edits {
            applier.send(edit);
        }
    }
}

/// [`Routes`] as a prop: the same cache, never compared by value.
#[derive(Clone)]
struct RoutesProp(Routes);

impl PartialEq for RoutesProp {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// [`Outbox`] as a prop: the same one.
#[derive(Clone)]
struct OutboxProp(Outbox);

impl PartialEq for OutboxProp {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0.0, &other.0.0)
    }
}

/// One route: its name, level, pan, mute, a send's tap, and remove.
#[component]
fn RouteRow(
    track: String,
    at: At,
    name: String,
    muted: bool,
    mode: SendMode,
    routes: RoutesProp,
    outbox: OutboxProp,
    tick: Signal<u64>,
) -> Element {
    let (routes, outbox) = (routes.0, outbox.0);
    let level = handle(&routes, &outbox, &track, at, Slider::Level);
    let pan = handle(&routes, &outbox, &track, at, Slider::Pan);
    let mute = {
        let (routes, outbox, track) = (routes.clone(), outbox.clone(), track.clone());
        move |_| {
            routes.predict(&track, at, |r| r.muted = !muted);
            outbox.push(Edit::SetRouteMute(track.clone(), at, !muted));
            tick += 1;
        }
    };
    let tap = {
        let (routes, outbox, track) = (routes.clone(), outbox.clone(), track.clone());
        move |_| {
            let next = next_tap(mode);
            routes.predict(&track, at, |r| r.send_mode = next);
            outbox.push(Edit::SetSendMode(track.clone(), at.index, next));
            tick += 1;
        }
    };
    let remove = {
        let (routes, outbox, track) = (routes.clone(), outbox.clone(), track.clone());
        move |_| {
            outbox.push(Edit::RemoveRoute(track.clone(), at));
            // Every route after it renumbers: shown so now, and the next
            // read settles both ends.
            routes.predict_removed(&track, at);
            tick += 1;
        }
    };
    let mute_look = if muted {
        "border-color:#f59e0b; color:#f59e0b;"
    } else {
        ""
    };
    rsx! {
        div {
            style: "display:flex; flex-direction:row; flex-wrap:wrap; align-items:center; gap:8px 12px; \
                    padding:10px 12px; border-radius:8px; background:#17181b; border:1px solid #2a2c31;",
            div {
                style: "flex:none; width:140px; font-size:13px; font-weight:600; color:#e5e7eb; \
                        white-space:nowrap; overflow:hidden;",
                "{name}"
            }
            div { style: "flex:1; min-width:180px;", HSlider { handle: level, height: 26.0 } }
            div { style: "flex:none; width:140px;", HSlider { handle: pan, height: 26.0 } }
            button { style: "{BUTTON} {mute_look}", onclick: mute, if muted { "Muted" } else { "Mute" } }
            if at.kind == RouteType::Send {
                button { style: "{BUTTON}", onclick: tap, "{tap_word(mode)}" }
            }
            button { style: "{BUTTON}", onclick: remove, "Remove" }
        }
    }
}

/// Which of a route's sliders.
#[derive(Clone, Copy)]
enum Slider {
    Level,
    Pan,
}

/// A route's level or pan as a slider's handle: read from the routes held,
/// predicted into them as it moves, each move an edit to the engine, a
/// gesture one undo step.
fn handle(routes: &Routes, outbox: &Outbox, track: &str, at: At, which: Slider) -> ParamHandle {
    let read = {
        let (routes, track) = (routes.clone(), track.to_owned());
        move || {
            routes
                .get(&track)
                .and_then(|w| w.at(at).cloned())
                .map(|r| (r.volume, r.pan))
        }
    };
    let (r1, r2) = (read.clone(), read);
    let begin = {
        let outbox = outbox.clone();
        move || outbox.push(Edit::BeginUndo(STEP.to_owned()))
    };
    let end = {
        let outbox = outbox.clone();
        move || outbox.push(Edit::EndUndo(STEP.to_owned()))
    };
    let set = {
        let (routes, outbox, track) = (routes.clone(), outbox.clone(), track.to_owned());
        move |position: f32| match which {
            Slider::Level => {
                let gain = level_gain(position);
                routes.predict(&track, at, |r| r.volume = gain);
                outbox.push(Edit::SetRouteVolume(track.clone(), at, gain));
            }
            Slider::Pan => {
                let pan = pan_at(position);
                routes.predict(&track, at, |r| r.pan = pan);
                outbox.push(Edit::SetRoutePan(track.clone(), at, pan));
            }
        }
    };
    let handle = ParamHandle::new(
        move || {
            r1().map_or(0.0, |(gain, pan)| match which {
                Slider::Level => level_position(gain),
                Slider::Pan => pan_position(pan),
            })
        },
        begin,
        set,
        end,
        move || {
            r2().map_or_else(String::new, |(gain, pan)| match which {
                Slider::Level => level_text(gain),
                Slider::Pan => pan_text(pan),
            })
        },
        move || {
            match which {
                Slider::Level => "Level",
                Slider::Pan => "Pan",
            }
            .to_owned()
        },
        move |text| match which {
            Slider::Level => parse_level(text),
            Slider::Pan => parse_pan(text),
        },
    );
    match which {
        Slider::Level => handle.with_default(level_position(1.0)).with_unit("dB"),
        Slider::Pan => handle.with_default(pan_position(0.0)).with_bipolar(true),
    }
}

const HEADING: &str = "font-size:11px; font-weight:650; letter-spacing:0.06em; \
     text-transform:uppercase; color:#9ca3af;";

const BUTTON: &str = "height:28px; padding:0 10px; border-radius:6px; border:1px solid #2a2c31; \
     background:#1c1e22; color:#e5e7eb; font-family:inherit; font-size:12px; \
     display:flex; align-items:center; justify-content:center; white-space:nowrap;";

#[cfg(test)]
mod tests {
    use super::*;

    fn route(kind: RouteType, index: u32, from: &str, to: Option<&str>) -> TrackRoute {
        TrackRoute {
            index,
            route_type: kind,
            source_track_guid: from.to_owned(),
            dest_track_guid: to.map(str::to_owned),
            dest_track_name: None,
            hw_output_index: None,
            hw_output_name: None,
            volume: 1.0,
            pan: 0.0,
            muted: false,
            mono: false,
            phase_inverted: false,
            send_mode: SendMode::PostFader,
            automation_mode: daw_proto::primitives::AutomationMode::default(),
            source_channels: daw_proto::routing::ChannelMapping::default(),
            dest_channels: daw_proto::routing::ChannelMapping::default(),
            midi_channel_mapping: None,
        }
    }

    #[test]
    fn unity_sits_where_zero_db_is_and_comes_back() {
        let at = level_position(1.0);
        let expected = (0.0 - FLOOR_DB) / (CEILING_DB - FLOOR_DB);
        assert!((f64::from(at) - expected).abs() < 1e-6);
        assert!((level_gain(at) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_bottom_of_the_slider_is_silence() {
        assert_eq!(level_position(0.0), 0.0);
        assert_eq!(level_gain(0.0), 0.0);
        assert_eq!(level_text(0.0), "-inf dB");
        assert_eq!(parse_level("-inf"), Some(0.0));
    }

    #[test]
    fn a_typed_level_lands_where_it_says() {
        let at = parse_level("-6 dB").unwrap_or_default();
        assert_eq!(level_text(level_gain(at)), "-6.0 dB");
        assert_eq!(parse_level("loud"), None);
    }

    #[test]
    fn pans_read_as_a_mixer_writes_them() {
        assert_eq!(pan_text(0.0), "C");
        assert_eq!(pan_text(-0.3), "L30");
        assert_eq!(pan_text(1.0), "R100");
        for text in ["C", "L30", "R100"] {
            let at = parse_pan(text).unwrap_or_default();
            assert_eq!(pan_text(pan_at(at)), text);
        }
    }

    #[test]
    fn the_tap_goes_round() {
        let mut mode = SendMode::PostFader;
        for _ in 0..3 {
            mode = next_tap(mode);
        }
        assert_eq!(mode, SendMode::PostFader);
        assert_eq!(tap_word(next_tap(SendMode::PostFader)), "Pre-fader");
    }

    fn track(guid: &str, name: &str, parent: Option<&str>) -> RouteTrack {
        RouteTrack {
            guid: guid.to_owned(),
            name: name.to_owned(),
            parent: parent.map(str::to_owned),
        }
    }

    #[test]
    fn a_route_is_named_for_its_far_end() {
        let tracks = vec![track("a", "Vox", None), track("b", "Verb", None)];
        let send = route(RouteType::Send, 0, "a", Some("b"));
        assert_eq!(route_name(&send, "a", &tracks), "Verb");
        let receive = route(RouteType::Receive, 0, "a", Some("b"));
        assert_eq!(route_name(&receive, "b", &tracks), "Vox");
        let mut out = route(RouteType::HardwareOutput, 0, "a", None);
        out.hw_output_index = Some(1);
        assert_eq!(route_name(&out, "a", &tracks), "Out 3/4");
    }

    fn offered(tracks: &[RouteTrack], mine: &str, wiring: &Wiring) -> Vec<String> {
        send_targets(tracks, mine, wiring, &std::collections::HashSet::new())
            .into_iter()
            .map(|t| t.name.clone())
            .collect()
    }

    #[test]
    fn a_send_is_offered_to_every_other_track_not_yet_sent_to() {
        let tracks = vec![
            track("a", "Vox", None),
            track("b", "Verb", None),
            track("c", "Delay", None),
        ];
        let wiring = Wiring {
            sends: vec![route(RouteType::Send, 0, "a", Some("b"))],
            ..Wiring::default()
        };
        assert_eq!(offered(&tracks, "a", &wiring), ["Delay"]);
        let pending = std::iter::once("c".to_owned()).collect();
        assert!(send_targets(&tracks, "a", &wiring, &pending).is_empty());
    }

    #[test]
    fn no_send_is_offered_to_what_feeds_the_track() {
        // Drums holds Kick, which holds In; Verb sends to Drums.
        let tracks = vec![
            track("d", "Drums", None),
            track("k", "Kick", Some("d")),
            track("i", "In", Some("k")),
            track("v", "Verb", None),
            track("o", "OH", None),
        ];
        let wiring = Wiring {
            receives: vec![route(RouteType::Receive, 0, "v", Some("d"))],
            ..Wiring::default()
        };
        assert_eq!(offered(&tracks, "d", &wiring), ["OH"]);
    }
}
