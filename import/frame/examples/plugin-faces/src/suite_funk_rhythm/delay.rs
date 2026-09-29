//! `12-repeat-bigbox-404x286`: *Repeat-t-t*, a stereo delay in a big-box
//! enclosure (404 × 286, the BigBox class) — bright green metallic, two
//! mini toggles (Sync: free or DAW; Single / Dual), a backlit LCD with the
//! sync mode, the left and right times and the tempo, MIX and FEEDBACK up
//! top, the low-cut, TIME L, TIME R and high-cut knobs, an infinity sign,
//! and two stomps: ENGAGE and a momentary TAP TEMPO, each with its LED.

use surface_proto::{Range, Unit};

use super::parts::{self, BIGBOX_W, PEDAL_H, Type};
use super::pedals::{self, K};
use crate::face::{Face, canvas};
use crate::reverb::ui;

const WHITE: &str = "#fbfaf6";
const DIVS: &[&str] = &["1/32", "1/16T", "1/16", "1/8T", "1/8", "1/8D", "1/4T", "1/4", "1/4D", "1/2", "1/1"];

pub(super) fn face() -> Face {
    let scene = canvas("Repeat-t-t Delay", |b, root| {
        let (w, h) = (BIGBOX_W, PEDAL_H);
        let lid = parts::stompbox(b, root, w, h, "#4cbd4f", "metallic", 16.0);
        let t = Type::new(7.2, WHITE, 800.0);
        // The mini toggles and their legends.
        parts::toggle(b, lid, "repeat/sync", "Sync", 9.0, "chrome", (124.0, 30.0));
        parts::toggle(b, lid, "repeat/dual", "Single/Dual", 9.0, "chrome", (282.0, 30.0));
        parts::centred(b, lid, "Sync", (124.0, 9.0), t);
        parts::centred(b, lid, "Single/Dual", (282.0, 9.0), t);
        // Knobs.
        let ks = [
            K("repeat/mix", "Mix", (52.0, 66.0), true),
            K("repeat/feedback", "Feedback", (352.0, 66.0), true),
            K("repeat/low", "", (52.0, 139.0), false),
            K("repeat/time_l", "Time L", (150.0, 139.0), false),
            K("repeat/time_r", "Time R", (254.0, 139.0), false),
            K("repeat/high", "", (352.0, 139.0), false),
        ];
        pedals::knobs(b, lid, &ks, 40.0, "white");
        pedals::filter_marks(b, lid, (52.0, 170.0), (352.0, 170.0));
        // The LCD: sync mode and voices, then the times and the tempo.
        let (x, y, dw, dh) = (112.0, 52.0, 180.0, 44.0);
        let face = ui::display(b, lid, "LCD", x, y, dw, dh, "#b8c8c0", "glass");
        let dim = "#8b9892";
        let lit = "#e6eee9";
        ui::lit_text(b, face, "FREE / DAW / TAP", 8.0, 12.0, 90.0, 6.0, dim, document_proto::TextAlign::Left, 600.0);
        ui::lit_text(b, face, "SINGLE / DUAL", dw - 98.0, 12.0, 90.0, 6.0, dim, document_proto::TextAlign::Right, 600.0);
        ui::lit_readout(b, face, "Time L readout", ui::bind("repeat/time_l"), 6.0, 30.0, 40.0, 10.0, lit, document_proto::TextAlign::Left);
        ui::lit_readout(b, face, "Tempo readout", ui::bind("repeat/bpm"), 52.0, 30.0, 76.0, 10.0, lit, document_proto::TextAlign::Center);
        ui::lit_readout(b, face, "Time R readout", ui::bind("repeat/time_r"), dw - 46.0, 30.0, 40.0, 10.0, lit, document_proto::TextAlign::Right);
        ui::glass(b, face);
        // LEDs over the stomps; the tap LED blinks the tempo.
        parts::status_led(b, lid, "repeat/on", 5.0, (52.0, 186.0), "#e8f6ff", "light", None);
        parts::status_led(b, lid, "repeat/tap", 5.0, (352.0, 186.0), "#e8f6ff", "light", Some("param(repeat/bpm)/60"));
        parts::footswitch(b, lid, "repeat/on", "Engage", 25.0, (52.0, 220.0), false);
        parts::footswitch(b, lid, "repeat/tap", "Tap Tempo", 25.0, (352.0, 220.0), true);
        parts::centred(b, lid, "Engage", (52.0, 256.0), Type::new(6.4, WHITE, 800.0));
        parts::centred(b, lid, "Tap Tempo", (352.0, 256.0), Type::new(6.4, WHITE, 800.0));
        // The infinity sign and the name.
        let inf: Vec<(f64, f64)> = (0..=64)
            .map(|i| {
                let a = std::f64::consts::TAU * f64::from(i) / 64.0;
                let d = 1.0 + a.sin().powi(2);
                (w / 2.0 + 30.0 * a.cos() / d, 208.0 + 30.0 * a.sin() * a.cos() / d)
            })
            .collect();
        parts::art(b, lid, &[inf], 3.4, WHITE);
        parts::centred(b, lid, "Repeat-t-t", (w / 2.0, 243.0), Type::new(12.5, WHITE, 800.0));
    });
    let params = vec![
        parts::pct("repeat/mix", "Mix", 30.0),
        parts::pct("repeat/feedback", "Feedback", 35.0),
        parts::hz("repeat/low", "Low Cut", 20.0, 1000.0, 100.0),
        parts::hz("repeat/high", "High Cut", 1000.0, 20000.0, 6000.0),
        parts::pick("repeat/time_l", "Time L", DIVS, 7),
        parts::pick("repeat/time_r", "Time R", DIVS, 5),
        parts::switch("repeat/sync", "Sync", true),
        parts::switch("repeat/dual", "Single/Dual", true),
        ui::param("repeat/bpm", "Tempo", Range::linear(40.0, 240.0, Unit::Bpm), 120.0).featured(),
        parts::switch("repeat/on", "Engage", true),
        parts::switch("repeat/tap", "Tap Tempo", false),
    ];
    ui::formatted(Face { file: "12-repeat-bigbox-404x286", plugin: pedals::PLUGIN, scene, params, streams: Vec::new() })
}
