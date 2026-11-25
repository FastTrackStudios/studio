//! What playing costs the pane: the arrangement as playback drives it,
//! measured as the extension's process sends it.
//!
//! Studio's panel and widget on the golden template, hosted as the pane
//! hosts them (`studio_arrangement::host::Hosted`), driven as playback
//! drives them: a frame every 1/60 s with the play cursor 1/60 s further
//! on (`PanelEvent::Frame`, which the pane's paint view sends from the
//! engine's transport). Each frame's layers go through Architect's layer
//! protocol (`architect_scene::ops::Sender`), as a process sends them; the
//! test reports the batch bytes a frame and the view's work a frame, at
//! 1280 px and at the 2 924 px a 5120 px window gives the arrangement.
//!
//! `cargo test -p studio-arrangement --test frames -- --nocapture`

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::chunks_exact_to_as_chunks,
    clippy::too_many_lines,
    reason = "a test: measuring, with pixel arithmetic over a raster it made"
)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use architect::action::{ActionCaller, ActionList};
use architect_scene::ops::Sender;
use architect_workspace::paint::PaintView as _;
use architect_workspace::{FrameClock, Geometry, KindId, PaneId, Placement, ViewCtx, ViewHost};
use arrangement_view::panel::ArrangementPanel;
use arrangement_view::widget::ArrangementWidget;
use dioxus::prelude::*;
use session_engine_client::song_model::StudioSession;
use studio_arrangement::host::{Hosted, InScope, Stats};

/// How many frames each size plays: two seconds at 60 Hz.
const FRAMES: usize = 120;

fn session() -> StudioSession {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/dynamic-template/fixtures/golden/template.rpp");
    session_engine::open::open_silent(&path).expect("open the golden template");
    let raw = daw_view_kit::studio::fetch().expect("read it back");
    let planner = daw_view_kit::studio::Planner {
        raw: Arc::new(raw),
        scene: Some("drum-mixing"),
        kinds: Arc::new(session_engine::plan::Kinds::read(&path)),
    };
    let (project, rows) = planner.plan(&planner.raw);
    let previews = daw_view_kit::studio::previews_of(&project);
    StudioSession {
        project,
        rows,
        previews,
        chart: None,
        chart_file: None,
        planner,
    }
}

type Built = (Rc<RefCell<ArrangementWidget>>, ArrangementPanel, InScope);

#[derive(Clone, Default)]
struct Got(Rc<RefCell<Option<Built>>>);

impl PartialEq for Got {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone)]
struct Song(StudioSession);

impl PartialEq for Song {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[component]
fn Root(song: Song, got: Got) -> Element {
    use_context_provider(|| song.0.clone());
    use_context_provider(|| Signal::new(session::modes::Mode::Live));
    use_context_provider(|| arrangement_view::panel::StartCompact(true));
    let engine: arrangement_view::panel::Engine = Rc::new(|_| {});
    let (panel, widget) =
        arrangement_view::panel::use_arrangement_panel(None, engine, |w| Rc::new(RefCell::new(w)));
    use_hook(|| {
        *got.0.borrow_mut() = Some((widget, panel, InScope::here()));
    });
    rsx! { div {} }
}

fn ctx((w, h): (u32, u32)) -> ViewCtx {
    ViewCtx::new(
        PaneId::new("arrangement"),
        KindId::paint(),
        (ViewHost::Blitz, Placement::Pane),
        (ActionList::new(), ActionCaller::app()),
        FrameClock::new(Duration::from_millis(4), || Duration::ZERO),
    )
    .with_geometry(Geometry {
        width: w as f32,
        height: h as f32,
        scale: 1.0,
    })
}

fn pct(v: &mut [f64], p: usize) -> f64 {
    v.sort_by(f64::total_cmp);
    v[(v.len() - 1) * p / 100]
}

/// Plays `FRAMES` frames at `size`: the first batch, then each later
/// frame's bytes and the view's work.
fn play(song: &StudioSession, size: (u32, u32)) {
    let got = Got::default();
    let mut vdom = VirtualDom::new_with_props(
        Root,
        RootProps {
            song: Song(song.clone()),
            got: got.clone(),
        },
    );
    vdom.rebuild_in_place();
    let (widget, panel, scope) = got.0.borrow().clone().expect("the panel mounted");
    let stats = Arc::new(Mutex::new(Stats::default()));
    // The play cursor, as a playing engine moves it: a frame's time a
    // frame.
    let clock = Rc::new(std::cell::Cell::new(0.0_f64));
    let mut hosted = Hosted::new(widget, panel, scope, Arc::clone(&stats)).with_play_at({
        let clock = Rc::clone(&clock);
        move || clock.get()
    });
    let ctx = ctx(size);
    let mut sender = Sender::default();
    // Opened: the first frames fit the song and settle.
    let mut first = 0;
    for _ in 0..10 {
        let layers = hosted.layers(&ctx).unwrap();
        first += sender.frame(&layers).map_or(0, |b| b.len());
    }
    // Playing.
    let mut bytes = Vec::with_capacity(FRAMES);
    let mut work = Vec::with_capacity(FRAMES);
    for i in 0..FRAMES {
        clock.set(8.0 + i as f64 / 60.0);
        let began = Instant::now();
        let layers = hosted.layers(&ctx).unwrap();
        work.push(began.elapsed().as_secs_f64() * 1000.0);
        bytes.push(sender.frame(&layers).map_or(0, |b| b.len()) as f64);
    }
    let changed = bytes.iter().filter(|b| **b > 0.0).count();
    let mean = bytes.iter().sum::<f64>() / bytes.len() as f64;
    println!(
        "arrangement {}x{} playing: opened in {first} bytes; then {changed} of {FRAMES} frames crossed, \
         bytes a frame mean {mean:.0} p50 {:.0} p99 {:.0} (a second at 60 Hz: {:.1} MB); \
         the view's work a frame p50 {:.2} ms p99 {:.2} ms",
        size.0,
        size.1,
        pct(&mut bytes.clone(), 50),
        pct(&mut bytes.clone(), 99),
        mean * 60.0 / 1e6,
        pct(&mut work.clone(), 50),
        pct(&mut work, 99),
    );
}

#[test]
fn playing_the_arrangement_measured() {
    let song = session();
    play(&song, (1280, 780));
    play(&song, (2924, 759));
}
