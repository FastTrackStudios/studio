//! Everyone else in the session, painted faintly over the arrangement
//! from the roster the kit keeps (`crate::ghosts`).

use std::collections::HashMap;

use session::sync::presence::{PeerState, Pointer};
use vello::kurbo::{Affine, BezPath, Point, Rect, RoundedRect, Stroke};
use vello::peniko::{Color, Fill};

use crate::arrangement::{Arrangement, Palette, Viewport};
#[allow(clippy::wildcard_imports)]
use crate::ghosts::*;
use crate::text::Font;

/// How strongly a ghost is drawn, against the local cursor's 1.0.
const FAINT: f32 = 0.55;

const NAME_SIZE: f32 = 11.0;

fn color_of(state: &PeerState) -> Color {
    let [r, g, b] = [
        (state.color >> 16) & 0xff,
        (state.color >> 8) & 0xff,
        state.color & 0xff,
    ]
    .map(|c| u8::try_from(c).unwrap_or(0));
    Color::from_rgba8(r, g, b, 0xff)
}

/// Paint every remote peer. `rows` is the arrangement's row → track list,
/// `lanes_at` the lanes' origin (as the selection overlay takes it), and
/// `height` how far down lines reach.
#[allow(clippy::too_many_arguments)] // the arrangement's own paint context
pub fn paint(
    painter: &mut impl anyrender::PaintScene,
    palette: &Palette,
    font: &Font,
    scene: &Arrangement,
    rows: &[(daw_proto::Track, u32)],
    view: Viewport,
    lanes_at: (f64, f64),
    height: f64,
) {
    let Ok(slot) = ROSTER.lock() else { return };
    let Some(published) = slot.as_ref() else {
        return;
    };
    let local_now = now_ms();
    let shared_now = local_now + published.clock_offset_ms;

    let left = scene.tcp.width();
    let x_of = |seconds: f64| seconds.mul_add(view.pps, left - view.scroll_x);
    let row_of: HashMap<&str, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, (t, _))| (t.guid.as_str(), i))
        .collect();
    let (_, oy) = lanes_at;
    let band = |row: usize| {
        scene
            .row_box(row)
            .map(|(top, h)| (top.mul_add(view.zoom_y, oy), h * view.zoom_y))
    };

    for peer in published.roster.peers.values() {
        let Some(state) = &peer.state else { continue };
        let color = color_of(state);

        // Their time selection, a wash fainter than yours.
        if let Some((start, end)) = state.time_selection {
            let (x0, x1) = (x_of(start).max(left), x_of(end).max(left));
            if x1 > x0 {
                painter.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    color.with_alpha(0.10),
                    None,
                    &Rect::new(x0, 0.0, x1, height),
                );
            }
        }
        // Their selected items, outlined in their colour.
        for item in scene
            .item_boxes()
            .iter()
            .filter(|i| state.selected_items.contains(&i.guid))
        {
            let Some((top, h)) = band(item.row) else {
                continue;
            };
            let r = Rect::new(
                item.x0.mul_add(view.pps, lanes_at.0),
                top + 1.0,
                item.x1.mul_add(view.pps, lanes_at.0),
                top + h - 1.0,
            );
            painter.stroke(
                &Stroke::new(1.5),
                Affine::IDENTITY,
                color.with_alpha(FAINT),
                None,
                &r,
            );
        }
        // Their selected tracks: a stripe down the lanes' left edge.
        for guid in &state.selected_tracks {
            let Some((top, h)) = row_of.get(guid.as_str()).and_then(|r| band(*r)) else {
                continue;
            };
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                color.with_alpha(FAINT),
                None,
                &Rect::new(left, top, left + 3.0, top + h),
            );
        }
        // Their edit cursor.
        if let Some(at) = state.edit_cursor {
            let x = x_of(at);
            if x >= left {
                painter.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    color.with_alpha(FAINT),
                    None,
                    &Rect::new(x, 0.0, x + 1.0, height),
                );
            }
        }
        // Their play cursor, when everyone plays on their own.
        if published.show_play
            && let Some(play) = peer.play
        {
            let x = x_of(play.position_at(shared_now));
            if x >= left {
                painter.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    color.with_alpha(if play.playing { 0.45 } else { 0.25 }),
                    None,
                    &Rect::new(x - 0.75, 0.0, x + 0.75, height),
                );
            }
        }
        // Their mouse, with their name: exactly where it is in the row,
        // interpolated on this screen so it glides from row to row.
        if peer.trail.latest().is_some_and(drawn_by_arrangement)
            && let Some((x, y)) = peer.trail.screen_at(local_now, |p| match p {
                // Over the track panel: across it, and down the row.
                Pointer::Anchor { panel, key, u, v } if panel == TCP => {
                    let y = if key.is_empty() {
                        v * crate::ruler_layout::ruler_h()
                    } else {
                        let (top, h) = row_of.get(key.as_str()).and_then(|r| band(*r))?;
                        v.mul_add(h, top)
                    };
                    let lanes_top = if key.is_empty() { 0.0 } else { oy };
                    let x = u * left;
                    ((0.0..=left).contains(&x) && (lanes_top..=height).contains(&y))
                        .then_some((x, y))
                }
                Pointer::Timeline { at, track, y } => {
                    let (x, y) = match track {
                        // Over a track: in its row, if this view shows the
                        // row — not scrolled under the ruler, not past the
                        // lanes' foot (where, in a shorter arrangement, the
                        // mixer or another panel is).
                        Some(guid) => {
                            let (top, h) = row_of.get(guid.as_str()).and_then(|r| band(*r))?;
                            (x_of(*at), y.mul_add(h, top))
                        }
                        None => (x_of(*at), y * crate::ruler_layout::ruler_h()),
                    };
                    let lanes_top = if track.is_some() { oy } else { 0.0 };
                    ((left..=view.width).contains(&x) && (lanes_top..=height).contains(&y))
                        .then_some((x, y))
                }
                _ => None,
            })
        {
            paint_pointer(painter, palette, font, &state.name, color, Point::new(x, y));
        }
    }
}

fn paint_pointer(
    painter: &mut impl anyrender::PaintScene,
    palette: &Palette,
    font: &Font,
    name: &str,
    color: Color,
    at: Point,
) {
    // An arrow pointing up-left, its tip on the spot.
    let mut arrow = BezPath::new();
    arrow.move_to(at);
    arrow.line_to((at.x, at.y + 14.0));
    arrow.line_to((at.x + 4.0, at.y + 10.5));
    arrow.line_to((at.x + 9.5, at.y + 11.0));
    arrow.close_path();
    painter.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color.with_alpha(0.9),
        None,
        &arrow,
    );
    painter.stroke(
        &Stroke::new(1.0),
        Affine::IDENTITY,
        palette.text.with_alpha(0.6),
        None,
        &arrow,
    );

    if name.is_empty() {
        return;
    }
    let width = font.width(name, NAME_SIZE);
    let tag = Rect::new(at.x + 10.0, at.y + 12.0, at.x + 18.0 + width, at.y + 27.0);
    painter.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color.with_alpha(0.85),
        None,
        &RoundedRect::from_rect(tag, 3.0),
    );
    let mut text = anyrender::Scene::new();
    crate::tcp::glyphs(
        &mut text,
        font,
        Color::from_rgba8(0x10, 0x10, 0x14, 0xff),
        name,
        tag.x0 + 4.0,
        tag.y1 - 4.0,
        NAME_SIZE,
    );
    for command in &text.commands {
        crate::palette::submit_command(painter, command, Affine::IDENTITY);
    }
}
