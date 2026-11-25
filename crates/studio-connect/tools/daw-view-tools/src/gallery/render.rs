//! One shot of the golden session, drawn: each view through the painter
//! its window mounts, into a CPU raster (no GPU, so the same pixels on
//! every machine).

use std::cell::RefCell;
use std::rc::Rc;

use anyrender::{ImageRenderer, PaintScene};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use daw_theme::gallery::{Frame, Overlay, Shot, Strips, View};
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Color, Fill};

use arrangement_view::arrangement::Palette;
use arrangement_view::widget::{ArrangementWidget, View as ArrangeView};

use crate::golden::Golden;

/// A shot, drawn: `width × height` RGBA8 pixels.
#[derive(Clone, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// What every shot of one theme draws with.
pub struct Stage {
    pub palette: Palette,
    pub font: daw_view_kit::text::Font,
    pub layout: daw_view_kit::layout::Layout,
}

/// The golden session planned the way the windows plan it: its rows with
/// the shut folders folded, its strips, its previews.
pub struct Planned {
    pub project: daw_ui::studio::ProjectRef,
    pub rows: daw_ui::studio::RowsRef,
    pub strips: daw_ui::studio::RowsRef,
    pub previews: daw_view_kit::midi::Previews,
}

/// The phase the mixer's racks are shown in.
const TONE: fts_session::mix_phases::MixPhase = fts_session::mix_phases::MixPhase::Tone;

/// Draw `shot` of `golden`.
#[must_use]
pub fn shot(golden: &Golden, planned: &Planned, stage: &Stage, shot: &Shot) -> Picture {
    let (w, h) = (shot.size.width, shot.size.height);
    match shot.view {
        View::Arrangement => raster(w, h, stage.palette.surface, |p| {
            arrangement(p, golden, planned, shot);
        }),
        View::Mixer => raster(w, h, stage.palette.surface, |p| {
            mixer(p, golden, planned, stage, shot);
        }),
        View::Fx => raster(w, h, stage.palette.surface, |p| fx(p, planned, shot)),
        View::Drums | View::Piano | View::Controllers | View::Vocal | View::Stack => {
            raster(w, h, stage.palette.surface, |p| {
                editor(p, golden, stage, shot);
            })
        }
    }
}

/// Paint into a fresh raster over the theme's ground.
fn raster(
    width: u32,
    height: u32,
    ground: Color,
    draw: impl FnOnce(&mut <VelloCpuImageRenderer as ImageRenderer>::ScenePainter<'_>),
) -> Picture {
    let mut renderer = VelloCpuImageRenderer::new(width, height);
    let mut rgba = Vec::new();
    renderer.render_to_vec(
        |painter| {
            painter.reset();
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                ground,
                None,
                &Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            );
            draw(painter);
        },
        &mut rgba,
    );
    Picture {
        width,
        height,
        rgba,
    }
}

/// The arrangement: the widget the window mounts, framed as the shot says.
fn arrangement(painter: &mut impl PaintScene, golden: &Golden, planned: &Planned, shot: &Shot) {
    let (w, h) = (shot.size.width, shot.size.height);
    let view: arrangement_view::widget::Shared = Rc::new(RefCell::new(ArrangeView::OPENING));
    let compact = w < 600;
    let mut widget = ArrangementWidget::for_session(
        &planned.project,
        &planned.rows,
        &planned.previews,
        compact,
        Rc::clone(&view),
        false,
    )
    .with_cursor(golden.cursor)
    // The frame is the shot's, not the window's opening fit.
    .with_fit_on_open(false);
    *view.borrow_mut() = arrange_view(&widget, planned, shot.frame, (w, golden.play_at));
    painter.append_scene(widget.paint_scene(w, h, 1.0), Affine::IDENTITY);
}

/// The arrangement's view for a frame: the span across the lanes, the
/// first track at the top.
fn arrange_view(
    widget: &ArrangementWidget,
    planned: &Planned,
    frame: Frame,
    (width, play_at): (u32, f64),
) -> ArrangeView {
    let scene = widget.scene();
    let lanes = (f64::from(width) - scene.tcp.width()).max(1.0);
    let pps = daw_view_kit::studio::PPS;
    let zoom_x = frame
        .time
        .map_or(1.0, |span| lanes / ((span.to - span.from).max(0.1) * pps));
    let scroll_x = frame.time.map_or(0.0, |span| span.from * pps * zoom_x);
    let scroll_y = first_row(planned.rows.as_slice(), frame.first)
        .and_then(|row| scene.row_box(row))
        .map_or(0.0, |(top, _)| top * frame.zoom_y);
    ArrangeView {
        scroll_x,
        scroll_y,
        zoom_x,
        zoom_y: frame.zoom_y,
        play_at,
    }
}

/// The row of the track named `first`, if the frame names one.
fn first_row(rows: &[(daw_proto::Track, u32)], first: Option<&str>) -> Option<usize> {
    let name = first?;
    rows.iter().position(|(t, _)| t.name == name)
}

/// The mixer: the recorded strips, the live controls over them with the
/// frozen meters, the Tone racks and the kick's Signal inserts, and the
/// rails, as the window draws them.
fn mixer(
    painter: &mut impl PaintScene,
    golden: &Golden,
    planned: &Planned,
    stage: &Stage,
    shot: &Shot,
) {
    use mixer_view::rails;

    let (w, h) = (f64::from(shot.size.width), f64::from(shot.size.height));
    let rows = &planned.strips;
    let tracks: Vec<daw_proto::Track> = rows.as_slice().iter().map(|(t, _)| t.clone()).collect();
    let mut store = mixer_view::tone::Store::default();
    store.seed(rows.as_slice());
    store.prerender_tails();
    let live_strips = shot.frame.strips == Strips::Live;
    let racks = if live_strips {
        &[][..]
    } else {
        mixer_view::tone::panels_for(TONE)
    };
    let settings = studio_engine::settings::Settings {
        live_strips,
        ..studio_engine::settings::Settings::default()
    };
    let built = mixer_view::mcp::Mixer::build(
        &stage.palette,
        &stage.font,
        &planned.project,
        rows,
        h - rails::TOP,
        stage.layout,
        racks,
        true,
        settings,
        &store,
    );
    let frame = rails::Frame::new(w, h);
    let scroll_x = first_row(rows.as_slice(), shot.frame.first)
        .and_then(|row| built.strip_box(row))
        .map_or(0.0, |(left, _, _)| left);
    let (mut history, mut spectra) = rack_signal(&tracks, &store);
    let mut inserts = signal_inserts(golden);
    let fold = mixer_view::tone::Fold::rest();
    let map = studio_engine::plan::Rows::of(rows.as_slice(), &tracks);
    let clips = clip_latches(golden);
    let at = Affine::translate((rails::SIDE - scroll_x, rails::TOP));
    built.replay(painter, scroll_x, frame.content_width(), at);
    mixer_view::strip_overlay::controls(
        painter,
        &stage.palette,
        &stage.font,
        &built,
        &tracks,
        &map,
        &daw_view_kit::pointer::Pointer::default(),
        &golden.meters.levels,
        &clips,
        0.0,
        &mut mixer_view::strip_overlay::Racks {
            settings: &store,
            history: &mut history,
            spectra: &mut spectra,
            lit: None,
            panels: racks,
            folded: &fold,
            signal: Some(&mut inserts),
        },
        scroll_x,
        frame.content_width(),
        at,
        None,
    );
    let profile = rails::profile(
        rails::Surface::Mixer,
        mixer_view::app::MIX,
        TONE,
        None,
        studio_engine::settings::Settings::default(),
        mixer_view::app::Audience::Engineer,
        "drums",
    );
    rails::draw(
        painter,
        &stage.palette,
        &stage.font,
        &mut daw_view_kit::icons::Icons::none(),
        (None, None),
        frame,
        &profile.left,
        &profile.right,
        &profile.top,
    );
}

/// The racks' displays as two and a half seconds of the simulated signal
/// leave them: a function of the track and the instant, no clock.
fn rack_signal(
    tracks: &[daw_proto::Track],
    store: &mixer_view::tone::Store,
) -> (
    std::collections::HashMap<String, mixer_view::tone::Levels>,
    std::collections::HashMap<String, mixer_view::tone::Analyser>,
) {
    const AT: f64 = 2.5;
    let mut history = std::collections::HashMap::new();
    let mut spectra = std::collections::HashMap::new();
    for (i, track) in tracks.iter().enumerate() {
        let Some(tone) = store.get(&track.guid) else {
            continue;
        };
        let entry: &mut mixer_view::tone::Levels = history.entry(track.guid.clone()).or_default();
        let mut last = None;
        for k in 0..mixer_view::tone::HISTORY {
            let back = daw_view_kit::num::coord(
                mixer_view::tone::HISTORY
                    .saturating_sub(1)
                    .saturating_sub(k),
            );
            let t = AT - back / f64::from(mixer_view::tone::PUBLISH_HZ);
            let meters = mixer_view::simulate::meters(i, t, tone);
            entry.push(meters.sat_peak);
            entry.push_fire(meters.deess_deepest());
            entry.push_ess(meters.ess_db, meters.ess_ref_db);
            last = Some(meters);
        }
        if let Some(meters) = last {
            spectra
                .entry(track.guid.clone())
                .or_insert_with(mixer_view::tone::Analyser::default)
                .set(meters);
        }
    }
    (history, spectra)
}

/// The kick's Signal chains as the mixer's inserts, In shown shut.
fn signal_inserts(golden: &Golden) -> mixer_view::signal::Inserts {
    use mixer_view::signal::source::MeterValue;
    use mixer_view::signal::{Fixture, Fold, Inserts, SignalSource};

    let fixture = std::sync::Arc::new(Fixture::new(golden.signal.clone()));
    for (path, chain) in &golden.signal {
        let meters = chain
            .meter_indices()
            .map(|index| MeterValue {
                index,
                value: f32::from(u8::try_from(index % 5).unwrap_or(0)).mul_add(-1.7, -2.5),
            })
            .collect();
        fixture.set_meters(path, meters);
    }
    let source: std::sync::Arc<dyn SignalSource> = fixture;
    let mut inserts = Inserts::new(Some(source));
    inserts.follow();
    inserts.read_meters(std::time::Instant::now());
    for track in &golden.signal_shut {
        inserts.set_fold(track, Some(Fold::Shut));
    }
    inserts
}

/// The clip latches the frozen meters light.
fn clip_latches(golden: &Golden) -> mixer_view::strip_overlay::Clips {
    let mut clips = mixer_view::strip_overlay::Clips::default();
    for guid in &golden.meters.clipped {
        if let Some(level) = golden
            .track_index(guid)
            .and_then(|i| golden.meters.levels.get(i))
        {
            clips.note(guid, *level);
        }
    }
    clips
}

/// The FX view on the frame's track: its mix phases, macro by macro.
fn fx(painter: &mut impl PaintScene, planned: &Planned, shot: &Shot) {
    let mut rows: Vec<(daw_proto::Track, u32)> = planned.strips.as_slice().to_vec();
    // Embedded in the binary: it does not fail to load.
    let Ok(font) = daw_view_kit::text::Font::embedded() else {
        return;
    };
    let mut store = mixer_view::tone::Store::default();
    store.seed(&rows);
    store.hold_placeholders();
    let first = first_row(&rows, shot.frame.first).unwrap_or(0);
    for (i, (track, _)) in rows.iter_mut().enumerate() {
        track.selected = i == first;
    }
    let scene = mixer_view::fx_view::shot(
        rows,
        store,
        1,
        (&[], false),
        font,
        (shot.size.width, shot.size.height),
    );
    painter.append_scene(scene, Affine::IDENTITY);
}

/// The MIDI editor on one of the golden session's takes.
fn editor(painter: &mut impl PaintScene, golden: &Golden, stage: &Stage, shot: &Shot) {
    let size = (f64::from(shot.size.width), f64::from(shot.size.height));
    let mut view = super::editor::open(golden, shot.view, size);
    view.set_look(midi_editor::expression::look_of(&stage.palette));
    if shot.frame.overlay == Overlay::Menu {
        super::editor::open_menu(&mut view, size);
    }
    view.paint(painter);
}
