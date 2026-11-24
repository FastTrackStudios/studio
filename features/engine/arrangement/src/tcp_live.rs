//! The track panel's live pass over the arrangement's recorded rows: the
//! controls under the pointer, the meters and the record arms, cut per
//! row and replayed for the rows a viewport can see. Built on the kit's
//! live pass (`crate::overlay`), which the mixer's strips share.

use anyrender::PaintScene;
use daw_proto::Track;
use daw_ui::art::mixer_controls::Interaction;
use daw_ui::art::paint::tcp as art;
use vello::kurbo::Affine;

#[allow(clippy::wildcard_imports)]
use crate::overlay::*;
use crate::palette::Palette;
use crate::text::Font;

/// The track panel's live controls, for the rows on screen.
///
/// The mixer's counterpart is [`controls`]; this is the same division
/// applied to the other panel. The recorded row holds its ground, its
/// rail, its colours and its name; the values — mute, solo, arm, volume
/// and pan — are drawn per frame from the tracks as they are now.
pub fn panel_controls(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    scene: &crate::arrangement::Arrangement,
    rows: &[(Track, u32)],
    tracks: &[Track],
    map: &crate::plan::Rows,
    view: crate::viewport::Viewport,
    pointer: &crate::pointer::Pointer<crate::pointer::RowSpot>,
    (transform, buttons_apart): (Affine, bool),
) -> crate::profile::Counts {
    let mut counts = crate::profile::Counts::default();
    let mut out = anyrender::Scene::new();
    for index in scene.visible_rows(view) {
        control_row(
            &mut out,
            palette,
            font,
            scene,
            rows,
            tracks,
            map,
            view,
            pointer,
            index,
            buttons_apart,
        );
    }
    for command in &out.commands {
        counts.replayed = counts.replayed.saturating_add(1);
        if crate::palette::submit_command(painter, command, transform) {
            counts.submitted = counts.submitted.saturating_add(1);
        }
    }
    counts
}

/// Each visible track's level, drawn IN its name field: one thin strip
/// low in the field, under the name, where it crosses nothing. Colour and
/// length only — the mixer's zones (safe, warm, hot, over, at
/// `METER_ZONES`) in flat segments, no scale and no well: at a glance
/// across forty rows, which are playing and which are hot is what an
/// arrangement meter is for. The louder channel is what it shows; the
/// mixer is where a pair is read apart.
///
/// Returns whether any strip is lit, so a caller knows to keep redrawing
/// while levels fall.
#[must_use]
pub fn row_meters(
    painter: &mut impl PaintScene,
    palette: &Palette,
    scene: &crate::arrangement::Arrangement,
    rows: &[(Track, u32)],
    tracks: &[Track],
    map: &crate::plan::Rows,
    view: crate::viewport::Viewport,
    levels: &[daw_proto::TrackLevels],
    transform: Affine,
) -> bool {
    use crate::row::{Control, Row};
    use daw_ui::art::paint::tcp::METER_ZONES;

    let mut lit = false;
    // The zone boundaries along a strip, as fractions of its length.
    let edges = METER_ZONES.map(|db| daw_ui::art::paint::tcp::meter_norm(db));
    let colors = [
        palette.meter_safe,
        palette.meter_warn,
        palette.meter_danger,
        palette.meter_danger,
    ];
    for index in scene.visible_rows(view) {
        let (Some((track, depth)), Some(live)) = (rows.get(index), map.live(tracks, index)) else {
            continue;
        };
        let Some(level) = usize::try_from(live.index).ok().and_then(|i| levels.get(i)) else {
            continue;
        };
        let Some((top, height)) = scene.panel_band(index, view) else {
            continue;
        };
        let row = Row::new(
            top,
            height,
            i32::try_from(*depth).unwrap_or(0),
            track.is_folder,
            scene.tcp,
        );
        let Some(field) = row.rect(Control::Name) else {
            continue;
        };
        let thick = STRIP.min(field.height() / 6.0);
        // In the lower half, a hair inside the field's bottom edge.
        let y0 = (field.y1 - thick - 1.0).max(field.y0 + field.height() / 2.0);
        let peak = level.peak_left.max(level.peak_right);
        let fraction = crate::engine::meter_fraction(peak).clamp(0.0, 1.0);
        if fraction <= 0.0 || peak <= 1e-5 {
            continue;
        }
        lit = true;
        let length = field.width() * fraction;
        let mut from = 0.0;
        for (zone, color) in colors.iter().enumerate() {
            let to = edges
                .get(zone)
                .map_or(length, |edge| (field.width() * edge).min(length));
            if to > from {
                painter.fill(
                    vello::peniko::Fill::NonZero,
                    transform,
                    *color,
                    None,
                    &vello::kurbo::Rect::new(field.x0 + from, y0, field.x0 + to, y0 + thick),
                );
                from = to;
            }
            if from >= length {
                break;
            }
        }
    }
    lit
}

/// Every row's live controls, recorded once with a command range per
/// row.
///
/// Recorded for ALL rows rather than the visible ones, which is the
/// whole point: a recording keyed to what is on screen is thrown away
/// by a scroll, and scrolling is when a window can least afford to
/// rebuild forty rows of vector art. Keyed per row instead, a scroll
/// replays a different span of the same recording.
///
/// What it is NOT keyed by is the pointer. Hover is one control drawn
/// again on top — see `crate::pointer` — so a recording taken at rest
/// stays good while the mouse moves across it.
#[must_use]
pub fn record_controls(
    palette: &Palette,
    font: &Font,
    scene: &crate::arrangement::Arrangement,
    rows: &[(Track, u32)],
    tracks: &[Track],
    map: &crate::plan::Rows,
    (view, buttons_apart): (crate::viewport::Viewport, bool),
) -> Controls {
    let mut out = anyrender::Scene::new();
    let mut spans = Vec::with_capacity(rows.len());
    let at_rest = crate::pointer::Pointer::default();
    for index in 0..rows.len() {
        let from = command_index(&out);
        control_row(
            &mut out,
            palette,
            font,
            scene,
            rows,
            tracks,
            map,
            view,
            &at_rest,
            index,
            buttons_apart,
        );
        spans.push(from..command_index(&out));
    }
    Controls { scene: out, spans }
}

/// One cut of the live controls.
pub struct Controls {
    scene: anyrender::Scene,
    spans: Vec<core::ops::Range<u32>>,
}

impl Controls {
    /// Replay the rows a viewport can see.
    pub fn replay(
        &self,
        painter: &mut impl PaintScene,
        scene: &crate::arrangement::Arrangement,
        view: crate::viewport::Viewport,
        transform: Affine,
    ) -> crate::profile::Counts {
        let mut counts = crate::profile::Counts::default();
        for row in scene.visible_rows(view) {
            let Some(span) = self.spans.get(row) else {
                continue;
            };
            let (from, to) = (span.start as usize, span.end as usize);
            let Some(commands) = self.scene.commands.get(from..to) else {
                continue;
            };
            for command in commands {
                counts.replayed = counts.replayed.saturating_add(1);
                if crate::palette::submit_command(painter, command, transform) {
                    counts.submitted = counts.submitted.saturating_add(1);
                }
            }
        }
        counts
    }
}

/// How many commands a scene holds, as the index of the next one.
fn command_index(scene: &anyrender::Scene) -> u32 {
    u32::try_from(scene.commands.len()).unwrap_or(u32::MAX)
}

/// One row's controls again, over the recording, so the control under
/// the pointer can show it.
///
/// The recording is taken at rest — see [`record_controls`] — which is
/// what lets it survive a mouse move. The cost of that is that a hover
/// has to be painted on top, and this is it: the whole row, redrawn,
/// with the pointer's verdict applied. One row of forty, on the frames
/// where the pointer is on one at all.
pub fn hovered_row(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    scene: &crate::arrangement::Arrangement,
    rows: &[(Track, u32)],
    tracks: &[Track],
    map: &crate::plan::Rows,
    view: crate::viewport::Viewport,
    pointer: &crate::pointer::Pointer<crate::pointer::RowSpot>,
    (transform, buttons_apart): (Affine, bool),
) -> crate::profile::Counts {
    let mut counts = crate::profile::Counts::default();
    let Some(row) = pointer.active().map(|(spot, _)| spot.row) else {
        return counts;
    };
    let mut out = anyrender::Scene::new();
    control_row(
        &mut out,
        palette,
        font,
        scene,
        rows,
        tracks,
        map,
        view,
        pointer,
        row,
        buttons_apart,
    );
    for command in &out.commands {
        counts.replayed = counts.replayed.saturating_add(1);
        if crate::palette::submit_command(painter, command, transform) {
            counts.submitted = counts.submitted.saturating_add(1);
        }
    }
    counts
}

/// Row `row`'s mute or solo button, `on` or not, into `out`: as the track
/// panel draws it.
fn row_button(
    out: &mut anyrender::Scene,
    (palette, font): (&Palette, &Font),
    row: &crate::row::Row,
    (control, label): (crate::row::Control, &str),
    on: bool,
    lit: daw_ui::theme::Color,
    state: Interaction,
) {
    let Some(r) = row.rect(control) else { return };
    // A touchscreen's fills its rect.
    if row.tcp.compact {
        crate::art::place(
            &mut *out,
            &art::gutter_button_sized(
                &palette.chrome,
                label,
                on,
                lit,
                state,
                (r.width(), r.height()),
            ),
            font,
            r.x0,
            r.y0,
        );
        return;
    }
    // Flattened to the rect on a row too short for the full button — the
    // row's shape says how tall, not the art.
    crate::art::squashed(
        &mut *out,
        &art::gutter_button(&palette.chrome, label, on, lit, state),
        font,
        r.x0,
        r.y0,
        1.0,
        (r.height() / crate::tcp::BUTTON.1).min(1.0),
    );
}

/// The track panel's mute and solo, lit, for the rows on screen.
///
/// Parts an engine's values show ([`crate::engine::track_value`]), over
/// the panel that [`control_row`] drew with them unlit (`buttons_apart`).
/// Placed by `transform` and cut to `clip` (the pane's), as the panel's
/// controls are. A row a theme draws keeps its own.
#[expect(
    clippy::too_many_arguments,
    reason = "what the panel's controls are drawn from, as `panel_controls` takes it"
)]
#[must_use]
pub fn panel_buttons(
    palette: &Palette,
    font: &Font,
    scene: &crate::arrangement::Arrangement,
    rows: &[(Track, u32)],
    tracks: &[Track],
    map: &crate::plan::Rows,
    view: crate::viewport::Viewport,
    pointer: &crate::pointer::Pointer<crate::pointer::RowSpot>,
    (transform, clip): (Affine, vello::kurbo::Rect),
) -> Vec<ValueLayer> {
    use crate::pointer::RowSpot;
    use crate::row::{Control as C, Row};
    let in_content = transform.inverse().transform_rect_bbox(clip);
    let mut parts = Vec::new();
    for index in scene.visible_rows(view) {
        let (Some((track, depth)), Some(live)) = (rows.get(index), map.live(tracks, index)) else {
            continue;
        };
        let Some(at) = button_index(true, live) else {
            continue;
        };
        let Some((top, height)) = scene.panel_band(index, view) else {
            continue;
        };
        let row = Row::new(
            top,
            height,
            i32::try_from(*depth).unwrap_or(0),
            track.is_folder,
            scene.tcp,
        )
        .with_lanes(!crate::lanes::speaks_for(tracks, live, scene.lane_tracks(index)).is_empty());
        let themed = crate::skin::loaded()
            .as_deref()
            .is_some_and(|t| t.has_role(crate::skin::ROW) && !row.tcp.compact);
        if row.density == crate::tcp::Density::Bar || themed {
            continue;
        }
        for (control, label, lit, part) in [
            (C::Mute, "M", crate::tcp::mute_lit(palette), "mute"),
            (C::Solo, "S", crate::tcp::solo_lit(palette), "solo"),
        ] {
            let mut shown = anyrender::Scene::new();
            let state = pointer.state(RowSpot {
                row: index,
                control,
            });
            row_button(
                &mut shown,
                (palette, font),
                &row,
                (control, label),
                true,
                lit,
                state,
            );
            if shown.commands.is_empty() {
                continue;
            }
            let mut part = lit_button(at, part, shown);
            part.transform = Some(transform);
            part.clip = Some(in_content);
            parts.push(part);
        }
    }
    parts
}

/// One row's controls, into `out`.
#[expect(
    clippy::too_many_arguments,
    reason = "the loop body of `panel_controls`, lifted out so a row that draws nothing can return"
)]
fn control_row(
    out: &mut anyrender::Scene,
    palette: &Palette,
    font: &Font,
    scene: &crate::arrangement::Arrangement,
    rows: &[(Track, u32)],
    tracks: &[Track],
    map: &crate::plan::Rows,
    view: crate::viewport::Viewport,
    pointer: &crate::pointer::Pointer<crate::pointer::RowSpot>,
    index: usize,
    buttons_apart: bool,
) {
    use crate::pointer::RowSpot;
    use crate::row::{Control as C, Indicator, Row};
    use daw_ui::art::geometry::tcp as gt;

    let (Some((track, depth)), Some(live)) = (rows.get(index), map.live(tracks, index)) else {
        return;
    };
    // The band on SCREEN, not in the session: these are drawn
    // under a translate so that the art keeps its own size, which
    // leaves the placing to `row_band`. See it for why.
    let Some((top, height)) = scene.panel_band(index, view) else {
        return;
    };
    let row = Row::new(
        top,
        height,
        i32::try_from(*depth).unwrap_or(0),
        track.is_folder,
        scene.tcp,
    )
    .with_lanes(!crate::lanes::speaks_for(tracks, live, scene.lane_tracks(index)).is_empty());
    if row.density == crate::tcp::Density::Bar {
        return;
    }
    // The pointer's verdict on this row's controls. The panel is a
    // recorded scene like the mixer is, so a hover cannot repaint
    // the row — it repaints the ONE control, here, in the same pass
    // that already redraws every live value.
    let look = |control| {
        pointer.state(RowSpot {
            row: index,
            control,
        })
    };

    // The fixed lanes' line, under mute and solo: the lane that plays,
    // between the steps to the one before and after. Over a theme's row
    // as over the built-in one.
    if let Some(band) = row.lane_line() {
        let under = crate::lanes::speaks_for(tracks, live, scene.lane_tracks(index));
        let label = crate::lanes::label_of(&under);
        lane_line(out, palette, font, &label, &row, band, &look);
        lanes_plus(out, palette, font, &row, crate::lanes::opened(live), &look);
    }

    // A theme's row draws its own live part — every value and the
    // pointer's state — over the recording it made.
    if let Some(theme) = crate::skin::loaded()
        .as_deref()
        .filter(|t| t.has_role(crate::skin::ROW) && !row.tcp.compact)
    {
        let all = [
            C::Phase,
            C::Mute,
            C::Solo,
            C::RecArm,
            C::Volume,
            C::Pan,
            C::Routing,
            C::Fx,
            C::Folder,
            C::Name,
        ];
        let fields = crate::skin::row_fields(
            palette,
            live,
            crate::skin::RowShown {
                depth: i32::try_from(*depth).unwrap_or(0),
                height,
                font: Some(font),
                hover: all.into_iter().find(|c| look(*c) == Interaction::Hover),
                pressed: all.into_iter().find(|c| look(*c) == Interaction::Pressed),
                idle: !row.tcp.recording,
                collapsed: crate::folds::shut(rows, index),
            },
        );
        theme.live(
            crate::skin::ROW,
            (scene.tcp.width(), height),
            &fields,
            &mut crate::skin::canvas::Host { scene: out, font },
            Affine::translate((0.0, top)),
        );
        return;
    }

    // Mute and solo: unlit where a host lights them by the engine's
    // values ([`panel_buttons`]).
    let apart = button_index(buttons_apart, live).is_some();
    for (control, label, on, lit) in [
        (C::Mute, "M", live.muted, crate::tcp::mute_lit(palette)),
        (C::Solo, "S", live.soloed, crate::tcp::solo_lit(palette)),
    ] {
        row_button(
            out,
            (palette, font),
            &row,
            (control, label),
            on && !apart,
            lit,
            look(control),
        );
    }

    // The FX button, from the chain the track actually has.
    if let Some(r) = row.rect(C::Fx) {
        crate::art::place(
            &mut *out,
            &art::fx_pill(
                &palette.chrome,
                crate::tcp::lit(palette),
                chain(live),
                look(C::Fx),
            ),
            font,
            r.x0,
            r.y0,
        );
    }

    // Routing, and polarity in the corner below it. Both are live
    // values with their own events — a strip that recorded them was
    // right until the first time anything changed one.
    if let Some(r) = row.rect(C::Routing) {
        crate::art::place(
            &mut *out,
            &art::routing(
                &palette.chrome,
                art::Axis::Horizontal,
                routes(live),
                route_ink(palette),
                look(C::Routing),
            ),
            font,
            r.x0,
            r.y0,
        );
    }
    if let Some(r) = row.rect(C::Phase) {
        crate::art::place(
            &mut *out,
            &art::phase(&palette.chrome, live.phase_inverted, look(C::Phase)),
            font,
            r.x0,
            r.y0,
        );
    }

    // The record arm, on rows tall enough to read one.
    if let Some(r) = row.rect(C::RecArm).filter(|_| row.tcp.compact) {
        // The compact panel's, on its second line: a button like the mute
        // and solo beside it, the record ring on it.
        let lit = crate::tcp::lit(palette).rec;
        crate::art::place(
            &mut *out,
            &art::gutter_button_sized(
                &palette.chrome,
                "",
                false,
                lit,
                look(C::RecArm),
                (r.width(), r.height()),
            ),
            font,
            r.x0,
            r.y0,
        );
        let size = r.width().min(r.height()) * 0.66;
        crate::art::scaled(
            &mut *out,
            &art::record_arm(
                &palette.chrome,
                lit,
                live.armed,
                look(C::RecArm),
                art::Arm::Panel,
                crate::tcp::to_theme(palette.tcp_field),
            ),
            font,
            r.x0 + (r.width() - size) / 2.0,
            r.y0 + (r.height() - size) / 2.0,
            size / 20.0,
        );
    } else if let Some(r) = row.rect(C::RecArm) {
        crate::art::place(
            &mut *out,
            &art::record_arm(
                &palette.chrome,
                crate::tcp::lit(palette).rec,
                live.armed,
                look(C::RecArm),
                art::Arm::Panel,
                crate::tcp::to_theme(palette.tcp_field),
            ),
            font,
            r.x0,
            r.y0,
        );
    }

    // Volume and pan, in whichever form the row is showing — a knob
    // where there is room to turn one, a flattened bar where there
    // is not. Both are the same VALUE; only the shape changes.
    let knob = row.indicator() == Indicator::Knob;
    if let Some(r) = row.rect(C::Volume) {
        let field_h = r.height();
        if knob {
            // Never past the size it was authored — the same rule
            // the rect above measures it by, so the picture and the
            // hit target stay the same shape.
            let scale = (field_h / 22.0).min(1.0);
            crate::art::scaled(
                &mut *out,
                &art::volume_knob(
                    &palette.chrome,
                    crate::tcp::lit(palette).volume,
                    crate::tcp::volume_fraction(live.volume),
                    look(C::Volume),
                    field_h,
                ),
                font,
                r.x0,
                r.y0,
                scale,
            );
        }
    }
    // The name plate has no hover cell in the traced art — REAPER
    // does not light one either — but in this window it is what you
    // double-click to rename, and a control that opens an editor
    // has to say so before you commit to the second click. One rule
    // under the name, in the accent, is the least that reads.
    if look(C::Name) != Interaction::Normal {
        if let Some(r) = row.rect(C::Name) {
            out.fill(
                vello::peniko::Fill::NonZero,
                Affine::IDENTITY,
                palette.accent,
                None,
                &vello::kurbo::Rect::new(r.x0, r.y1 - 1.0, r.x1, r.y1),
            );
        }
    }

    if let Some(r) = row.rect(C::Pan) {
        let field_h = r.height();
        if knob {
            let scale = (field_h / 25.0).min(1.0);
            crate::art::scaled(
                &mut *out,
                &art::pan_knob(
                    &palette.chrome,
                    live.pan.clamp(-1.0, 1.0),
                    crate::tcp::to_theme(palette.pan),
                    look(C::Pan),
                ),
                font,
                f64::from(gt::PAN_KNOB_X),
                r.y0 + 25.0_f64.mul_add(-scale, field_h) / 2.0,
                scale,
            );
        }
    }
}

/// The lanes button as a pill: `Lanes ▾` while the track shows the lane
/// that plays (press to show them all), `Lanes ▴` lit in the accent while
/// it shows them all (press to fold them back to that one).
fn lanes_pill(
    out: &mut anyrender::Scene,
    palette: &Palette,
    font: &Font,
    r: vello::kurbo::Rect,
    open: bool,
    look: Interaction,
) {
    use vello::kurbo::{BezPath, RoundedRect};
    use vello::peniko::Fill;
    const SIZE: f32 = 9.0;
    /// The chevron's half width and half height, and its room at the end.
    const W: f64 = 3.0;
    const H: f64 = 1.8;
    const ROOM: f64 = 12.0;
    let lit = !matches!(look, Interaction::Normal);
    let well = match (open, lit) {
        (true, _) => palette.accent.multiply_alpha(0.75),
        (false, true) => palette.divider,
        (false, false) => palette.divider.multiply_alpha(0.6),
    };
    let pill = RoundedRect::from_rect(r, 4.0);
    out.fill(Fill::NonZero, Affine::IDENTITY, well, None, &pill);
    out.stroke(
        &vello::kurbo::Stroke::new(1.0),
        Affine::IDENTITY,
        palette.accent.multiply_alpha(if open { 1.0 } else { 0.5 }),
        None,
        &pill,
    );
    let room = (r.width() - ROOM - 4.0).max(0.0);
    let label = font.elide("Lanes", SIZE, room);
    let width = font.width(&label, SIZE);
    let x = r.x0 + 4.0 + (room - width).max(0.0) / 2.0;
    let y = f64::from(SIZE).mul_add(0.35, r.center().y);
    crate::tcp::glyphs(out, font, palette.text, &label, x, y, SIZE);
    // Down: there are more to show; up: fold them back.
    let (cx, cy) = (r.x1 - ROOM / 2.0 - 1.0, r.center().y);
    let dir = if open { -1.0 } else { 1.0 };
    let mut chevron = BezPath::new();
    chevron.move_to((cx - W, cy - H * dir));
    chevron.line_to((cx, cy + H * dir));
    chevron.line_to((cx + W, cy - H * dir));
    out.stroke(
        &vello::kurbo::Stroke::new(1.4),
        Affine::IDENTITY,
        palette.text,
        None,
        &chevron,
    );
}

/// The `+` beside the lane line ([`crate::row::Control::LaneShow`]): a
/// well with a plus while the track shows the lane that plays, a minus
/// while it shows them all — lit under the pointer, accented open.
fn lanes_plus(
    out: &mut anyrender::Scene,
    palette: &Palette,
    font: &Font,
    row: &crate::row::Row,
    open: bool,
    look: &dyn Fn(crate::row::Control) -> Interaction,
) {
    use crate::row::Control as C;
    use vello::kurbo::{Rect, RoundedRect};
    use vello::peniko::Fill;
    /// Half a stroke's length, and half its weight.
    const ARM: f64 = 3.0;
    const HALF: f64 = 0.6;
    let Some(r) = row.rect(C::LaneShow) else {
        return;
    };
    if row.lane_show_is_pill() {
        lanes_pill(out, palette, font, r, open, look(C::LaneShow));
        return;
    }
    let well = if open {
        palette.accent.multiply_alpha(0.35)
    } else {
        palette.divider.multiply_alpha(0.5)
    };
    out.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        well,
        None,
        &RoundedRect::from_rect(r, 3.0),
    );
    let ink = match look(C::LaneShow) {
        Interaction::Normal if !open => palette.text_dim,
        _ => palette.text,
    };
    let (cx, cy) = (r.center().x, r.center().y);
    out.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        ink,
        None,
        &Rect::new(cx - ARM, cy - HALF, cx + ARM, cy + HALF),
    );
    if !open {
        out.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            ink,
            None,
            &Rect::new(cx - HALF, cy - ARM, cx + HALF, cy + ARM),
        );
    }
}

/// The lane line ([`crate::row::Row::lane_line`]): a well, the steps at
/// each end (lit under the pointer), the playing lane's name between.
fn lane_line(
    out: &mut anyrender::Scene,
    palette: &Palette,
    font: &Font,
    label: &str,
    row: &crate::row::Row,
    band: vello::kurbo::Rect,
    look: &dyn Fn(crate::row::Control) -> Interaction,
) {
    use crate::row::Control as C;
    use vello::kurbo::{BezPath, RoundedRect};
    use vello::peniko::Fill;
    /// The name's type size.
    const SIZE: f32 = 9.0;
    out.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        palette.divider.multiply_alpha(0.5),
        None,
        &RoundedRect::from_rect(band, 3.0),
    );
    for (control, forward) in [(C::LanePrev, false), (C::LaneNext, true)] {
        let Some(r) = row.rect(control) else { continue };
        let ink = match look(control) {
            Interaction::Normal => palette.text_dim,
            _ => palette.text,
        };
        let (cx, cy) = (r.center().x, r.center().y);
        let (w, h) = (2.5, 3.5);
        let mut arrow = BezPath::new();
        if forward {
            arrow.move_to((cx - w, cy - h));
            arrow.line_to((cx + w, cy));
            arrow.line_to((cx - w, cy + h));
        } else {
            arrow.move_to((cx + w, cy - h));
            arrow.line_to((cx - w, cy));
            arrow.line_to((cx + w, cy + h));
        }
        arrow.close_path();
        out.fill(Fill::NonZero, Affine::IDENTITY, ink, None, &arrow);
    }
    let (Some(prev), Some(next)) = (row.rect(C::LanePrev), row.rect(C::LaneNext)) else {
        return;
    };
    let room = (next.x0 - prev.x1).max(0.0);
    let label = font.elide(label, SIZE, room);
    let width = font.width(&label, SIZE);
    crate::tcp::glyphs(
        out,
        font,
        palette.text,
        &label,
        prev.x1 + (room - width).max(0.0) / 2.0,
        f64::from(SIZE).mul_add(0.35, band.center().y),
        SIZE,
    );
}
/// How thick a name field's meter strip is, at most.
const STRIP: f64 = 2.5;
