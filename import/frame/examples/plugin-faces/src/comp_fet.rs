//! Faces: the FET limiter — one circuit in its three finishes (blackface,
//! brushed silver, the LN's plum).
//!
//! Laid out from the unit, left to right on one centre line: INPUT and
//! OUTPUT, the two big silver-tops with their ∞…0 attenuation scales (there
//! is no threshold — INPUT drives into a fixed one, which is the unit's whole
//! character); ATTACK over RELEASE at half the size, both running "backwards"
//! (7, fully clockwise, is fastest); the ratio push-buttons 4 / 8 / 12 / 20
//! with "All" (all-buttons-in) above them, printed 20-at-the-top as the unit
//! does; the painted blue meter band with the power jewel, the VU movement
//! reading gain reduction (0 GR rests on 0 VU and swings left) and the model
//! lettering; and the meter-source bank GR / +8 / +4 / Off.
//!
//! What the plugin adds over the hardware, kept quiet so it still reads as
//! the unit: a small value readout under INPUT and OUTPUT, a POWER caption
//! by the jewel, and an engraved readout window under the meter with the
//! live GR and output level.
//!
//! Every part is a `surface-kit` component (`panel.brushed`,
//! `panel.section`, `knob.silver_top`, `meter.vu_ivory`,
//! `button.push_in_bank`, `button.jewel`); this module only lays them out,
//! prints the silkscreen, and wires the bindings.

use document_proto::{Node, NodeId, NodeKind, TextAlign, TextCase, Transform};
use surface_kit::{Place, draw, place};
use surface_proto::{
    Binding, ButtonMode, Control, ControlKind, EntryId, ParamInfo, Range, SlotRole, StreamInfo,
    StreamKind, Unit,
};
use surface_widgets::Builder;

use crate::face::{Face, canvas};

// ── the unit's controls ──────────────────────────────────────────────────

/// The attenuation scale printed round INPUT and OUTPUT, ∞ at the stop.
const ATTEN: &[&str] = &["∞", "48", "36", "30", "24", "18", "12", "6", "0"];
/// The time scales, 1 (slowest) to 7 (fastest), clockwise.
const TIMES: &[&str] = &["1", "2", "3", "4", "5", "6", "7"];
/// The ratio param's options, in param order.
const RATIOS: &[&str] = &["4", "8", "12", "20", "All"];
/// The meter-source param's options, in param order (= top to bottom).
const METER: &[&str] = &["GR", "+8", "+4", "Off"];

/// Addresses — one namespace for all three finishes: they're the same unit.
pub const INPUT: &str = "1176/input";
pub const OUTPUT: &str = "1176/output";
pub const ATTACK: &str = "1176/attack";
pub const RELEASE: &str = "1176/release";
pub const RATIO: &str = "1176/ratio";
pub const METER_SRC: &str = "1176/meter";
pub const POWER: &str = "1176/on";
pub const GR: &str = "1176/gr";
pub const OUT_LEVEL: &str = "1176/out";

fn params() -> Vec<ParamInfo> {
    let grp = "FET";
    vec![
        ParamInfo::new(INPUT, "Input", Range::linear(-48.0, 0.0, Unit::Db))
            .with_default_real(-24.0)
            .in_group(grp)
            .featured(),
        ParamInfo::new(OUTPUT, "Output", Range::linear(-48.0, 0.0, Unit::Db))
            .with_default_real(-18.0)
            .in_group(grp)
            .featured(),
        ParamInfo::new(ATTACK, "Attack", Range::linear(1.0, 7.0, Unit::None))
            .with_default_real(3.0)
            .in_group(grp)
            .featured(),
        ParamInfo::new(RELEASE, "Release", Range::linear(1.0, 7.0, Unit::None))
            .with_default_real(5.0)
            .in_group(grp)
            .featured(),
        ParamInfo::new(RATIO, "Ratio", Range::default())
            .choice(RATIOS)
            .in_group(grp)
            .featured(),
        ParamInfo::new(METER_SRC, "Meter", Range::default())
            .choice(METER)
            .in_group(grp)
            .featured(),
        ParamInfo::new(POWER, "Power", Range::default())
            .toggle()
            .with_default_real(1.0)
            .in_group(grp)
            .featured(),
    ]
}

fn streams() -> Vec<StreamInfo> {
    vec![
        StreamInfo {
            id: EntryId::new(GR),
            name: "Gain Reduction".into(),
            kind: StreamKind::GainReduction,
            rate_hz: 30.0,
        },
        StreamInfo {
            id: EntryId::new(OUT_LEVEL),
            name: "Output Level".into(),
            kind: StreamKind::Level { channels: 2 },
            rate_hz: 30.0,
        },
    ]
}

// ── the finishes ─────────────────────────────────────────────────────────

/// What differs between the three: paint and the ink printed on it.
struct Finish {
    file: &'static str,
    title: &'static str,
    /// `panel.brushed` variant.
    panel: &'static str,
    /// Silkscreen: legends, scales.
    ink: &'static str,
    /// Secondary silkscreen: the value readouts.
    dim: &'static str,
}

const BLACK: Finish = Finish {
    file: "01-1176-black",
    title: "FET Limiter — Blackface",
    panel: "black",
    ink: "#e4ddcf",
    dim: "#a49d8e",
};
const SILVER: Finish = Finish {
    file: "02-1176-silver",
    title: "FET Limiter — Silver",
    panel: "natural",
    ink: "#17181b",
    dim: "#44464b",
};
const LN: Finish = Finish {
    file: "03-1176-ln",
    title: "FET Limiter — LN",
    panel: "plum",
    ink: "#f1e6f0",
    dim: "#b3a4b2",
};

pub fn faces() -> Vec<Face> {
    [BLACK, SILVER, LN]
        .iter()
        .map(|f| Face {
            file: f.file,
            plugin: "comp",
            scene: canvas(f.title, |b, root| unit(b, root, f)),
            params: params(),
            streams: streams(),
        })
        .collect()
}

// ── layout ───────────────────────────────────────────────────────────────

/// Panel size: a 1U-proportioned plate, ears included.
const W: f64 = 960.0;
const H: f64 = 300.0;
/// The centre line everything hangs on.
const ROW: f64 = 150.0;
/// The blue meter band (x, width), full height.
const BAND_X: f64 = 628.0;
const BAND_W: f64 = 204.0;
/// Where both push-in banks start, so their titles share a line.
const BANK_TOP: f64 = 74.0;

fn unit(b: &mut Builder<'_>, root: NodeId, f: &Finish) {
    if let Some(n) = b.scene.node_mut(root) {
        n.size = document_proto::Size {
            width: W,
            height: H,
        };
    }
    let panel = kit(
        b,
        root,
        "panel.brushed",
        &Place::new("Faceplate").variant(f.panel).options(&[
            &format!("w={W}"),
            &format!("h={H}"),
            "ends=ears",
        ]),
    );
    // The renderer paints an *inside* stroke on a clipped, rounded frame as
    // a ~15 px dark band along the top edge; centred, it's the hairline the
    // kit means.
    if let Some(n) = b.scene.node_mut(panel) {
        n.stroke.align = document_proto::StrokeAlign::Center;
    }

    // ── INPUT / OUTPUT ──
    big_knob(b, panel, f, "Input", INPUT, 140.0);
    big_knob(b, panel, f, "Output", OUTPUT, 322.0);

    // ── ATTACK over RELEASE, half size, on the same centre line ──
    time_knob(b, panel, f, "Attack", ATTACK, 466.0, 94.0);
    time_knob(b, panel, f, "Release", RELEASE, 466.0, 222.0);

    // ── ratio: push-ins, printed 20 at the top with All above ──
    let ratio = bank(
        b,
        panel,
        "Ratio",
        RATIO,
        &["All", "20", "12", "8", "4"],
        572.0,
        f,
    );
    reverse_select(b, ratio);

    // ── the blue band: jewel, meter, lettering, readouts ──
    meter_band(b, panel);

    // ── meter source ──
    bank(b, panel, "Meter", METER_SRC, METER, 888.0, f);
}

/// Place a kit component (the ids here are the kit's own; a typo is a bug).
fn kit(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place) -> NodeId {
    place(b, parent, id, p).unwrap_or_else(|e| panic!("{e}"))
}

fn size_of(b: &Builder<'_>, id: NodeId) -> (f64, f64) {
    b.scene
        .node(id)
        .map_or((0.0, 0.0), |n| (n.size.width, n.size.height))
}

fn move_to(b: &mut Builder<'_>, id: NodeId, x: f64, y: f64) {
    if let Some(n) = b.scene.node_mut(id) {
        n.transform = Transform::translate(x, y);
    }
}

/// Repaint every text / scale node under `id` whose name is in `names`.
fn repaint(b: &mut Builder<'_>, id: NodeId, names: &[&str], color: &str) {
    for n in b.scene.walk(id) {
        if let Some(node) = b.scene.node_mut(n)
            && names.contains(&node.name.as_str())
        {
            node.fills = vec![draw::solid(color)];
        }
    }
}

/// Silkscreen: uppercase, tracked, a stated weight, centred on (`cx`, `cy`).
fn ink(
    name: &str,
    s: &str,
    (cx, cy): (f64, f64),
    size: f64,
    color: &str,
    (weight, track): (f64, f64),
) -> Node {
    let w = s.chars().count() as f64 * size * (0.72 + track) + 8.0;
    let mut n = draw::text(
        name,
        s,
        cx - w / 2.0,
        cy - size * 0.68,
        w,
        size,
        color,
        TextAlign::Center,
    );
    if let NodeKind::Text(t) = &mut n.kind {
        t.font_weight = Some(weight);
        t.text_case = TextCase::Upper;
        t.letter_spacing = size * track;
    }
    n
}

/// Set a knob's legend (the kit's `Label` slot) in the panel's ink, and
/// give it a `Value` readout under the legend.
fn style_knob(
    b: &mut Builder<'_>,
    knob: NodeId,
    f: &Finish,
    legend_size: f64,
    readout: Option<f64>,
) {
    repaint(b, knob, &["numeral", "scale dots"], f.ink);
    let (w, h) = size_of(b, knob);
    for n in b.scene.walk(knob) {
        if let Some(node) = b.scene.node_mut(n)
            && node.name == "legend"
            && let NodeKind::Text(t) = &mut node.kind
        {
            t.font_size = legend_size;
            t.font_weight = Some(750.0);
            t.letter_spacing = legend_size * 0.16;
            t.text_case = TextCase::Upper;
            node.size.height = (legend_size * 1.35).ceil();
            node.fills = vec![draw::solid(f.ink)];
        }
    }
    if let Some(size) = readout {
        let value = draw::text(
            "readout",
            "—",
            0.0,
            h + 1.0,
            w,
            size,
            f.dim,
            TextAlign::Center,
        );
        let value = b.add(knob, with_weight(value, 500.0));
        if let Some(n) = b.scene.node_mut(knob) {
            n.size.height = h + 1.0 + (size * 1.35).ceil();
        }
        if let Some(c) = b.controls.get_mut(&knob) {
            c.slots.push(surface_proto::Slot {
                role: SlotRole::Value,
                node: value.0,
            });
        }
    }
}

fn with_weight(mut n: Node, weight: f64) -> Node {
    if let NodeKind::Text(t) = &mut n.kind {
        t.font_weight = Some(weight);
    }
    n
}

/// INPUT / OUTPUT: the big silver-top on the centre line, legend under it,
/// the attenuation readout under that.
fn big_knob(b: &mut Builder<'_>, panel: NodeId, f: &Finish, legend: &str, addr: &str, cx: f64) {
    let k = kit(
        b,
        panel,
        "knob.silver_top",
        &Place::new(legend)
            .size(100.0)
            .legend(legend)
            .scale(ATTEN)
            .variant("silver")
            .bind(Binding::param(addr)),
    );
    style_knob(b, k, f, 12.0, Some(10.0));
    let (w, _) = size_of(b, k);
    move_to(b, k, cx - w / 2.0, ROW - 8.0 - w / 2.0);
}

/// ATTACK / RELEASE: half-size, the legend printed *above* (as on the unit,
/// so the stacked pair reads as two captions, not one between them).
fn time_knob(
    b: &mut Builder<'_>,
    panel: NodeId,
    f: &Finish,
    legend: &str,
    addr: &str,
    cx: f64,
    cy: f64,
) {
    let k = kit(
        b,
        panel,
        "knob.silver_top",
        &Place::new(legend)
            .size(56.0)
            .legend(legend)
            .scale(TIMES)
            .variant("silver")
            .bind(Binding::param(addr)),
    );
    style_knob(b, k, f, 10.0, None);
    let (w, _) = size_of(b, k);
    // The kit's legend row sits under the knob; lift it over the top and
    // trim the frame to the square round the shaft, so the pair can't
    // overlap and the caption can't be mistaken for the other knob's.
    let kids: Vec<NodeId> = b
        .scene
        .node(k)
        .map(|n| n.children.clone())
        .unwrap_or_default();
    for id in kids {
        if let Some(n) = b.scene.node_mut(id)
            && n.name == "legend"
        {
            n.transform = Transform::translate(n.transform.tx, -n.size.height - 4.0);
        }
    }
    if let Some(n) = b.scene.node_mut(k) {
        n.size.height = w;
    }
    // Small knobs print small numerals; bring them up to a readable size.
    for id in b.scene.walk(k) {
        if let Some(n) = b.scene.node_mut(id)
            && n.name == "numeral"
            && let NodeKind::Text(t) = &mut n.kind
            && t.font_size < 7.5
        {
            let grow = 7.5 / t.font_size;
            let (cx0, cy0) = (
                n.transform.tx + n.size.width / 2.0,
                n.transform.ty + n.size.height / 2.0,
            );
            t.font_size = 7.5;
            t.font_weight = Some(600.0);
            n.size.width *= grow;
            n.size.height = (n.size.height * grow).ceil();
            n.transform = Transform::translate(cx0 - n.size.width / 2.0, cy0 - n.size.height / 2.0);
        }
    }
    move_to(b, k, cx - w / 2.0, cy - w / 2.0);
}

/// A push-in bank with its caps centred on `cx`, title above, in the
/// panel's ink.
fn bank(
    b: &mut Builder<'_>,
    panel: NodeId,
    title: &str,
    addr: &str,
    options: &[&str],
    cx: f64,
    f: &Finish,
) -> NodeId {
    let cap = 30.0;
    let id = kit(
        b,
        panel,
        "button.push_in_bank",
        &Place::new(title)
            .size(cap)
            .options(options)
            .variant("black")
            .bind(Binding::param(addr)),
    );
    repaint(b, id, &["legend"], f.ink);
    let (w, _) = size_of(b, id);
    // The caps are the right-hand column of the bank (housing = cap + 2 × 2 px pad).
    let housing = cap + 4.0 * cap / 26.0;
    let x = cx - (w - housing / 2.0);
    let y = BANK_TOP;
    move_to(b, id, x, y);
    b.add(
        panel,
        ink("title", title, (cx, y - 14.0), 10.0, f.ink, (750.0, 0.16)),
    );
    id
}

/// The bank prints its options top-to-bottom as select indices 0…n; ours
/// are printed in the opposite order to the param's, so flip the indices.
fn reverse_select(b: &mut Builder<'_>, bank: NodeId) {
    let rows: Vec<NodeId> = b
        .scene
        .node(bank)
        .map(|n| n.children.clone())
        .unwrap_or_default();
    for row in rows {
        if let Some(Control {
            kind:
                ControlKind::Button {
                    mode: ButtonMode::Select { index, count },
                },
            ..
        }) = b.controls.get_mut(&row)
        {
            *index = *count - 1 - *index;
        }
    }
}

/// The painted band: power jewel on the blue, the VU movement reading gain
/// reduction, the model lettering, and an engraved readout window.
fn meter_band(b: &mut Builder<'_>, panel: NodeId) {
    let band = kit(
        b,
        panel,
        "panel.section",
        &Place::new("Meter band")
            .variant("fet_blue")
            .options(&[&format!("w={BAND_W}"), &format!("h={H}")]),
    );
    move_to(b, band, BAND_X, 0.0);
    // Hairline seams where the paint meets the metal.
    for x in [0.0, BAND_W - 1.0] {
        b.add(
            band,
            draw::rect("seam", x, 0.0, 1.0, H, draw::solid("#0000004d")),
        );
    }
    let cx = BAND_W / 2.0;

    // The jewel: the power switch, lit when on.
    let jewel = kit(
        b,
        band,
        "button.jewel",
        &Place::new("Power")
            .size(24.0)
            .variant("red")
            .bind(Binding::param(POWER)),
    );
    move_to(b, jewel, cx - 12.0, 30.0);
    // Its caption, small and to the side, as the jewel's `Label`.
    let caption = b.add(
        jewel,
        ink(
            "legend",
            "Power",
            (44.0, 12.0),
            7.0,
            "#bcd6e2",
            (700.0, 0.2),
        ),
    );
    if let Some(c) = b.controls.get_mut(&jewel) {
        c.slots.push(surface_proto::Slot {
            role: SlotRole::Label,
            node: caption.0,
        });
    }

    // The movement: VU card, GR stream. The kit binds a level stream
    // through the card's law over −20…+3, so a GR of 0 dB lands on 0 VU and
    // −5 on the printed 5 — the needle rests at 0 and swings left.
    let vu = kit(
        b,
        band,
        "meter.vu_ivory",
        &Place::new("GR meter")
            .size(182.0)
            .legend("VU")
            .bind(Binding::stream(GR)),
    );
    let (vw, vh) = size_of(b, vu);
    move_to(b, vu, cx - vw / 2.0, 66.0);
    let below = 66.0 + vh;

    // Lettering.
    let white = "#f2f6f8";
    let pale = "#bcd6e2";
    b.add(
        band,
        ink(
            "model",
            "Limiting Amplifier",
            (cx, below + 20.0),
            12.5,
            white,
            (800.0, 0.10),
        ),
    );
    b.add(
        band,
        ink(
            "model sub",
            "FET Peak Limiter · 76",
            (cx, below + 37.0),
            8.0,
            pale,
            (600.0, 0.22),
        ),
    );

    // The readout window: GR and output level, engraved into the paint.
    let (ww, wh) = (150.0, 22.0);
    let wy = below + 50.0;
    let mut window = draw::rect(
        "readout window",
        cx - ww / 2.0,
        wy,
        ww,
        wh,
        draw::linear(180.0, &[(0.0, "#0a1f28"), (1.0, "#123644")]),
    );
    window = draw::with_effects(
        draw::stroked(draw::rounded(window, 3.0), "#00000080", 1.0),
        vec![
            draw::inner_shadow("#000000b3", 0.0, 1.5, 3.0),
            draw::drop_shadow("#ffffff1f", 0.0, 1.0, 0.0),
        ],
    );
    b.add(band, window);
    readout(b, band, "GR", GR, "0.0", (cx - ww / 2.0, wy, ww / 2.0, wh));
    readout(b, band, "Out", OUT_LEVEL, "—", (cx, wy, ww / 2.0, wh));
    b.add(
        band,
        draw::rect(
            "divider",
            cx - 0.5,
            wy + 5.0,
            1.0,
            wh - 10.0,
            draw::solid("#bcd6e233"),
        ),
    );
}

/// One half of the readout window: a caption and a live `Value` from `stream`.
fn readout(
    b: &mut Builder<'_>,
    band: NodeId,
    caption: &str,
    stream: &str,
    rest: &str,
    (x, y, w, h): (f64, f64, f64, f64),
) {
    let root = b.add(band, draw::frame(&format!("{caption} readout"), x, y, w, h));
    let cap = ink(
        "caption",
        caption,
        (18.0, h / 2.0),
        7.0,
        "#8fb4c4",
        (700.0, 0.14),
    );
    b.add(root, cap);
    let mut value = draw::text(
        "value",
        rest,
        26.0,
        h / 2.0 - 6.8,
        w - 32.0,
        10.0,
        "#e9f3f7",
        TextAlign::Right,
    );
    value = with_weight(value, 600.0);
    let value = b.add(root, value);
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, value.0);
    c.binding = Some(Binding::stream(stream));
    b.controls.insert(root, c);
}
