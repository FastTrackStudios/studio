//! What the arrangement sends its host, and what each frame costs the
//! view and the host: the real view, driven by real input events, through
//! the same `ops::Sender` a process uses and the `Receiver` and composer
//! the host uses.
//!
//! `cargo test -p studio-arrangement --test measure -- --nocapture`
//! (the table is printed by `measure_*`, which are `--ignored`).

#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::cast_precision_loss)]

use std::time::{Duration, Instant};

use architect::action::{ActionCaller, ActionList};
use architect_scene::anim::{Reading, Readings};
use architect_scene::ops::{Receiver, Sender};
use architect_workspace::paint::peniko::kurbo::Affine;
use architect_workspace::paint::{Layer, LayerKey, PaintView, Scene};
use architect_workspace::{
    FrameClock, Geometry, HostData, KindId, PaneId, Placement, ViewCtx, ViewHost, ViewInput,
};
use studio_arrangement::model::{self, Model};
use studio_arrangement::view::{ArrangeView, Shared};

struct Pane {
    view: ArrangeView,
    ctx: ViewCtx,
    sender: Sender,
    receiver: Receiver,
}

fn pane(m: Model, width: f32, height: f32) -> Pane {
    let ctx = ViewCtx::new(
        PaneId::new("arrangement"),
        KindId::paint(),
        (ViewHost::Blitz, Placement::Pane),
        (ActionList::new(), ActionCaller::app()),
        FrameClock::new(Duration::from_millis(4), || Duration::ZERO),
    )
    .with_geometry(Geometry {
        width,
        height,
        scale: 1.0,
    });
    Pane {
        view: ArrangeView::new(
            HostData::new(m),
            HostData::new(Shared::default()),
            Box::new(|_| {}),
        ),
        ctx,
        sender: Sender::default(),
        receiver: Receiver::new(),
    }
}

/// One frame: the view's layers through the sender into the host. The
/// bytes that crossed and how long the view took to give its layers.
fn frame(p: &mut Pane) -> (usize, Duration) {
    let began = Instant::now();
    let layers: Vec<(LayerKey, Layer)> = p.view.layers(&p.ctx).unwrap();
    let work = began.elapsed();
    let batch = p.sender.frame(&layers).unwrap_or_default();
    if !batch.is_empty() {
        p.receiver.apply(&batch).unwrap();
    }
    (batch.len(), work)
}

/// The host composing the pane at playhead `t`: how long it takes.
fn compose(p: &Pane, t: f64) -> Duration {
    let readings = Readings {
        wall: Some(t),
        playhead: Some(Reading::moving(t, 1.0)),
    };
    let mut scene = Scene::new();
    let began = Instant::now();
    p.receiver
        .layers()
        .compose_at(&mut scene, Affine::IDENTITY, &readings, p.receiver.values());
    began.elapsed()
}

fn wheel(dx: f32, dy: f32, mods: Vec<&'static str>) -> ViewInput {
    ViewInput::Wheel {
        dx,
        dy,
        x: 700.0,
        y: 300.0,
        mods,
    }
}

fn pct(mut v: Vec<Duration>, p: usize) -> f64 {
    v.sort();
    let i = (v.len().saturating_sub(1) * p / 100).min(v.len().saturating_sub(1));
    v.get(i).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

struct Row {
    gesture: &'static str,
    frames: usize,
    bytes: Vec<usize>,
    work: Vec<Duration>,
    host: Vec<Duration>,
}

impl Row {
    fn print(&self) {
        let total: usize = self.bytes.iter().sum();
        println!(
            "| {} | {} | {} | {:.1} | {:.3} / {:.3} | {:.3} / {:.3} |",
            self.gesture,
            self.frames,
            total,
            total as f64 / self.frames.max(1) as f64,
            pct(self.work.clone(), 50),
            pct(self.work.clone(), 99),
            pct(self.host.clone(), 50),
            pct(self.host.clone(), 99),
        );
    }
}

fn run(
    p: &mut Pane,
    gesture: &'static str,
    frames: usize,
    mut input: impl FnMut(usize) -> Option<ViewInput>,
) -> Row {
    let mut row = Row {
        gesture,
        frames,
        bytes: Vec::new(),
        work: Vec::new(),
        host: Vec::new(),
    };
    for i in 0..frames {
        let began = Instant::now();
        if let Some(e) = input(i) {
            p.view.input(&p.ctx, &e);
        }
        let input_time = began.elapsed();
        let (b, w) = frame(p);
        row.bytes.push(b);
        row.work.push(w + input_time);
        row.host.push(compose(p, i as f64 / 240.0));
    }
    row
}

fn measure(name: &str, m: Model, width: f32, height: f32) {
    let (rows, clips) = (m.rows.len(), m.clip_count());
    let mut p = pane(m, width, height);
    let began = Instant::now();
    let (first, work) = frame(&mut p);
    let first_total = began.elapsed();
    println!("\n### {name}: {rows} tracks, {clips} clips, {width}×{height}\n");
    println!(
        "first paint: {first} bytes; the view's work {:.2} ms; with the host's decode {:.2} ms; {} layers",
        work.as_secs_f64() * 1000.0,
        first_total.as_secs_f64() * 1000.0,
        p.receiver.layers().len()
    );
    println!(
        "\n| gesture | frames | bytes | bytes/frame | view work p50 / p99 ms | host compose p50 / p99 ms |"
    );
    println!("|---|---|---|---|---|---|");
    run(&mut p, "playback (the host runs the playhead)", 240, |_| {
        None
    })
    .print();
    run(
        &mut p,
        "pan (wheel, sideways), into tiles not yet seen",
        120,
        |_| Some(wheel(0.0, 8.0, vec!["Shift"])),
    )
    .print();
    run(&mut p, "pan back over tiles already sent", 120, |_| {
        Some(wheel(0.0, -8.0, vec!["Shift"]))
    })
    .print();
    run(
        &mut p,
        "scroll (wheel, down), into tiles not yet seen",
        120,
        |_| Some(wheel(0.0, 6.0, vec![])),
    )
    .print();
    run(
        &mut p,
        "scroll back up over tiles already sent",
        120,
        |_| Some(wheel(0.0, -6.0, vec![])),
    )
    .print();
    run(&mut p, "zoom gesture (Ctrl-wheel)", 60, |i| {
        Some(wheel(
            0.0,
            if i % 2 == 0 { -40.0 } else { 30.0 },
            vec!["Ctrl"],
        ))
    })
    .print();
    std::thread::sleep(studio_arrangement::view::SETTLE + Duration::from_millis(20));
    run(&mut p, "zoom at rest (re-recorded once)", 1, |_| None).print();
}

#[test]
fn playback_sends_nothing_and_a_pan_sends_only_transforms() {
    let mut p = pane(model::fixture(8, 40), 1280.0, 800.0);
    let (first, _) = frame(&mut p);
    assert!(first > 10_000, "the first paint carries the clips: {first}");
    for _ in 0..30 {
        assert_eq!(frame(&mut p).0, 0, "a frame of playback sends nothing");
    }
    p.view.input(&p.ctx, &wheel(0.0, 8.0, vec!["Shift"]));
    let (pan, _) = frame(&mut p);
    assert!(pan > 0 && pan < 1_500, "a pan is transforms: {pan} bytes");
}

#[test]
#[ignore = "a measurement: run with --ignored --nocapture"]
fn measure_8_tracks() {
    measure("8 tracks", model::fixture(8, 40), 1280.0, 800.0);
}

#[test]
#[ignore = "a measurement: run with --ignored --nocapture"]
fn measure_300_tracks_20000_clips() {
    measure("300 tracks", model::fixture(300, 67), 3440.0, 1440.0);
}
