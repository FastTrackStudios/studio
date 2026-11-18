//! The same pixels: Studio's mixer, and its selected-track strip, as the
//! window's widget paints them and as the extension's panes get them
//! through Architect's layer protocol.
//!
//! One song (the golden template), the mixer's own widget
//! (`session_ui::mixer_panel::MixerWidget`, over the strips the window
//! plans), drawn twice: `paint_scene`, what the widget hands Blitz in
//! `studio-desktop`'s window; and the extension's paint view
//! (`studio_mixer::host`), its layers sent as a process sends them
//! (`architect_scene::ops::Sender`) and composed by the host's
//! `Receiver`. Rasterised on the CPU and compared pixel for pixel; a
//! difference writes both pictures and their difference (into
//! `target/parity/<view>`, or `PARITY_OUT/<view>`, which writes them on a
//! pass too).
//!
//! `cargo test -p studio-mixer --test parity -- --nocapture`

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
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
use peniko::kurbo::Affine;
use session_ui::mixer_panel::{HostedMixer, Links};
use session_ui::studio::StudioSession;
use studio_mixer::host::Hosted;
use studio_view_link::{InScope, Stats};

fn fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/dynamic-template/fixtures/golden/template.rpp")
}

/// The song as `blitz_shot` (the app's headless picture) opens it.
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

#[derive(Clone)]
struct Song(StudioSession);

impl PartialEq for Song {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

type Built = (Rc<HostedMixer>, InScope);

#[derive(Clone, Default)]
struct Got(Rc<RefCell<Option<Built>>>);

impl PartialEq for Got {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The widget as the extension's pane builds it: the window's mode Live,
/// every strip (`alone`), or the inspector's.
#[component]
fn Root(song: Song, inspector: bool, got: Got) -> Element {
    let session = use_context_provider(|| song.0.clone());
    use_context_provider(|| Signal::new(session::modes::Mode::Live));
    use_hook(|| {
        let rows = session
            .planner
            .plan_strips(&session.planner.raw)
            .as_slice()
            .to_vec();
        let mut links = Links::alone(rows, std::collections::HashMap::new());
        links.planner = Some(session.planner.clone());
        links.inspector = inspector;
        *got.0.borrow_mut() = Some((Rc::new(HostedMixer::new(links)), InScope::here()));
    });
    rsx! { div {} }
}

fn ctx((w, h): (u32, u32)) -> ViewCtx {
    ViewCtx::new(
        PaneId::new("mixer"),
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

fn raster(
    (w, h): (u32, u32),
    draw: impl FnOnce(&mut anyrender_vello_cpu::VelloCpuScenePainter),
) -> Vec<u8> {
    use anyrender::ImageRenderer as _;
    let mut renderer = anyrender_vello_cpu::VelloCpuImageRenderer::new(w, h);
    let mut out = Vec::new();
    renderer.render_to_vec(draw, &mut out);
    out
}

fn write_png(path: &std::path::Path, (w, h): (u32, u32), rgba: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

/// One view at `size`: the window's paint against the pane's layers.
fn same_pixels(view: &str, song: &StudioSession, inspector: bool, size: (u32, u32)) {
    let got = Got::default();
    let mut vdom = VirtualDom::new_with_props(
        Root,
        RootProps {
            song: Song(song.clone()),
            inspector,
            got: got.clone(),
        },
    );
    vdom.rebuild_in_place();
    let (mixer, scope) = got.0.borrow().clone().expect("the widget built");
    let stats = Arc::new(Mutex::new(Stats::default()));
    let mut hosted = Hosted::new(Rc::clone(&mixer), scope.clone(), stats);
    let ctx = ctx(size);
    // Settled: the strips recorded on the first paint, a few more frames.
    for _ in 0..5 {
        hosted.layers(&ctx).unwrap();
    }
    let in_app = scope.run(|| mixer.widget.borrow_mut().paint_scene(size.0, size.1, 1.0));
    assert!(
        !in_app.commands.is_empty(),
        "{view}: the widget painted nothing"
    );
    let layers = hosted.layers(&ctx).unwrap();
    assert_eq!(
        layers[0].1.commands.as_slice(),
        in_app.commands.as_slice(),
        "{view}: the pane's layer is the widget's scene, command for command"
    );
    let mut sender = Sender::default();
    let batch = sender.frame(&layers).unwrap();
    let mut receiver = Receiver::new();
    receiver.apply(&batch).unwrap();
    let a = raster(size, |p| p.append_scene(in_app.clone(), Affine::IDENTITY));
    let b = raster(size, |p| {
        receiver
            .layers()
            .compose_at(p, Affine::IDENTITY, &Readings::NONE, receiver.values());
    });
    let differing = a
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(x, y)| x != y)
        .count();
    let first = &a[..4];
    let other = a.chunks_exact(4).filter(|p| *p != first).count();
    println!(
        "{view} {}×{}: {} commands, a batch of {} bytes, {differing} of {} pixels differ",
        size.0,
        size.1,
        in_app.commands.len(),
        batch.len(),
        size.0 * size.1
    );
    let asked = std::env::var("PARITY_OUT").ok();
    if differing > 0 || asked.is_some() {
        let base =
            asked.unwrap_or_else(|| format!("{}/../../target/parity", env!("CARGO_MANIFEST_DIR")));
        let out = std::path::Path::new(&base).join(view);
        std::fs::create_dir_all(&out).unwrap();
        write_png(&out.join("in-app.png"), size, &a);
        write_png(&out.join("runtime.png"), size, &b);
        println!("written to {}", out.display());
    }
    assert!(
        other > a.len() / 64,
        "{view}: the widget painted (almost) one colour"
    );
    assert_eq!(
        differing, 0,
        "{view}: the pane's pixels differ from the window's"
    );
}

#[test]
fn the_panes_show_the_pixels_studio_s_window_paints() {
    let song = session();
    // The dock under the arrangement, the height the setlist window gives
    // it; and the inspector's column.
    same_pixels("mixer", &song, false, (1280, 400));
    same_pixels("strip", &song, true, (120, 640));
}
