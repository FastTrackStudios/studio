//! The console bus compressor (G-series centre section), as a rack unit.
//!
//! A gain-reduction VU at the left, then the five controls in the order the
//! module has them — THRESHOLD, RATIO, ATTACK, RELEASE, MAKE-UP — and the
//! amber IN switch. RATIO, ATTACK and RELEASE are rotary *switches*: they
//! click through printed detents (2 · 4 · 10, 0.1 → 30 ms, 0.1 s → AUTO)
//! and turn exactly as far as those detents go. The two continuous knobs
//! carry live readouts under their legends. MIX (the plugin's parallel
//! blend) sits with IN at the right, apart from the circuit's own controls.

use surface_kit::{Ink, Place, TOUCH_MIN};

use super::ui::{self, bind};
use crate::face::{Face, canvas};

const W: f64 = 900.0;
const H: f64 = 300.0;
const INK: &str = "#e6e8ea";
const DIM: &str = "#9ca3aa";
const ROW: f64 = 150.0;
const KNOB: f64 = 58.0;

const RATIOS: &[&str] = &["2", "4", "10"];
const ATTACKS: &[&str] = &[".1", ".3", "1", "3", "10", "30"];
const RELEASES: &[&str] = &[".1", ".3", ".6", "1.2", "Auto"];

/// A collet cap turned into a rotary switch: stepped through its printed
/// detents, turning exactly as far as they go.
fn switch_knob(b: &mut surface_widgets::Builder<'_>, panel: document_proto::NodeId, addr: &str, legend: &str, labels: &[&str], cx: f64) {
    let p = Place::new(legend).size(KNOB).variant("black").legend(legend).options(labels).ink(Ink::Color(INK.into())).touch(TOUCH_MIN).bind(bind(addr));
    ui::put(b, panel, "knob.collet", &p, cx, ROW);
}

pub fn face() -> Face {
    let scene = canvas("SSL Bus Compressor", |b, root| {
        ui::fit_root(b, root, W, H);
        let panel = ui::panel(b, root, "panel.painted", "charcoal", W, H, "ears");

        // ── Model line ─────────────────────────────────────────────────
        ui::silk_left(b, panel, "Stereo Bus Compressor", 46.0, 34.0, 12.0, INK, 800.0);
        ui::silk_left(b, panel, "Quad VCA · G Series", 46.0, 52.0, 7.5, DIM, 600.0);

        // ── Gain reduction: a VU card read as compression ─────────────
        let meter = Place::new("Compression").size(214.0).legend("GR").options(&["glass"]).bind(surface_proto::Binding::stream("sslbus/gr"));
        ui::put(b, panel, "meter.vu_ivory", &meter, 160.0, 160.0);

        // ── The five controls ─────────────────────────────────────────
        let thr = Place::new("Threshold").size(KNOB).variant("grey").legend("Threshold").bind(bind("sslbus/threshold")).scale(&["-30", "-20", "-10", "0"]);
        let thr = ui::put(b, panel, "knob.collet", &thr, 340.0, ROW);
        ui::add_value(b, thr, 8.0, DIM);

        switch_knob(b, panel, "sslbus/ratio", "Ratio", RATIOS, 444.0);
        switch_knob(b, panel, "sslbus/attack", "Attack ms", ATTACKS, 548.0);
        switch_knob(b, panel, "sslbus/release", "Release s", RELEASES, 652.0);

        let mk = Place::new("Make-up").size(KNOB).format(surface_proto::ValueFormat::decimals(1)).variant("grey").legend("Make-up").bind(bind("sslbus/makeup")).scale(&["0", "6", "12", "18"]);
        let mk = ui::put(b, panel, "knob.collet", &mk, 756.0, ROW);
        ui::add_value(b, mk, 8.0, DIM);

        // A hairline between the circuit and the plugin's additions.
        ui::rule(b, panel, 813.0, 40.0, 1.0, 220.0, "#00000066");
        ui::rule(b, panel, 814.0, 40.0, 1.0, 220.0, "#ffffff14");

        // ── IN and MIX ─────────────────────────────────────────────────
        let inb = Place::new("In").size(38.0).variant("amber").legend("In").bind(bind("sslbus/in"));
        ui::put(b, panel, "button.illuminated", &inb, 846.0, 86.0);
        let mix = Place::new("Mix").size(34.0).variant("cream").legend("Mix").bind(bind("sslbus/mix")).scale(&["Dry", "", "", "", "Wet"]);
        let mix = ui::put(b, panel, "knob.collet", &mix, 846.0, 186.0);
        ui::add_value(b, mix, 7.0, DIM);
    });

    let params = vec![
        ui::db("sslbus/threshold", "Threshold", -30.0, 0.0, -12.0).featured(),
        ui::choice("sslbus/ratio", "Ratio", &["2:1", "4:1", "10:1"], 1).featured(),
        ui::choice("sslbus/attack", "Attack", &["0.1 ms", "0.3 ms", "1 ms", "3 ms", "10 ms", "30 ms"], 3),
        ui::choice("sslbus/release", "Release", &["0.1 s", "0.3 s", "0.6 s", "1.2 s", "Auto"], 4),
        ui::db("sslbus/makeup", "Make-up", 0.0, 18.0, 4.0).featured(),
        ui::percent("sslbus/mix", "Mix", 100.0),
        ui::toggle("sslbus/in", "In", true).featured(),
    ];
    let streams = vec![ui::gr_stream("sslbus/gr", "Gain Reduction")];
    Face { file: "08-ssl-bus", plugin: "comp", scene, params, streams }
}
