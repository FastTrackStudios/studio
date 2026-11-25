//! Reference Mode with the runtime alone: a song opened from a
//! `SongSource`, heard by its reference with the click and guide over it,
//! the transport reporting where it is — `session-runtime` and `daw`, no
//! `session-engine`, no window, no audio device.
//!
//! The source is a fake in memory; the host's part of opening (preparing
//! the song: its click and guide) is a test `OpenHost`; the transport is
//! driven offline by the same block loop a device callback runs
//! (`render_set_block`), with the reference mixed in by the engine's
//! post-render hook (`reference_play::renderer`).

// Test fixtures: block counts and sample positions are small and known,
// and a fixture that cannot be built fails the test.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use daw::service::{ProjectContext, Tracks};
use daw::standalone::Standalone;
use daw::standalone::audio_engine::{
    AuxClock, BlockClock, SetBlock, Voice, VoiceSetup, Voices, render_set_block,
};
use daw_proto::transport::service::Transport;
use fts_sample::ogg_index::OggIndex;
use session_runtime::folder::Folder;
use session_runtime::guide_instrument::{self, Library};
use session_runtime::open_core::{Media, OpenHost, Opened, open_song_in};
use session_runtime::reference_play;
use session_runtime::song_stream::{Keep, Reference, arrived, mirror};
use session_runtime::source::{Listing, Pending, SongSource};

const RATE: u32 = 48_000;
/// 480 frames: a period of exactly 10 000 µs.
const BLOCK: usize = 480;
const PERIOD_MICROS: f64 = 10_000.0;
/// The song, seconds: two bars at 120.
const SONG: f64 = 4.0;
/// The reference's level: a constant, told apart from the click's.
const LEVEL: f32 = 0.25;

/// A song of one recorded track, its start and end, and one section.
const RPP: &str = "<REAPER_PROJECT 0.1 \"7.0/test\" 1700000000
  SAMPLERATE 48000 0 0
  TEMPO 120 4 4 0
  MARKER 1 0 \"SONGSTART\" 0
  MARKER 2 4 \"SONGEND\" 0
  MARKER 3 0 \"VS 1\" 1
  MARKER 3 4 \"\" 1
  <TRACK {0B5E0000-0000-0000-0000-000000000001}
    NAME Guitar
  >
>
";

/// A song's files, in memory: its project and its reference proxy (with
/// the page index a streamed proxy is read by).
struct FakeShare {
    files: Vec<(String, Vec<u8>)>,
}

impl FakeShare {
    fn new() -> Self {
        let frames = vec![LEVEL; usize::try_from(u64::from(RATE) * 6).unwrap_or(0) * 2];
        let ogg = fts_sample::cache::encode_ogg_vorbis(&frames, 2, RATE, 0.4).expect("encoded");
        let index = OggIndex::build(&ogg, u64::from(RATE)).expect("indexed");
        let proxy = "Media/Proxies/Reference.ogg";
        Self {
            files: vec![
                ("Song.RPP".to_owned(), RPP.as_bytes().to_vec()),
                (
                    OggIndex::path_for(Path::new(proxy))
                        .to_string_lossy()
                        .into_owned(),
                    index.to_text().into_bytes(),
                ),
                (proxy.to_owned(), ogg),
            ],
        }
    }
}

impl SongSource for FakeShare {
    fn label(&self) -> String {
        "fake share".to_owned()
    }

    fn list(&self) -> Pending<eyre::Result<Listing>> {
        let entries: Vec<(String, u64)> = self
            .files
            .iter()
            .map(|(path, bytes)| (path.clone(), bytes.len() as u64))
            .collect();
        Box::pin(async move { Ok(entries.into()) })
    }

    fn read(&self, path: String, range: std::ops::Range<u64>) -> Pending<Result<Vec<u8>, String>> {
        let bytes = self
            .files
            .iter()
            .find(|(p, _)| *p == path)
            .map(|(_, bytes)| {
                let end = usize::try_from(range.end)
                    .unwrap_or(usize::MAX)
                    .min(bytes.len());
                let start = usize::try_from(range.start).unwrap_or(end).min(end);
                bytes[start..end].to_vec()
            })
            .ok_or_else(|| format!("{path}: no such file"));
        Box::pin(async move { bytes })
    }
}

/// The host's part of opening the song: its click and guide, generated
/// onto the instrument the runtime installs (what Session's `Prepare`
/// does as its last step).
struct ClickAndGuide;

impl OpenHost for ClickAndGuide {
    fn nothing_to_prepare(&self) -> bool {
        false
    }

    fn prepare(&self, _folder: &dyn Folder, opened: &Opened) -> eyre::Result<()> {
        session::guide::Guide::new(opened.daw.clone())
            .with_instrument(guide_instrument::IDENT)
            .generate(session::guide::GuideScope::All)
            .map_err(|e| eyre::eyre!("guide: {e}"))
    }
}

/// Play the set's blocks from `first` for `blocks` blocks, the reference
/// mixed in after each as the device callback does. Returns the output.
fn play(
    voices: &Voices,
    hook: &mut daw::standalone::audio_engine::AuxRenderer,
    first: usize,
    blocks: usize,
) -> Vec<f32> {
    let mut out = vec![0.0; blocks * BLOCK * 2];
    for (i, chunk) in out.chunks_mut(BLOCK * 2).enumerate() {
        let clock = BlockClock {
            frames: BLOCK as u32,
            stamp_micros: (first + i) as f64 * PERIOD_MICROS,
            period_micros: PERIOD_MICROS,
        };
        let block = render_set_block(voices, clock, chunk);
        hook(chunk, &aux_clock(&block));
    }
    out
}

/// The post-render hook's clock for a block (as the device callback makes
/// it): where the block starts, and whether it plays.
fn aux_clock(block: &SetBlock) -> AuxClock {
    let rate = f64::from(RATE);
    let (playing, pos_seconds, playrate) = block.current.map_or((false, 0.0, 1.0), |plan| {
        let run = plan.last_playing().unwrap_or_else(|| plan.first());
        (
            plan.any_playing(),
            run.start_at_frame_zero().max(0.0) / rate,
            run.rate,
        )
    });
    AuxClock {
        playing,
        pos_seconds,
        pos_beats: 0.0,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
        sample_rate: rate,
        channels: 2,
        playrate,
    }
}

#[test]
fn a_song_from_a_source_plays_its_reference_with_click_and_guide() {
    // The backend runs its transport pumps on the runtime; the host's
    // runtime is where the song's fetcher runs too.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime");
    let _entered = runtime.enter();
    let handle = runtime.handle().clone();

    // Streamed from the source: the small files mirrored into memory.
    let song = runtime
        .block_on(mirror(
            Arc::new(FakeShare::new()),
            Keep::Memory(session_runtime::folder::Memory::new()),
        ))
        .expect("mirrored")
        .with_spawn(Arc::new(move |fetching| {
            handle.spawn(fetching);
        }));
    assert!(song.has_reference());

    // Opened the one way songs open, prepared by the host: click and guide.
    let daw = Standalone::new();
    guide_instrument::install(
        &daw,
        Library::Files {
            files: Arc::default(),
            ext: "ogg",
        },
    );
    let (opened, plan) = open_song_in(
        song.folder.as_ref(),
        &daw,
        &song.project,
        &ClickAndGuide,
        Media::Deferred,
    )
    .expect("the song opens");
    assert!(plan.prepare, "prepared on the way in");
    let project = opened.project_guid;
    let names: Vec<String> = Tracks::all(&daw, ProjectContext::Project(project.clone()))
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert!(
        names.iter().any(|n| n == "Click"),
        "a click track: {names:?}"
    );

    // Heard by its reference: fetched from the source, decoded, heard.
    let Some(Reference { full, .. }) = song.reference() else {
        panic!("the song has a reference");
    };
    let (take, mut feeder) = full;
    let takes = Arc::new(Mutex::new(vec![take]));
    song.fetch(
        Arc::clone(&takes),
        Arc::new(|| 0.0),
        Arc::new(AtomicBool::new(false)),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    while arrived(&takes) < 1 {
        assert!(Instant::now() < deadline, "the reference never arrived");
        std::thread::sleep(Duration::from_millis(5));
    }
    while feeder.pump(usize::MAX) {}
    reference_play::hear(&project, feeder.source().clone(), None);
    let mut hook = reference_play::renderer(project.clone());

    // Played: the transport moves, the output is the reference plus the
    // click and guide.
    let voice = Arc::new(Voice::open(&daw, &project, &VoiceSetup::offline(RATE)).expect("a voice"));
    let voices = Voices::default().with(voice).with_current(Some(&project));
    let at = ProjectContext::Project(project);
    Transport::play(&daw, at.clone()).expect("plays");
    let blocks = (SONG * 1_000_000.0 / PERIOD_MICROS) as usize / 2;
    let out = play(&voices, &mut hook, 0, blocks);

    let position = Transport::get_position(&daw, at);
    let played = blocks as f64 * BLOCK as f64 / f64::from(RATE);
    assert!(
        (position - played).abs() < 0.05,
        "the transport reports {position:.3} s after {played:.3} s"
    );
    // The reference alone is a constant LEVEL on both sides; the click
    // and guide are what rise above it.
    let left: Vec<f32> = out.iter().step_by(2).copied().collect();
    let reference_heard = left.iter().filter(|s| (**s - LEVEL).abs() < 0.02).count();
    let above = left.iter().filter(|s| (**s - LEVEL).abs() > 0.05).count();
    assert!(
        reference_heard > left.len() / 2,
        "the reference is the bed: {reference_heard} of {} samples",
        left.len()
    );
    assert!(above > 0, "the click is heard over it");
}
