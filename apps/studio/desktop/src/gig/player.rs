//! The practice player: one song's full mix on its own output stream, under
//! the rig rather than through it.
//!
//! Deliberately small and self-contained — no DAW, no project, no setlist
//! service — so the gig page plays on any machine the app runs on, beside
//! whatever the rig is doing with its own device. (The Session engine plays
//! multitrack projects; a practice track is one stereo file.)
//!
//! - **Decode** happens off the audio thread, a song at a time
//!   ([`Player::load`]), resampled once to the device's rate, so the
//!   callback only ever copies.
//! - **Speed without pitch** goes through the `stretch` crate's phase
//!   vocoder (the one the DAW renders stretched takes with). At 100 % the
//!   file is copied as it is — the vocoder only runs when it is needed.
//! - **Loop** is a pair of frames the playhead wraps between: a section's
//!   bounds, or an A/B the player set by hand.
//!
//! The callback takes no lock it could wait on: a new track, a seek or a
//! setting arrives through atomics and a `try_lock` (a contended block
//! plays silence and tries again next block).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// "No seek pending" in [`Shared::seek`].
const NO_SEEK: u64 = u64::MAX;

/// The most output frames rendered per inner step (the scratch size).
const BLOCK: usize = 2048;

/// One song, decoded: stereo at the device's rate.
pub struct Track {
    left: Vec<f32>,
    right: Vec<f32>,
}

impl Track {
    fn frames(&self) -> usize {
        self.left.len().min(self.right.len())
    }
}

/// The stretcher's view of a track: frames by index, silence outside it.
struct TrackSource<'a>(&'a Track);

impl dsp::stretch::Source for TrackSource<'_> {
    fn read(&mut self, start: i64, left: &mut [f32], right: &mut [f32]) {
        let frames = self.0.frames();
        for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            let at = i64::try_from(i).unwrap_or(i64::MAX).saturating_add(start);
            match usize::try_from(at) {
                Ok(at) if at < frames => {
                    *l = self.0.left.get(at).copied().unwrap_or(0.0);
                    *r = self.0.right.get(at).copied().unwrap_or(0.0);
                }
                _ => {
                    *l = 0.0;
                    *r = 0.0;
                }
            }
        }
    }
}

/// What the UI and the audio callback share.
struct Shared {
    playing: AtomicBool,
    /// A seek the callback has not taken yet, in frames ([`NO_SEEK`]: none).
    seek: AtomicU64,
    /// Playback speed (f64 bits): 1.0 is as recorded.
    speed: AtomicU64,
    /// Output gain (f32 bits).
    volume: AtomicU32,
    looping: AtomicBool,
    loop_a: AtomicU64,
    loop_b: AtomicU64,
    /// Where the playhead is, in frames — written by the callback.
    position: AtomicU64,
    /// Length of the current track, in frames (0: none).
    length: AtomicU64,
    /// The output's sample rate, Hz (0: no output open).
    rate: AtomicU32,
    /// The ticket of the output whose voice plays (see [`try_open`]); a
    /// voice with another ticket stays silent and takes nothing.
    active_output: AtomicU64,
    /// The sample rate the current track was decoded at (0: none).
    track_rate: AtomicU32,
    /// The ticket of the voice that took the current track up.
    track_ticket: AtomicU64,
    /// A freshly decoded track for the callback to take up.
    pending: Mutex<Option<Arc<Track>>>,
    /// Tracks the callback let go of, freed off the audio thread.
    retired: Mutex<Vec<Arc<Track>>>,
    /// Bumped by every load; a decode that finishes after a newer load
    /// started is dropped.
    generation: AtomicU64,
    loading: AtomicBool,
    /// The last thing worth telling the player (a decode or device error).
    status: Mutex<String>,
    /// The output device's name, as opened.
    device: Mutex<String>,
}

impl Shared {
    fn new() -> Self {
        Self {
            playing: AtomicBool::new(false),
            seek: AtomicU64::new(NO_SEEK),
            speed: AtomicU64::new(1.0f64.to_bits()),
            volume: AtomicU32::new(1.0f32.to_bits()),
            looping: AtomicBool::new(false),
            loop_a: AtomicU64::new(0),
            loop_b: AtomicU64::new(0),
            position: AtomicU64::new(0),
            length: AtomicU64::new(0),
            rate: AtomicU32::new(0),
            active_output: AtomicU64::new(0),
            track_rate: AtomicU32::new(0),
            track_ticket: AtomicU64::new(u64::MAX),
            pending: Mutex::new(None),
            retired: Mutex::new(Vec::with_capacity(8)),
            generation: AtomicU64::new(0),
            loading: AtomicBool::new(false),
            status: Mutex::new(String::new()),
            device: Mutex::new(String::new()),
        }
    }

    fn set_status(&self, text: impl Into<String>) {
        if let Ok(mut s) = self.status.lock() {
            *s = text.into();
        }
    }
}

/// The audio callback's own state.
struct Voice {
    shared: Arc<Shared>,
    track: Option<Arc<Track>>,
    /// Playhead, in input frames (fractional while stretching).
    pos: f64,
    stretcher: dsp::stretch::Stretcher,
    /// The stretcher is primed at `pos` (false: seek it before rendering).
    primed: bool,
    left: Vec<f32>,
    right: Vec<f32>,
    /// Which output this voice belongs to (see [`Shared::active_output`]).
    ticket: u64,
}

impl Voice {
    fn new(shared: Arc<Shared>, rate: u32, ticket: u64) -> Self {
        Self {
            shared,
            track: None,
            pos: 0.0,
            stretcher: dsp::stretch::Stretcher::new(dsp::stretch::Config::for_sample_rate(
                f64::from(rate),
            )),
            primed: false,
            left: vec![0.0; BLOCK],
            right: vec![0.0; BLOCK],
            ticket,
        }
    }

    fn active(&self) -> bool {
        self.shared.active_output.load(Ordering::Acquire) == self.ticket
    }

    /// Take up a new track and any pending seek.
    fn take_messages(&mut self) {
        if !self.active() {
            return;
        }
        if let Ok(mut pending) = self.shared.pending.try_lock()
            && let Some(track) = pending.take()
        {
            let old = self.track.replace(track);
            self.shared
                .track_ticket
                .store(self.ticket, Ordering::Release);
            self.pos = 0.0;
            self.primed = false;
            if let Some(old) = old
                && let Ok(mut retired) = self.shared.retired.try_lock()
            {
                retired.push(old);
            }
        }
        let seek = self.shared.seek.swap(NO_SEEK, Ordering::AcqRel);
        if seek != NO_SEEK {
            self.pos = frames_f64(seek);
            self.primed = false;
        }
    }

    fn render(&mut self, data: &mut [f32], channels: usize) {
        self.take_messages();
        data.fill(0.0);
        if !self.active() {
            return;
        }
        let channels = channels.max(1);
        let Some(track) = self.track.clone() else {
            return;
        };
        if !self.shared.playing.load(Ordering::Acquire) {
            self.publish();
            return;
        }
        let total = frames_f64(u64::try_from(track.frames()).unwrap_or(0));
        let speed = f64::from_bits(self.shared.speed.load(Ordering::Relaxed)).clamp(0.25, 2.0);
        let gain = f32::from_bits(self.shared.volume.load(Ordering::Relaxed));
        let looping = self.shared.looping.load(Ordering::Relaxed);
        let loop_a = frames_f64(self.shared.loop_a.load(Ordering::Relaxed));
        let loop_b = frames_f64(self.shared.loop_b.load(Ordering::Relaxed));
        let looped = looping && loop_b > loop_a + 1.0;

        let mut remaining = data.len() / channels;
        let mut out = data.chunks_mut(channels);
        while remaining > 0 {
            if looped && (self.pos >= loop_b || self.pos < loop_a - 1.0) {
                self.pos = loop_a;
                self.primed = false;
            }
            if self.pos >= total {
                // The end: stop, and be back at the top for the next take.
                self.shared.playing.store(false, Ordering::Release);
                self.pos = if looped { loop_a } else { 0.0 };
                self.primed = false;
                break;
            }
            let mut n = remaining.min(BLOCK);
            if looped {
                // Stop the step at the loop's end, so the wrap is exact.
                let until = ((loop_b - self.pos) / speed).ceil().max(1.0);
                n = n.min(frames_usize(until));
            }
            let (Some(left), Some(right)) = (self.left.get_mut(..n), self.right.get_mut(..n))
            else {
                break;
            };
            if (speed - 1.0).abs() < 1e-3 {
                let start = frames_usize(self.pos.round());
                for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
                    let at = start.saturating_add(i);
                    *l = track.left.get(at).copied().unwrap_or(0.0);
                    *r = track.right.get(at).copied().unwrap_or(0.0);
                }
                self.pos = frames_f64(u64::try_from(start.saturating_add(n)).unwrap_or(u64::MAX));
                self.primed = false;
            } else {
                let mut source = TrackSource(&track);
                if !self.primed {
                    self.stretcher.seek(self.pos, speed, 1.0, &mut source);
                    self.primed = true;
                }
                self.stretcher.render(left, right, speed, 1.0, &mut source);
                self.pos += frames_f64(u64::try_from(n).unwrap_or(0)) * speed;
            }
            for (i, frame) in out.by_ref().take(n).enumerate() {
                let l = left.get(i).copied().unwrap_or(0.0) * gain;
                let r = right.get(i).copied().unwrap_or(0.0) * gain;
                let mut slots = frame.iter_mut();
                if let Some(s) = slots.next() {
                    *s = l;
                }
                match slots.next() {
                    Some(s) => *s = r,
                    None => {
                        // A mono output gets both sides.
                        if let Some(s) = frame.first_mut() {
                            *s = 0.5 * (l + r);
                        }
                    }
                }
            }
            remaining = remaining.saturating_sub(n);
        }
        self.publish();
    }

    fn publish(&self) {
        self.shared
            .position
            .store(frames_u64(self.pos), Ordering::Relaxed);
    }
}

fn frames_f64(frames: u64) -> f64 {
    // Frame counts stay far below 2^52: exact.
    #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
    let f = frames as f64;
    f
}

fn frames_u64(frames: f64) -> u64 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::as_conversions
    )]
    let n = frames.max(0.0) as u64;
    n
}

fn frames_usize(frames: f64) -> usize {
    usize::try_from(frames_u64(frames)).unwrap_or(usize::MAX)
}

/// What the audio thread is asked to do.
enum Command {
    /// Open this output (`None`: the system default), closing the last.
    Open(Option<String>),
}

/// The handle the UI holds. One per process ([`player`]).
pub struct Player {
    shared: Arc<Shared>,
    commands: Mutex<mpsc::Sender<Command>>,
    /// The file the current track was decoded from — reloaded when a new
    /// device runs at another rate.
    current: Mutex<Option<PathBuf>>,
}

/// The process's practice player, its output opened on first use.
pub fn player() -> &'static Player {
    static PLAYER: OnceLock<Player> = OnceLock::new();
    PLAYER.get_or_init(|| {
        let shared = Arc::new(Shared::new());
        let (tx, rx) = mpsc::channel();
        let thread_shared = Arc::clone(&shared);
        let spawned = std::thread::Builder::new()
            .name("gig-player".into())
            .spawn(move || audio_thread(&thread_shared, &rx));
        if let Err(e) = spawned {
            shared.set_status(format!("could not start the audio thread: {e}"));
        }
        let wanted = std::env::var("FTS_GIG_OUTPUT")
            .ok()
            .filter(|s| !s.is_empty());
        let _ = tx.send(Command::Open(wanted));
        Player {
            shared,
            commands: Mutex::new(tx),
            current: Mutex::new(None),
        }
    })
}

/// How long a device may take to open before the next one is tried. A
/// wedged CoreAudio server blocks `build_output_stream` indefinitely (seen
/// on a Mac whose `coreaudiod` stopped answering); the attempt's thread is
/// left behind and the player moves on.
const OPEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Owns the output: opens the asked-for device (else the default), and on
/// failure or a hang tries every other output in turn.
fn audio_thread(shared: &Arc<Shared>, commands: &mpsc::Receiver<Command>) {
    // Dropping it closes the open stream (its thread is waiting on it).
    let mut open: Option<mpsc::Sender<()>> = None;
    while let Ok(command) = commands.recv() {
        match command {
            Command::Open(name) => {
                drop(open.take());
                shared.active_output.fetch_add(1, Ordering::AcqRel);
                shared.rate.store(0, Ordering::Release);
                if let Ok(mut d) = shared.device.lock() {
                    d.clear();
                }
                let mut candidates: Vec<Option<String>> = vec![name.clone()];
                if name.is_some() {
                    candidates.push(None);
                }
                // Enumerating is itself a server call: bounded the same way.
                let others = run_bounded(output_devices).unwrap_or_default();
                candidates.extend(others.into_iter().map(Some));
                let mut failed: Vec<String> = Vec::new();
                for candidate in candidates {
                    let label = candidate.clone().unwrap_or_else(|| "default output".into());
                    if failed.iter().any(|f| f.starts_with(&format!("{label}:"))) {
                        continue;
                    }
                    match try_open(shared, candidate) {
                        Ok(stop) => {
                            open = Some(stop);
                            break;
                        }
                        Err(e) => {
                            tracing::warn!("gig player: {label}: {e}");
                            failed.push(format!("{label}: {e}"));
                        }
                    }
                }
                if open.is_none() {
                    shared.set_status(format!("No audio output — {}", failed.join("; ")));
                } else if failed.is_empty() {
                    shared.set_status("");
                } else {
                    shared.set_status(format!("Skipped {}", failed.join("; ")));
                }
            }
        }
    }
}

/// `f` on a thread of its own, given [`OPEN_TIMEOUT`] to answer.
fn run_bounded<T: Send + 'static>(f: fn() -> T) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("gig-probe".into())
        .spawn(move || {
            let _ = tx.send(f());
        })
        .ok()?;
    rx.recv_timeout(OPEN_TIMEOUT).ok()
}

/// Open `name` (`None`: the default) on a thread that then keeps the
/// stream until the returned sender is dropped. A device that does not
/// open within [`OPEN_TIMEOUT`] is given up on; should it open later, its
/// voice is not the active one and stays silent, and its thread closes it.
fn try_open(shared: &Arc<Shared>, name: Option<String>) -> Result<mpsc::Sender<()>, String> {
    let ticket = shared
        .active_output
        .fetch_add(1, Ordering::AcqRel)
        .wrapping_add(1);
    let (result_tx, result_rx) = mpsc::channel::<Result<(u32, String), String>>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let thread_shared = Arc::clone(shared);
    std::thread::Builder::new()
        .name("gig-output".into())
        .spawn(
            move || match open_output(&thread_shared, name.as_deref(), ticket) {
                Ok((stream, rate, device)) => {
                    if result_tx.send(Ok((rate, device))).is_ok() {
                        // Hold the stream until told to close.
                        let _ = stop_rx.recv();
                    }
                    drop(stream);
                }
                Err(e) => {
                    let _ = result_tx.send(Err(e));
                }
            },
        )
        .map_err(|e| e.to_string())?;
    match result_rx.recv_timeout(OPEN_TIMEOUT) {
        Ok(Ok((rate, device))) => {
            if let Ok(mut d) = shared.device.lock() {
                d.clone_from(&device);
            }
            shared.rate.store(rate, Ordering::Release);
            tracing::info!("gig player: output '{device}' at {rate} Hz");
            Ok(stop_tx)
        }
        Ok(Err(e)) => Err(e),
        Err(_) => Err("did not open (timed out)".into()),
    }
}

fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_default()
}

/// The output devices this machine has, by name.
pub fn output_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices()
        .map(|devices| devices.map(|d| device_name(&d)).collect())
        .unwrap_or_default()
}

fn open_output(
    shared: &Arc<Shared>,
    name: Option<&str>,
    ticket: u64,
) -> Result<(cpal::Stream, u32, String), String> {
    let host = cpal::default_host();
    let device = match name {
        Some(name) => host
            .output_devices()
            .map_err(|e| e.to_string())?
            .find(|d| device_name(d) == name)
            .or_else(|| {
                host.output_devices()
                    .ok()?
                    .find(|d| device_name(d).contains(name))
            }),
        None => host.default_output_device(),
    }
    .ok_or_else(|| "no such output device".to_string())?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let config = supported.config();
    let rate = config.sample_rate;
    let channels = usize::from(config.channels);
    let mut voice = Voice::new(Arc::clone(shared), rate, ticket);
    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| voice.render(data, channels),
            |err| tracing::warn!("gig player: output stream error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, rate, device_name(&device)))
}

impl Player {
    /// Decode `path` (off the UI and audio threads) and make it the track,
    /// stopped at its top. A load superseded by a newer one is dropped.
    pub fn load(&self, path: &Path) {
        let shared = Arc::clone(&self.shared);
        shared.playing.store(false, Ordering::Release);
        let generation = shared
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        shared.loading.store(true, Ordering::Release);
        shared.set_status("");
        if let Ok(mut c) = self.current.lock() {
            *c = Some(path.to_path_buf());
        }
        let path = path.to_path_buf();
        let spawned = std::thread::Builder::new()
            .name("gig-decode".into())
            .spawn(move || {
                // Free whatever the callback let go of, here and not there.
                if let Ok(mut retired) = shared.retired.lock() {
                    retired.clear();
                }
                // Wait (briefly) for the output, so the decode is at its rate.
                let mut rate = shared.rate.load(Ordering::Acquire);
                for _ in 0..50 {
                    if rate != 0 {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(40));
                    rate = shared.rate.load(Ordering::Acquire);
                }
                let target = (rate != 0).then_some(rate);
                let decoded = fts_sample::load_planar_f32(
                    &path,
                    target,
                    fts_sample::ResampleQuality::default(),
                );
                if shared.generation.load(Ordering::Acquire) != generation {
                    return;
                }
                match decoded {
                    Ok(audio) => {
                        let track_rate = audio.sample_rate;
                        let mut channels = audio.channels.into_iter();
                        let left = channels.next().unwrap_or_default();
                        let right = channels.next().unwrap_or_else(|| left.clone());
                        let track = Track { left, right };
                        let frames = u64::try_from(track.frames()).unwrap_or(0);
                        if let Ok(mut pending) = shared.pending.lock() {
                            *pending = Some(Arc::new(track));
                        }
                        shared.track_rate.store(track_rate, Ordering::Release);
                        shared.length.store(frames, Ordering::Release);
                        shared.position.store(0, Ordering::Release);
                        shared.looping.store(false, Ordering::Release);
                        tracing::info!("gig player: loaded {} ({frames} frames)", path.display());
                    }
                    Err(e) => {
                        tracing::warn!("gig player: {e}");
                        shared.length.store(0, Ordering::Release);
                        shared.set_status(format!("Could not read the audio: {e}"));
                    }
                }
                shared.loading.store(false, Ordering::Release);
            });
        if let Err(e) = spawned {
            self.shared
                .set_status(format!("could not start decoding: {e}"));
            self.shared.loading.store(false, Ordering::Release);
        }
    }

    /// Stop and drop the track (a song with no audio).
    pub fn unload(&self) {
        self.shared.playing.store(false, Ordering::Release);
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        self.shared.loading.store(false, Ordering::Release);
        self.shared.length.store(0, Ordering::Release);
        self.shared.position.store(0, Ordering::Release);
        if let Ok(mut c) = self.current.lock() {
            *c = None;
        }
    }

    /// Open another output (`None`: the default). The track follows it
    /// ([`Player::poll`] reloads it for the new voice, at the new rate).
    pub fn open_device(&self, name: Option<String>) {
        if let Ok(tx) = self.commands.lock() {
            let _ = tx.send(Command::Open(name));
        }
    }

    /// Keep the track with the output, called on the UI's tick: once an
    /// output is open, a track decoded at another rate, or taken up by a
    /// voice that is no longer playing, is decoded again for this one.
    pub fn poll(&self) {
        let rate = self.shared.rate.load(Ordering::Acquire);
        if rate == 0 || self.is_loading() || !self.is_loaded() {
            return;
        }
        let Some(path) = self.current.lock().ok().and_then(|c| c.clone()) else {
            return;
        };
        let waiting = self
            .shared
            .pending
            .try_lock()
            .map(|p| p.is_some())
            .unwrap_or(true);
        let stale_rate = self.shared.track_rate.load(Ordering::Acquire) != rate;
        let orphaned = !waiting
            && self.shared.track_ticket.load(Ordering::Acquire)
                != self.shared.active_output.load(Ordering::Acquire);
        if stale_rate || orphaned {
            tracing::info!("gig player: reloading {} for the output", path.display());
            self.load(&path);
        }
    }

    /// The rate positions are counted in: the track's own, else the output's.
    fn rate(&self) -> f64 {
        let track = self.shared.track_rate.load(Ordering::Acquire);
        let rate = if track != 0 {
            track
        } else {
            self.shared.rate.load(Ordering::Acquire)
        };
        f64::from(rate.max(1))
    }

    fn to_frames(&self, seconds: f64) -> u64 {
        frames_u64(seconds.max(0.0) * self.rate())
    }

    fn to_seconds(&self, frames: u64) -> f64 {
        frames_f64(frames) / self.rate()
    }

    pub fn play(&self) {
        if self.shared.length.load(Ordering::Acquire) > 0 {
            self.shared.playing.store(true, Ordering::Release);
        }
    }

    pub fn pause(&self) {
        self.shared.playing.store(false, Ordering::Release);
    }

    pub fn toggle(&self) {
        if self.is_playing() {
            self.pause();
        } else {
            self.play();
        }
    }

    pub fn seek(&self, seconds: f64) {
        let frames = self.to_frames(seconds.min(self.duration()));
        self.shared.seek.store(frames, Ordering::Release);
        self.shared.position.store(frames, Ordering::Release);
    }

    pub fn set_speed(&self, speed: f64) {
        self.shared
            .speed
            .store(speed.clamp(0.25, 2.0).to_bits(), Ordering::Relaxed);
    }

    pub fn speed(&self) -> f64 {
        f64::from_bits(self.shared.speed.load(Ordering::Relaxed))
    }

    pub fn set_volume(&self, volume: f32) {
        self.shared
            .volume
            .store(volume.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.shared.volume.load(Ordering::Relaxed))
    }

    /// Loop between `a` and `b` seconds (`None`: stop looping).
    pub fn set_loop(&self, range: Option<(f64, f64)>) {
        match range {
            Some((a, b)) if b > a => {
                self.shared
                    .loop_a
                    .store(self.to_frames(a), Ordering::Relaxed);
                self.shared
                    .loop_b
                    .store(self.to_frames(b), Ordering::Relaxed);
                self.shared.looping.store(true, Ordering::Release);
            }
            _ => self.shared.looping.store(false, Ordering::Release),
        }
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Acquire)
    }

    pub fn is_loading(&self) -> bool {
        self.shared.loading.load(Ordering::Acquire)
    }

    pub fn is_loaded(&self) -> bool {
        self.shared.length.load(Ordering::Acquire) > 0
    }

    pub fn position(&self) -> f64 {
        self.to_seconds(self.shared.position.load(Ordering::Relaxed))
    }

    pub fn duration(&self) -> f64 {
        self.to_seconds(self.shared.length.load(Ordering::Acquire))
    }

    pub fn status(&self) -> String {
        self.shared
            .status
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    pub fn device(&self) -> String {
        self.shared
            .device
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }
}
