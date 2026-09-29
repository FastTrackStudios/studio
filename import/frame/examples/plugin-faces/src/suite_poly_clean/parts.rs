//! What the suite's units share: the satin pedal enclosure (every pedal is
//! the Standard class, 156 × 286 design px; the big-box delay 404 × 286),
//! the jack glyphs printed along a pedal's top edge, lower-case geometric
//! lettering, knobs placed by their shaft, and the catalog entries.

use document_proto::{CornerRadii, NodeId, NodeKind, TextAlign};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{ParamInfo, Range, Unit};
use surface_widgets::Builder;

use crate::reverb::ui::{self, blend};

/// The Standard pedal class (1590B-sized), design px at 60 px/in.
pub const STD_W: f64 = 156.0;
/// Every pedal's height (the Standard class's depth).
pub const PEDAL_H: f64 = 286.0;
/// The BigBox class (double-wide floor unit).
pub const BIG_W: f64 = 404.0;
/// The enclosure's bottom lip below the lid.
pub const LIP: f64 = 9.0;

/// A satin-anodised enclosure `w × h` filling `root`, painted top → bottom
/// in `paint`, with its lighter edge `rim`: the lid, and under it the
/// darker lip of the box's front side. Returns the lid (origin at the
/// root's).
pub fn pedal(b: &mut Builder<'_>, root: NodeId, w: f64, h: f64, paint: [&str; 3], rim: &str) -> NodeId {
    ui::fit_root(b, root, w, h);
    let mut side = draw::frame("Side", 0.0, LIP * 2.0, w, h - LIP * 2.0);
    side.fills = vec![draw::linear(180.0, &[(0.0, &blend(paint[2], "#000000", 0.45)), (1.0, &blend(paint[2], "#000000", 0.7))])];
    side.corner_radii = CornerRadii::uniform(10.0);
    side.effects = vec![draw::drop_shadow("#00000099", 0.0, 4.0, 10.0)];
    b.add(root, side);
    let mut lid = draw::frame("Lid", 0.0, 0.0, w, h - LIP);
    lid.fills = vec![
        draw::linear(180.0, &[(0.0, paint[0]), (0.55, paint[1]), (1.0, paint[2])]),
        draw::linear(100.0, &[(0.0, "#ffffff00"), (0.3, "#ffffff10"), (0.55, "#ffffff00"), (1.0, "#0000000c")]),
    ];
    lid.corner_radii = CornerRadii::uniform(9.0);
    lid.effects = vec![draw::inner_shadow(rim, 0.0, 1.2, 0.6), draw::inner_shadow("#00000040", 0.0, -1.5, 2.0), draw::drop_shadow("#00000070", 0.0, 1.5, 1.5)];
    let lid = b.add(root, lid);
    let edge = draw::stroked(draw::rounded(draw::frame("edge", 0.5, 0.5, w - 1.0, h - LIP - 1.0), 8.5), rim, 1.0);
    b.add(lid, edge);
    lid
}

/// The jack glyphs along a pedal's top: ▲ input, (`plug`) the power plug, ▼ output.
pub fn jacks(b: &mut Builder<'_>, lid: NodeId, cx: f64, y: f64, spread: f64, ink: &str, plug: bool) {
    let s = 3.2;
    let up = draw::polygon(&[(cx - spread - s, y + s * 0.8), (cx - spread + s, y + s * 0.8), (cx - spread, y - s * 0.8)]);
    let down = draw::polygon(&[(cx + spread - s, y - s * 0.8), (cx + spread + s, y - s * 0.8), (cx + spread, y + s * 0.8)]);
    b.add(lid, draw::path("jacks", 0.0, 0.0, 2.0 * cx, 2.0 * y, format!("{up} {down}"), draw::solid(ink)));
    if !plug {
        return;
    }
    ui::line(b, lid, "plug", &[(cx - 9.0, y), (cx + 9.0, y)], 0.8, ink);
    for dx in [-7.0, 0.0, 7.0] {
        let ring = draw::circle("plug pin", cx + dx, y, 1.8, draw::solid("#00000000"));
        b.add(lid, draw::stroked(ring, ink, 0.8));
    }
}

/// Lower-case geometric lettering, centred on (`cx`, `cy`), tracked
/// `track` em apart.
#[allow(clippy::too_many_arguments)]
pub fn word(b: &mut Builder<'_>, parent: NodeId, s: &str, cx: f64, cy: f64, size: f64, color: &str, weight: f64, track: f64) -> NodeId {
    let w = s.chars().count() as f64 * size * (0.62 + track) + size * 2.0;
    let mut t = ui::text("word", s, cx - w / 2.0, cy, w, size, color, TextAlign::Center, weight, false);
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = size * track;
    }
    b.add(parent, t)
}

/// A kit knob (or other rotary) with its shaft at (`cx`, `cy`), a tight
/// touch box, no printed legend (the unit letters its own).
#[allow(clippy::too_many_arguments)]
pub fn knob(b: &mut Builder<'_>, parent: NodeId, id: &str, variant: &str, addr: &str, name: &str, d: f64, cx: f64, cy: f64) -> NodeId {
    let p = Place::new(name).size(d).variant(variant).bind(ui::bind(addr)).legend("").touch(TOUCH_MIN);
    let k = ui::put(b, parent, id, &p, cx, cy);
    ui::strip_legend(b, k);
    k
}

/// Same as [`knob`] with `options` (a kit component configured by
/// `key=value`, e.g. `ticks=11`).
#[allow(clippy::too_many_arguments)]
pub fn knob_with(b: &mut Builder<'_>, parent: NodeId, id: &str, variant: &str, addr: &str, name: &str, d: f64, cx: f64, cy: f64, opts: &[&str], ink: &str) -> NodeId {
    let p = Place::new(name).size(d).variant(variant).bind(ui::bind(addr)).legend("").touch(TOUCH_MIN).options(opts).ink(surface_kit::Ink::Color(ink.into()));
    let k = ui::put(b, parent, id, &p, cx, cy);
    ui::strip_legend(b, k);
    k
}

/// A switch component (footswitch, lever) centred on its child named one
/// of `anchor` at (`cx`, `cy`).
#[allow(clippy::too_many_arguments)]
pub fn switch(b: &mut Builder<'_>, parent: NodeId, id: &str, variant: &str, addr: &str, name: &str, d: f64, cx: f64, cy: f64, opts: &[&str], anchor: &[&str]) -> NodeId {
    let mut p = Place::new(name).size(d).bind(ui::bind(addr)).options(opts);
    if !variant.is_empty() {
        p = p.variant(variant);
    }
    ui::put_by(b, parent, id, &p, anchor, cx, cy)
}

/// The printed high-pass / low-pass glyph (a knee), `w` wide at (`cx`, `cy`).
pub fn cut_glyph(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, w: f64, high_pass: bool, ink: &str) {
    let h = w * 0.45;
    let pts = if high_pass {
        vec![(cx - w / 2.0, cy + h / 2.0), (cx - w * 0.1, cy - h / 2.0), (cx + w / 2.0, cy - h / 2.0)]
    } else {
        vec![(cx - w / 2.0, cy - h / 2.0), (cx + w * 0.1, cy - h / 2.0), (cx + w / 2.0, cy + h / 2.0)]
    };
    ui::line(b, parent, "cut glyph", &pts, 1.3, ink);
}

// ── catalog ──────────────────────────────────────────────────────────────

pub fn pct(ns: &str, id: &str, name: &str, default: f64) -> ParamInfo {
    ui::percent(&format!("{ns}/{id}"), name, default)
}

pub fn db(ns: &str, id: &str, name: &str, lo: f64, hi: f64, default: f64) -> ParamInfo {
    ui::param(&format!("{ns}/{id}"), name, Range::linear(lo, hi, Unit::Db), default)
}

/// An amp's 0–10 dial.
pub fn dial10(ns: &str, id: &str, name: &str, default: f64) -> ParamInfo {
    ui::param(&format!("{ns}/{id}"), name, Range::linear(0.0, 10.0, Unit::None), default)
}

pub fn hz(ns: &str, id: &str, name: &str, lo: f64, hi: f64, default: f64) -> ParamInfo {
    ui::param(&format!("{ns}/{id}"), name, Range::log(lo, hi, Unit::Hz), default)
}

pub fn on(ns: &str, id: &str, name: &str, default: bool) -> ParamInfo {
    ui::toggle(&format!("{ns}/{id}"), name, default)
}

pub fn pick(ns: &str, id: &str, name: &str, options: &[&str], default: usize) -> ParamInfo {
    ui::choice(&format!("{ns}/{id}"), name, options, default)
}
