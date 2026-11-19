//! A song's reference: its whole mix but the guide, bounced to one stereo
//! stream.
//!
//! What a phone plays by default in a shared set (the web page's
//! reference mode): one proxy of a few megabytes instead of every stem.
//! The click, count and cues are left out because they play on their own
//! (the guide instrument), where each person can turn them up or down;
//! the Keyflow folder carries no audio worth hearing twice.
//!
//! The bounce is written beside the song's media the way its stems are,
//! so it syncs and streams with it:
//!
//! - `Media/Reference.wav` — 48 kHz, 32-bit float;
//! - `Media/Proxies/Reference.ogg` and its page index — what streams;
//! - `Media/Proxies/Reference Preview.ogg` and its index — the same mix at
//!   a quarter of the size (mono, 24 kHz), what a page plays first, until
//!   the reference proper has arrived where it is playing;
//! - `Media/Peaks/Reference.wav.sessionpeaks` — its waveform.
//!
//! The song itself is not changed: the reference is not a track of it,
//! so nothing about it is shared, saved or mixed by accident. A song that
//! already plays a `Media/Reference.wav` of its own (a multitrack pack's
//! reference mix) keeps it: that one is proxied, not rendered over.
//!
//! What the reference leaves out ([`left_out`]) is also what a page in
//! reference mode lets you change without the stems: those tracks play
//! live either way.

use std::collections::HashSet;

use daw::service::{ProjectContext, Tracks};
use daw::standalone::Standalone;
use daw::standalone::audio_engine::streamed::{CHUNK, Streamed};

/// The reference proxy's file stem, lower-case, as a streamed song keys
/// its proxies (`Media/Proxies/Reference.ogg`).
pub const STEM: &str = "reference";
/// The preview proxy's file stem, lower-case.
pub const PREVIEW_STEM: &str = "reference preview";
/// The reference's sample rate: the web page's output rate, so its frames
/// line up with the playhead without resampling.
pub const RATE: u32 = 48_000;
/// The reference's name in the song's `Media` folder.
pub const NAME: &str = "Reference.wav";
/// The folders whose tracks the reference leaves out.
const LEFT_OUT: [&str; 2] = ["Guide", "Keyflow"];

fn ctx(project: &str) -> ProjectContext {
    ProjectContext::Project(project.to_owned())
}

/// The tracks of `project` the reference leaves out: the folders it names
/// and everything in them, to any depth.
#[must_use]
pub fn left_out(daw: &Standalone, project: &str) -> HashSet<String> {
    let tracks = Tracks::all(daw, ctx(project));
    let mut out: HashSet<String> = tracks
        .iter()
        .filter(|t| LEFT_OUT.contains(&t.name.as_str()))
        .map(|t| t.guid.clone())
        .collect();
    loop {
        let before = out.len();
        for track in &tracks {
            if track.parent_guid.as_ref().is_some_and(|p| out.contains(p)) {
                out.insert(track.guid.clone());
            }
        }
        if out.len() == before {
            return out;
        }
    }
}

/// The top-level folder a song's own stereo reference recording is in.
pub const REFERENCE_FOLDER: &str = "Reference";

/// The organizer's folder of bounces and rehearsals to listen against
/// (`dynamic_template::apply::REFERENCES_FOLDER`), muted on purpose.
const REFERENCES: &str = "REFERENCES";

/// Whether top-level track `name` is neither an instrument folder nor the
/// reference: the guide and the buses, which both ways of hearing leave,
/// and the organizer's `REFERENCES` (the stems do not turn them on).
#[must_use]
pub fn left_alone(name: &str) -> bool {
    let n = name.trim().to_ascii_uppercase();
    n == "GUIDE" || n.ends_with(" BUS") || n == "KEYFLOW" || n == REFERENCES
}

/// Hear song `project` by its own reference recording (`reference`: its
/// top-level `Reference` folder on, every instrument folder muted) or by
/// its stems (the instrument folders on, the reference muted). The guide
/// and the buses are left as they are. A song with no `Reference` folder
/// stays on its stems. Whether it has one.
///
/// A set is saved in whichever of the two it was last heard by, so what
/// opens it elsewhere (a link's engine, a bounce) says which it wants.
///
/// # Errors
///
/// A track's mute could not be set.
pub fn hear(daw: &Standalone, project: &ProjectContext, reference: bool) -> Result<bool, String> {
    let top: Vec<_> = Tracks::all(daw, project.clone())
        .into_iter()
        .filter(|t| t.parent_guid.is_none())
        .collect();
    let has = top
        .iter()
        .any(|t| t.name.trim().eq_ignore_ascii_case(REFERENCE_FOLDER));
    if reference && !has {
        // Nothing to hear it by: left on its stems.
        return Ok(false);
    }
    // A song that is only its reference (a recording, no stems yet) has
    // nothing else to play: it is heard by it either way.
    let has_parts = top
        .iter()
        .any(|t| !left_alone(&t.name) && !t.name.trim().eq_ignore_ascii_case(REFERENCE_FOLDER));
    let reference = reference || (has && !has_parts);
    for track in top.iter().filter(|t| !left_alone(&t.name)) {
        let is_reference = track.name.trim().eq_ignore_ascii_case(REFERENCE_FOLDER);
        let mute = is_reference != reference;
        if track.muted != mute {
            Tracks::set_muted(
                daw,
                project.clone(),
                daw::service::TrackRef::Guid(track.guid.clone()),
                mute,
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(has)
}

/// Add a song's reference into `buf` — interleaved, `channels` a frame, at
/// `rate` — from `at` seconds on, at the transport's `playrate`: the
/// reference proper where it has arrived at the playhead, else its
/// `preview`. Each is told that is where it is read, so both decode there.
/// Silent where neither has arrived. Allocation-free: it runs on the audio
/// thread (a device's callback, a page's audio loop).
pub fn mix(
    full: &Streamed,
    preview: Option<&Streamed>,
    at: f64,
    rate: f64,
    playrate: f64,
    buf: &mut [f32],
    channels: usize,
) {
    let channels = channels.max(1);
    let frames = buf.len() / channels;
    let here = |s: &Streamed| (at.max(0.0) * f64::from(s.sample_rate())) as u64;
    full.want(here(full));
    if let Some(preview) = preview {
        preview.want(here(preview));
    }
    let arrived = |s: &Streamed| {
        let chunk = |frame: u64| usize::try_from(frame).unwrap_or(usize::MAX) / CHUNK;
        let span = frames as f64 * f64::from(s.sample_rate()) / rate.max(1.0) * playrate;
        let last = here(s) + span as u64;
        s.resident(chunk(here(s))) && s.resident(chunk(last.min(s.frames().saturating_sub(1))))
    };
    let source = match preview {
        Some(preview) if !arrived(full) => preview,
        _ => full,
    };
    mix_in(source, at, rate, playrate, buf, channels);
}

/// Add `source` from `at` seconds into `buf` — read at its own rate between
/// samples, a mono one to both sides, and both sides folded into a mono
/// output.
fn mix_in(source: &Streamed, at: f64, rate: f64, playrate: f64, buf: &mut [f32], channels: usize) {
    let source_rate = f64::from(source.sample_rate().max(1));
    let step = source_rate / rate.max(1.0) * playrate;
    let right = usize::from(source.channels() > 1);
    let from = at.max(0.0) * source_rate;
    for (i, out) in buf.chunks_exact_mut(channels).enumerate() {
        let position = from + i as f64 * step;
        let frame = position as usize;
        let frac = (position - frame as f64) as f32;
        let read = |ch: usize| {
            let a = source.sample(frame, ch);
            a + (source.sample(frame + 1, ch) - a) * frac
        };
        let (l, r) = (read(0), read(right));
        match out {
            [mono] => *mono += (l + r) * 0.5,
            [left, right, ..] => {
                *left += l;
                *right += r;
            }
            [] => {}
        }
    }
}
