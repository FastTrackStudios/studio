//! `fts.daw` as its views see it: the contract between the `daw` engine
//! extension and any view of what it plays (Studio's arrangement and
//! mixer, or a replacement for either).
//!
//! - [`SERVICE`] and its methods ([`method`]), each answered by the
//!   engine's action `DAW_<METHOD>`.
//! - The answers' shapes ([`State`], [`Track`], [`Clip`], [`Peaks`]) and
//!   the requests' ([`Transport`], [`SetTrack`]), as JSON.
//! - The names of the values in the engine's shared table ([`names`]):
//!   meters it writes, faders the host's drags write.
//! - The laws both sides agree on: a fader's position to gain
//!   ([`fader`]), a peak to a meter's height ([`meter`]).
//! - Peaks: min/max pairs at [`PEAK_FRAMES`] frames a pair, named by the
//!   hash of their bytes ([`peaks`]).
//! - The bar and beat grid from the session's tempo data ([`grid`]).
//!
//! Light on purpose (serde, base64): a view links this and nothing of the
//! engine.

pub mod grid;
pub mod peaks;

use serde::{Deserialize, Serialize};

/// The service the engine provides.
pub const SERVICE: &str = "fts.daw";

/// The engine extension's id: the owner of `daw.track` lanes and
/// `daw.item` items, and whose shared value table views read.
pub const ENGINE: &str = "daw";

/// The lane kind of a track, and the item kind of a clip on one.
pub const TRACK_KIND: &str = "daw.track";
pub const ITEM_KIND: &str = "daw.item";

/// `fts.daw`'s methods.
pub mod method {
    /// [`super::State`]. Read.
    pub const STATE: &str = "state";
    /// `[`[`super::Track`]`]`. Read.
    pub const TRACKS: &str = "tracks";
    /// `[`[`super::Clip`]`]`. Read.
    pub const CLIPS: &str = "clips";
    /// [`super::Peaks`] for `{"hash": …}`. Read.
    pub const PEAKS: &str = "peaks";
    /// A [`super::Transport`]. Mutating.
    pub const TRANSPORT: &str = "transport";
    /// A [`super::SetTrack`]. Mutating.
    pub const SET: &str = "set";
    /// Opens `{"path": …}`: a session snapshot. Mutating.
    pub const OPEN: &str = "open";
}

/// What the engine is playing, and how far its data has moved.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// The open session's document id, if one is open.
    pub session: Option<String>,
    pub name: String,
    /// Moves on with every change to what the views read (tracks, clips,
    /// mixer state): a view re-reads when it moved.
    pub revision: u64,
    pub playing: bool,
    /// Seconds, when this was answered.
    pub position: f64,
    /// The song's length, seconds.
    pub duration: f64,
    pub sample_rate: u32,
    /// The shared value table's path, when the engine runs one.
    pub values: Option<String>,
    /// Why nothing plays, if it doesn't (no device, a load that failed).
    pub problem: Option<String>,
}

/// A track: a `daw.track` lane and its mixer state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Its lane's id in the session.
    pub lane: String,
    /// Its slot in the value table ([`names`]) and in the engine.
    pub index: u32,
    pub name: String,
    /// `0xRRGGBB`.
    pub color: Option<u32>,
    /// Folder depth (0 at the top).
    pub depth: u32,
    /// The fader's position, 0..1 ([`fader`]).
    pub volume: f64,
    /// -1 (left) .. 1 (right).
    pub pan: f64,
    pub mute: bool,
    pub solo: bool,
}

/// A clip: a `daw.item` with what drawing it needs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: String,
    pub lane: String,
    /// Seconds.
    pub start: f64,
    pub end: f64,
    pub name: String,
    /// `0xRRGGBB`.
    pub color: Option<u32>,
    pub mute: bool,
    /// Linear.
    pub gain: f64,
    /// Where in its source it starts, seconds.
    pub offset: f64,
    /// Its source's peaks, by hash ([`method::PEAKS`]), once computed.
    pub peaks: Option<String>,
}

/// What a transport request asks.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "do")]
pub enum Transport {
    Play,
    Pause,
    Stop,
    Toggle,
    Seek { seconds: f64 },
}

/// A track's field, set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetTrack {
    pub lane: String,
    pub field: Field,
    pub value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    /// The fader's position, 0..1.
    Volume,
    /// -1..1.
    Pan,
    /// 0 or 1.
    Mute,
    /// 0 or 1.
    Solo,
}

/// A source's peaks, by hash: what `peaks` answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Peaks {
    pub hash: String,
    /// The source's sample rate: a pair covers [`PEAK_FRAMES`] of its frames.
    pub sample_rate: u32,
    /// Base64 of the pairs ([`peaks::encode`]).
    pub data: String,
}

/// Frames a peak pair covers.
pub const PEAK_FRAMES: u32 = 512;

/// The names in the engine's shared table.
///
/// Three slots a track, so a table of 1,024 holds [`MAX_TRACKS`].
pub mod names {
    /// Tracks a table names.
    pub const MAX_TRACKS: u32 = 340;

    /// Track `n`'s meter, written by the engine: 0..1 ([`super::meter`]).
    #[must_use]
    pub fn peak(n: u32) -> String {
        format!("engine.daw.track.{n}.peak")
    }

    /// The master's meter.
    pub const MASTER_PEAK: &str = "engine.daw.master.peak";

    /// Track `n`'s fader while a drag holds it, written by the host:
    /// 0..1.
    #[must_use]
    pub fn volume(n: u32) -> String {
        format!("host.daw.track.{n}.volume")
    }

    /// Track `n`'s pan while a drag holds it, written by the host: -1..1.
    #[must_use]
    pub fn pan(n: u32) -> String {
        format!("host.daw.track.{n}.pan")
    }
}

/// A fader's position (0 at the bottom, 1 at the top) and its gain.
///
/// `gain = 4 · position⁴`: silence at 0, unity at ≈0.707, +12 dB at the
/// top, and fine control around unity where a mix is made.
pub mod fader {
    /// Where unity gain sits.
    pub const UNITY: f64 = 0.707_106_781_186_547_6;

    /// The gain at `position`.
    #[must_use]
    pub fn gain(position: f64) -> f64 {
        let p = position.clamp(0.0, 1.0);
        4.0 * p * p * p * p
    }

    /// The position for `gain`.
    #[must_use]
    pub fn position(gain: f64) -> f64 {
        (gain.max(0.0) / 4.0).powf(0.25).clamp(0.0, 1.0)
    }

    /// `position` in dB, for a label.
    #[must_use]
    pub fn db(position: f64) -> f64 {
        let g = gain(position);
        if g <= 1e-9 {
            f64::NEG_INFINITY
        } else {
            20.0 * g.log10()
        }
    }
}

/// A peak's height on a meter: linear magnitude to 0..1, -60 dB at the
/// bottom, +6 dB at the top.
pub mod meter {
    pub const FLOOR_DB: f64 = -60.0;
    pub const TOP_DB: f64 = 6.0;

    #[must_use]
    pub fn height(linear: f64) -> f64 {
        if linear <= 1e-6 {
            return 0.0;
        }
        let db = 20.0 * linear.log10();
        ((db - FLOOR_DB) / (TOP_DB - FLOOR_DB)).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_is_where_the_law_says() {
        assert!((fader::gain(fader::UNITY) - 1.0).abs() < 1e-9);
        assert!((fader::db(1.0) - 12.041).abs() < 1e-3);
        for p in [0.0, 0.1, 0.5, fader::UNITY, 0.9, 1.0] {
            assert!((fader::position(fader::gain(p)) - p).abs() < 1e-9);
        }
    }

    #[test]
    fn a_meter_spans_sixty_six_db() {
        assert_eq!(meter::height(0.0), 0.0);
        assert!((meter::height(1.0) - 60.0 / 66.0).abs() < 1e-9);
        assert_eq!(meter::height(10.0), 1.0);
    }

    #[test]
    fn requests_read_as_json() {
        let t: Transport = serde_json::from_str(r#"{"do":"seek","seconds":4.5}"#).unwrap();
        assert_eq!(t, Transport::Seek { seconds: 4.5 });
        let s: SetTrack =
            serde_json::from_str(r#"{"lane":"t1","field":"volume","value":0.5}"#).unwrap();
        assert_eq!(s.field, Field::Volume);
    }

    #[test]
    fn three_slots_a_track_fit_one_table() {
        assert!(names::MAX_TRACKS * 3 + 1 <= 1024);
        assert!(names::peak(339).len() <= 64 && names::volume(339).len() <= 64);
    }
}
