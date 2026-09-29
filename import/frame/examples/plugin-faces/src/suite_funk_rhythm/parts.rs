//! What the amp-suite faces share: sizing the page to the unit's form,
//! placing kit parts by their centre with the face's own legends, the
//! printed lettering, and the catalog entries a unit declares.
//!
//! Used by both artist-style suites (`suite_funk_rhythm`,
//! `suite_boutique_clean`).

use document_proto::{NodeId, NodeKind, TextAlign, TextCase, TextRun};
use surface_kit::{Ink, Place, TOUCH_MIN, draw};
use surface_proto::{ParamInfo, Range, Unit};
use surface_widgets::Builder;

use crate::reverb::ui;

// ── form factors (docs/form-factors.md, 60 design px per inch) ────────────

/// Every pedal's height (the Standard class's depth).
pub const PEDAL_H: f64 = 286.0;
/// Standard pedal width.
pub const STANDARD_W: f64 = 156.0;
/// Big-box (double) pedal width.
pub const BIGBOX_W: f64 = 404.0;
/// The treadle (wah / expression) class.
pub const TREADLE: (f64, f64) = (241.0, 600.0);
/// An amp head's width; the height follows the faceplate's aspect.
pub const HEAD_W: f64 = 1200.0;
/// The cab page.
pub const CAB: f64 = 900.0;
/// A 19-inch rack face without ears.
pub const RACK_W: f64 = 1140.0;

// ── placing ──────────────────────────────────────────────────────────────

/// Size the page's root to the unit.
pub fn page(b: &mut Builder<'_>, root: NodeId, w: f64, h: f64) {
    ui::fit_root(b, root, w, h);
}

/// A painted stompbox enclosure filling the page (`variant`: `metallic` or
/// `satin`), returned as the parent for its parts.
pub fn stompbox(b: &mut Builder<'_>, root: NodeId, w: f64, h: f64, color: &str, variant: &str, radius: f64) -> NodeId {
    page(b, root, w, h);
    let opts = [format!("w={w}"), format!("h={h}"), format!("color={color}"), format!("radius={radius}")];
    let p = Place { options: Some(opts.to_vec()), ..Place::new("Enclosure").variant(variant) };
    ui::put_at(b, root, "panel.stompbox", &p, 0.0, 0.0)
}

/// A kit knob `id`/`variant` of diameter `d` on `addr`, its shaft at
/// (`cx`, `cy`), its hit box tight round the body, the scale printed in
/// `ink`; `opts` are the kit's options (`dots=`, `ticks=`, `minmax`). The
/// face prints the legend.
#[allow(clippy::too_many_arguments)]
pub fn knob(b: &mut Builder<'_>, parent: NodeId, id: &str, variant: &str, addr: &str, name: &str, d: f64, (cx, cy): (f64, f64), opts: &[&str], ink: &str) -> NodeId {
    let mut p = Place::new(name).size(d).bind(ui::bind(addr)).ink(Ink::Color(ink.into())).keep_case().touch(TOUCH_MIN);
    if !variant.is_empty() {
        p = p.variant(variant);
    }
    if !opts.is_empty() {
        p = p.options(opts);
    }
    let k = ui::put(b, parent, id, &p, cx, cy);
    ui::strip_legend(b, k);
    k
}

/// A kit control placed by the centre of its child named one of `anchor`
/// (or its box), legend stripped.
#[allow(clippy::too_many_arguments)]
pub fn part(b: &mut Builder<'_>, parent: NodeId, id: &str, p: Place, anchor: &[&str], (cx, cy): (f64, f64)) -> NodeId {
    let k = ui::put_by(b, parent, id, &p, anchor, cx, cy);
    ui::strip_legend(b, k);
    k
}

/// A two-position toggle (`switch.toggle_nut`, chrome or black bat) on
/// `addr`, its nut at (`cx`, `cy`); legends are the face's.
pub fn toggle(b: &mut Builder<'_>, parent: NodeId, addr: &str, name: &str, size: f64, variant: &str, at: (f64, f64)) -> NodeId {
    let p = Place::new(name).size(size).variant(variant).bind(ui::bind(addr)).legend("").options(&["legend_right"]).touch(TOUCH_MIN);
    part(b, parent, "switch.toggle_nut", p, &["bushing"], at)
}

/// Move a placed control so its box is centred on (`cx`, `cy`).
pub fn centre_on(b: &mut Builder<'_>, id: NodeId, (cx, cy): (f64, f64)) {
    let (w, h) = b.scene.node(id).map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    ui::move_to(b, id, cx - w / 2.0, cy - h / 2.0);
}

/// A stomp footswitch on toggle `addr` (no LED of its own — the pedal's
/// LED is a separate [`status_led`]), its actuator `d` across centred at
/// (`cx`, `cy`). `momentary` for a tap switch.
#[allow(clippy::too_many_arguments)]
pub fn footswitch(b: &mut Builder<'_>, parent: NodeId, addr: &str, name: &str, d: f64, at: (f64, f64), momentary: bool) -> NodeId {
    let mut p = Place::new(name).size(d).variant("none").bind(ui::bind(addr));
    if momentary {
        p = p.options(&["momentary"]);
    }
    let k = ui::put_by(b, parent, "switch.footswitch", &p, &["cap"], at.0, at.1);
    ui::strip_legend(b, k);
    k
}

/// An LED showing toggle `addr`, lens `d` across, in `color`, bezel
/// `variant` (`light`, `dark`, `chrome`); `pulse` makes it breathe at a
/// rate (a motion expression in Hz).
#[allow(clippy::too_many_arguments)]
pub fn status_led(b: &mut Builder<'_>, parent: NodeId, addr: &str, d: f64, at: (f64, f64), color: &str, variant: &str, pulse: Option<&str>) -> NodeId {
    let mut opts = vec![format!("color={color}")];
    if let Some(r) = pulse {
        opts.push(format!("pulse={r}"));
    }
    let p = Place { options: Some(opts), ..Place::new("LED").size(d).variant(variant).bind(ui::bind(addr)) };
    let k = surface_kit::place(b, parent, "meter.status_led", &p).unwrap_or_else(|e| panic!("{e}"));
    centre_on(b, k, at);
    k
}

// ── lettering ────────────────────────────────────────────────────────────

/// How a line of lettering is set.
#[derive(Clone, Copy)]
pub struct Type<'a> {
    pub size: f64,
    pub color: &'a str,
    pub weight: f64,
    /// Letter spacing, in ems.
    pub track: f64,
    pub upper: bool,
}

impl<'a> Type<'a> {
    #[must_use]
    pub const fn new(size: f64, color: &'a str, weight: f64) -> Self {
        Self { size, color, weight, track: 0.0, upper: false }
    }
    #[must_use]
    pub const fn track(mut self, em: f64) -> Self {
        self.track = em;
        self
    }
    #[must_use]
    pub const fn upper(mut self) -> Self {
        self.upper = true;
        self
    }
}

/// A line of lettering in a box `w` wide from `x`, aligned `align`,
/// vertically centred on `cy`.
#[allow(clippy::too_many_arguments)]
pub fn words(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, w: f64, align: TextAlign, t: Type<'_>) -> NodeId {
    let make = |dx: f64| {
        let mut n = ui::text("print", s, x + dx, cy, w, t.size, t.color, align, t.weight, false);
        if let NodeKind::Text(d) = &mut n.kind {
            d.letter_spacing = t.size * t.track;
            if t.upper {
                d.text_case = TextCase::Upper;
            }
            // The weight as a run, which the renderer sets bold from.
            if t.weight >= 600.0 {
                d.runs = vec![TextRun { text: s.to_string(), fills: Vec::new(), font_family: None, font_size: None, font_weight: Some(t.weight), font_style: None, letter_spacing: None }];
            }
        }
        n
    };
    // Heavier than bold: the same line drawn again either side, a hair
    // apart, so a black-weight name reads black with one font file.
    if t.weight >= 750.0 {
        let e = t.size * if t.weight >= 850.0 { 0.05 } else { 0.03 };
        for dx in [-e, e] {
            b.add(parent, make(dx));
        }
    }
    b.add(parent, make(0.0))
}

/// Lettering centred on (`cx`, `cy`).
pub fn centred(b: &mut Builder<'_>, parent: NodeId, s: &str, (cx, cy): (f64, f64), t: Type<'_>) -> NodeId {
    let w = (s.chars().count() as f64 * t.size * (0.72 + t.track) + t.size * 2.0).max(8.0);
    words(b, parent, s, cx - w / 2.0, cy, w, TextAlign::Center, t)
}

/// Stacked lines set from the left at `x`, the first centred on `cy`,
/// `lead` px apart (a pedal's name in the corner).
pub fn stacked(b: &mut Builder<'_>, parent: NodeId, lines: &[&str], (x, cy): (f64, f64), lead: f64, t: Type<'_>) {
    for (i, l) in lines.iter().enumerate() {
        let w = l.chars().count() as f64 * t.size * 0.8 + t.size * 2.0;
        words(b, parent, l, x, cy + lead * i as f64, w, TextAlign::Left, t);
    }
}

/// Lettering with an outline (the cartoon maker's mark): `outline` drawn
/// as a slightly larger dark copy offset round the text, then the fill.
pub fn outlined(b: &mut Builder<'_>, parent: NodeId, s: &str, (cx, cy): (f64, f64), t: Type<'_>, outline: &str, px: f64) {
    for (dx, dy) in [(-px, 0.0), (px, 0.0), (0.0, -px), (0.0, px), (-px, -px), (px, px), (-px, px), (px, -px), (0.0, px * 2.0)] {
        centred(b, parent, s, (cx + dx, cy + dy), Type { color: outline, ..t });
    }
    centred(b, parent, s, (cx, cy), t);
}

/// A stroked line drawing (icons, printed artwork), points in `parent`'s
/// space, round caps.
pub fn art(b: &mut Builder<'_>, parent: NodeId, lines: &[Vec<(f64, f64)>], weight: f64, color: &str) -> NodeId {
    b.add(parent, ui::strokes("art", lines, weight, draw::solid(color)))
}

/// A circle's outline as a polyline (for [`art`]).
#[must_use]
pub fn ring(cx: f64, cy: f64, r: f64) -> Vec<(f64, f64)> {
    ui::ellipse_pts(cx, cy, r, r, 48)
}

/// A rounded rectangle's outline as a polyline (for [`art`]).
#[must_use]
pub fn round_rect(x: f64, y: f64, w: f64, h: f64, r: f64) -> Vec<(f64, f64)> {
    let mut pts = Vec::new();
    let corners = [(x + w - r, y + r, -90.0), (x + w - r, y + h - r, 0.0), (x + r, y + h - r, 90.0), (x + r, y + r, 180.0)];
    for (cx, cy, a0) in corners {
        for i in 0..=6 {
            let a = (a0 + 15.0 * f64::from(i)).to_radians();
            pts.push((cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    pts.push(pts[0]);
    pts
}

/// A small screw head.
pub fn screw(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64, metal: &str) {
    let head = draw::circle("screw", cx, cy, r, draw::radial_at(0.35, 0.3, 0.8, &[(0.0, "#f6f5f0"), (1.0, metal)]));
    b.add(parent, draw::with_effects(head, vec![draw::inner_shadow("#00000080", 0.0, 0.0, r * 0.35), draw::drop_shadow("#00000066", 0.0, r * 0.2, r * 0.3)]));
    b.add(parent, draw::rotated(draw::rect("slot", cx - r * 0.62, cy - r * 0.12, r * 1.24, r * 0.24, draw::solid("#00000099")), 35.0));
}

/// An input jack with a cable running out of it and down off the unit.
pub fn cable(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, to: &[(f64, f64)]) {
    let nut = draw::path("jack nut", cx - r * 1.3, cy - r * 1.3, r * 2.6, r * 2.6, draw::regular(r * 1.3, r * 1.3, r * 1.3, 6, 0.0), draw::linear(160.0, &[(0.0, "#f2f2ee"), (0.5, "#8d8e8b"), (1.0, "#d8d8d4")]));
    b.add(parent, draw::with_effects(nut, vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)]));
    b.add(parent, draw::circle("jack", cx, cy, r * 0.8, draw::radial(&[(0.0, "#1a1a1a"), (1.0, "#444444")])));
    let mut pts = vec![(cx, cy)];
    pts.extend_from_slice(to);
    b.add(parent, draw::with_effects(ui::strokes("cable", &[pts.clone()], r * 0.9, draw::solid("#1b1b1d")), vec![draw::drop_shadow("#00000070", 0.0, 3.0, 4.0)]));
    b.add(parent, ui::strokes("cable sheen", &[pts.iter().map(|(x, y)| (x - r * 0.15, y - r * 0.15)).collect()], r * 0.2, draw::solid("#ffffff22")));
    b.add(parent, draw::circle("plug", cx, cy, r * 0.62, draw::radial_at(0.4, 0.35, 0.7, &[(0.0, "#e8e8e4"), (1.0, "#6a6a68")])));
}

// ── catalog ──────────────────────────────────────────────────────────────

/// A 0–10 amp-style control (printed 0…10), default `v`.
#[must_use]
pub fn dial10(addr: &str, name: &str, v: f64) -> ParamInfo {
    ui::param(addr, name, Range::linear(0.0, 10.0, Unit::None), v).featured()
}

/// A 0–100 % control.
#[must_use]
pub fn pct(addr: &str, name: &str, v: f64) -> ParamInfo {
    ui::percent(addr, name, v).featured()
}

/// A dB control over `lo..hi`.
#[must_use]
pub fn db(addr: &str, name: &str, lo: f64, hi: f64, v: f64) -> ParamInfo {
    let p = ui::param(addr, name, Range::linear(lo, hi, Unit::Db), v).featured();
    if lo < 0.0 && hi > 0.0 { p.bipolar() } else { p }
}

/// A frequency control over `lo..hi` Hz, logarithmic.
#[must_use]
pub fn hz(addr: &str, name: &str, lo: f64, hi: f64, v: f64) -> ParamInfo {
    ui::param(addr, name, Range::log(lo, hi, Unit::Hz), v).featured()
}

/// A time control in ms over `lo..hi`, logarithmic.
#[must_use]
pub fn ms(addr: &str, name: &str, lo: f64, hi: f64, v: f64) -> ParamInfo {
    ui::param(addr, name, Range::log(lo, hi, Unit::Milliseconds), v).featured()
}

/// An on/off.
#[must_use]
pub fn switch(addr: &str, name: &str, on: bool) -> ParamInfo {
    ui::toggle(addr, name, on).featured()
}

/// A stepped choice.
#[must_use]
pub fn pick(addr: &str, name: &str, options: &[&str], default: usize) -> ParamInfo {
    ui::choice(addr, name, options, default).featured()
}
