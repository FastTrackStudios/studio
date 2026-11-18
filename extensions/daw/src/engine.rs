//! The `daw` engine playing the open session (feature `audio`).
//!
//! One thread owns the engine (`daw_standalone::sync::Standalone`, its
//! cpal output and the shared value table) and takes commands from the
//! extension's actions. What it plays is built from the session's `daw`
//! data each time the session is opened ([`load`]): the tracks, clips and
//! mixer state as REAPER text, which the engine's loader reads
//! (`load_rpp_via_bay`) and decodes. A mixer edit afterwards goes to the
//! engine directly ([`set`]).
//!
//! The thread ticks every [`TICK`]:
//! - the meters: each track's peak, as a meter's height
//!   ([`daw_view::meter`]), written to the shared table
//!   (`engine.daw.track.<n>.peak`): two atomic stores each, read by the
//!   host at compose;
//! - the faders: a value the host's drag wrote (`host.daw.track.<n>.volume`,
//!   `.pan`) that moved since the last tick goes to the engine at once,
//!   while the drag is held;
//! - the transport: its position and whether it plays, for `DAW_STATE`,
//!   and the deck re-anchored every [`crate::deck::REANCHOR`].

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{LazyLock, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use architect_scene::anim::Name;
use architect_scene::engine::{EngineWriter, Slot, shared};
use daw_proto::transport::service::Transport as _;
use daw_proto::{ProjectContext, TrackRef, Tracks as _};
use daw_standalone::media_bay::ProjectRelativeResolver;
use daw_standalone::sync::Standalone;
use daw_view::{Field, Transport, fader, meter, names};
use session::Session;

use crate::{deck, import, rpp};

/// How often the engine thread looks at its meters, faders and transport.
pub const TICK: Duration = Duration::from_millis(2);

enum Cmd {
    Load {
        name: String,
        root: std::path::PathBuf,
        project: rpp::Project,
    },
    Transport(Transport),
    Set(u32, Field, f64),
    Follow(Option<bool>, Option<f64>),
}

#[derive(Default)]
struct Shared {
    playing: AtomicBool,
    position: AtomicU64,
    duration: AtomicU64,
    rate: AtomicU32,
    problem: Mutex<Option<String>>,
    values: Mutex<Option<String>>,
    /// Ticks run, and the slowest, for `DAW_STATE`'s callers to judge.
    ticks: AtomicU64,
}

static SHARED: LazyLock<Shared> = LazyLock::new(Shared::default);
static TX: OnceLock<Mutex<Sender<Cmd>>> = OnceLock::new();

fn send(cmd: Cmd) {
    let tx = TX.get_or_init(|| {
        let (tx, rx) = channel();
        let _ = std::thread::Builder::new()
            .name("daw-engine".to_owned())
            .spawn(move || run(&rx));
        Mutex::new(tx)
    });
    let _ = tx.lock().unwrap_or_else(PoisonError::into_inner).send(cmd);
}

fn problem(why: Option<String>) {
    *SHARED
        .problem
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = why;
}

/// `(playing, position, duration, sample_rate, problem, values table)`.
#[must_use]
pub fn status() -> (bool, f64, f64, u32, Option<String>, Option<String>) {
    (
        SHARED.playing.load(Ordering::Relaxed),
        f64::from_bits(SHARED.position.load(Ordering::Relaxed)),
        f64::from_bits(SHARED.duration.load(Ordering::Relaxed)),
        SHARED.rate.load(Ordering::Relaxed),
        SHARED
            .problem
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone(),
        SHARED
            .values
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone(),
    )
}

/// Plays `s` from now on (in place of what played).
pub fn load(s: &Session) {
    let (_, root) = import::engine_tracks(s);
    let project = import::project_of(s);
    let duration = project
        .tracks
        .iter()
        .flat_map(|t| t.items.iter().map(|c| c.position + c.length))
        .fold(0.0_f64, f64::max);
    SHARED.duration.store(duration.to_bits(), Ordering::Relaxed);
    send(Cmd::Load {
        name: s.name.clone(),
        root,
        project,
    });
}

pub fn transport(t: Transport) {
    send(Cmd::Transport(t));
}

pub fn set(index: u32, field: Field, value: f64) {
    send(Cmd::Set(index, field, value));
}

/// The deck moved by someone else: follow it.
pub fn follow(playing: Option<bool>, time: Option<f64>) {
    send(Cmd::Follow(playing, time));
}

/// A track's slots in the table.
struct Strip {
    peak: Slot,
    volume: Slot,
    pan: Slot,
    last_peak: f64,
    last_volume: Option<f64>,
    last_pan: Option<f64>,
}

struct Playing {
    ctx: ProjectContext,
    // Held for its lifetime: dropping it stops the audio.
    _audio: daw_standalone::audio_engine::AudioEngine,
    writer: EngineWriter,
    strips: Vec<Strip>,
    master: Slot,
    last_master: f64,
}

fn name(n: &str) -> Option<Name> {
    Name::new(n).ok()
}

fn start(
    daw: &Standalone,
    name_: &str,
    root: &std::path::Path,
    project: &rpp::Project,
) -> Result<Playing, String> {
    let text = rpp::write(project);
    daw.media_bay()
        .set_file_resolver(Box::new(ProjectRelativeResolver::new(root.to_owned())));
    let hint = root.join(format!("{name_}.rpp"));
    let (proj, audio) = daw_standalone::project_loader::load_rpp_via_bay(
        daw,
        name_,
        hint.to_string_lossy().as_ref(),
        &text,
    )?;
    if !audio.failed.is_empty() {
        problem(Some(format!(
            "{} of {} sources failed to decode",
            audio.failed.len(),
            audio.loaded + audio.failed.len()
        )));
    }
    let engine = daw.attach_audio_engine(&proj.project_guid)?;
    SHARED.rate.store(engine.sample_rate(), Ordering::Relaxed);
    let n = u32::try_from(project.tracks.len().min(names::MAX_TRACKS as usize)).unwrap_or(0);
    let mut all = vec![names::MASTER_PEAK.to_owned()];
    for i in 0..n {
        all.extend([names::peak(i), names::volume(i), names::pan(i)]);
    }
    let path = shared::path_of(daw_view::ENGINE).map_err(|e| e.to_string())?;
    let (table, writer) =
        shared::create(&path, all.iter().filter_map(|n| name(n))).map_err(|e| e.to_string())?;
    let slot = |n: &str| name(n).and_then(|n| table.slot(&n));
    let strips = (0..n)
        .filter_map(|i| {
            Some(Strip {
                peak: slot(&names::peak(i))?,
                volume: slot(&names::volume(i))?,
                pan: slot(&names::pan(i))?,
                last_peak: -1.0,
                last_volume: None,
                last_pan: None,
            })
        })
        .collect();
    let master = slot(names::MASTER_PEAK).ok_or("no master slot")?;
    *SHARED.values.lock().unwrap_or_else(PoisonError::into_inner) =
        Some(path.to_string_lossy().into_owned());
    Ok(Playing {
        ctx: ProjectContext::Project(proj.project_guid),
        _audio: engine,
        writer,
        strips,
        master,
        last_master: -1.0,
    })
}

fn report(daw: &Standalone, p: &Playing) {
    let playing = daw.is_playing(p.ctx.clone());
    let pos = daw.get_position(p.ctx.clone());
    SHARED.playing.store(playing, Ordering::Relaxed);
    SHARED.position.store(pos.to_bits(), Ordering::Relaxed);
    deck::moved(playing, pos);
}

fn apply(daw: &Standalone, p: &Playing, index: u32, field: Field, value: f64) {
    let (ctx, track) = (p.ctx.clone(), TrackRef::Index(index));
    let _ = match field {
        Field::Volume => daw.set_volume(ctx, track, fader::gain(value)),
        Field::Pan => daw.set_pan(ctx, track, value.clamp(-1.0, 1.0)),
        Field::Mute => daw.set_muted(ctx, track, value != 0.0),
        Field::Solo => daw.set_soloed(ctx, track, value != 0.0),
    };
}

fn run(rx: &Receiver<Cmd>) {
    let Ok(rt) = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    else {
        problem(Some("no runtime for the engine".to_owned()));
        return;
    };
    let _guard = rt.enter();
    let daw = Standalone::new();
    let mut playing: Option<Playing> = None;
    let mut anchored = Instant::now();
    let mut ticks = 0_u64;
    loop {
        match rx.recv_timeout(TICK) {
            Ok(Cmd::Load {
                name,
                root,
                project,
            }) => {
                playing = None;
                problem(None);
                match start(&daw, &name, &root, &project) {
                    Ok(p) => playing = Some(p),
                    Err(e) => problem(Some(e)),
                }
                if let Some(p) = &playing {
                    report(&daw, p);
                }
            }
            Ok(Cmd::Transport(t)) => {
                if let Some(p) = &playing {
                    let ctx = p.ctx.clone();
                    let _ = match t {
                        Transport::Play => daw.play(ctx),
                        Transport::Pause => daw.pause(ctx),
                        Transport::Stop => daw.stop(ctx.clone()).and_then(|()| daw.goto_start(ctx)),
                        Transport::Toggle => daw.play_pause(ctx),
                        Transport::Seek { seconds } => daw.set_position(ctx, seconds.max(0.0)),
                    };
                    report(&daw, p);
                    anchored = Instant::now();
                }
            }
            Ok(Cmd::Set(i, field, value)) => {
                if let Some(p) = &playing {
                    apply(&daw, p, i, field, value);
                }
            }
            Ok(Cmd::Follow(play, time)) => {
                if let Some(p) = &playing {
                    let ctx = p.ctx.clone();
                    let pos = daw.get_position(ctx.clone());
                    if let Some(t) = time.filter(|t| (t - pos).abs() > 0.25) {
                        let _ = daw.set_position(ctx.clone(), t.max(0.0));
                    }
                    match play {
                        Some(true) if !daw.is_playing(ctx.clone()) => {
                            let _ = daw.play(ctx);
                        }
                        Some(false) if daw.is_playing(ctx.clone()) => {
                            let _ = daw.pause(ctx);
                        }
                        _ => {}
                    }
                    let now = daw.is_playing(p.ctx.clone());
                    SHARED.playing.store(now, Ordering::Relaxed);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        }
        let Some(p) = &mut playing else { continue };
        ticks += 1;
        SHARED.ticks.store(ticks, Ordering::Relaxed);

        // Faders the host is dragging: to the engine as they move.
        let mut moves = Vec::new();
        for (i, s) in p.strips.iter_mut().enumerate() {
            let index = u32::try_from(i).unwrap_or(u32::MAX);
            let v = p.writer.read(s.volume);
            if v.is_some() && v != s.last_volume {
                s.last_volume = v;
                moves.push((index, Field::Volume, v.unwrap_or(fader::UNITY)));
            }
            let pan = p.writer.read(s.pan);
            if pan.is_some() && pan != s.last_pan {
                s.last_pan = pan;
                moves.push((index, Field::Pan, pan.unwrap_or(0.0)));
            }
        }
        for (i, field, value) in moves {
            apply(&daw, p, i, field, value);
        }

        // Meters: each a height, written when it moved.
        let meters = daw.meters();
        let mut loudest = 0.0_f64;
        for (i, s) in p.strips.iter_mut().enumerate() {
            let Some(cell) = meters.cell(i) else { continue };
            let peak = f64::from(cell.peak(0).max(cell.peak(1)));
            loudest = loudest.max(peak);
            let h = meter::height(peak);
            if (h - s.last_peak).abs() > 1e-4 {
                s.last_peak = h;
                p.writer.write(s.peak, h);
            }
        }
        let h = meter::height(loudest);
        if (h - p.last_master).abs() > 1e-4 {
            p.last_master = h;
            p.writer.write(p.master, h);
        }

        // The transport, for DAW_STATE; the deck re-anchored while playing.
        let is = daw.is_playing(p.ctx.clone());
        let pos = daw.get_position(p.ctx.clone());
        SHARED.position.store(pos.to_bits(), Ordering::Relaxed);
        let was = SHARED.playing.swap(is, Ordering::Relaxed);
        if was != is || (is && anchored.elapsed() >= deck::REANCHOR) {
            deck::moved(is, pos);
            anchored = Instant::now();
        }
    }
}
