//! A streamed song heard by its reference on this device — what a phone
//! joining a shared set plays by default, as the web page does
//! (`session_engine::web_engine`): one stereo stream of a few megabytes (the
//! song's mix but the guide, [`crate::reference`]) instead of every stem,
//! the guide playing live over it from the engine.
//!
//! The stems are attached all the same (their waveforms draw) and come in
//! behind the reference — the song on screen's once its reference is here,
//! the rest once every song's is — decoded but not heard; each song keeps
//! its reference until its stems have arrived and decoded where it is
//! (`session_engine::stream_set` hands over), so there is no gap and no
//! doubling. Loading the multitracks (`session_engine::reference_play::
//! load_multitracks`) brings every song's stems in at once; that, and the
//! flag it sets, stay with Session's stream set.
//!
//! The reference is mixed into the device's output by the audio engine's
//! post-render hook ([`renderer`], which the host installs:
//! `session_engine::open` on the song it makes current), for the song the
//! engine is playing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

use daw::standalone::audio_engine::AuxRenderer;
use daw::standalone::audio_engine::streamed::Streamed;

/// A song's reference: the reference proper, and its preview.
struct Heard {
    full: Streamed,
    preview: Option<Streamed>,
}

/// The songs heard by their reference, by project.
static HEARD: LazyLock<Mutex<HashMap<String, Heard>>> = LazyLock::new(Mutex::default);

/// Hear `project` by its reference: `full`, and its `preview` where that
/// has not arrived.
pub fn hear(project: &str, full: Streamed, preview: Option<Streamed>) {
    if let Ok(mut heard) = HEARD.lock() {
        heard.insert(project.to_owned(), Heard { full, preview });
    }
}

/// Hear `project` — a song opened from `folder` on this device — by its
/// reference, when it came down by its reference only
/// ([`crate::song_stream::REFERENCE_ONLY`]): its stems are not here, and
/// its reference is, read from disk as a streamed one is from the network.
/// Nothing for a song downloaded whole, or never downloaded.
#[cfg(not(target_arch = "wasm32"))]
pub fn hear_local(project: &str, folder: &std::path::Path) {
    let proxies = folder.join("Media").join("Proxies");
    if !proxies.join(crate::song_stream::REFERENCE_ONLY).exists() {
        return;
    }
    let file = proxies.join("Reference.ogg");
    match fts_sample::ogg_stream::OggStream::open_file(&file) {
        Ok(stream) => {
            let full = Streamed::new(stream.channels(), stream.sample_rate(), stream.frames());
            let feeder =
                daw::standalone::audio_engine::streamed::StreamFeeder::new(full.clone(), stream);
            daw::standalone::audio_engine::streamed::butler_adopt(feeder);
            hear(project, full, None);
        }
        Err(e) => {
            tracing::warn!(error = %e, reference.song = %project, "reference: a downloaded song's reference would not open; it plays its guide alone")
        }
    }
}

/// Hear `project` by its stems: its reference goes.
pub fn stop(project: &str) {
    if let Ok(mut heard) = HEARD.lock() {
        heard.remove(project);
    }
}

/// Whether any song is heard by its reference — what Settings offers the
/// multitracks for.
#[must_use]
pub fn any() -> bool {
    HEARD.lock().is_ok_and(|heard| !heard.is_empty())
}

/// The audio engine's post-render hook for `project`: its reference, mixed
/// into the output while the transport plays and the song is heard by it.
///
/// On the audio thread: the table is only `try_lock`ed (a block is skipped
/// the moment a song is added or taken off, never waited for), and the mix
/// allocates nothing.
#[must_use]
pub fn renderer(project: String) -> AuxRenderer {
    Box::new(move |buf, clock| {
        if !clock.playing {
            return;
        }
        let Ok(heard) = HEARD.try_lock() else {
            return;
        };
        let Some(song) = heard.get(project.as_str()) else {
            return;
        };
        // Until it is first heard, what the block held before the mix: the
        // difference is the reference's.
        let before: f32 = if HEARD_ONCE.load(Ordering::Relaxed) {
            0.0
        } else {
            buf.iter().map(|s| s.abs()).sum()
        };
        crate::reference::mix(
            &song.full,
            song.preview.as_ref(),
            clock.pos_seconds,
            clock.sample_rate,
            clock.playrate,
            buf,
            clock.channels,
        );
        if !HEARD_ONCE.load(Ordering::Relaxed) {
            let after: f32 = buf.iter().map(|s| s.abs()).sum();
            if (after - before).abs() > 1e-3 && !HEARD_ONCE.swap(true, Ordering::Relaxed) {
                // Once per process: the one line that says the reference
                // reached the output (it allocates, and only this once).
                tracing::info!(reference.song = %project, reference.at = clock.pos_seconds, "reference: first heard");
            }
        }
    })
}

/// Whether a reference has been heard yet — see [`renderer`].
static HEARD_ONCE: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
mod tests {
    use daw::standalone::audio_engine::AuxClock;
    use daw::standalone::audio_engine::streamed::{Decode, DecodeError, StreamFeeder, Streamed};

    use super::{any, hear, renderer, stop};

    /// A reference that is one level throughout, all of it here.
    struct Level {
        frames: u64,
        at: u64,
    }

    impl Decode for Level {
        fn seek(&mut self, frame: u64) -> Result<(), DecodeError> {
            self.at = frame;
            Ok(())
        }

        fn decode(&mut self, out: &mut Vec<f32>) -> Result<Option<(u64, usize)>, DecodeError> {
            if self.at >= self.frames {
                return Ok(None);
            }
            let start = self.at;
            let n = 1024.min(self.frames.saturating_sub(start));
            let samples = usize::try_from(n.saturating_mul(2)).unwrap_or(0);
            out.extend(std::iter::repeat_n(0.5, samples));
            self.at = self.at.saturating_add(n);
            Ok(Some((start, usize::try_from(n).unwrap_or(0))))
        }
    }

    /// A second of reference, decoded where the playhead is.
    fn reference() -> Streamed {
        let full = Streamed::new(2, 48_000, 48_000);
        let mut feeder = StreamFeeder::new(
            full.clone(),
            Level {
                frames: 48_000,
                at: 0,
            },
        );
        while feeder.pump(48_000) {}
        full
    }

    fn clock(playing: bool) -> AuxClock {
        AuxClock {
            playing,
            pos_seconds: 0.0,
            pos_beats: 0.0,
            tempo_bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
            sample_rate: 48_000.0,
            channels: 2,
            playrate: 1.0,
        }
    }

    /// What `project`'s renderer adds to a silent block.
    fn rendered(project: &str, playing: bool) -> f32 {
        let mut buf = vec![0.0_f32; 256];
        renderer(project.to_owned())(&mut buf, &clock(playing));
        buf.iter().map(|s| s.abs()).sum()
    }

    #[test]
    fn a_song_heard_by_its_reference_has_it_mixed_in_while_it_plays() {
        hear("mixed", reference(), None);
        assert!(any());
        assert!(
            rendered("mixed", true) > 1.0,
            "the reference is in the block"
        );
        assert!(
            rendered("mixed", false) < f32::EPSILON,
            "stopped, nothing is added"
        );
    }

    #[test]
    fn a_song_heard_by_its_stems_is_left_alone() {
        hear("handed over", reference(), None);
        stop("handed over");
        assert!(rendered("handed over", true) < f32::EPSILON);
        assert!(rendered("never heard", true) < f32::EPSILON);
    }
}
