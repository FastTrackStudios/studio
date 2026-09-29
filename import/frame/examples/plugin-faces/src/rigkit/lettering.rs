//! Display lettering cut as outlines.
//!
//! The renderer sets text in one weight, so logos and model names — whose
//! weight, slant and width are most of what makes them read as *that*
//! maker's lettering — are set here as glyph outlines from Inter at any
//! weight (100–900), upright or italic, condensed or extended, and stored
//! as the text node's own geometry (the node keeps its characters, so it
//! still reads as text to lint and to a screen reader).

use document_proto::{NodeId, TextAlign};
use surface_kit::draw;
use surface_widgets::Builder;

/// How a line of display lettering is cut.
#[derive(Clone, Copy)]
pub struct Letter<'a> {
    pub size: f64,
    pub weight: f64,
    pub italic: bool,
    /// Letter spacing, em.
    pub track: f64,
    pub color: &'a str,
    /// Horizontal scale: < 1 condenses, > 1 extends.
    pub stretch: f64,
}

impl<'a> Letter<'a> {
    #[must_use]
    pub const fn new(size: f64, weight: f64, color: &'a str) -> Self {
        Self { size, weight, italic: false, track: 0.0, color, stretch: 1.0 }
    }
    #[must_use]
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    #[must_use]
    pub const fn track(mut self, t: f64) -> Self {
        self.track = t;
        self
    }
    #[must_use]
    pub const fn stretch(mut self, s: f64) -> Self {
        self.stretch = s;
        self
    }
    #[must_use]
    pub const fn color(mut self, c: &'a str) -> Self {
        self.color = c;
        self
    }
}

const INTER: &[u8] = include_bytes!("../../../../apps/frame/cli/assets/inter/Inter-Variable.ttf");
const INTER_ITALIC: &[u8] = include_bytes!("../../../../apps/frame/cli/assets/inter/Inter-Italic-Variable.ttf");

struct SvgPen {
    d: String,
    ox: f64,
    oy: f64,
    sx: f64,
    /// Then this affine (a, b, c, d, e, f): x' = a·x + c·y + e, y' = b·x + d·y + f.
    m: [f64; 6],
}

impl SvgPen {
    fn p(&self, x: f32, y: f32) -> (f64, f64) {
        let (x, y) = (self.ox + f64::from(x) * self.sx, self.oy - f64::from(y));
        let m = self.m;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }
}

impl skrifa::outline::OutlinePen for SvgPen {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.p(x, y);
        self.d.push_str(&format!("M {x:.2} {y:.2} "));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.p(x, y);
        self.d.push_str(&format!("L {x:.2} {y:.2} "));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (a, b) = self.p(cx0, cy0);
        let (x, y) = self.p(x, y);
        self.d.push_str(&format!("Q {a:.2} {b:.2} {x:.2} {y:.2} "));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (a, b) = self.p(cx0, cy0);
        let (c, d) = self.p(cx1, cy1);
        let (x, y) = self.p(x, y);
        self.d.push_str(&format!("C {a:.2} {b:.2} {c:.2} {d:.2} {x:.2} {y:.2} "));
    }
    fn close(&mut self) {
        self.d.push_str("Z ");
    }
}

/// `s` cut as outlines: the path (in a box whose top-left is the origin,
/// the baseline at `0.78 × size`), and the set width.
#[must_use]
pub fn outline(s: &str, l: &Letter<'_>) -> (String, f64) {
    outline_with(s, l, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
}

/// [`outline`] with every point mapped through the affine `m`.
#[must_use]
pub fn outline_with(s: &str, l: &Letter<'_>, m: [f64; 6]) -> (String, f64) {
    use skrifa::MetadataProvider;
    use skrifa::instance::{LocationRef, Size};
    use skrifa::outline::DrawSettings;
    let Ok(font) = skrifa::FontRef::new(if l.italic { INTER_ITALIC } else { INTER }) else { return (String::new(), 0.0) };
    let loc = font.axes().location([("wght", l.weight as f32), ("opsz", (l.size as f32).clamp(14.0, 32.0))]);
    let size = Size::new(l.size as f32);
    let charmap = font.charmap();
    let gm = font.glyph_metrics(size, LocationRef::from(&loc));
    let outlines = font.outline_glyphs();
    let mut pen = SvgPen { d: String::new(), ox: 0.0, oy: l.size * 0.78, sx: l.stretch, m };
    let mut x = 0.0;
    let chars: Vec<char> = s.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        let gid = charmap.map(*ch).unwrap_or_default();
        pen.ox = x;
        if let Some(g) = outlines.get(gid) {
            let _ = g.draw(DrawSettings::unhinted(size, LocationRef::from(&loc)), &mut pen);
        }
        x += f64::from(gm.advance_width(gid).unwrap_or_default()) * l.stretch;
        if i + 1 < chars.len() {
            x += l.track * l.size;
        }
    }
    (pen.d, x)
}

/// Display lettering centred on (`cx`, `cy`) — `cy` the middle of the caps.
pub fn letter(b: &mut Builder<'_>, parent: NodeId, s: &str, (cx, cy): (f64, f64), l: Letter<'_>) -> NodeId {
    letter_aligned(b, parent, s, (cx, cy), l, TextAlign::Center)
}

/// Display lettering turned by `deg` (clockwise) about its own centre,
/// which lands on (`cx`, `cy`) — a legend printed along a knob's slot,
/// a vertical label. The node's box is the square the turned line sweeps.
pub fn letter_turned(b: &mut Builder<'_>, parent: NodeId, s: &str, (cx, cy): (f64, f64), deg: f64, l: Letter<'_>) -> NodeId {
    let (_, w) = outline(s, &l);
    let r = w.max(l.size) / 2.0 + 2.0;
    let (sn, cs) = deg.to_radians().sin_cos();
    // Centre the line (its caps' middle) on the origin, turn, move to (r, r).
    let (x0, y0) = (w / 2.0, l.size * 0.78 - l.size * 0.727 / 2.0);
    let m = [cs, sn, -sn, cs, r - (cs * x0 - sn * y0), r - (sn * x0 + cs * y0)];
    let (d, _) = outline_with(s, &l, m);
    let mut n = draw::text("lettering", s, cx - r, cy - r, 2.0 * r, l.size, l.color, TextAlign::Center);
    n.name = format!("lettering {s}");
    n.size.height = 2.0 * r;
    n.fill_geometry = vec![d];
    b.add(parent, n)
}

/// Display lettering sized so it sets `width` wide, centred on (`cx`, `cy`);
/// returns the node and the size it was cut at.
pub fn letter_fit(b: &mut Builder<'_>, parent: NodeId, s: &str, (cx, cy): (f64, f64), width: f64, l: Letter<'_>) -> (NodeId, f64) {
    let probe = Letter { size: 100.0, ..l };
    let (_, w) = outline(s, &probe);
    let size = if w > 0.0 { 100.0 * width / w } else { l.size };
    (letter(b, parent, s, (cx, cy), Letter { size, ..l }), size)
}

/// Display lettering anchored at `x` by `align`, the caps centred on `cy`.
pub fn letter_aligned(b: &mut Builder<'_>, parent: NodeId, s: &str, (x, cy): (f64, f64), l: Letter<'_>, align: TextAlign) -> NodeId {
    let (d, w) = outline(s, &l);
    let x0 = match align {
        TextAlign::Center => x - w / 2.0,
        TextAlign::Right => x - w,
        _ => x,
    };
    // Inter's cap height is ≈ 0.727 em: centre the caps on `cy`.
    let top = cy - (l.size * 0.78 - l.size * 0.727 / 2.0);
    let mut n = draw::text("lettering", s, x0, top, w.max(1.0), l.size, l.color, TextAlign::Left);
    n.name = format!("lettering {s}");
    n.size.height = l.size;
    n.fill_geometry = vec![d];
    b.add(parent, n)
}
