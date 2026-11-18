//! What the mixer sends its host per frame while the engine plays (its
//! meters), while a fader is dragged, and while it scrolls: the real view
//! through the `ops::Sender` a process uses, the drag's value written the
//! way the host's region code writes it.
//!
//! `cargo test -p studio-mixer --test measure -- --ignored --nocapture`

#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::cast_precision_loss)]

use std::time::Duration;

use architect::action::{ActionCaller, ActionList};
use architect_scene::anim::{Name, Readings, Space};
use architect_scene::ops::{Receiver, Sender};
use architect_workspace::paint::peniko::kurbo::Affine;
use architect_workspace::paint::{PaintView, Scene};
use architect_workspace::{
    FrameClock, Geometry, HostData, KindId, PaneId, Placement, ViewCtx, ViewHost, ViewInput,
};
use daw_view::{Track, fader, names};
use studio_mixer::view::{MixerView, Model, Shape, Shared};

fn ctx(w: f32, h: f32) -> ViewCtx {
    ViewCtx::new(
        PaneId::new("mixer"),
        KindId::paint(),
        (ViewHost::Blitz, Placement::Pane),
        (ActionList::new(), ActionCaller::app()),
        FrameClock::new(Duration::from_millis(4), || Duration::ZERO),
    )
    .with_geometry(Geometry {
        width: w,
        height: h,
        scale: 1.0,
    })
}

fn tracks(n: u32) -> Vec<Track> {
    (0..n)
        .map(|i| Track {
            lane: format!("t{i}"),
            index: i,
            name: format!("Track {}", i + 1),
            volume: fader::UNITY,
            ..Track::default()
        })
        .collect()
}

fn view(n: u32, shape: Shape) -> MixerView {
    let model = HostData::new(Model {
        revision: 1,
        tracks: tracks(n),
        engine: true,
        problem: None,
    });
    MixerView::new(
        shape,
        model,
        HostData::new(Shared::default()),
        Box::new(|_| {}),
    )
}

fn measure(n: u32, shape: Shape, w: f32, h: f32) {
    let mut view = view(n, shape);
    let ctx = ctx(w, h);
    let (mut sender, mut receiver) = (Sender::default(), Receiver::new());
    let mut send = |view: &mut MixerView, receiver: &mut Receiver| {
        let layers = view.layers(&ctx).unwrap();
        let values = view.values(&ctx);
        let b = sender.frame_with(&layers, &values).unwrap_or_default();
        if !b.is_empty() {
            receiver.apply(&b).unwrap();
        }
        b.len()
    };
    let first = send(&mut view, &mut receiver);
    // The engine's meters: written to its table, read at compose; nothing
    // asks the view, so its frames send nothing.
    let meters: usize = (0..240).map(|_| send(&mut view, &mut receiver)).sum();
    // A fader dragged: the host writes its value each frame (as the
    // region code does) and composes; the view sends nothing.
    let vol = Name::new(&names::volume(0)).unwrap();
    let mut drag = 0;
    let began = std::time::Instant::now();
    for i in 0..120 {
        receiver
            .values_mut()
            .write(Space::Host, &vol, f64::from(i) / 120.0)
            .unwrap();
        let mut scene = Scene::new();
        receiver.layers().compose_at(
            &mut scene,
            Affine::IDENTITY,
            &Readings::NONE,
            receiver.values(),
        );
        drag += send(&mut view, &mut receiver);
    }
    let compose = began.elapsed().as_secs_f64() * 1000.0 / 120.0;
    // Scrolling: one value a frame, whatever the track count.
    let mut scroll = 0;
    for _ in 0..120 {
        view.input(
            &ctx,
            &ViewInput::Wheel {
                dx: 0.0,
                dy: 6.0,
                x: 100.0,
                y: 100.0,
                mods: vec![],
            },
        );
        scroll += send(&mut view, &mut receiver);
    }
    println!(
        "| {n} | {} | {first} | {} | {meters} | {drag} | {:.1} | {compose:.3} |",
        if shape == Shape::Strips {
            "mixer"
        } else {
            "tcp"
        },
        receiver.layers().len(),
        scroll as f64 / 120.0,
    );
}

#[test]
fn a_dragged_fader_and_moving_meters_send_nothing() {
    let mut view = view(8, Shape::Strips);
    let ctx = ctx(1280.0, 600.0);
    let mut sender = Sender::default();
    let first = sender
        .frame_with(&view.layers(&ctx).unwrap(), &view.values(&ctx))
        .unwrap_or_default()
        .len();
    assert!(first > 0);
    for _ in 0..60 {
        let b = sender
            .frame_with(&view.layers(&ctx).unwrap(), &view.values(&ctx))
            .unwrap_or_default();
        assert!(b.is_empty(), "nothing moved that is the view's");
    }
}

#[test]
#[ignore = "a measurement: run with --ignored --nocapture"]
fn measure_mixer() {
    println!(
        "\n| tracks | view | first paint B | layers | 240 frames of meters B | 120 frames of a fader drag B | scroll B/frame | host compose ms/frame (drag) |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    measure(8, Shape::Strips, 1280.0, 600.0);
    measure(300, Shape::Strips, 3440.0, 600.0);
    measure(8, Shape::Rows, 480.0, 800.0);
    measure(300, Shape::Rows, 480.0, 1440.0);
}
