//! The same pixels: a rack panel's close-up as the window's close-up layer
//! paints it and as the `closeup` pane gets it through Architect's layer
//! protocol.
//!
//! One song (the golden template) and one track's panels, each drawn by
//! Studio's own `mixer_view::closeup::RackWidget` twice: `paint_scene`,
//! what the widget hands Blitz in `studio-desktop`'s close-up; and the
//! extension's paint view (`studio_mixer::closeup::HostedRack`), sent as a
//! process sends it (`architect_scene::ops::Sender`) and composed by the
//! host's `Receiver`. Rasterised on the CPU and compared pixel for pixel;
//! a difference (or `PARITY_OUT`) writes both pictures.
//!
//! `cargo test -p studio-mixer --test closeup -- --nocapture`

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::as_conversions,
    clippy::indexing_slicing,
    reason = "a test: measuring, over a raster it made"
)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyrender::PaintScene as _;
use architect::action::{ActionCaller, ActionList};
use architect_scene::anim::Readings;
use architect_scene::ops::{Receiver, Sender};
use architect_workspace::paint::PaintView as _;
use architect_workspace::{FrameClock, Geometry, KindId, PaneId, Placement, ViewCtx, ViewHost};
use dioxus::prelude::*;
use mixer_view::closeup::RackWidget;
use mixer_view::tone::Which;
use peniko::kurbo::Affine;
use session_engine_client::song_model::StudioSession;
use studio_mixer::closeup::HostedRack;
use studio_view_link::{InScope, Stats};

/// The pane: the size `studio-desktop`'s close-up gives its body in a
/// 1280×800 window.
const SIZE: (u32, u32) = (1280, 752);

fn session() -> StudioSession {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/dynamic-template/fixtures/golden/template.rpp");
    // Once a process: the engine opens one song (`cargo test` runs both
    // tests in one process, nextest each in its own).
    static OPEN: std::sync::Once = std::sync::Once::new();
    OPEN.call_once(|| {
        session_engine::open::open_silent(&path).expect("open the golden template");
    });
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

#[derive(Clone, Default)]
struct Got(Rc<RefCell<Option<InScope>>>);

impl PartialEq for Got {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A Dioxus scope for the pane's host to run the widget in.
#[component]
fn Root(got: Got) -> Element {
    use_hook(|| *got.0.borrow_mut() = Some(InScope::here()));
    rsx! { div {} }
}

fn ctx() -> ViewCtx {
    ViewCtx::new(
        PaneId::new("closeup"),
        KindId::paint(),
        (ViewHost::Blitz, Placement::Pane),
        (ActionList::new(), ActionCaller::app()),
        FrameClock::new(Duration::from_millis(4), || Duration::ZERO),
    )
    .with_geometry(Geometry {
        width: SIZE.0 as f32,
        height: SIZE.1 as f32,
        scale: 1.0,
    })
}

fn raster(draw: impl FnOnce(&mut anyrender_vello_cpu::VelloCpuScenePainter)) -> Vec<u8> {
    use anyrender::ImageRenderer as _;
    let mut renderer = anyrender_vello_cpu::VelloCpuImageRenderer::new(SIZE.0, SIZE.1);
    let mut out = Vec::new();
    renderer.render_to_vec(draw, &mut out);
    out
}

fn write_png(path: &std::path::Path, rgba: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), SIZE.0, SIZE.1);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

#[test]
fn the_pane_shows_the_pixels_studio_s_close_up_paints() {
    let song = session();
    let got = Got::default();
    let mut vdom = VirtualDom::new_with_props(Root, RootProps { got: got.clone() });
    vdom.rebuild_in_place();
    let scope = got.0.borrow().clone().expect("a scope");
    // A track with a rack: the first strip the mixer shows.
    let rows = song.planner.plan_strips(&song.planner.raw);
    let tone = studio_mixer::closeup::settings(&song);
    let guid = rows
        .as_slice()
        .iter()
        .map(|(t, _)| t.guid.clone())
        .find(|g| tone.store.borrow().get(g).is_some())
        .expect("a track with a rack");
    for which in [Which::Eq, Which::Comp, Which::Sat] {
        let pane = Rc::new(RefCell::new(RackWidget::new(
            Rc::clone(&tone),
            guid.clone(),
            which,
            false,
        )));
        let mut hosted = HostedRack::new(
            Rc::clone(&pane),
            scope.clone(),
            Arc::new(Mutex::new(Stats::default())),
        );
        let layers = hosted.layers(&ctx()).unwrap();
        let mut sender = Sender::default();
        let mut receiver = Receiver::new();
        receiver.apply(&sender.frame(&layers).unwrap()).unwrap();
        // The window's: the same widget over the same settings, painted
        // as its close-up layer paints it.
        let mut window = RackWidget::new(Rc::clone(&tone), guid.clone(), which, false);
        let in_app = scope.run(|| window.paint_scene(SIZE.0, SIZE.1, 1.0));
        assert!(
            !in_app.commands.is_empty(),
            "{which:?}: the widget painted nothing"
        );
        let a = raster(|p| p.append_scene(in_app.clone(), Affine::IDENTITY));
        let b = raster(|p| {
            receiver
                .layers()
                .compose_at(p, Affine::IDENTITY, &Readings::NONE, receiver.values());
        });
        let differing = a
            .chunks_exact(4)
            .zip(b.chunks_exact(4))
            .filter(|(x, y)| x != y)
            .count();
        println!(
            "close-up {} {}×{}: {} commands, {differing} of {} pixels differ",
            which.slug(),
            SIZE.0,
            SIZE.1,
            in_app.commands.len(),
            SIZE.0 * SIZE.1
        );
        let asked = std::env::var("PARITY_OUT").ok();
        if differing > 0 || asked.is_some() {
            let out =
                std::path::PathBuf::from(asked.unwrap_or_else(|| {
                    format!("{}/../../target/parity", env!("CARGO_MANIFEST_DIR"))
                }))
                .join(format!("closeup-{}", which.slug()));
            std::fs::create_dir_all(&out).unwrap();
            write_png(&out.join("in-app.png"), &a);
            write_png(&out.join("runtime.png"), &b);
        }
        assert_eq!(
            differing, 0,
            "{which:?}: the pane's pixels differ from the window's"
        );
    }
}

/// `SESSION_CLOSEUP` asking another panel of an open pane (`closeup::ask`,
/// what studio.mixer's handler calls): the pane's host shows it from its
/// next frame, the very pixels the window's widget paints for that panel.
#[test]
fn a_panel_asked_for_later_is_the_panes_to_the_pixel() {
    let song = session();
    let got = Got::default();
    let mut vdom = VirtualDom::new_with_props(Root, RootProps { got: got.clone() });
    vdom.rebuild_in_place();
    let scope = got.0.borrow().clone().expect("a scope");
    let rows = song.planner.plan_strips(&song.planner.raw);
    let tone = studio_mixer::closeup::settings(&song);
    let guid = rows
        .as_slice()
        .iter()
        .map(|(t, _)| t.guid.clone())
        .find(|g| tone.store.borrow().get(g).is_some())
        .expect("a track with a rack");
    // Open on the EQ, following asks as the pane does.
    let pane = Rc::new(RefCell::new(RackWidget::new(
        Rc::clone(&tone),
        guid.clone(),
        Which::Eq,
        false,
    )));
    let mut hosted = HostedRack::new(
        Rc::clone(&pane),
        scope.clone(),
        Arc::new(Mutex::new(Stats::default())),
    )
    .following_asks(Rc::clone(&tone));
    let mut sender = Sender::default();
    let mut receiver = Receiver::new();
    receiver
        .apply(&sender.frame(&hosted.layers(&ctx()).unwrap()).unwrap())
        .unwrap();
    // Asked the compressor: the next frame is it.
    studio_mixer::closeup::ask(&guid, Which::Comp);
    if let Some(batch) = sender.frame(&hosted.layers(&ctx()).unwrap()) {
        receiver.apply(&batch).unwrap();
    }
    let mut window = RackWidget::new(Rc::clone(&tone), guid, Which::Comp, false);
    let in_app = scope.run(|| window.paint_scene(SIZE.0, SIZE.1, 1.0));
    let a = raster(|p| p.append_scene(in_app.clone(), Affine::IDENTITY));
    let b = raster(|p| {
        receiver
            .layers()
            .compose_at(p, Affine::IDENTITY, &Readings::NONE, receiver.values());
    });
    let differing = a
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(x, y)| x != y)
        .count();
    println!(
        "close-up asked comp after eq: {differing} of {} pixels differ",
        SIZE.0 * SIZE.1
    );
    assert_eq!(differing, 0, "the pane did not show the panel asked for");
}
