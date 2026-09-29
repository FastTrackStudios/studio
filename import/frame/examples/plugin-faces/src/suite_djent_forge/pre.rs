//! Djent Forge pre-FX: five pedals in one row, all one height. Four are the
//! Standard 125B class (156 × 286), the last a Micro (92 × 286).
//!
//! What they share, as the plugin draws them: a powder-coated lid, glossy
//! black lobed knobs with a white line, small caps legends, the model name
//! in widely tracked light capitals across the middle, a chrome-rimmed blue
//! LED over a knurled stomp switch in the bottom band, and the maker's
//! mark (invented) or a thin printed rule with a "designed by" line under
//! the switch.

use surface_kit::Place;

use super::{BLUE, LED_BLUE, PLUGIN};
use crate::delay::parts::{self, Ink};
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, Enclosure, KnobStyle, Letter};

pub fn faces() -> Vec<Face> {
    vec![compressor(), tape_drift(), twin_octave(), tight_drive(), havoc()]
}

const KNOB: KnobStyle<'static> = KnobStyle::new("knob.pedal_lobed", "black", 46.0, "#ffffff");

/// The maker's mark, as the silver and navy boxes print it: a heavy
/// geometric word in blue over a widely spaced second line.
fn maker_mark(b: &mut surface_widgets::Builder<'_>, parent: document_proto::NodeId, cx: f64, cy: f64, w: f64, color: &str) {
    let (_, size) = rk::letter_fit(b, parent, "AZIMUTH", (cx, cy), w, Letter::new(20.0, 780.0, color).stretch(0.9).track(0.02));
    rk::letter_fit(b, parent, "CIRCUITS", (cx, cy + size * 0.66), w, Letter::new(6.0, 500.0, color).track(1.55));
}

/// The bottom band every pre-FX box shares: the chrome-rimmed blue LED
/// over the knurled stomp switch.
fn stomp_band(b: &mut surface_widgets::Builder<'_>, parent: document_proto::NodeId, w: f64, addr: &str) {
    rk::stomp(b, parent, "Bypass", addr, (w / 2.0, 229.0), 42.0, LED_BLUE, (0.0, -29.0), 12.0, "bezel", false);
}

/// The printed rule with a small notch over the "designed by" line (the
/// green and champagne boxes).
fn rule_and_line(b: &mut surface_widgets::Builder<'_>, parent: document_proto::NodeId, w: f64, y: f64, color: &str) {
    let (x0, x1, cx) = (w * 0.2, w * 0.8, w / 2.0);
    rk::line(b, parent, &[(x0, y), (cx - 6.0, y), (cx - 2.0, y + 3.5), (cx + 2.0, y - 3.5), (cx + 6.0, y), (x1, y)], 0.8, color);
    rk::print(b, parent, "DESIGNED IN THE FORGE", (cx, y + 13.0), Ink::new(5.2, color).weight(500.0).track(0.28));
}

// ── 01 · Skyline Compressor ──────────────────────────────────────────────

/// Skyline Compressor — Pedal, Standard (156 × 286). Silver powder coat,
/// blue lettering; DETAIL, MIX, VOLUME, COMP round an INTENSITY dome
/// button; a row of ten blue LEDs reads gain reduction (−1 … −20 dB, lit
/// from the right).
fn compressor() -> Face {
    let ns = "dfcomp";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Skyline Compressor", |b, root| {
        let e = Enclosure::new("standard", "#b8babd", "powder").base("#d5d6d8");
        let p = rk::pedal_body(b, root, &e);
        let (w, _) = e.size();
        let legend = Ink::new(6.4, BLUE).weight(600.0).track(0.02);
        let knob_at = [("detail", "Detail", (31.0, 38.0), 64.0), ("mix", "Mix", (w - 31.0, 38.0), 64.0), ("volume", "Volume", (31.0, 92.0), 118.0), ("comp", "Comp", (w - 31.0, 92.0), 118.0)];
        for (addr, name, at, ly) in knob_at {
            rk::knob_lt(b, p, &KNOB, &a(addr), name, at, ly, legend);
        }
        // The GR meter: a black slot with ten lamps, printed thresholds over it.
        let (mx0, mx1, my) = (47.0, 111.0, 16.5);
        let slot = draw_slot(b, p, (mx0 - 2.0, my - 4.2, mx1 - mx0 + 4.0, 8.4));
        let _ = slot;
        rk::gr_dots(b, p, "Compression", &a("gr"), (mx0, mx1, my), 4.6, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 10.0, 15.0, 20.0], LED_BLUE, true);
        for (i, l) in ["-20", "-10", "-7", "-5", "-3", "-1"].iter().enumerate() {
            rk::print(b, p, l, (mx0 + 3.0 + (mx1 - mx0 - 6.0) * i as f64 / 5.0, 8.8), Ink::new(4.6, BLUE).weight(600.0).track(0.0));
        }
        rk::print(b, p, "Compression", (w / 2.0, 27.5), Ink::new(6.4, BLUE).weight(600.0).track(0.02));
        let dome = Place::new("Intensity").size(21.0).variant("chrome").bind(rk::bind(&a("intensity"))).touch(surface_kit::TOUCH_MIN);
        let dome = parts::put(b, p, "switch.push_dome", &dome, w / 2.0, 62.0);
        rk::label(b, dome, "Intensity", (w / 2.0, 83.0), legend);

        rk::letter(b, p, "COMPRESSOR", (w / 2.0, 137.0), Letter::new(13.5, 420.0, BLUE).track(0.16));
        stomp_band(b, p, w, &a("on"));
        maker_mark(b, p, w / 2.0, 254.0, w * 0.74, BLUE);
    });
    let params = vec![
        rk::percent(&a("detail"), "Detail", 50.0),
        rk::percent(&a("mix"), "Mix", 100.0).featured(),
        rk::db(&a("volume"), "Volume", -12.0, 12.0, 0.0).featured(),
        rk::percent(&a("comp"), "Comp", 50.0).featured(),
        rk::toggle(&a("intensity"), "Intensity", false),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face { file: "01-pre-standard-compressor", plugin: PLUGIN, scene, params, streams: vec![rk::gr_stream(&a("gr"), "Compression")] }
}

/// A recessed black slot (the GR meter's window).
fn draw_slot(b: &mut surface_widgets::Builder<'_>, parent: document_proto::NodeId, r: (f64, f64, f64, f64)) -> document_proto::NodeId {
    let fill = surface_kit::draw::linear(180.0, &[(0.0, "#050506"), (1.0, "#1c1d20")]);
    parts::well(b, parent, "meter slot", r, fill, r.3 / 2.0)
}

/// The legend ink every box but the silver one prints in: white.
const WHITE: &str = "#f2f2ef";

/// The five-knob layout the green box uses and the others vary: two
/// knobs up top at the edges, a row of three below.
const TOP_L: (f64, f64) = (29.0, 37.0);
const TOP_R: (f64, f64) = (127.0, 37.0);
const LOW_L: (f64, f64) = (29.0, 92.0);
const LOW_M: (f64, f64) = (78.0, 92.0);
const LOW_R: (f64, f64) = (127.0, 92.0);

// ── 02 · Tape Drift ──────────────────────────────────────────────────────

/// Tape Drift — Pedal, Standard (156 × 286). A tape-echo box in sage green
/// powder coat: MIX and FEED up top either side of a small dark LCD that
/// reads the time ("2000 MS"), a MS / NOTE mini toggle under it, TIME,
/// MOD and TONE below; a printed rule and "designed in" line under the
/// switch.
fn tape_drift() -> Face {
    let ns = "dftape";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Tape Drift", |b, root| {
        let e = Enclosure::new("standard", "#7b8d6d", "powder").base("#5d6b52");
        let p = rk::pedal_body(b, root, &e);
        let (w, _) = e.size();
        let legend = Ink::new(6.6, WHITE).weight(600.0).track(0.02);
        rk::knob_lt(b, p, &KNOB, &a("mix"), "Mix", TOP_L, 64.0, legend);
        rk::knob_lt(b, p, &KNOB, &a("feedback"), "Feed", TOP_R, 64.0, legend);
        rk::knob_lt(b, p, &KNOB, &a("time"), "Time", LOW_L, 119.0, legend);
        rk::knob_lt(b, p, &KNOB, &a("mod"), "Mod", LOW_M, 119.0, legend);
        rk::knob_lt(b, p, &KNOB, &a("tone"), "Tone", LOW_R, 119.0, legend);
        // The time readout: a small dark LCD in a bezel.
        let win = parts::screen(b, p, "Time display", (56.0, 10.0, 46.0, 18.0), parts::Screen { glass: ("#0d0e0f", "#1a1b1d"), ink: "#9a9c9e", mesh: false });
        let r = parts::readout(b, win, "Time", rk::bind(&a("time")), (1.0, 9.0, 44.0), 8.5, "#a7a9ab", document_proto::TextAlign::Center);
        parts::format(b, r, surface_proto::ValueFormat::decimals(0).unit(" MS"));
        // MS / NOTE: the time runs free or locks to a note.
        let tm = Place::new("Time mode").size(9.0).variant("black").bind(rk::bind(&a("sync"))).legend("").touch(22.0);
        let tm = parts::put(b, p, "switch.toggle_nut", &tm, 78.0, 48.0);
        parts::strip_legend(b, tm);
        let small = Ink::new(4.8, WHITE).weight(600.0).track(0.04);
        rk::print(b, p, "• MS", (95.0, 42.0), small);
        rk::print(b, p, "• NOTE", (97.0, 56.0), small);
        rk::letter(b, p, "TAPE DRIFT", (w / 2.0, 138.0), Letter::new(13.5, 420.0, WHITE).track(0.17));
        stomp_band(b, p, w, &a("on"));
        rule_and_line(b, p, w, 257.0, "#e9ece4");
    });
    let params = vec![
        rk::percent(&a("mix"), "Mix", 35.0).featured(),
        rk::percent(&a("feedback"), "Feed", 40.0).featured(),
        rk::ms(&a("time"), "Time", 40.0, 2000.0, 2000.0).featured(),
        rk::percent(&a("mod"), "Mod", 25.0),
        rk::percent(&a("tone"), "Tone", 50.0),
        rk::toggle(&a("sync"), "Note Sync", false),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face { file: "02-pre-standard-tape-drift", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 03 · Twin Octave ─────────────────────────────────────────────────────

/// Twin Octave — Pedal, Standard (156 × 286). An octave box in champagne
/// powder coat: OCT−1 and OCT+1 up top at the edges, DIRECT alone below
/// between them; the same rule and line under the switch.
fn twin_octave() -> Face {
    let ns = "dfoct";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Twin Octave", |b, root| {
        let e = Enclosure::new("standard", "#c3b393", "powder").base("#9f9278");
        let p = rk::pedal_body(b, root, &e);
        let (w, _) = e.size();
        let legend = Ink::new(6.6, WHITE).weight(600.0).track(0.02);
        rk::knob_lt(b, p, &KNOB, &a("sub"), "Oct-1", TOP_L, 64.0, legend);
        rk::knob_lt(b, p, &KNOB, &a("up"), "Oct+1", TOP_R, 64.0, legend);
        rk::knob_lt(b, p, &KNOB, &a("direct"), "Direct", LOW_M, 119.0, legend);
        rk::letter(b, p, "TWIN OCTAVE", (w / 2.0, 138.0), Letter::new(12.8, 420.0, WHITE).track(0.15));
        stomp_band(b, p, w, &a("on"));
        rule_and_line(b, p, w, 257.0, "#f4efe4");
    });
    let params = vec![
        rk::percent(&a("sub"), "Octave Down", 50.0).featured(),
        rk::percent(&a("up"), "Octave Up", 50.0).featured(),
        rk::percent(&a("direct"), "Direct", 100.0).featured(),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face { file: "03-pre-standard-twin-octave", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 04 · Tight Drive ─────────────────────────────────────────────────────

/// Tight Drive — Pedal, Standard (156 × 286). A tight-bass overdrive in
/// navy sparkle: VOL and BRIGHT up top pulled in from the edges, ATTACK and
/// DRIVE lower at the edges, a small white GATE knob lit from inside
/// between them; the model name in two lines of wide blue capitals and the
/// maker's mark in blue.
fn tight_drive() -> Face {
    let ns = "dfdrive";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Tight Drive", |b, root| {
        let e = Enclosure::new("standard", "#20232b", "powder").base("#15171c");
        let p = rk::pedal_body(b, root, &e);
        let (w, _) = e.size();
        let legend = Ink::new(6.6, "#2f7fd0").weight(600.0).track(0.02);
        rk::knob_lt(b, p, &KNOB, &a("volume"), "Vol", (52.0, 27.0), 54.0, legend);
        rk::knob_lt(b, p, &KNOB, &a("bright"), "Bright", (104.0, 27.0), 54.0, legend);
        rk::knob_lt(b, p, &KNOB, &a("attack"), "Attack", (29.0, 77.0), 107.0, legend);
        rk::knob_lt(b, p, &KNOB, &a("drive"), "Drive", (127.0, 77.0), 107.0, legend);
        // The gate: a small translucent knob with a blue lamp under it.
        let glow = surface_kit::draw::circle("gate glow", 78.0, 91.0, 15.0, surface_kit::draw::radial(&[(0.0, "#8fd0ffcc"), (0.55, "#3f8fe066"), (1.0, "#3f8fe000")]));
        b.add(p, parts::emit(glow, "#6fb8ff", 0.8, 14.0));
        let gate = KnobStyle::new("knob.pedal_lobed", "white", 22.0, "#ffffff");
        rk::knob_lt(b, p, &gate, &a("gate"), "Gate", (78.0, 91.0), 73.0, legend);
        let title = Letter::new(12.5, 430.0, "#2f7fd0").track(0.2);
        rk::letter(b, p, "TIGHT", (w / 2.0, 130.0), title);
        rk::letter(b, p, "DRIVE", (w / 2.0, 145.0), title);
        stomp_band(b, p, w, &a("on"));
        maker_mark(b, p, w / 2.0, 254.0, w * 0.74, "#1f6ab8");
    });
    let params = vec![
        rk::db(&a("volume"), "Volume", -24.0, 12.0, 0.0).featured(),
        rk::percent(&a("bright"), "Bright", 50.0),
        rk::percent(&a("attack"), "Attack", 50.0),
        rk::percent(&a("drive"), "Drive", 40.0).featured(),
        rk::percent(&a("gate"), "Gate", 30.0),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face { file: "04-pre-standard-tight-drive", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

// ── 05 · Havoc ───────────────────────────────────────────────────────────

/// Havoc — Pedal, Micro (92 × 286). A narrow oxblood box with one big MIX
/// knob, the name in wide capitals, and a printed square-wave squiggle
/// over a two-line "designed in" credit under the switch.
fn havoc() -> Face {
    let ns = "dfhavoc";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Havoc", |b, root| {
        let e = Enclosure::new("micro", "#7d3530", "powder").base("#5a2522");
        let p = rk::pedal_body(b, root, &e);
        let (w, _) = e.size();
        let legend = Ink::new(6.8, WHITE).weight(600.0).track(0.02);
        rk::knob_lt(b, p, &KNOB.size(62.0), &a("mix"), "Mix", (w / 2.0, 46.0), 97.0, legend.size(8.0));
        rk::letter(b, p, "HAVOC", (w / 2.0, 138.0), Letter::new(12.5, 420.0, WHITE).track(0.2));
        rk::stomp(b, p, "Bypass", &a("on"), (w / 2.0, 229.0), 38.0, LED_BLUE, (0.0, -29.0), 11.0, "bezel", false);
        // The squiggle: a flat line that breaks into a few jagged teeth.
        let y = 257.0;
        let mut pts = vec![(12.0, y), (36.0, y)];
        for (i, dx) in [0.0, 3.0, 6.0, 9.0, 12.0, 15.0].iter().enumerate() {
            pts.push((38.0 + dx, if i % 2 == 0 { y - 4.0 } else { y + 1.0 }));
        }
        pts.extend([(56.0, y), (80.0, y)]);
        rk::line(b, p, &pts, 0.8, "#efe6e2");
        let small = Letter::new(4.6, 500.0, "#efe6e2").track(0.3);
        rk::letter(b, p, "DESIGNED", (w / 2.0, 266.0), small);
        rk::letter(b, p, "IN THE FORGE", (w / 2.0, 273.0), small);
    });
    let params = vec![rk::percent(&a("mix"), "Mix", 50.0).featured(), rk::toggle(&a("on"), "Bypass", true).featured()];
    Face { file: "05-pre-micro-havoc", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
