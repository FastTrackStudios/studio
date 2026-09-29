//! The three amp heads (Head form, 1200 wide; the height is the head's
//! own aspect with its handle and feet). Each faceplate strip is the
//! control surface, legends lowercase over the knobs, and a cartoon
//! maker's mark — heavy stacked capitals with an outline — at the right.
//!
//! - `06-critic-head-1200x580`: *The Tone Critic* — blue tolex with black
//!   corners, a grille of blue chevron bars over glowing power valves, a
//!   cream strip: volume, bright, bass, middle, treble, master, presence,
//!   output, drive, power.
//! - `07-engine-head-1200x600`: *The Clean Engine* — a walnut box, a cream
//!   perforated grille, a green strip with white knurled knobs: volume,
//!   bright, bass, middle, treble, presence, output, power.
//! - `08-desk-head-1200x596`: *Studio Funk Desk* — a white vented chassis
//!   on wooden legs with the valves glowing through the slots, a cream
//!   strip with slate teardrop knobs: comp (slow/fast), tube sat, high
//!   pass, low pass, lows, mids, highs (each with a frequency toggle),
//!   output, power.
//!
//! The valves' glow flickers (a motion on their light), the pilot LED
//! follows the power switch.

use document_proto::NodeId;
use surface_kit::{Place, TOUCH_MIN, draw, motion};
use surface_proto::ParamInfo;
use surface_widgets::Builder;

use super::parts::{self, HEAD_W, Type};
use super::pedals::PLUGIN;
use crate::face::{Face, canvas};
use crate::reverb::ui;

/// A knob on a strip: address, legend, x.
struct Knob<'a>(&'a str, &'a str, f64);

/// How a strip's knobs look.
struct Strip<'a> {
    id: &'a str,
    variant: &'a str,
    d: f64,
    ink: &'a str,
    knob_y: f64,
    legend_y: f64,
    opts: &'a [&'a str],
}

fn strip_knobs(b: &mut Builder<'_>, parent: NodeId, s: &Strip<'_>, ks: &[Knob<'_>]) {
    for Knob(addr, legend, x) in ks {
        parts::knob(b, parent, s.id, s.variant, addr, legend, s.d, (*x, s.knob_y), s.opts, s.ink);
        parts::centred(b, parent, legend, (*x, s.legend_y), Type::new(12.0, s.ink, 600.0));
    }
}

/// A cream push power button with the pilot LED above it.
fn power(b: &mut Builder<'_>, parent: NodeId, addr: &str, (x, y): (f64, f64), led_y: f64, d: f64, led: &str) {
    let p = Place::new("Power").size(d).variant("cream").bind(ui::bind(addr)).options(&["nolamp"]).legend("").touch(TOUCH_MIN);
    let k = surface_kit::place(b, parent, "button.round_cap", &p).unwrap_or_else(|e| panic!("{e}"));
    ui::strip_legend(b, k);
    parts::centre_on(b, k, (x, y));
    parts::status_led(b, parent, addr, 5.0, (x, led_y), led, "dark", None);
}

/// The maker's mark: two stacked heavy words, outlined, and the model
/// under them.
#[allow(clippy::too_many_arguments)]
fn mark(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), fill: &str, outline: &str, model: &[&str], model_ink: &str) {
    let t = Type::new(27.0, fill, 900.0).upper().track(-0.04);
    parts::outlined(b, parent, "Funk", (cx, cy - 14.0), t, outline, 1.6);
    parts::outlined(b, parent, "Works", (cx, cy + 12.0), t, outline, 1.6);
    for (i, l) in model.iter().enumerate() {
        parts::centred(b, parent, l, (cx, cy + 36.0 + 11.0 * i as f64), Type::new(10.5, model_ink, 700.0));
    }
}

/// Power valves behind a grille: glass envelopes with glowing plates and
/// filaments whose light flickers.
fn valves(b: &mut Builder<'_>, parent: NodeId, xs: &[f64], y: f64, w: f64, h: f64) {
    for (i, x) in xs.iter().enumerate() {
        let glass = draw::rounded(draw::rect("valve", x - w / 2.0, y, w, h, draw::linear(90.0, &[(0.0, "#2a2320"), (0.3, "#4a403a"), (0.5, "#221c19"), (1.0, "#15110f")])), w / 2.0);
        b.add(parent, draw::with_effects(glass, vec![draw::drop_shadow("#00000099", 0.0, 2.0, 6.0)]));
        b.add(parent, draw::rect("plate", x - w * 0.32, y + h * 0.25, w * 0.64, h * 0.5, draw::linear(90.0, &[(0.0, "#3b3a38"), (0.5, "#8a8986"), (1.0, "#3b3a38")])));
        let glow = draw::rect("filament", x - w * 0.18, y + h * 0.3, w * 0.36, h * 0.4, draw::radial(&[(0.0, "#ffd08a"), (0.5, "#ff8a2a"), (1.0, "#ff6a1000")]));
        let glow = b.add(parent, draw::emissive(glow, draw::hex("#ff8a2a"), 0.8, w * 0.9));
        motion::add(b, glow, motion::flicker(0.12 + 0.03 * i as f64));
        b.add(parent, draw::rect("glint", x - w * 0.36, y + h * 0.08, w * 0.08, h * 0.8, draw::solid("#ffffff24")));
    }
}

fn face(file: &'static str, scene: document_proto::Scene, params: Vec<ParamInfo>) -> Face {
    ui::formatted(Face { file, plugin: PLUGIN, scene, params, streams: Vec::new() })
}

// ── The Tone Critic ──────────────────────────────────────────────────────

pub(super) fn critic() -> Face {
    let (w, h) = (HEAD_W, 580.0);
    let scene = canvas("The Tone Critic", |b, root| {
        parts::page(b, root, w, h);
        let (top, bottom) = (60.0, 545.0);
        // Feet and handle.
        for x in [80.0, w - 150.0] {
            b.add(root, draw::rounded(draw::rect("foot", x, bottom - 10.0, 70.0, h - bottom + 10.0, draw::linear(180.0, &[(0.0, "#2a2a2c"), (1.0, "#0d0d0e")])), 6.0));
        }
        let handle = ui::strokes("handle", &[(0..=40).map(|i| {
            let u = f64::from(i) / 40.0;
            (w / 2.0 - 205.0 + 410.0 * u, top - 8.0 - 44.0 * (u * std::f64::consts::PI).sin().powf(0.35))
        }).collect()], 24.0, draw::solid("#2e2e31"));
        b.add(root, draw::with_effects(handle, vec![draw::drop_shadow("#00000080", 0.0, 3.0, 5.0)]));
        for x in [w / 2.0 - 205.0, w / 2.0 + 205.0] {
            b.add(root, draw::rounded(draw::rect("strap end", x - 10.0, top - 22.0, 20.0, 26.0, draw::linear(90.0, &[(0.0, "#7c7d7b"), (0.5, "#f0f0ec"), (1.0, "#6b6c6a")])), 4.0));
        }
        // The tolex box and its corners.
        let tolex = draw::rect("tolex", 0.0, top, w, bottom - top, draw::linear(175.0, &[(0.0, "#7b9bd6"), (0.5, "#6689c8"), (1.0, "#4f71b3")]));
        b.add(root, draw::with_effects(draw::rounded(tolex, 10.0), vec![draw::drop_shadow("#00000080", 0.0, 6.0, 14.0), draw::inner_shadow("#ffffff40", 0.0, 2.0, 2.0)]));
        for (x, y) in [(0.0, top), (w - 64.0, top), (0.0, bottom - 64.0), (w - 64.0, bottom - 64.0)] {
            let c = draw::rounded(draw::rect("corner", x, y, 64.0, 64.0, draw::linear(160.0, &[(0.0, "#3a3a3d"), (1.0, "#0c0c0d")])), 10.0);
            b.add(root, draw::with_effects(c, vec![draw::inner_shadow("#ffffff30", 1.0, 1.0, 1.0)]));
        }
        // The grille: glowing valves under blue chevron bars.
        let (gx, gy, gw, gh) = (28.0, top + 28.0, w - 56.0, 275.0);
        let mut grille = draw::frame("grille", gx, gy, gw, gh);
        grille.fills = vec![draw::linear(180.0, &[(0.0, "#0c0c0e"), (1.0, "#141418")])];
        grille.clips_content = true;
        grille.effects = vec![draw::inner_shadow("#000000", 0.0, 3.0, 8.0)];
        let grille = b.add(root, grille);
        valves(b, grille, &[300.0, 420.0, 560.0, 700.0, 840.0], 70.0, 70.0, 150.0);
        let period = 322.0;
        let mut rows = Vec::new();
        for r in -7..8 {
            let y0 = f64::from(r) * 42.0;
            rows.push((0..=9).map(|i| (-period / 4.0 + period / 2.0 * f64::from(i), y0 + if i % 2 == 0 { 0.0 } else { 190.0 })).collect::<Vec<_>>());
        }

        b.add(grille, ui::strokes("chevron shade", &rows.iter().map(|l| l.iter().map(|(x, y)| (x + 2.0, y + 4.0)).collect()).collect::<Vec<_>>(), 15.0, draw::solid("#00000099")));
        b.add(grille, ui::strokes("chevrons", &rows, 16.0, draw::solid("#34599d")));
        b.add(grille, ui::strokes("chevron light", &rows.iter().map(|l| l.iter().map(|(x, y)| (*x, y - 5.0)).collect()).collect::<Vec<_>>(), 2.0, draw::solid("#8fb0e855")));
        for (sx, sy) in [(8.0, 8.0), (gw - 8.0, 8.0), (8.0, gh - 8.0), (gw - 8.0, gh - 8.0)] {
            parts::screw(b, grille, sx, sy, 3.5, "#6d6e6c");
        }
        // The cream strip.
        let (sx, sy, sw, sh) = (28.0, top + 309.0, w - 56.0, 142.0);
        let strip = draw::rect("faceplate", sx, sy, sw, sh, draw::linear(180.0, &[(0.0, "#efe6d2"), (1.0, "#dccfb4")]));
        let strip = b.add(root, draw::with_effects(strip, vec![draw::inner_shadow("#00000040", 0.0, 2.0, 4.0), draw::drop_shadow("#00000066", 0.0, 1.0, 2.0)]));
        let ink = "#232120";
        let knob_y = 66.0;
        let s = Strip { id: "knob.dot_knurl", variant: "black", d: 46.0, ink, knob_y, legend_y: 22.0, opts: &["dots=11"] };
        strip_knobs(b, strip, &s, &[
            Knob("critic/volume", "volume", 199.0),
            Knob("critic/bass", "bass", 338.0),
            Knob("critic/middle", "middle", 440.0),
            Knob("critic/treble", "treble", 540.0),
            Knob("critic/master", "master", 642.0),
            Knob("critic/presence", "presence", 742.0),
            Knob("critic/output", "output", 842.0),
        ]);
        for (addr, legend, x) in [("critic/bright", "bright", 267.0), ("critic/drive", "drive", 913.0)] {
            parts::toggle(b, strip, addr, legend, 17.0, "black", (x, knob_y));
            parts::centred(b, strip, legend, (x, 30.0), Type::new(12.0, ink, 600.0));
        }
        power(b, strip, "critic/power", (968.0, knob_y), 36.0, 24.0, "#8ef0ff");
        mark(b, strip, (1052.0, 56.0), "#1c1b1b", "#f6efe0", &["The Tone", "Critic"], ink);
        parts::cable(b, strip, (89.0, knob_y), 12.0, &[(80.0, 110.0), (40.0, 170.0), (-28.0, 190.0)]);
    });
    let params = vec![
        parts::dial10("critic/volume", "Volume", 5.0),
        parts::dial10("critic/bass", "Bass", 5.0),
        parts::dial10("critic/middle", "Middle", 5.0),
        parts::dial10("critic/treble", "Treble", 6.0),
        parts::dial10("critic/master", "Master", 5.0),
        parts::dial10("critic/presence", "Presence", 5.0),
        parts::dial10("critic/output", "Output", 5.0),
        parts::switch("critic/bright", "Bright", false),
        parts::switch("critic/drive", "Drive", false),
        parts::switch("critic/power", "Power", true),
    ];
    face("06-critic-head-1200x580", scene, params)
}

// ── The Clean Engine ─────────────────────────────────────────────────────

pub(super) fn engine() -> Face {
    let (w, h) = (HEAD_W, 600.0);
    let scene = canvas("The Clean Engine", |b, root| {
        parts::page(b, root, w, h);
        let bottom = 558.0;
        for x in [70.0, w - 150.0] {
            b.add(root, draw::rounded(draw::rect("foot", x, bottom - 10.0, 80.0, h - bottom + 10.0, draw::linear(180.0, &[(0.0, "#303032"), (1.0, "#0e0e0f")])), 8.0));
        }
        // The walnut box.
        let wood = draw::rect("walnut", 0.0, 0.0, w, bottom, draw::linear(90.0, &[(0.0, "#8a4b26"), (0.03, "#b2693a"), (0.5, "#a45f33"), (0.97, "#b2693a"), (1.0, "#8a4b26")]));
        let wood = b.add(root, draw::with_effects(draw::rounded(wood, 26.0), vec![draw::drop_shadow("#00000080", 0.0, 6.0, 14.0), draw::inner_shadow("#ffffff30", 0.0, 2.0, 2.0)]));
        let mut grain = Vec::new();
        for i in 0..28 {
            let y = 6.0 + f64::from(i) * 19.7;
            grain.push((0..=60).map(|j| {
                let x = f64::from(j) * 20.0;
                (x, y + 2.2 * ((x * 0.013 + f64::from(i)).sin() + 0.6 * (x * 0.041 + f64::from(i) * 1.7).sin()))
            }).collect::<Vec<_>>());
        }
        b.add(wood, ui::strokes("grain", &grain, 1.2, draw::solid("#5a2c1433")));
        // The cream perforated grille.
        let (gx, gy, gw, gh) = (35.0, 36.0, w - 70.0, 317.0);
        let mut grille = draw::frame("grille", gx, gy, gw, gh);
        grille.fills = vec![draw::linear(180.0, &[(0.0, "#eadcbe"), (1.0, "#dccba6")])];
        grille.effects = vec![draw::inner_shadow("#00000066", 0.0, 3.0, 6.0)];
        let grille = b.add(root, grille);
        let mut holes = String::new();
        let (cols, rows) = (40, 10);
        for r in 0..rows {
            for c in 0..cols {
                let (x, y) = (60.0 + f64::from(c) * 25.75, 50.0 + f64::from(r) * 25.0);
                holes.push_str(&format!("M {:.2} {y:.2} a 4 4 0 1 0 8 0 a 4 4 0 1 0 -8 0 Z ", x - 4.0));
            }
        }
        b.add(grille, draw::path("holes", 0.0, 0.0, gw, gh, holes, draw::solid("#1a1410")));
        for (sx, sy) in [(12.0, 14.0), (gw - 12.0, 14.0), (12.0, gh - 14.0), (gw - 12.0, gh - 14.0)] {
            parts::screw(b, grille, sx, sy, 7.0, "#2a2a2a");
        }
        // The green strip.
        let (sx, sy, sw, sh) = (35.0, 357.0, w - 70.0, 165.0);
        let strip = draw::rect("faceplate", sx, sy, sw, sh, draw::linear(180.0, &[(0.0, "#27634a"), (1.0, "#1d4f3b")]));
        let strip = b.add(root, draw::with_effects(strip, vec![draw::inner_shadow("#00000059", 0.0, 2.0, 5.0)]));
        let ink = "#f4f1e8";
        let knob_y = 80.0;
        let s = Strip { id: "knob.dot_knurl", variant: "white", d: 56.0, ink, knob_y, legend_y: 30.0, opts: &["dots=11"] };
        strip_knobs(b, strip, &s, &[
            Knob("engine/volume", "volume", 205.0),
            Knob("engine/bass", "bass", 357.0),
            Knob("engine/middle", "middle", 477.0),
            Knob("engine/treble", "treble", 597.0),
            Knob("engine/presence", "presence", 717.0),
            Knob("engine/output", "output", 838.0),
        ]);
        parts::toggle(b, strip, "engine/bright", "bright", 18.0, "chrome", (281.0, knob_y));
        parts::centred(b, strip, "bright", (281.0, 42.0), Type::new(12.0, ink, 600.0));
        power(b, strip, "engine/power", (955.0, knob_y), 42.0, 26.0, "#ff5a3c");
        mark(b, strip, (1047.0, 62.0), "#f7f3ea", "#173b2c", &["The Clean", "Engine"], ink);
        parts::cable(b, strip, (93.0, knob_y), 13.0, &[(88.0, 125.0), (60.0, 190.0), (-30.0, 250.0)]);
    });
    let params = vec![
        parts::dial10("engine/volume", "Volume", 5.0),
        parts::dial10("engine/bass", "Bass", 5.0),
        parts::dial10("engine/middle", "Middle", 5.0),
        parts::dial10("engine/treble", "Treble", 6.0),
        parts::dial10("engine/presence", "Presence", 5.0),
        parts::dial10("engine/output", "Output", 5.0),
        parts::switch("engine/bright", "Bright", false),
        parts::switch("engine/power", "Power", true),
    ];
    face("07-engine-head-1200x600", scene, params)
}

// ── Studio Funk Desk ─────────────────────────────────────────────────────

pub(super) fn desk() -> Face {
    let (w, h) = (HEAD_W, 596.0);
    let scene = canvas("Studio Funk Desk", |b, root| {
        parts::page(b, root, w, h);
        let (top, bottom) = (65.0, 560.0);
        // Wooden legs, the chrome handle.
        for (x, lean) in [(62.0, -1.0), (w - 118.0, 1.0)] {
            let leg = draw::polygon(&[(x, bottom - 6.0), (x + 56.0, bottom - 6.0), (x + 56.0 + 14.0 * lean, h), (x + 14.0 * lean, h)]);
            b.add(root, draw::path("leg", 0.0, 0.0, w, h, leg, draw::linear(90.0, &[(0.0, "#a86e3c"), (0.5, "#d49a5e"), (1.0, "#9c6534")])));
        }
        let bar = ui::strokes("handle", &[vec![(443.0, top + 4.0), (443.0, 8.0), (750.0, 8.0), (750.0, top + 4.0)]], 10.0, draw::solid("#c9cac6"));
        b.add(root, draw::with_effects(bar, vec![draw::drop_shadow("#00000066", 0.0, 2.0, 3.0)]));
        // The white chassis.
        let body = draw::rect("chassis", 0.0, top, w, bottom - top, draw::linear(180.0, &[(0.0, "#fbfbf9"), (0.7, "#efefec"), (1.0, "#dcdcd8")]));
        let body = b.add(root, draw::with_effects(draw::rounded(body, 30.0), vec![draw::drop_shadow("#0000008c", 0.0, 6.0, 14.0), draw::inner_shadow("#00000026", 0.0, -3.0, 6.0)]));
        // Valves behind the middle vent, then the three vents' slots.
        let mut well = draw::frame("valve well", 440.0, 38.0, 322.0, 262.0);
        well.fills = vec![draw::solid("#0e0d0c")];
        let well = b.add(body, well);
        valves(b, well, &[95.0, 228.0], 40.0, 64.0, 200.0);
        for (x0, x1) in [(28.0, 404.0), (440.0, 762.0), (853.0, 1173.0)] {
            for r in 0..10 {
                let y = 42.0 + f64::from(r) * 25.7;
                // Wall between slots over the well, the slot's dark mouth elsewhere.
                let slot = draw::rounded(draw::rect("slot", x0, y, x1 - x0, 10.0, draw::solid(if x0 > 430.0 && x0 < 450.0 { "#00000000" } else { "#0f0f10" })), 5.0);
                b.add(body, draw::with_effects(slot, vec![draw::inner_shadow("#000000", 0.0, 2.0, 3.0)]));
            }
        }
        for r in 0..11 {
            let y = 32.0 + f64::from(r) * 25.7;
            b.add(body, draw::rect("vent bar", 440.0, y, 322.0, 10.0, draw::linear(180.0, &[(0.0, "#ffffff"), (1.0, "#e2e2de")])));
        }
        // The cream strip.
        let (sx, sy, sw, sh) = (28.0, 315.0, w - 56.0, 148.0);
        let strip = draw::rounded(draw::rect("faceplate", sx, sy, sw, sh, draw::linear(180.0, &[(0.0, "#f2ecdd"), (1.0, "#e6dcc6")])), 12.0);
        let strip = b.add(body, draw::with_effects(strip, vec![draw::inner_shadow("#00000059", 0.0, 3.0, 5.0)]));
        let ink = "#262422";
        let knob_y = 78.0;
        let s = Strip { id: "knob.teardrop", variant: "slate", d: 44.0, ink, knob_y, legend_y: 35.0, opts: &["dots=11"] };
        strip_knobs(b, strip, &s, &[
            Knob("desk/comp", "comp", 132.0),
            Knob("desk/sat", "tube sat", 262.0),
            Knob("desk/hpf", "high pass", 357.0),
            Knob("desk/lpf", "low pass", 453.0),
            Knob("desk/lows", "lows", 551.0),
            Knob("desk/mids", "mids", 681.0),
            Knob("desk/highs", "highs", 809.0),
            Knob("desk/output", "output", 938.0),
        ]);
        for (addr, up, down, x) in [
            ("desk/fast", "slow", "fast", 197.0),
            ("desk/low_f", "150 Hz", "225 Hz", 618.0),
            ("desk/mid_f", "600 Hz", "900 Hz", 746.0),
            ("desk/high_f", "2.5 kHz", "4 kHz", 872.0),
        ] {
            parts::toggle(b, strip, addr, up, 16.0, "chrome"
, (x, knob_y));
            parts::centred(b, strip, up, (x, 49.0), Type::new(10.5, ink, 600.0));
            parts::centred(b, strip, down, (x, 108.0), Type::new(10.5, ink, 600.0));
        }
        power(b, strip, "desk/power", (1012.0, knob_y), 44.0, 24.0, "#8ef0ff");
        mark(b, strip, (1081.0, 60.0), "#2a2826", "#f3eee2", &["Studio Funk", "Desk"], ink);
        parts::cable(b, strip, (56.0, knob_y), 12.0, &[(50.0, 125.0), (20.0, 200.0), (-40.0, 250.0)]);
    });
    let params = vec![
        parts::pct("desk/comp", "Comp", 40.0),
        parts::pct("desk/sat", "Tube Sat", 30.0),
        parts::hz("desk/hpf", "High Pass", 20.0, 500.0, 60.0),
        parts::hz("desk/lpf", "Low Pass", 2000.0, 20000.0, 12000.0),
        parts::db("desk/lows", "Lows", -12.0, 12.0, 0.0),
        parts::db("desk/mids", "Mids", -12.0, 12.0, 0.0),
        parts::db("desk/highs", "Highs", -12.0, 12.0, 0.0),
        parts::db("desk/output", "Output", -24.0, 12.0, 0.0),
        parts::switch("desk/fast", "Comp Speed", false),
        parts::switch("desk/low_f", "Lows Frequency", false),
        parts::switch("desk/mid_f", "Mids Frequency", false),
        parts::switch("desk/high_f", "Highs Frequency", false),
        parts::switch("desk/power", "Power", true),
    ];
    face("08-desk-head-1200x596", scene, params)
}
