//! The same pixels: Studio's mixer, and its selected-track strip, as the
//! window's widget paints them and as the extension's panes get them
//! through Architect's layer protocol.
//!
//! One song (the golden template), the mixer's own widget
//! (`mixer_view::mixer_panel::MixerWidget`, over the strips the window
//! plans), drawn twice: `paint_scene`, what the widget hands Blitz in
//! `studio-desktop`'s window, meters and all; and the extension's paint
//! view (`studio_mixer::host`): the picture with its meters fully lit, the
//! meters' parts moved by the engine's values and the caps over them, sent
//! as a process sends them (`architect_scene::ops::Sender`) and composed by
//! the host's `Receiver` with the levels in an engine value table, as the
//! `daw` extension writes them. Rasterised on the CPU and compared pixel for pixel; a
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

use anyrender::PaintScene as _;
use architect::action::{ActionCaller, ActionList};
use architect_scene::anim::Name;
use architect_scene::anim::Readings;
use architect_scene::engine::{EngineTable, EngineWriter};
use architect_scene::ops::{Receiver, Sender};
use architect_workspace::paint::PaintView as _;
use architect_workspace::{FrameClock, Geometry, KindId, PaneId, Placement, ViewCtx, ViewHost};
use dioxus::prelude::*;
use engine_client::song_model::StudioSession;
use mixer_view::mixer_panel::{HostedMixer, Links};
use peniko::kurbo::Affine;
use studio_engine::engine::{
    METER_PARTS, METER_TRACKS, TRACK_PARTS, VALUE_TRACKS, meter_fraction, meter_value, track_value,
};
use studio_mixer::host::Hosted;
use studio_view_link::{InScope, Stats};

fn fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../studio-engine/adapters/dynamic-template/fixtures/golden/template.rpp")
}

/// The song as `blitz_shot` (the app's headless picture) opens it.
fn session() -> StudioSession {
    let path = fixture();
    studio_engine::open::open_silent(&path).expect("open the golden template");
    let mut raw = daw_view_kit::studio::fetch().expect("read it back");
    // Some lit: muted, soloed, both (the buttons lit by the engine's
    // values are what is compared).
    for (i, (mute, solo)) in [
        (0, (true, false)),
        (5, (false, true)),
        (11, (true, true)),
        (1, (true, false)),
        (2, (false, true)),
        (3, (true, true)),
    ] {
        if let Some(t) = raw.tracks.get_mut(i) {
            t.muted = mute;
            t.soloed = solo;
        }
    }
    let planner = engine_client::song_model::template_planner(
        Arc::new(raw),
        Some("drum-mixing"),
        Arc::new(engine_client::plan::Kinds::read(&path)),
    );
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
    use_context_provider(|| Signal::new(engine_domain::modes::Mode::Live));
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

/// The panel's backdrop: its `surface`, what the mixer's pane is filled
/// with under the widget in the window and in Architect alike.
fn backdrop(p: &mut anyrender_vello_cpu::VelloCpuScenePainter, (w, h): (u32, u32)) {
    let surface =
        daw_view_kit::palette::Palette::from_theme(&daw_ui::theming::Theme::dark()).surface;
    p.fill(
        peniko::Fill::NonZero,
        Affine::IDENTITY,
        surface,
        None,
        &peniko::kurbo::Rect::new(0.0, 0.0, f64::from(w), f64::from(h)),
    );
}

/// The engine's meter and button values, as the `daw` extension
/// publishes them.
fn table() -> (Arc<EngineTable>, EngineWriter) {
    let names: Vec<Name> = (0..METER_TRACKS)
        .flat_map(|i| METER_PARTS.iter().map(move |p| meter_value(i, p)))
        .chain((0..VALUE_TRACKS).flat_map(|i| TRACK_PARTS.iter().map(move |p| track_value(i, p))))
        .map(|n| Name::new(&n).unwrap())
        .collect();
    EngineTable::new(names).unwrap()
}

/// `tracks`' mute and solo into the table, as the `daw` extension writes
/// them from the engine's track events.
fn write_buttons(table: &EngineTable, writer: &EngineWriter, tracks: &[daw_proto::Track]) {
    for t in tracks {
        let Ok(i) = usize::try_from(t.index) else {
            continue;
        };
        for (part, on) in TRACK_PARTS.iter().zip([t.muted, t.soloed]) {
            if let Some(slot) = Name::new(&track_value(i, part))
                .ok()
                .and_then(|n| table.slot(&n))
            {
                writer.write(slot, if on { 1.0 } else { 0.0 });
            }
        }
    }
}

/// `levels` into the table, as the engine writes them.
fn write_levels(table: &EngineTable, writer: &EngineWriter, levels: &[daw_proto::TrackLevels]) {
    for i in 0..METER_TRACKS {
        let l = levels.get(i).copied().unwrap_or_default();
        let parts = [
            meter_fraction(l.peak_left),
            meter_fraction(l.peak_right),
            meter_fraction(l.hold_left),
            meter_fraction(l.hold_right),
            meter_fraction(l.peak_left.max(l.peak_right)),
        ];
        for (part, v) in METER_PARTS.iter().zip(parts) {
            let slot = table
                .slot(&Name::new(&meter_value(i, part)).unwrap())
                .unwrap();
            writer.write(slot, v);
        }
    }
}

/// How far apart two pictures are: pixels that differ, those by more than
/// a step of 24 in a channel, and the largest step.
fn apart(a: &[u8], b: &[u8]) -> (usize, usize, u8) {
    let mut out = (0, 0, 0);
    for (x, y) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        let d = x
            .iter()
            .zip(y)
            .map(|(p, q)| p.abs_diff(*q))
            .max()
            .unwrap_or(0);
        if d > 0 {
            out.0 += 1;
        }
        if d > 24 {
            out.1 += 1;
        }
        out.2 = out.2.max(d);
    }
    out
}

/// One view at `size`, over `frames` frames (the signal simulated and
/// held at each frame's moment when `playing`): each frame the window's
/// picture against the pane's layers composed at the levels it was drawn
/// at. Answers the worst frame's (differing, by more than 24, largest
/// step) and the bytes each frame after the first sent.
fn same_pixels(
    view: &str,
    song: &StudioSession,
    inspector: bool,
    size: (u32, u32),
    playing: bool,
) -> ((usize, usize, u8), Vec<usize>) {
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
    if playing {
        mixer.simulate();
    }
    let stats = Arc::new(Mutex::new(Stats::default()));
    let mut hosted =
        Hosted::new(Rc::clone(&mixer), scope.clone(), stats).with_buttons_lit_by_values(true);
    let ctx = ctx(size);
    let (table, writer) = table();
    let mut sender = Sender::default();
    let mut receiver = Receiver::new();
    receiver.grant(Some(Arc::clone(&table)));
    write_buttons(&table, &writer, &song.planner.raw.tracks);
    let frames = if playing { 30 } else { 6 };
    let mut worst = (0, 0, 0);
    let mut bytes = Vec::new();
    let mut moved = 0;
    let mut first_picture: Option<Vec<u8>> = None;
    for i in 0..frames {
        let at = 1.0 + f64::from(i) / 30.0;
        mixer.hold_simulation(playing.then_some(at));
        let layers = hosted.layers(&ctx).unwrap();
        let sent = sender.frame(&layers).map_or(0, |b| {
            receiver.apply(&b).unwrap();
            b.len()
        });
        if i > 0 {
            bytes.push(sent);
        }
        write_levels(&table, &writer, &mixer.painted_levels());
        // The window's picture of the same moment.
        let in_app = scope.run(|| mixer.widget.borrow_mut().paint_scene(size.0, size.1, 1.0));
        // Over the panel's backdrop, as both the window and the pane draw
        // it (the strips' ground is a wash over it).
        let a = raster(size, |p| {
            backdrop(p, size);
            p.append_scene(in_app.clone(), Affine::IDENTITY);
        });
        let b = raster(size, |p| {
            backdrop(p, size);
            receiver
                .layers()
                .compose_at(p, Affine::IDENTITY, &Readings::NONE, receiver.values());
        });
        let d = apart(&a, &b);
        if d.0 > worst.0 {
            worst = d;
        }
        match &first_picture {
            None => first_picture = Some(a.clone()),
            Some(f) if *f != a => moved += 1,
            Some(_) => {}
        }
        let last = i == frames - 1;
        if (last && std::env::var("PARITY_OUT").is_ok()) || (d.0 > 0 && d.1 > 0 && last) {
            let base = std::env::var("PARITY_OUT")
                .unwrap_or_else(|_| format!("{}/../../target/parity", env!("CARGO_MANIFEST_DIR")));
            let out = std::path::Path::new(&base)
                .join(format!("{view}{}", if playing { "-playing" } else { "" }));
            std::fs::create_dir_all(&out).unwrap();
            write_png(&out.join("in-app.png"), size, &a);
            write_png(&out.join("runtime.png"), size, &b);
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
            write_png(&out.join("diff.png"), size, &diff);
        }
    }
    let first = first_picture.unwrap();
    let other = first.chunks_exact(4).filter(|p| *p != &first[..4]).count();
    assert!(
        other > first.len() / 64,
        "{view}: the widget painted (almost) one colour"
    );
    if playing {
        assert!(
            moved > frames as usize / 2,
            "{view}: the meters did not move"
        );
    }
    println!(
        "{view} {}x{}{}: worst frame {} of {} pixels apart ({} by more than 24, largest step {}); bytes a frame after the first: {:?}",
        size.0,
        size.1,
        if playing { " playing" } else { "" },
        worst.0,
        size.0 * size.1,
        worst.1,
        worst.2,
        bytes
    );
    (worst, bytes)
}

/// Whether every strip `view` shows is a track the engine publishes meter
/// values for (`METER_TRACKS`): a track past them keeps its meter in the
/// picture, so a playing frame of it crosses.
fn all_published(song: &StudioSession, inspector: bool) -> bool {
    inspector
        || song
            .planner
            .plan_strips(&song.planner.raw)
            .as_slice()
            .iter()
            .all(|(t, _)| usize::try_from(t.index).is_ok_and(|i| i < METER_TRACKS))
}

#[test]
fn the_panes_show_the_pixels_studio_s_window_paints() {
    let song = session();
    // The dock under the arrangement, the height the setlist window gives
    // it; and the inspector's column. Stopped, then playing.
    for (view, inspector, size) in [("mixer", false, (1280, 400)), ("strip", true, (120, 640))] {
        let pixels = (size.0 * size.1) as usize;
        let (stopped, _) = same_pixels(view, &song, inspector, size, false);
        assert_eq!(
            stopped.1, 0,
            "{view}: stopped, a pixel differs by more than 24"
        );
        let (playing, bytes) = same_pixels(view, &song, inspector, size, true);
        // Playing, what is left is where a cover's soft edge meets the
        // window's (the level's, a hold's) and the order two transparent
        // paints blend in inside a cap's pane: a pixel in a thousand.
        assert!(
            playing.1 * 1000 <= pixels,
            "{view}: playing, {} pixels differ by more than 24",
            playing.1
        );
        // Playing, the meters move by values: nothing crosses, for the
        // tracks the engine publishes.
        if all_published(&song, inspector) {
            assert!(
                bytes.iter().all(|b| *b == 0),
                "{view}: a playing frame sent {bytes:?} bytes"
            );
        } else {
            println!("{view}: strips past {METER_TRACKS} tracks keep their meters in the picture");
        }
    }
}
