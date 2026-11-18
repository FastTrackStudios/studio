//! The same pixels: Studio's arrangement as its own window paints it, and
//! as the runtime extension's pane gets it through Architect's layer
//! protocol.
//!
//! One song (the golden template), one widget
//! (`session_ui::widget::ArrangementWidget`, built by the panel the app
//! mounts), drawn twice:
//!
//! - **in-app:** `paint_scene`, what the widget hands Blitz in
//!   `studio-desktop`'s window;
//! - **runtime:** the extension's paint view (`studio_arrangement::host`)
//!   gives its layers, they cross as a process's batches do
//!   (`architect_scene::ops::Sender`), and the host's `Receiver` composes
//!   them.
//!
//! Both are rasterised on the CPU and compared pixel for pixel; a
//! difference fails the test and writes both pictures and their
//! difference beside each other (into `target/parity/arrangement`, or
//! `PARITY_OUT`, which writes them on a pass too).
//!
//! `cargo test -p studio-arrangement --test parity -- --nocapture`

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
use std::time::Duration;

use anyrender::{ImageRenderer as _, PaintScene as _};
use architect::action::{ActionCaller, ActionList};
use architect_scene::anim::Readings;
use architect_scene::ops::{Receiver, Sender};
use architect_workspace::paint::PaintView as _;
use architect_workspace::{FrameClock, Geometry, KindId, PaneId, Placement, ViewCtx, ViewHost};
use dioxus::prelude::*;
use peniko::kurbo::Affine;
use session_ui::panel::{ArrangementPanel, PanelEvent};
use session_ui::studio::StudioSession;
use session_ui::widget::ArrangementWidget;
use studio_arrangement::host::{Hosted, InScope, Stats};

const W: u32 = 1280;
const H: u32 = 780;

fn fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/dynamic-template/fixtures/golden/template.rpp")
}

/// The song as `blitz_shot` (the app's headless picture) opens it: on the
/// process's engine with no audio, read back, planned by the app's scene.
fn session() -> StudioSession {
    let path = fixture();
    session_engine::open::open_silent(&path).expect("open the golden template");
    let raw = session_ui::studio::fetch().expect("read it back");
    let planner = session_ui::studio::Planner {
        raw: Arc::new(raw),
        scene: Some("drum-mixing"),
        kinds: Arc::new(session_engine::plan::Kinds::read(&path)),
    };
    let (project, rows) = planner.plan(&planner.raw);
    let previews = session_ui::studio::previews_of(&project);
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

/// The panel and its widget, as the extension's pane builds them (the
/// window's mode Live, the track panel compact).
#[component]
fn Root(song: Song, got: Got) -> Element {
    use_context_provider(|| song.0.clone());
    use_context_provider(|| Signal::new(session::modes::Mode::Live));
    use_context_provider(|| session_ui::panel::StartCompact(true));
    let engine: session_ui::panel::Engine = Rc::new(|_| {});
    let (panel, widget) =
        session_ui::panel::use_arrangement_panel(None, engine, |w| Rc::new(RefCell::new(w)));
    use_hook(|| {
        *got.0.borrow_mut() = Some((
            widget,
            panel,
            InScope {
                runtime: dioxus::core::Runtime::current(),
                scope: dioxus::core::current_scope_id(),
            },
        ));
    });
    rsx! { div {} }
}

fn ctx() -> ViewCtx {
    ViewCtx::new(
        PaneId::new("arrangement"),
        KindId::paint(),
        (ViewHost::Blitz, Placement::Pane),
        (ActionList::new(), ActionCaller::app()),
        FrameClock::new(Duration::from_millis(4), || Duration::ZERO),
    )
    .with_geometry(Geometry {
        width: W as f32,
        height: H as f32,
        scale: 1.0,
    })
}

/// `draw` rasterised on the CPU: RGBA bytes.
fn raster(
    draw: impl FnOnce(
        &mut <anyrender_vello_cpu::VelloCpuImageRenderer as anyrender::ImageRenderer>::ScenePainter<
            '_,
        >,
    ),
) -> Vec<u8> {
    let mut renderer = anyrender_vello_cpu::VelloCpuImageRenderer::new(W, H);
    let mut out = Vec::new();
    renderer.render_to_vec(draw, &mut out);
    out
}

fn write_png(path: &std::path::Path, rgba: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

#[test]
fn the_pane_shows_the_pixels_studio_s_window_paints() {
    let song = session();
    let got = Got::default();
    let mut vdom = VirtualDom::new_with_props(
        Root,
        RootProps {
            song: Song(song),
            got: got.clone(),
        },
    );
    vdom.rebuild_in_place();
    let (widget, panel, scope) = got.0.borrow().clone().expect("the panel mounted");

    // The extension's paint view over the same widget: what the pane runs.
    let stats = Arc::new(Mutex::new(Stats::default()));
    let mut hosted = Hosted::new(Rc::clone(&widget), panel.clone(), scope.clone(), stats);
    let ctx = ctx();
    // Settled: the song fitted on its first paint, the panel's frames run
    // until the scene stops changing.
    let mut last = None;
    for _ in 0..30 {
        let layers = hosted.layers(&ctx).unwrap();
        let content = Arc::clone(&layers[0].1.commands);
        if last
            .as_ref()
            .is_some_and(|l: &Arc<_>| Arc::ptr_eq(l, &content))
        {
            break;
        }
        last = Some(content);
    }

    // In-app: what the widget hands Blitz, painted now at the same state.
    let in_app = scope.run(|| {
        panel.handle(PanelEvent::Frame { play_at: 0.0 });
        widget.borrow_mut().paint_scene(W, H, 1.0)
    });
    assert!(!in_app.commands.is_empty(), "the widget painted nothing");

    // Runtime: the pane's layers (the picture, the play cursor over it)
    // through the protocol a process uses.
    let layers = hosted.layers(&ctx).unwrap();
    assert_eq!(layers.len(), 2, "the picture and the play cursor");
    let mut sender = Sender::default();
    let batch = sender.frame(&layers).unwrap();
    let mut receiver = Receiver::new();
    receiver.apply(&batch).unwrap();

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
        "arrangement {W}×{H}: {} commands, a batch of {} bytes, {differing} of {} pixels differ",
        in_app.commands.len(),
        batch.len(),
        W * H
    );
    // A non-trivial picture: most of it is not one colour.
    let first = &a[..4];
    let other = a.chunks_exact(4).filter(|p| *p != first).count();
    assert!(
        other > a.len() / 16,
        "the widget painted (almost) one colour"
    );
    let asked = std::env::var("PARITY_OUT").ok();
    if differing > 0 || asked.is_some() {
        let out = std::path::PathBuf::from(asked.unwrap_or_else(|| {
            format!(
                "{}/../../target/parity/arrangement",
                env!("CARGO_MANIFEST_DIR")
            )
        }));
        std::fs::create_dir_all(&out).unwrap();
        write_png(&out.join("in-app.png"), &a);
        write_png(&out.join("runtime.png"), &b);
        let diff: Vec<u8> = a
            .chunks_exact(4)
            .zip(b.chunks_exact(4))
            .flat_map(|(x, y)| {
                if x == y {
                    [0, 0, 0, 255]
                } else {
                    [255, 0, 255, 255]
                }
            })
            .collect();
        write_png(&out.join("diff.png"), &diff);
        println!("written to {}", out.display());
    }
    assert_eq!(differing, 0, "the pane's pixels differ from the window's");
}
