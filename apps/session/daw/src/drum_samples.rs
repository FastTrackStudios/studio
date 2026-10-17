//! Drum samples under the recorded kit: signal's sampled kit, played from
//! the hits the triggers caught.
//!
//! Session's hit markers say where the kit's hits are (`crate::hits`,
//! found once from the triggers and mics); signal plays a sampled kit
//! from notes. This joins them, blended under the recording:
//!
//! - each piece's hits written as notes on its **Sample** track (the
//!   kit's General MIDI note for the piece, the velocity from the hit's
//!   level), and an `FTS Kit Mic` there playing every mic of that piece,
//!   split: its **close** mics on the track's own output, its
//!   **overhead** mics on 3/4 and its **room** mics on 5/6;
//! - the Sample track sending its 3/4 into the recorded overheads and its
//!   5/6 into the rooms, so the sample's ambience is processed and faded
//!   with theirs — sends, not tracks of their own;
//! - the Sample track's polarity set to pull with the piece's close mic
//!   as the mix plays it ([`measure`], [`crate::sample_align`]), so the
//!   blend adds to the recorded low end instead of cancelling it.
//!
//! One instrument plays all of a piece's mics from the same notes, so each
//! hit is the same sample on every one. The notes are the hits as MIDI
//! ([`crate::hits_midi`]), kept where the hits play through every edit —
//! and an edit of them is an edit of the hits.

use daw::service::{FxChainContext, ProjectContext, TrackRef, Tracks};
use daw_proto::Routing as _;

use crate::hits_midi::{Note, hit_notes, note_for, sample_track};

/// What setting the samples up did, or would.
#[derive(Debug, Default)]
pub struct Report {
    /// One line a track.
    pub tracks: Vec<String>,
    pub notes: usize,
}

/// Set the samples up on the kit; `dry`, only say.
///
/// # Errors
///
/// No song open, or the engine refusing a write.
pub fn set_up(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    preset: &std::path::Path,
    dry: bool,
) -> Result<Report, String> {
    let hits = hit_notes(&crate::hits::playback(daw, project), &|t| t);
    if hits.is_empty() {
        return Err("no hit markers: find the hits first (HITS_DETECT)".to_owned());
    }
    let tracks = Tracks::all(daw, project.clone());
    let triggers = crate::drum_reference::pieces_of(&tracks);
    let overheads = bus_track(&tracks, &["ohs", "overheads", "oh"]).cloned();
    let rooms = bus_track(&tracks, &["rooms", "room"]).cloned();
    let mut report = Report::default();
    for (name, _) in &triggers {
        let Some(note) = note_for(name) else {
            report
                .tracks
                .push(format!("{name}: no note in the kit for it"));
            continue;
        };
        let Some(piece) = signal_fx::kit::piece_for_note(preset, note) else {
            report
                .tracks
                .push(format!("{name}: the kit has nothing on note {note}"));
            continue;
        };
        let Some(sample) = triggers
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, trigger)| sample_track(&tracks, trigger))
        else {
            report
                .tracks
                .push(format!("{name}: no Sample track beside its Trig"));
            continue;
        };
        let notes: Vec<Note> = hits
            .iter()
            .filter(|h| h.pitch == note)
            .map(|h| (h.time, h.pitch, h.velocity))
            .collect();
        report.notes = report.notes.saturating_add(notes.len());
        report.tracks.push(format!(
            "{}: {} notes on {note} ({piece}), its close mics out, overheads on 3/4, rooms on 5/6",
            sample.name,
            notes.len(),
        ));
        if !dry {
            crate::hits_midi::write(daw, project, &sample.guid, &notes)?;
            let state = serde_json::json!({
                "preset": preset,
                "pieces": [piece],
                "mics": ["close", "overhead", "room"],
                "split": true,
            });
            kit_mic(daw, project, &sample.guid, &state)?;
            for (from, bus) in [(2, &overheads), (4, &rooms)] {
                if let Some(bus) = bus {
                    send_from(daw, project, &sample.guid, &bus.guid, from)?;
                }
            }
        }
    }
    // The separate ambience tracks an earlier set-up made: their mics are
    // now the Sample tracks' own, sent from 3/4 and 5/6.
    for t in &tracks {
        if ["oh sample", "room sample"].contains(&t.name.trim().to_ascii_lowercase().as_str()) {
            report
                .tracks
                .push(format!("{}: removed — sent from each Sample track", t.name));
            if !dry {
                Tracks::remove(daw, project.clone(), TrackRef::Guid(t.guid.clone()))
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    for (bus, pair, beside) in [
        (&overheads, "3/4", &["ohs", "overheads", "oh"][..]),
        (&rooms, "5/6", &["rooms", "room"][..]),
    ] {
        report.tracks.push(bus.as_ref().map_or_else(
            || format!("no {} track to send {pair} into", beside.join(" / ")),
            |bus| format!("each Sample's {pair} sent into {}", bus.name),
        ));
    }
    if !dry {
        let measured = measure(daw, project, preset);
        report.tracks.extend(set_polarity(daw, project, &measured)?);
        crate::studio::request_resync();
        crate::actions::list().changed("drums.samples", "the kit's samples were set up");
    }
    Ok(report)
}

/// How many of a piece's hits its sample is measured against.
const MEASURED_HITS: usize = 64;
/// How firmly a sample's note is played to hear its attack.
const MEASURED_VELOCITY: u8 = 100;

/// Fewer hits than this, or fewer of them agreeing than [`SURE`], and a
/// sample's polarity is left as it is.
const SURE_HITS: usize = 8;
/// The share of the hits that must agree on a sample's polarity.
const SURE: f64 = 0.65;

/// One piece's sample against its close mic.
#[derive(Debug)]
pub struct Measured {
    pub piece: String,
    /// The kit's piece that plays it (`kick`, `rtom-a`).
    pub sample: String,
    /// Its Sample track.
    pub track: Option<daw_proto::Track>,
    /// `None`: no recording or no sound to measure.
    pub alignment: Option<crate::sample_align::Alignment>,
    /// Whether the Sample track must be flipped to pull with the close
    /// mic as the mix plays it (the mic track's own flip counted); `None`
    /// when the measure is too unsure to say.
    pub flip: Option<bool>,
}

/// How each piece's sample in `preset` sits against its close mic
/// ([`crate::sample_align`]).
#[must_use]
pub fn measure(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    preset: &std::path::Path,
) -> Vec<Measured> {
    let tracks = Tracks::all(daw, project.clone());
    let flipped = |guid: &str| tracks.iter().any(|t| t.guid == guid && t.phase_inverted);
    crate::drum_reference::pieces_of(&tracks)
        .into_iter()
        .filter_map(|(name, trigger)| {
            let note = note_for(&name)?;
            let sample = signal_fx::kit::piece_for_note(preset, note)?;
            let heard = crate::hits::heard_hits(daw, project, &name, MEASURED_HITS);
            let alignment = heard.as_ref().and_then(|heard| {
                let sound = hit_sound(preset, &sample, note, heard.rate);
                crate::sample_align::align(&heard.audio, &heard.at, &sound, heard.rate)
            });
            let flip = heard.zip(alignment).and_then(|(heard, a)| {
                (a.hits >= SURE_HITS && a.consistent >= SURE)
                    .then(|| a.inverted != flipped(&heard.mic))
            });
            Some(Measured {
                piece: name,
                sample,
                track: sample_track(&tracks, &trigger).cloned(),
                alignment,
                flip,
            })
        })
        .collect()
}

/// Each measured Sample track's polarity set as [`Measured::flip`] says;
/// one line a piece.
///
/// # Errors
///
/// The engine refusing a write.
pub fn set_polarity(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    measured: &[Measured],
) -> Result<Vec<String>, String> {
    measured
        .iter()
        .map(|m| {
            let Some(track) = &m.track else {
                return Ok(format!("{}: no Sample track", m.piece));
            };
            let Some(flip) = m.flip else {
                return Ok(format!(
                    "{} ({}): too unsure to say, left as it is",
                    m.piece, track.name
                ));
            };
            if flip != track.phase_inverted {
                daw.set_phase_inverted(project.clone(), TrackRef::Guid(track.guid.clone()), flip)
                    .map_err(|e| e.to_string())?;
            }
            Ok(format!(
                "{} ({}): {}",
                m.piece,
                track.name,
                if flip { "flipped" } else { "as it plays" }
            ))
        })
        .collect()
}

/// `piece` of the kit `preset` struck on `note`, its close mics as a Kit
/// Mic plays them, 100 ms at `rate`.
fn hit_sound(preset: &std::path::Path, piece: &str, note: u8, rate: f64) -> Vec<f32> {
    use daw_standalone::plugin::PluginInstance as _;
    let state = serde_json::json!({ "preset": preset, "pieces": [piece], "mics": ["close"] });
    let mut kit = signal_fx::kit::KitMic::new(rate);
    if kit.load_state(state.to_string().as_bytes()).is_err() || kit.prepare(rate, 512).is_err() {
        return Vec::new();
    }
    let frames = num_traits::ToPrimitive::to_usize(&(rate * 0.1)).unwrap_or(4_800);
    kit.hit_sound(note, MEASURED_VELOCITY, frames)
}

/// The recorded mics' track a sample bus sends into: the first of
/// `names` (in order of preference) that a track of the kit is called —
/// never a reverb's return of the same name (`Kick/Verb/Room`).
fn bus_track<'a>(tracks: &'a [daw_proto::Track], names: &[&str]) -> Option<&'a daw_proto::Track> {
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let in_a_verb = |t: &daw_proto::Track| {
        crate::gain_stage::path_of(t, &by_guid)
            .iter()
            .any(|p| p.trim().eq_ignore_ascii_case("verb"))
    };
    names.iter().find_map(|name| {
        tracks
            .iter()
            .find(|t| !t.is_folder && t.name.trim().eq_ignore_ascii_case(name) && !in_a_verb(t))
    })
}

/// A send from `track`'s channels `from`+1/`from`+2 (2: its 3/4, 4: its
/// 5/6) into `bus`, post-fader — the one there, or a new one.
fn send_from(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    bus: &str,
    from: u32,
) -> Result<(), String> {
    let source = TrackRef::Guid(track.to_owned());
    let there = daw
        .sends(project.clone(), source.clone())
        .into_iter()
        .find(|s| {
            s.dest_track_guid.as_deref() == Some(bus) && s.source_channels.start_channel == from
        });
    let index = match there {
        Some(send) => send.index,
        None => daw
            .add_send(
                project.clone(),
                source.clone(),
                TrackRef::Guid(bus.to_owned()),
            )
            .ok_or("the send would not add")?,
    };
    daw.set_source_channels(
        project.clone(),
        daw_proto::RouteLocation {
            track: source,
            route_type: daw_proto::RouteType::Send,
            route: daw_proto::RouteRef::Index(index),
        },
        from,
        2,
    )
    .map_err(|e| e.to_string())
}

/// An `FTS Kit Mic` on `track` (the one there, or a new one) playing what
/// `state` says.
fn kit_mic(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    state: &serde_json::Value,
) -> Result<(), String> {
    let chain = FxChainContext::Track(track.to_owned());
    let fx = match daw::service::Effects::list(daw, project.clone(), chain.clone())
        .into_iter()
        .find(|f| f.name.eq_ignore_ascii_case(signal_fx::KIT_MIC))
    {
        Some(f) => f.guid,
        None => daw::service::Effects::add(daw, project.clone(), chain, signal_fx::KIT_MIC)
            .ok_or("the kit mic would not add")?,
    };
    daw.apply_plugin_state(&fx, state.to_string().as_bytes())
        .map_err(|e| format!("the kit mic would not load: {e}"))
}
