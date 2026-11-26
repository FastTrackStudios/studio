//! The null output: the set's callback driven at real time by a thread of
//! its own, its samples thrown away. What an engine with no sound card
//! plays through (a headless test, a machine without speakers), chosen
//! by naming the output device [`NAME`].
//!
//! Unlike ALSA's `null` device, which takes a buffer as fast as it is
//! written (2 s of audio in 6 ms), it asks for one block per block's
//! worth of time, so the transport, the render ahead and the first-audio
//! probe run as they do against a real device.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use audio::io::AudioIoPrefs;

/// The output device name that opens the null output.
pub const NAME: &str = "null";

/// Its rate and block when the prefs leave them to the device.
const RATE: u32 = 48_000;
const BLOCK: u32 = 512;
const CHANNELS: u16 = 2;

/// Whether `prefs` asks for the null output.
#[must_use]
pub fn chosen(prefs: &AudioIoPrefs) -> bool {
    prefs.output_name() == Some(NAME)
}

/// What the null output runs at: `prefs`' rate and buffer, else 48 kHz
/// and 512 frames, in stereo.
#[must_use]
pub fn config(prefs: &AudioIoPrefs) -> cpal::StreamConfig {
    cpal::StreamConfig {
        channels: CHANNELS,
        sample_rate: prefs.sample_rate_opt().unwrap_or(RATE),
        buffer_size: cpal::BufferSize::Fixed(prefs.buffer_size_opt().unwrap_or(BLOCK)),
    }
}

/// The running null output; dropping or [`Self::pause`] stops it.
pub struct NullOutput {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl NullOutput {
    /// Calls `callback` with one interleaved block of `config` per block
    /// of time, on a thread named `audio-null`, until stopped. A block
    /// that runs late is not made up: the clock moves on from it, as a
    /// device would drop out.
    ///
    /// # Errors
    ///
    /// The thread could not be started.
    pub fn start(
        config: &cpal::StreamConfig,
        mut callback: impl FnMut(&mut [f32]) + Send + 'static,
    ) -> Result<Self, String> {
        let channels = usize::from(config.channels.max(1));
        let frames = match config.buffer_size {
            cpal::BufferSize::Fixed(n) => n.max(1) as usize,
            cpal::BufferSize::Default => BLOCK as usize,
        };
        let period = Duration::from_secs_f64(frames as f64 / f64::from(config.sample_rate.max(1)));
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::Builder::new()
            .name("audio-null".into())
            .spawn(move || {
                let mut block = vec![0.0f32; frames * channels];
                let mut due = Instant::now();
                while !stopped.load(Ordering::Relaxed) {
                    callback(&mut block);
                    due += period;
                    let now = Instant::now();
                    if due > now {
                        std::thread::sleep(due - now);
                    } else {
                        due = now;
                    }
                }
            })
            .map_err(|e| format!("starting the null output: {e}"))?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }

    /// Stop calling the callback; returns once the thread has ended.
    pub fn pause(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for NullOutput {
    fn drop(&mut self) {
        self.pause();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn it_asks_for_blocks_at_real_time_not_as_fast_as_it_can() {
        let config = cpal::StreamConfig {
            channels: 2,
            sample_rate: 48_000,
            buffer_size: cpal::BufferSize::Fixed(480),
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let mut out = NullOutput::start(&config, move |block| {
            assert_eq!(block.len(), 960);
            counted.fetch_add(1, Ordering::Relaxed);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        out.pause();
        // 10 ms blocks over 200 ms: about 20, never thousands.
        let n = calls.load(Ordering::Relaxed);
        assert!((10..=30).contains(&n), "{n} blocks in 200 ms");
    }

    #[test]
    fn only_the_name_null_chooses_it() {
        let mut prefs = AudioIoPrefs::default();
        assert!(!chosen(&prefs));
        prefs.output_device = "null".into();
        assert!(chosen(&prefs));
        prefs.output_device = "Yamaha TF".into();
        assert!(!chosen(&prefs));
    }
}
