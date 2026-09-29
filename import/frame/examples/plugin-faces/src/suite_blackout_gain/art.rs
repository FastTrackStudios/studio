//! The Blackout Gain family's printed art: white line-work on black —
//! profile skulls, braided locks, knots and petal rosettes, the notched
//! border every box is framed in, and the maker's badge (an invented
//! "VANTOM" plate with hex bolts at its ends).
//!
//! Every motif is authored once in a 100 × 100 box facing right and put
//! down with [`Put`] (where, how big, mirrored or not).

use document_proto::NodeId;
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_widgets::Builder;

use crate::delay::parts::{self, Ink};
use crate::rigkit::{self as rk, Letter};

/// The print: an off-white with a little warmth.
pub const INK: &str = "#e4e1da";
/// The shading on the print (skull shadows, ornament fills).
pub const SHADE: &str = "#8d8a84";
/// The lid.
pub const BLACK: &str = "#161616";
/// The LED green of the gate and boost boxes.
pub const GREEN: &str = "#3dff6a";
/// The drive's red.
pub const RED: &str = "#ff3326";

/// Where a motif goes: its box's top-left, its size (the 100-unit box
/// becomes `s` px), mirrored left–right or not.
#[derive(Clone, Copy)]
pub struct Put {
    pub x: f64,
    pub y: f64,
    pub s: f64,
    pub flip: bool,
    /// Vertical scale on top of `s` (1 = square).
    pub tall: f64,
}

impl Put {
    #[must_use]
    pub const fn new(x: f64, y: f64, s: f64, flip: bool) -> Self {
        Self { x, y, s, flip, tall: 1.0 }
    }
    fn pt(&self, u: f64, v: f64) -> (f64, f64) {
        let u = if self.flip { 100.0 - u } else { u };
        (self.x + u * self.s / 100.0, self.y + v * self.s * self.tall / 100.0)
    }
}

/// Map an absolute SVG path (M L C Q Z with coordinate pairs) through `p`.
#[must_use]
pub fn map_path(d: &str, p: &Put) -> String {
    let mut out = String::new();
    let mut nums: Vec<f64> = Vec::new();
    let flush = |nums: &mut Vec<f64>, out: &mut String| {
        for pair in nums.chunks(2) {
            if let [u, v] = pair {
                let (x, y) = p.pt(*u, *v);
                out.push_str(&format!("{x:.2} {y:.2} "));
            }
        }
        nums.clear();
    };
    for tok in d.split_whitespace() {
        if let Ok(v) = tok.parse::<f64>() {
            nums.push(v);
        } else {
            flush(&mut nums, &mut out);
            out.push_str(tok);
            out.push(' ');
        }
    }
    flush(&mut nums, &mut out);
    out
}

/// A filled motif path under `parent` (which spans `size`).
pub fn fill(b: &mut Builder<'_>, parent: NodeId, name: &str, size: (f64, f64), d: &str, p: &Put, color: &str) -> NodeId {
    b.add(parent, draw::path(name, 0.0, 0.0, size.0, size.1, map_path(d, p), draw::solid(color)))
}

/// A stroked motif path.
#[allow(clippy::too_many_arguments)]
pub fn stroke(b: &mut Builder<'_>, parent: NodeId, name: &str, size: (f64, f64), d: &str, p: &Put, weight: f64, color: &str) -> NodeId {
    let mut n = draw::path(name, 0.0, 0.0, size.0, size.1, map_path(d, p), draw::solid(color));
    n.fills = Vec::new();
    n.stroke = document_proto::Stroke {
        paints: vec![draw::solid(color)],
        weight,
        cap: document_proto::StrokeCap::Round,
        join: document_proto::StrokeJoin::Round,
        align: document_proto::StrokeAlign::Center,
        ..document_proto::Stroke::default()
    };
    b.add(parent, n)
}

// ── skull ────────────────────────────────────────────────────────────────

const CRANIUM: &str = "M 8 46 C 4 26 18 8 42 5 C 66 2 86 12 92 30 C 94 38 92 44 95 50 L 99 58 L 93 60 L 94 64 L 90 66 C 80 67 60 68 44 70 C 32 71 26 74 22 80 C 14 72 8 60 8 46 Z";
const JAW: &str = "M 30 76 C 40 73 70 72 91 71 L 90 80 C 88 88 80 94 66 96 C 52 97 42 94 36 90 C 32 86 30 81 30 76 Z";
const BACK_SHADE: &str = "M 8 46 C 4 26 18 8 42 5 C 28 12 18 26 18 44 C 18 58 24 70 30 76 L 22 80 C 14 72 8 60 8 46 Z";
const EYE: &str = "M 58 30 C 64 22 80 22 86 32 C 89 41 82 49 72 48 C 62 47 55 40 58 30 Z";
const BROW: &str = "M 50 24 C 62 14 84 14 92 28 C 84 20 64 20 54 30 Z";
const NOSE: &str = "M 89 42 C 92 47 94 52 95 56 L 88 56 C 89 51 88 46 89 42 Z";
const CHEEK: &str = "M 44 52 C 54 46 72 48 84 56 C 72 54 58 54 46 58 Z";
const TEMPLE: &str = "M 28 14 C 22 26 22 38 30 48 M 34 58 C 40 52 48 52 54 58 M 40 88 C 50 90 60 90 70 88";

/// A profile skull, grinning, facing right (or left if flipped).
pub fn skull(b: &mut Builder<'_>, parent: NodeId, size: (f64, f64), p: &Put) {
    let k = p.s / 100.0;
    let p = &Put { tall: p.tall * 0.86, ..*p };
    fill(b, parent, "skull", size, CRANIUM, p, INK);
    fill(b, parent, "skull shade", size, BACK_SHADE, p, SHADE);
    fill(b, parent, "skull jaw", size, JAW, p, INK);
    fill(b, parent, "skull brow", size, BROW, p, SHADE);
    fill(b, parent, "skull eye", size, EYE, p, BLACK);
    fill(b, parent, "skull nose", size, NOSE, p, BLACK);
    fill(b, parent, "skull cheek", size, CHEEK, p, SHADE);
    fill(b, parent, "skull temple", size, "M 32 26 C 44 30 50 42 46 56 C 40 50 34 40 32 26 Z", p, SHADE);
    fill(b, parent, "skull hinge", size, "M 26 66 C 30 70 34 74 36 80 C 32 80 28 76 26 66 Z", p, BLACK);
    stroke(b, parent, "skull cracks", size, TEMPLE, p, 0.9 * k.max(0.4), SHADE);
    // Teeth: the upper row hangs from the maxilla, the lower rises from
    // the jaw; the gap between them is the grin.
    let mut upper = String::new();
    let mut lower = String::new();
    for i in 0..9 {
        let u = 48.0 + f64::from(i) * 4.8;
        upper.push_str(&format!("M {u} 64.5 L {} 64.5 L {} 71.5 L {} 72 L {} 71.5 Z ", u + 4.1, u + 3.7, u + 2.05, u + 0.4));
        lower.push_str(&format!("M {u} 80 L {} 80 L {} 74 L {} 73.5 L {} 74 Z ", u + 4.1, u + 3.7, u + 2.05, u + 0.4));
    }
    fill(b, parent, "skull gap", size, "M 44 63 L 94 62 L 92 81 L 42 82 Z", p, BLACK);
    fill(b, parent, "skull teeth", size, &upper, p, INK);
    fill(b, parent, "skull teeth", size, &lower, p, INK);
}

/// Braided locks hanging from `top` down `len` px at `x`: a few twisting
/// strands with the braid's cross ticks.
#[allow(clippy::too_many_arguments)]
pub fn locks(b: &mut Builder<'_>, parent: NodeId, size: (f64, f64), (x, top): (f64, f64), len: f64, strands: usize, spread: f64, weight: f64) {
    for s in 0..strands {
        let x0 = x + (s as f64 - (strands as f64 - 1.0) / 2.0) * spread;
        let pts: Vec<(f64, f64)> = (0..=12).map(|i| {
            let t = f64::from(i) / 12.0;
            (x0 + (t * 7.0 + s as f64).sin() * spread * 0.35, top + t * len)
        }).collect();
        rk::curve(b, parent, "lock", size, &pts, weight, INK);
        let ticks: Vec<Vec<(f64, f64)>> = pts.windows(2).map(|w| {
            let (mx, my) = ((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0);
            vec![(mx - spread * 0.3, my - 1.2), (mx + spread * 0.3, my + 1.2)]
        }).collect();
        let mut n = parts::stroke_path("braid", (0.0, 0.0, size.0, size.1), &ticks, weight * 0.7, BLACK, false);
        n.stroke.cap = document_proto::StrokeCap::None;
        b.add(parent, n);
    }
}

/// A petal rosette (the gate's flower behind its knob): `n` white petals
/// round (`cx`, `cy`), outlined, their hearts black.
pub fn rosette(b: &mut Builder<'_>, parent: NodeId, size: (f64, f64), (cx, cy): (f64, f64), r: f64, n: u32) {
    // A fan of round petals over the top half, each with a black keyhole
    // in it, and a white ring round the knob's seat.
    let step = 200.0 / f64::from(n.max(2) - 1);
    let pr = r * 0.34;
    let mut white = String::new();
    let mut holes = String::new();
    let circ = |x: f64, y: f64, r: f64| {
        let k = 0.552_284_75 * r;
        format!(
            "M {:.2} {y:.2} C {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} C {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} Z ",
            x + r, x + r, y + k, x + k, y + r, y + r, x - k, y + r, x - r, y + k, x - r, x - r, y - k, x - k, y - r, y - r, x + k, y - r, x + r, y - k, x + r
        )
    };
    for i in 0..n {
        let a = -100.0 + step * f64::from(i);
        let (x, y) = draw::polar(cx, cy, r - pr, a);
        white.push_str(&circ(x, y, pr));
        let (hx, hy) = draw::polar(cx, cy, r - pr * 0.8, a);
        holes.push_str(&circ(hx, hy, pr * 0.32));
    }
    white.push_str(&circ(cx, cy, r * 0.68));
    b.add(parent, draw::path("rosette", 0.0, 0.0, size.0, size.1, white, draw::solid(INK)));
    b.add(parent, draw::path("rosette holes", 0.0, 0.0, size.0, size.1, holes, draw::solid(BLACK)));
    b.add(parent, draw::circle("rosette seat", cx, cy, r * 0.6, draw::solid(BLACK)));
}

/// A knot square (a celtic tile): an over-under loop in a square box.
pub fn knot(b: &mut Builder<'_>, parent: NodeId, size: (f64, f64), (cx, cy): (f64, f64), s: f64) {
    let h = s / 2.0;
    let p = Put::new(cx - h, cy - h, s, false);
    const LOOP: &str = "M 20 50 C 20 20 50 20 50 50 C 50 80 80 80 80 50 C 80 20 50 20 50 50 C 50 80 20 80 20 50 Z M 50 20 C 80 20 80 50 50 50 C 20 50 20 80 50 80 C 80 80 80 50 50 50 C 20 50 20 20 50 20 Z";
    stroke(b, parent, "knot", size, LOOP, &p, (s * 0.07).max(1.0), INK);
    stroke(b, parent, "knot", size, LOOP, &p, (s * 0.025).max(0.5), BLACK);
}

/// A scroll flourish (a curl pair) centred on (`cx`, `cy`), `w` wide.
pub fn flourish(b: &mut Builder<'_>, parent: NodeId, size: (f64, f64), (cx, cy): (f64, f64), w: f64, flip: bool) {
    const CURL: &str = "M 0 50 C 20 50 30 30 45 30 C 60 30 62 48 52 52 C 44 55 40 46 46 42 M 45 30 C 60 20 80 40 100 40";
    let p = Put::new(cx - w / 2.0, cy - w / 2.0, w, flip);
    stroke(b, parent, "flourish", size, CURL, &p, (w * 0.035).max(0.8), INK);
}

// ── frame, badge, jacks ──────────────────────────────────────────────────

/// The notched white border inset `m` px inside `w × h`, from `top` down to
/// `bottom`, with the jack arrows and the supply legend over the top rule.
pub fn border(b: &mut Builder<'_>, parent: NodeId, (w, h): (f64, f64), m: f64, (top, bottom): (f64, f64), supply: &str) {
    let n = 7.0;
    let (x0, x1, y0, y1) = (m, w - m, top, bottom);
    let d = format!(
        "M {} {y0} L {} {y0} L {x1} {} L {x1} {} L {} {y1} L {} {y1} L {x0} {} L {x0} {} Z",
        x0 + n,
        x1 - n,
        y0 + n,
        y1 - n,
        x1 - n,
        x0 + n,
        y1 - n,
        y0 + n
    );
    let mut rim = draw::path("border", 0.0, 0.0, w, h, d, draw::solid(INK));
    rim.fills = Vec::new();
    rim.stroke = document_proto::Stroke { paints: vec![draw::solid(INK)], weight: 1.6, join: document_proto::StrokeJoin::Miter, ..document_proto::Stroke::default() };
    b.add(parent, rim);
    // A gap in the top rule for the arrows and the supply legend.
    b.add(parent, draw::rect("border gap", w / 2.0 - 30.0, y0 - 2.0, 60.0, 4.0, draw::solid(BLACK)));
    for (x, up) in [(w * 0.3, false), (w * 0.7, true)] {
        let tri = if up { draw::polygon(&[(x - 3.2, y0 + 1.5), (x + 3.2, y0 + 1.5), (x, y0 - 4.0)]) } else { draw::polygon(&[(x - 3.2, y0 - 4.0), (x + 3.2, y0 - 4.0), (x, y0 + 1.5)]) };
        b.add(parent, draw::path("jack arrow", 0.0, 0.0, w, h, tri, draw::solid(INK)));
    }
    rk::silk(b, parent, supply, w / 2.0, y0 - 1.0, Ink::new(4.4, INK).weight(700.0).track(0.02));
    // The barrel-jack glyph under it.
    let gy = y0 + 5.0;
    b.add(parent, draw::stroked(draw::circle("dc glyph", w / 2.0, gy, 1.8, draw::solid(BLACK)), INK, 0.7));
    parts::rule(b, parent, w / 2.0 - 7.0, w / 2.0 - 2.0, gy, 0.7, INK);
    parts::rule(b, parent, w / 2.0 + 2.0, w / 2.0 + 7.0, gy, 0.7, INK);
}

/// A hex bolt head, printed (the badge's ends).
pub fn bolt(b: &mut Builder<'_>, parent: NodeId, size: (f64, f64), (cx, cy): (f64, f64), r: f64) {
    let mut hexn = draw::path("bolt", 0.0, 0.0, size.0, size.1, draw::regular(cx, cy, r, 6, 0.0), draw::solid(BLACK));
    hexn = draw::stroked(hexn, INK, (r * 0.22).max(0.6));
    b.add(parent, hexn);
    b.add(parent, draw::circle("bolt dot", cx, cy, r * 0.38, draw::solid(INK)));
}

/// The maker's badge centred on (`cx`, `cy`), `w` wide: a stretched
/// octagonal plate outlined in white, bolts at both ends, the invented
/// maker's word in squared extended capitals and a spaced line under it.
pub fn badge(b: &mut Builder<'_>, parent: NodeId, size: (f64, f64), (cx, cy): (f64, f64), w: f64) {
    let h = w * 0.2;
    let (x0, x1, y0, y1) = (cx - w / 2.0, cx + w / 2.0, cy - h / 2.0, cy + h / 2.0);
    let c = h * 0.45;
    let d = format!(
        "M {} {y0} L {} {y0} L {x1} {} L {x1} {} L {} {y1} L {} {y1} L {x0} {} L {x0} {} Z",
        x0 + c,
        x1 - c,
        y0 + c,
        y1 - c,
        x1 - c,
        x0 + c,
        y1 - c,
        y0 + c
    );
    let plate = draw::stroked(draw::path("badge", 0.0, 0.0, size.0, size.1, d, draw::solid(BLACK)), INK, (h * 0.07).max(0.9));
    b.add(parent, plate);
    let br = h * 0.24;
    bolt(b, parent, size, (x0 + c + br * 0.4, cy), br);
    bolt(b, parent, size, (x1 - c - br * 0.4, cy), br);
    let (_, s) = rk::letter_fit(b, parent, "VANTOM", (cx, cy - h * 0.1), w * 0.6, Letter::new(10.0, 560.0, INK).stretch(1.25).track(0.08));
    rk::letter_fit(b, parent, "AMPLIFICATION", (cx, cy + s * 0.62), w * 0.44, Letter::new(3.0, 500.0, INK).track(0.5));
}

/// A white printed ring round a stomp switch, with a flourish either side.
pub fn switch_ring(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64) {
    b.add(parent, draw::stroked(draw::circle("switch ring", cx, cy, r, draw::solid("#00000000")), INK, 2.2));
}

/// The plain stomp switch, its button centred on (`cx`, `cy`), the LED
/// (`dx`, `dy`) from it.
#[allow(clippy::too_many_arguments)]
pub fn stomp(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, (cx, cy): (f64, f64), d: f64, led: &str, (dx, dy): (f64, f64), led_size: f64, momentary: bool) -> NodeId {
    let mut opts = vec![format!("color={led}"), format!("led_dx={dx}"), format!("led_dy={dy}"), format!("led_size={led_size}")];
    if momentary {
        opts.push("momentary".into());
    }
    let p = Place { options: Some(opts), ..Place::new(name).size(d).bind(rk::bind(addr)).touch(TOUCH_MIN) };
    let root = surface_kit::place(b, parent, "switch.stomp_box", &p).unwrap_or_else(|e| panic!("{e}"));
    let at = b.scene.node(root).and_then(|n| {
        n.children.iter().find_map(|c| b.scene.node(*c).filter(|k| k.name == "washer").map(|k| (k.transform.tx + k.size.width / 2.0, k.transform.ty + k.size.height / 2.0)))
    });
    let (ax, ay) = at.unwrap_or((0.0, 0.0));
    parts::move_to(b, root, cx - ax, cy - ay);
    root
}

/// A legend in the family's squared capitals.
pub fn legend(size: f64) -> Ink<'static> {
    Ink::new(size, INK).weight(700.0).track(0.04)
}

/// Model-name lettering: tall condensed heavy capitals.
pub fn title(b: &mut Builder<'_>, parent: NodeId, s: &str, (cx, cy): (f64, f64), w: f64) -> f64 {
    let (_, size) = rk::letter_fit(b, parent, s, (cx, cy), w, Letter::new(20.0, 800.0, INK).stretch(0.78).track(0.04));
    size
}

/// Model-name lettering cut hollow: the letters' outlines only, as the
/// gate prints its name.
pub fn hollow_title(b: &mut Builder<'_>, parent: NodeId, s: &str, (cx, cy): (f64, f64), (w, h): (f64, f64), weight: f64) {
    use crate::rigkit::lettering::outline;
    let l = Letter::new(100.0, 900.0, INK).stretch(0.72).track(0.1);
    // Cut at 100 px, then scale to `w` wide and `h` tall (the caps: the
    // cap line sits 5.3 px under the box top at 100 px, the baseline at 78).
    let (d, set) = outline(s, &l);
    let (sx, sy) = (w / set.max(1.0), h / 72.7);
    let (x0, y0) = (cx - w / 2.0, cy - h / 2.0 - 5.3 * sy);
    let mapped = map_xy(&d, |x, y| (x0 + x * sx, y0 + y * sy));
    let mut n = draw::path(&format!("lettering {s}"), 0.0, 0.0, cx + w / 2.0, cy + h / 2.0, mapped, draw::solid(INK));
    n.fills = Vec::new();
    n.stroke = document_proto::Stroke { paints: vec![draw::solid(INK)], weight, join: document_proto::StrokeJoin::Miter, ..document_proto::Stroke::default() };
    b.add(parent, n);
}

/// Map every coordinate pair of an absolute path through `f`.
#[must_use]
pub fn map_xy(d: &str, f: impl Fn(f64, f64) -> (f64, f64)) -> String {
    let mut out = String::new();
    let mut nums: Vec<f64> = Vec::new();
    let flush = |nums: &mut Vec<f64>, out: &mut String| {
        for pair in nums.chunks(2) {
            if let [u, v] = pair {
                let (x, y) = f(*u, *v);
                out.push_str(&format!("{x:.2} {y:.2} "));
            }
        }
        nums.clear();
    };
    for tok in d.split_whitespace() {
        if let Ok(v) = tok.parse::<f64>() {
            nums.push(v);
        } else {
            flush(&mut nums, &mut out);
            out.push_str(tok);
            out.push(' ');
        }
    }
    flush(&mut nums, &mut out);
    out
}

/// A control's printed legend cut as display lettering, centred on panel
/// (`cx`, `cy`), made the control's `Label` (so the word on the panel still
/// names the control).
pub fn label_letter(b: &mut Builder<'_>, control: NodeId, s: &str, (cx, cy): (f64, f64), l: Letter<'_>) -> NodeId {
    let (tx, ty) = b.scene.node(control).map_or((0.0, 0.0), |n| (n.transform.tx, n.transform.ty));
    let id = rk::letter(b, control, s, (cx - tx, cy - ty), l);
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.retain(|s| s.role != surface_proto::SlotRole::Label);
        c.slots.push(surface_proto::Slot { role: surface_proto::SlotRole::Label, node: id.0 });
    }
    id
}

/// The family's knob legend: condensed heavy capitals.
#[must_use]
pub fn knob_letter(size: f64) -> Letter<'static> {
    Letter::new(size, 760.0, INK).stretch(0.82).track(0.02)
}

/// Black leather tolex over `(x, y, w, h)` (rounded `radius`): a near-black
/// base and the grain's crackle — short creases in a darker and a lighter
/// line — with the lighting shader on the grain (GPU renderers raise it
/// into relief; without effects it still reads as leather). Returns the
/// covering frame (clipped), a parent for anything on the covering.
pub fn tolex(b: &mut Builder<'_>, parent: NodeId, name: &str, (x, y, w, h): (f64, f64, f64, f64), radius: f64, seed: u64) -> NodeId {
    let mut n = draw::frame(name, x, y, w, h);
    n.fills = vec![draw::linear(180.0, &[(0.0, "#232323"), (0.5, "#1b1b1b"), (1.0, "#141414")])];
    n.clips_content = true;
    let root = b.add(parent, draw::rounded(n, radius));
    let mut s = seed | 1;
    let mut rnd = move || {
        s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (s >> 33) as f64 / f64::from(u32::MAX >> 1)
    };
    for (color, weight, per) in [("#080808", 0.6, 34.0), ("#383838", 0.5, 60.0)] {
        let mut d = String::new();
        let count = (w * h / per) as usize;
        for _ in 0..count {
            let (px, py) = (rnd() * w, rnd() * h);
            let a = rnd() * std::f64::consts::TAU;
            let len = 1.4 + rnd() * 2.6;
            let bend = (rnd() - 0.5) * 2.4;
            let (qx, qy) = (px + a.cos() * len, py + a.sin() * len);
            let (mx, my) = ((px + qx) / 2.0 - a.sin() * bend, (py + qy) / 2.0 + a.cos() * bend);
            d.push_str(&format!("M{px:.1} {py:.1} Q{mx:.1} {my:.1} {qx:.1} {qy:.1} "));
        }
        let mut g = draw::path("grain", 0.0, 0.0, w, h, d, draw::solid(color));
        g.fills = Vec::new();
        g.stroke = document_proto::Stroke { paints: vec![draw::solid(color)], weight, cap: document_proto::StrokeCap::Round, ..document_proto::Stroke::default() };
        let g = b.add(root, g);
        parts::shader(b, g, "lighting", &[("strength", 0.6), ("scale", 1.5)]);
    }
    // The room's light across it.
    b.add(root, draw::rect("sheen", 0.0, 0.0, w, h, draw::linear(165.0, &[(0.0, "#ffffff10"), (0.4, "#ffffff00"), (1.0, "#00000030")])));
    root
}

/// A piping line (the welt) inset along a rounded rectangle.
pub fn piping(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), radius: f64) {
    let n = draw::rounded(draw::frame("piping", x, y, w, h), radius);
    let n = draw::stroked(n, "#0a0a0a", 2.0);
    b.add(parent, draw::with_effects(n, vec![draw::drop_shadow("#ffffff14", 0.0, 1.0, 0.0)]));
}

/// Left-aligned silkscreen helper re-exported for the units.
pub fn silk_left(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, ink: Ink<'_>) -> NodeId {
    parts::silk_left(b, parent, s, x, cy, ink)
}

