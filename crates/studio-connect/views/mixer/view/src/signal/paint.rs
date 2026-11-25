//! A Signal FX's rows, drawn in a strip's rack box.
//!
//! The group's header, each phase's bar (its tint, bypass and make-up),
//! each block as an insert (name, kind, switch, status or key), and the
//! meters over them.
//!
//! The rows change with the chain's revision and are drawn once per
//! change ([`rows_cached`]); the meters change ten times a second and
//! are drawn over them every frame ([`meters`]), a bar per block.

use anyrender::{PaintScene, Scene};
use vello::kurbo::{Affine, Circle, Rect, RoundedRect};
use vello::peniko::{Color, Fill};

use super::chain::{Block, Phase, Status};
use super::view::{self, Fold, Row, Strip};
use crate::palette::Palette;
use crate::text::Font;
use crate::tone::Panel;

const NAME_PX: f32 = 9.0;
const SMALL_PX: f32 = 7.5;
const PAD: f64 = 6.0;

/// The colours a status is said in.
const fn trouble_ink(status: &Status) -> Color {
    match status {
        Status::Ok => Color::from_rgb8(0x9c, 0xa3, 0xaf),
        Status::Unavailable => Color::from_rgb8(0xf5, 0x9e, 0x0b),
        Status::Error | Status::Other(_) => Color::from_rgb8(0xef, 0x44, 0x44),
    }
}

/// `strip`'s rows and meters over a strip's rack box `rack` (in the
/// mixer's space), its chain scrolled up by `scroll`, clipped to the box.
pub fn over_rack(
    scene: &mut Scene,
    palette: &Palette,
    font: &Font,
    strip: &mut Strip,
    rack: Panel,
    scroll: f64,
) {
    scene.push_clip_layer(
        Fill::NonZero,
        Affine::IDENTITY,
        &Rect::new(rack.x, rack.y, rack.x + rack.width, rack.y + rack.height),
    );
    let at = rack.up(scroll);
    let rows = rows_cached(strip, palette, font, at);
    scene.commands.extend_from_slice(&rows.commands);
    meters(scene, font, strip, at);
    scene.pop_layer();
}

/// The rows of `strip` at `at`, from its picture when nothing moved since
/// it was drawn.
pub fn rows_cached<'s>(
    strip: &'s mut Strip,
    palette: &Palette,
    font: &Font,
    at: Panel,
) -> &'s Scene {
    let key = view::Picture::of(strip, at);
    if strip.picture.as_ref().is_none_or(|(k, _)| *k != key) {
        let mut scene = Scene::new();
        rows(&mut scene, palette, font, &strip.chain, strip.fold, at);
        strip.picture = Some((key, scene));
    }
    match &strip.picture {
        Some((_, scene)) => scene,
        None => empty(),
    }
}

fn empty() -> &'static Scene {
    static EMPTY: std::sync::OnceLock<Scene> = std::sync::OnceLock::new();
    EMPTY.get_or_init(Scene::new)
}

/// Every row of `chain` under `fold`, from the top of `at`, over the
/// whole box (opaque: it covers the recorded rack under it).
pub fn rows(
    scene: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    chain: &super::chain::Chain,
    fold: Fold,
    at: Panel,
) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        palette.surface,
        None,
        &Rect::new(at.x, at.y, at.x + at.width, at.y + at.height.max(0.0) * 4.0),
    );
    for (row, place) in view::layout(&view::rows(chain, fold), at) {
        match row {
            Row::Fx { chain, fold } => fx_head(scene, palette, font, chain, fold, place),
            Row::Phase(phase) => phase_bar(scene, palette, font, phase, place),
            Row::Block { phase, block } => block_row(scene, palette, font, phase, block, place),
        }
    }
}

/// The meters of `strip`'s blocks over their rows: a bar along each
/// block's foot for its first meter, read as dB of reduction (or level),
/// and its number.
pub fn meters(scene: &mut impl PaintScene, font: &Font, strip: &Strip, at: Panel) {
    if strip.fold == Fold::Shut {
        return;
    }
    for (row, place) in view::layout(&view::rows(&strip.chain, strip.fold), at) {
        match row {
            Row::Block { block, .. } => {
                if let Some((meter, value)) = block
                    .meters
                    .first()
                    .and_then(|m| Some((m, strip.meter(m.index)?)))
                {
                    meter_bar(scene, font, meter, value, place);
                }
            }
            Row::Phase(phase) => {
                if let Some(value) = phase.meters.first().and_then(|m| strip.meter(m.index)) {
                    phase_level(scene, value, place);
                }
            }
            Row::Fx { .. } => {}
        }
    }
}

/// A phase's output level: a hairline along the foot of its bar, from
/// -48 dB to 0.
fn phase_level(scene: &mut impl PaintScene, db: f32, at: Panel) {
    let amount = ((f64::from(db) + 48.0) / 48.0).clamp(0.0, 1.0);
    let x0 = at.x + 2.0;
    let x1 = x0 + (at.width - 2.0 - view::SWITCH_W) * amount;
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0x22, 0xc5, 0x5e, 0xb0),
        None,
        &Rect::new(x0, at.y + at.height - 2.0, x1, at.y + at.height - 1.0),
    );
}

/// How many dB fill a meter's bar.
const METER_RANGE_DB: f64 = 24.0;

fn meter_bar(
    scene: &mut impl PaintScene,
    font: &Font,
    meter: &super::chain::Meter,
    value: f32,
    at: Panel,
) {
    let value = f64::from(value);
    let reduction = meter.id == "gain_reduction";
    let amount = if reduction {
        value.abs() / METER_RANGE_DB
    } else {
        (value + METER_RANGE_DB) / METER_RANGE_DB
    }
    .clamp(0.0, 1.0);
    let well = Rect::new(
        at.x + PAD,
        at.y + at.height - 4.0,
        at.x + at.width - view::SWITCH_W,
        at.y + at.height - 2.0,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0xff, 0xff, 0xff, 0x14),
        None,
        &well,
    );
    // Reduction pulls in from the right, as a compressor's needle does;
    // a level rises from the left.
    let lit = if reduction {
        Rect::new(well.x1 - well.width() * amount, well.y0, well.x1, well.y1)
    } else {
        Rect::new(well.x0, well.y0, well.x0 + well.width() * amount, well.y1)
    };
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        if reduction {
            Color::from_rgb8(0xf5, 0x9e, 0x0b)
        } else {
            Color::from_rgb8(0x22, 0xc5, 0x5e)
        },
        None,
        &lit,
    );
    let words = if meter.unit.is_empty() {
        format!("{value:.1}")
    } else {
        format!("{value:.1} {}", meter.unit)
    };
    let w = font.width(&words, SMALL_PX);
    crate::tcp::glyphs(
        scene,
        font,
        Color::from_rgb8(0xd1, 0xd5, 0xdb),
        &words,
        at.x + at.width - view::SWITCH_W - 2.0 - w,
        at.y + at.height - 7.0,
        SMALL_PX,
    );
}

fn fx_head(
    scene: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    chain: &super::chain::Chain,
    fold: Fold,
    at: Panel,
) {
    fill(scene, palette.tcp_field, at);
    let troubled = chain
        .phases
        .iter()
        .flat_map(|p| &p.blocks)
        .filter(|b| b.status.is_trouble())
        .count();
    let count = chain.block_count();
    let words = match fold {
        _ if chain.is_kit() => format!("Signal · kit · {} lanes", chain.lanes.len()),
        Fold::Open => "Signal".to_owned(),
        Fold::Shut => format!("Signal · {count} blocks"),
    };
    let max = at.width - PAD * 2.0 - 12.0;
    crate::tcp::glyphs(
        scene,
        font,
        palette.text,
        &font.elide(&words, NAME_PX, max),
        at.x + PAD,
        at.y + at.height - 5.0,
        NAME_PX,
    );
    if troubled > 0 {
        let dot = Circle::new((at.x + at.width - 22.0, at.y + at.height / 2.0), 3.0);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            trouble_ink(&Status::Error),
            None,
            &dot,
        );
    }
    chevron(
        scene,
        palette.text_dim,
        at.x + at.width - 9.0,
        at.y + at.height / 2.0,
        fold,
    );
}

fn phase_bar(
    scene: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    phase: &Phase,
    at: Panel,
) {
    fill(scene, palette.tcp_meter_well, at);
    let tint = phase
        .id
        .mix_phase()
        .map_or(palette.accent, crate::tone::phase_tint);
    let tint = if phase.bypass {
        tint.multiply_alpha(0.35)
    } else {
        tint
    };
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        tint,
        None,
        &Rect::new(at.x, at.y, at.x + 2.0, at.y + at.height),
    );
    let ink = if phase.bypass {
        palette.text_faint
    } else {
        palette.text_dim
    };
    let readout = view::makeup_box(at);
    crate::tcp::glyphs(
        scene,
        font,
        ink,
        &font.elide(&phase.name, 8.0, readout.x - at.x - PAD - 2.0),
        at.x + PAD,
        at.y + at.height - 5.0,
        8.0,
    );
    if phase.is_source() {
        return;
    }
    let makeup = format!("{:+.1} dB", phase.makeup_db);
    let w = font.width(&makeup, SMALL_PX);
    crate::tcp::glyphs(
        scene,
        font,
        ink,
        &makeup,
        readout.x + readout.width - w - 2.0,
        at.y + at.height - 5.0,
        SMALL_PX,
    );
    switch(scene, palette, view::switch_box(at), !phase.bypass);
}

fn block_row(
    scene: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    phase: &Phase,
    block: &Block,
    at: Panel,
) {
    let inset = Panel {
        x: at.x + 1.0,
        y: at.y + 1.0,
        width: at.width - 2.0,
        height: at.height - 2.0,
    };
    fill(scene, palette.tcp_button, inset);
    let off = block.bypass || phase.bypass || block.status.is_trouble();
    let name_ink = if off {
        palette.text_faint
    } else {
        palette.text
    };
    let right = at.x + at.width - view::SWITCH_W - 2.0;
    let kind_w = font.width(&block.kind, SMALL_PX);
    let name = font.elide(&block.name, NAME_PX, right - at.x - PAD - kind_w - 6.0);
    let name_w = font.width(&name, NAME_PX);
    crate::tcp::glyphs(
        scene,
        font,
        name_ink,
        &name,
        at.x + PAD,
        at.y + 12.0,
        NAME_PX,
    );
    if name_w + kind_w + 6.0 < right - at.x - PAD {
        crate::tcp::glyphs(
            scene,
            font,
            palette.text_faint,
            &block.kind,
            at.x + PAD + name_w + 4.0,
            at.y + 12.0,
            SMALL_PX,
        );
    }
    let (second, ink) = second_line(block);
    if !second.is_empty() {
        crate::tcp::glyphs(
            scene,
            font,
            ink,
            &font.elide(&second, SMALL_PX, right - at.x - PAD - 34.0),
            at.x + PAD,
            at.y + 22.0,
            SMALL_PX,
        );
    }
    if block.status.is_trouble() {
        // A block that cannot run says so across its row, not in a tooltip.
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            trouble_ink(&block.status),
            None,
            &Rect::new(at.x + 1.0, at.y + 1.0, at.x + 3.0, at.y + at.height - 1.0),
        );
    }
    switch(scene, palette, view::switch_box(at), !block.bypass);
}

/// A block's second line: its trouble, or else where its key comes from.
#[must_use]
pub fn second_line(block: &Block) -> (String, Color) {
    if block.status.is_trouble() {
        return (block.status.label().to_owned(), trouble_ink(&block.status));
    }
    let Some(key) = &block.key else {
        return (String::new(), trouble_ink(&Status::Ok));
    };
    let from = key
        .from
        .as_deref()
        .and_then(|f| f.rsplit('/').next())
        .unwrap_or_default();
    let words = match (&key.via, from.is_empty()) {
        (_, true) => format!("key: {}", key.via.label()),
        // Fell back: the source it wanted is not there.
        (super::chain::Via::OwnInput, false) => format!("key: self (no {from})"),
        (via, false) => format!("key: {} ← {from}", via.label()),
    };
    let ink = if key.via == super::chain::Via::OwnInput {
        // Fell back: worth seeing, not alarming.
        trouble_ink(&Status::Unavailable)
    } else {
        Color::from_rgb8(0x60, 0xa5, 0xfa)
    };
    (words, ink)
}

/// An insert's on/off switch: lit when on.
fn switch(scene: &mut impl PaintScene, palette: &Palette, at: Panel, on: bool) {
    let (cx, cy) = (at.x + at.width / 2.0 - 1.0, at.y + at.height / 2.0);
    let knob = Circle::new((cx, cy), 4.0);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        if on {
            Color::from_rgb8(0x22, 0xc5, 0x5e)
        } else {
            palette.tcp_meter_well
        },
        None,
        &knob,
    );
    scene.stroke(
        &vello::kurbo::Stroke::new(1.0),
        Affine::IDENTITY,
        palette.text_faint,
        None,
        &knob,
    );
}

fn chevron(scene: &mut impl PaintScene, ink: Color, cx: f64, cy: f64, fold: Fold) {
    let arm = 3.2;
    let (tip, base) = match fold {
        Fold::Open => (cy - arm * 0.6, cy + arm * 0.6),
        Fold::Shut => (cy + arm * 0.6, cy - arm * 0.6),
    };
    let mut path = vello::kurbo::BezPath::new();
    path.move_to((cx - arm, base));
    path.line_to((cx, tip));
    path.line_to((cx + arm, base));
    scene.stroke(
        &vello::kurbo::Stroke::new(1.4),
        Affine::IDENTITY,
        ink,
        None,
        &path,
    );
}

fn fill(scene: &mut impl PaintScene, color: Color, at: Panel) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color,
        None,
        &RoundedRect::new(at.x, at.y, at.x + at.width, at.y + at.height, 2.0),
    );
}
