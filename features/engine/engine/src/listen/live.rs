//! Live listening: a point of a track measured while the song plays (the
//! transport looping a range, say) — `LISTEN_LIVE` opens a tap on it,
//! `LISTEN_LIVE_READ` measures its last few seconds.
//!
//! The audio thread only copies each block of the point into the tap's
//! ring (daw's `live_tap`: preallocated, lock-free). A thread of the
//! tap's own drains the ring every [`DRAIN_MS`] into a rolling buffer of
//! its last `keep_s` seconds; a read measures from that buffer, on the
//! caller's thread. Nothing of the analysis is on the audio thread.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use daw_standalone::audio_engine::render::live_tap::LiveTap;
use daw_standalone::audio_engine::render::offline::{Probe, ProbeAt};
use daw_standalone::sync::Standalone;
use serde_json::{Value, json};

use super::measure;

/// How often a tap's ring is drained, ms.
pub const DRAIN_MS: u64 = 50;
/// How much a tap's ring holds, seconds: what the drain may fall behind by.
const RING_S: f64 = 2.0;

/// What a tap has heard: its last `keep` frames, interleaved stereo.
#[derive(Default)]
struct Rolling {
    samples: VecDeque<f32>,
    keep: usize,
    /// Frames heard since it opened.
    heard: u64,
    /// When it last heard anything.
    last: Option<web_time::Instant>,
    dropped: u64,
}

impl Rolling {
    fn take(&mut self, block: &[f32]) {
        if block.is_empty() {
            return;
        }
        self.samples.extend(block.iter().copied());
        let over = self.samples.len().saturating_sub(self.keep * 2);
        self.samples.drain(..over);
        self.heard += (block.len() / 2) as u64;
        self.last = Some(web_time::Instant::now());
    }

    /// The last `frames` frames (fewer if it has not heard that much).
    fn last_frames(&self, frames: usize) -> Vec<f32> {
        let n = (frames * 2).min(self.samples.len());
        self.samples
            .iter()
            .skip(self.samples.len() - n)
            .copied()
            .collect()
    }
}

/// One open tap.
struct Live {
    path: String,
    at: String,
    slot: usize,
    rate: u32,
    rolling: Arc<Mutex<Rolling>>,
    stop: Arc<AtomicBool>,
    drainer: Option<std::thread::JoinHandle<()>>,
}

/// The open taps, by track (guid) and point.
fn lives() -> &'static Mutex<HashMap<(String, Probe), Live>> {
    static LIVES: std::sync::OnceLock<Mutex<HashMap<(String, Probe), Live>>> =
        std::sync::OnceLock::new();
    LIVES.get_or_init(Mutex::default)
}

/// Drain `tap` into `rolling` every [`DRAIN_MS`] until `stop`.
fn drain_until(mut tap: LiveTap, rolling: &Mutex<Rolling>, stop: &AtomicBool) {
    let mut block = Vec::new();
    while !stop.load(Ordering::Acquire) {
        block.clear();
        tap.drain(&mut block);
        {
            let mut r = rolling
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            r.take(&block);
            r.dropped = tap.dropped();
        }
        std::thread::sleep(std::time::Duration::from_millis(DRAIN_MS));
    }
}

/// What a point is called, for the answer.
pub struct Named<'a> {
    pub path: &'a str,
    pub at: &'a str,
}

/// Open a tap on `point` of song `song` (`named` how it is said), keeping
/// its last `keep_s` seconds. Already open: as it is.
///
/// # Errors
/// Every tap is taken, or its drainer would not start.
pub fn open(
    daw: &Standalone,
    song: &str,
    point: &ProbeAt,
    named: &Named<'_>,
    keep_s: f64,
) -> Result<Value, String> {
    let mut lives = lives()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let key = (point.track.clone(), point.at);
    if let Some(l) = lives.get(&key) {
        return Ok(state(l));
    }
    let rate = super::render::RATE;
    let tap = daw
        .live_taps()
        .open(song, point.clone(), (RING_S * f64::from(rate)) as usize)?;
    let slot = tap.slot;
    let rolling = Arc::new(Mutex::new(Rolling {
        keep: (keep_s.clamp(1.0, 120.0) * f64::from(rate)) as usize,
        ..Rolling::default()
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let (r, s) = (rolling.clone(), stop.clone());
    let drainer = std::thread::Builder::new()
        .name("listen-live".into())
        .spawn(move || drain_until(tap, &r, &s))
        .map_err(|e| {
            daw.live_taps().close(slot);
            format!("the tap's drainer would not start: {e}")
        })?;
    let live = Live {
        path: named.path.to_owned(),
        at: named.at.to_owned(),
        slot,
        rate,
        rolling,
        stop,
        drainer: Some(drainer),
    };
    let out = state(&live);
    lives.insert(key, live);
    Ok(out)
}

/// Close the tap on `point`; whether one was open.
pub fn close(daw: &Standalone, point: &ProbeAt) -> bool {
    let live = lives()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&(point.track.clone(), point.at));
    let Some(mut live) = live else { return false };
    live.stop.store(true, Ordering::Release);
    daw.live_taps().close(live.slot);
    if let Some(d) = live.drainer.take() {
        let _ = d.join();
    }
    true
}

/// A tap's state, as answered.
fn state(l: &Live) -> Value {
    let r = l
        .rolling
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    json!({
        "track": l.path,
        "at": l.at,
        "on": true,
        "keep_s": r.keep as f64 / f64::from(l.rate),
        "heard_s": r.heard as f64 / f64::from(l.rate),
        "receiving": r.last.is_some_and(|t| t.elapsed().as_secs_f64() < 0.5),
        "dropped_blocks": r.dropped,
    })
}

/// Every open tap's last `seconds` measured (those on `only`, when given,
/// by track guid).
#[must_use]
pub fn read(only: Option<&ProbeAt>, seconds: f64) -> Vec<Value> {
    let lives = lives()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    lives
        .iter()
        .filter(|(k, _)| only.is_none_or(|p| k.0 == p.track && k.1 == p.at))
        .map(|(_, l)| {
            let last = {
                let r = l
                    .rolling
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                r.last_frames((seconds * f64::from(l.rate)) as usize)
            };
            let mut row = state(l);
            row["seconds"] = json!(last.len() as f64 / 2.0 / f64::from(l.rate));
            row["figures"] = if last.is_empty() {
                Value::Null
            } else {
                measure::figures(&last, l.rate)
            };
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rolling_buffer_keeps_its_last_seconds() {
        let mut r = Rolling {
            keep: 4,
            ..Rolling::default()
        };
        r.take(&[1.0, 1.0, 2.0, 2.0, 3.0, 3.0]);
        r.take(&[4.0, 4.0, 5.0, 5.0, 6.0, 6.0]);
        assert_eq!(r.heard, 6);
        assert_eq!(r.samples.len(), 8);
        assert_eq!(r.last_frames(2), vec![5.0, 5.0, 6.0, 6.0]);
        assert_eq!(r.last_frames(10).len(), 8);
    }
}
