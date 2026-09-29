//! Paint the hardware kit to one PNG, for review.
//!
//! ```text
//! kit_sheet --out kit.png [--family knob|meter|button|switch|lever|panel] [--scale 2] [--swatch]
//!           [--fx gpu|fallback|off]
//! ```
//! Every registered component (or one family), in a labelled grid, each
//! bound to a demo param at a mid value so pointers and meters show a
//! position. `--swatch` instead paints the kit's paint helpers (angled
//! linears, offset radials, conics) to check gradient geometry.

#[cfg(not(feature = "anyrender"))]
fn main() {
    eprintln!("kit_sheet requires --features anyrender");
    std::process::exit(2);
}

#[cfg(feature = "anyrender")]
fn main() {
    if let Err(e) = run() {
        eprintln!("kit_sheet: {e}");
        std::process::exit(1);
    }
}

#[cfg(feature = "anyrender")]
fn run() -> Result<(), String> {
    use std::collections::BTreeMap;

    use document_proto::{AutoLayout, CrossAxisAlign, LayoutDirection, Node, NodeKind, Scene, TextAlign};
    use surface_kit::draw;
    use surface_proto::{Binding, EntryId, ParamValue, Source, StreamFrame};
    use surface_runtime::{Runtime, Surface};
    use surface_widgets::{Builder, Theme, hug};

    let mut out = None;
    let mut family: Option<String> = None;
    let mut scale = 1.0_f64;
    let mut swatch = false;
    let mut fx = render::fx::Mode::Gpu;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = args.next(),
            "--family" => family = args.next(),
            "--scale" => scale = args.next().ok_or("--scale needs a factor")?.parse().map_err(|e| format!("{e}"))?,
            "--swatch" => swatch = true,
            "--fx" => fx = args.next().ok_or("--fx needs gpu|fallback|off")?.parse()?,
            other => return Err(format!("unknown arg {other}")),
        }
    }
    let out = out.ok_or("--out <png> is required")?;

    let mut scene = Scene::new("Kit");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
    let mut root = Node::new(NodeKind::Frame, "sheet");
    root.fills = vec![draw::solid("#3a3a3e")];
    root.layout = Some(AutoLayout {
        direction: LayoutDirection::Vertical,
        gap: 28.0,
        padding_left: 32.0,
        padding_right: 32.0,
        padding_top: 32.0,
        padding_bottom: 32.0,
        wrap: false,
        main_align: None,
        cross_align: Some(CrossAxisAlign::Start),
        hug_main: true,
        hug_cross: true,
    });
    let root = b.add(page, root);

    let row = |b: &mut Builder<'_>, parent, name: &str| {
        let mut n = Node::new(NodeKind::Frame, name);
        n.layout = Some(AutoLayout {
            direction: LayoutDirection::Horizontal,
            gap: 28.0,
            padding_left: 0.0,
            padding_right: 0.0,
            padding_top: 0.0,
            padding_bottom: 0.0,
            wrap: false,
            main_align: None,
            cross_align: Some(CrossAxisAlign::Center),
            hug_main: true,
            hug_cross: true,
        });
        b.add(parent, n)
    };
    let cell = |b: &mut Builder<'_>, parent, title: &str| {
        let mut n = Node::new(NodeKind::Frame, title);
        n.layout = Some(AutoLayout {
            direction: LayoutDirection::Vertical,
            gap: 10.0,
            padding_left: 14.0,
            padding_right: 14.0,
            padding_top: 14.0,
            padding_bottom: 12.0,
            wrap: false,
            main_align: None,
            cross_align: Some(CrossAxisAlign::Center),
            hug_main: true,
            hug_cross: true,
        });
        n.fills = vec![draw::solid("#1b1b1e")];
        n.corner_radii = document_proto::CornerRadii::uniform(10.0);
        let c = b.add(parent, n);
        b.add(c, draw::text("id", title, 0.0, 0.0, 180.0, 11.0, "#9a9aa2", TextAlign::Center));
        c
    };

    let mut catalog = surface_memory::demo_catalog();
    // A gain-reduction stream for the meters that read one.
    catalog.streams.push(surface_proto::StreamInfo { id: EntryId::new("comp/gr"), name: "GR".into(), kind: surface_proto::StreamKind::GainReduction, rate_hz: 30.0 });
    if swatch {
        let r = row(&mut b, root, "linear");
        for angle in [0.0, 45.0, 90.0, 135.0, 180.0, 270.0] {
            let c = cell(&mut b, r, &format!("linear {angle}°"));
            b.add(c, draw::rect("sw", 0.0, 0.0, 120.0, 80.0, draw::linear(angle, &[(0.0, "#ffffff"), (1.0, "#d0342c")])));
        }
        let r = row(&mut b, root, "radial");
        for (cx, cy, rr) in [(0.5, 0.5, 0.5), (0.3, 0.3, 0.6), (0.7, 0.25, 0.4), (0.5, 0.5, 0.25)] {
            let c = cell(&mut b, r, &format!("radial {cx},{cy} r{rr}"));
            b.add(c, draw::circle("sw", 50.0, 50.0, 50.0, draw::radial_at(cx, cy, rr, &[(0.0, "#ffffff"), (1.0, "#1d4f8f")])));
        }
        let r = row(&mut b, root, "conic");
        for start in [0.0, 45.0, 90.0] {
            let c = cell(&mut b, r, &format!("conic from {start}°"));
            b.add(
                c,
                draw::circle("sw", 50.0, 50.0, 50.0, draw::conic(start, &[(0.0, "#ffffff"), (0.25, "#555555"), (0.5, "#ffffff"), (0.75, "#555555"), (1.0, "#ffffff")])),
            );
        }
    } else {
        let defs: Vec<_> = surface_kit::components()
            .into_iter()
            .filter(|c| family.as_deref().is_none_or(|f| c.family.name() == f))
            .collect();
        if defs.is_empty() {
            return Err("no components registered (for that family)".into());
        }
        let mut current = None;
        let mut per_row = 0;
        let mut r = root;
        for (i, def) in defs.iter().enumerate() {
            if current != Some(def.family) || per_row == 6 {
                r = row(&mut b, root, def.family.name());
                current = Some(def.family);
                per_row = 0;
            }
            per_row += 1;
            let c = cell(&mut b, r, def.id);
            let binding = match def.binds {
                "level stream" => Binding { source: Source::Stream { id: EntryId::new("out_level"), channel: Some(0) }, ..Binding::stream("out_level") },
                "scalar stream" => Binding::stream("tempo"),
                "choice param" => Binding::param("amp/channel"),
                "toggle param" => Binding::param(if i % 2 == 0 { "drive/on" } else { "delay/on" }),
                "action" => Binding::action("tap_tempo"),
                "none" => Binding::param(""),
                _ => Binding::param("delay/mix"),
            };
            let place = surface_kit::Place {
                binding: (def.binds != "none").then_some(binding),
                inner: Some(Box::new(surface_kit::Place::new("Bass").bind(Binding::param("amp/bass")).legend("Bass"))),
                ..surface_kit::Place::new(def.title)
            };
            // In a cell the size of its drawing: a tight box's print hangs out.
            surface_kit::place_in_cell(&mut b, c, def.id, &place)?;
        }
        // The options each family takes, shown once: inks, tight boxes,
        // stepped sweeps, reversed banks, gain reduction, ladders.
        let extras = variations(family.as_deref());
        if !extras.is_empty() {
            let r = row(&mut b, root, "variations");
            for (title, id, place, light) in extras {
                let c = cell(&mut b, r, &title);
                if light {
                    // A light panel, for dark ink.
                    if let Some(n) = b.scene.node_mut(c) {
                        n.fills = vec![draw::solid("#c9c6bd")];
                    }
                }
                surface_kit::place_in_cell(&mut b, c, id, &place)?;
            }
        }
    }
    hug(&mut scene, root);
    for (id, c) in &controls {
        if let Some(n) = scene.node_mut(*id) {
            n.control = Some(c.clone());
        }
    }

    let mut rt = Runtime::new(Surface::from_scene(scene), catalog);
    rt.apply_values([
        ParamValue { addr: surface_proto::ParamAddr::new("delay/mix"), value: 0.62 },
        ParamValue { addr: surface_proto::ParamAddr::new("amp/channel"), value: 0.5 },
        ParamValue { addr: surface_proto::ParamAddr::new("drive/on"), value: 1.0 },
    ]);
    rt.apply_frame(StreamFrame { id: EntryId::new("out_level"), values: vec![-9.0, -12.0] }, 0.0);
    rt.apply_frame(StreamFrame { id: EntryId::new("tempo"), values: vec![180.0] }, 0.0);
    rt.apply_frame(StreamFrame { id: EntryId::new("in_level"), values: vec![-5.0] }, 0.0);
    let frame = rt.frame(0.0);
    let size = frame.node(frame.node(frame.pages()[0]).and_then(|p| p.children.first().copied()).ok_or("empty")?).ok_or("no root")?.size;
    let (w, h) = ((size.width * scale).ceil() as u32, (size.height * scale).ceil() as u32);
    let font = include_bytes!("../../assets/inter/Inter-Variable.ttf");
    let values: std::collections::HashMap<_, _> =
        rt.surface().controls.iter().map(|(id, c)| (*id, rt.position(*id, c))).collect();
    let px = render::offscreen::Offscreen::new()?.render_with_effects(&render::fx::Frame {
        transform: kurbo::Affine::scale(scale),
        base: peniko::Color::from_rgba8(58, 58, 62, 255),
        font: Some(render::TextFontRef { bytes: font, index: 0 }),
        mode: fx,
        values: Some(&values),
        ..render::fx::Frame::new(&frame, frame.pages()[0], w, h)
    })?;
    image::RgbaImage::from_raw(px.width, px.height, px.rgba).ok_or("bad pixels")?.save(&out).map_err(|e| format!("{out}: {e}"))?;
    println!("{{\"ok\":true,\"out\":\"{out}\",\"width\":{w},\"height\":{h}}}");
    Ok(())
}

/// Extra cells showing what a family's `Place` options do:
/// `(title, component, place, on a light panel)`.
#[cfg(feature = "anyrender")]
fn variations(family: Option<&str>) -> Vec<(String, &'static str, surface_kit::Place, bool)> {
    use surface_kit::{Ink, Place, TOUCH_MIN};
    use surface_proto::Binding;
    let mix = || Binding::param("delay/mix");
    let choice = || Binding::param("amp/channel");
    let level = || Binding::stream("in_level");
    let mut v: Vec<(String, &'static str, Place, bool)> = Vec::new();
    let all = family.is_none();
    if all || family == Some("knob") {
        v.push(("dark ink, kept case".into(), "knob.bakelite", Place::new("Trim").bind(mix()).legend("Trim dB").ink(Ink::Dark).keep_case(), true));
        v.push(("touch(44): tight box".into(), "knob.collet", Place::new("Freq").size(36.0).bind(mix()).legend("Freq").touch(TOUCH_MIN), false));
        v.push(("stepped by options".into(), "knob.chicken_head", Place::new("Mode").bind(choice()).legend("Mode").options(&["Clean", "Crunch", "Lead"]), false));
        v.push(("sweep 270, red ink".into(), "knob.neve", Place::new("Gain").bind(mix()).legend("Gain").sweep(270.0).ink(Ink::Color("#d0342c".into())), false));
        v.push((
            "concentric grey_black".into(),
            "knob.concentric",
            Place::new("Freq").bind(mix()).legend("kHz").keep_case().variant("grey_black").inner(Place::new("Gain").bind(Binding::param("amp/bass")).legend("Gain")),
            false,
        ));
        v.push(("stepped words at 44 px".into(), "knob.chicken_head", Place::new("Stage").size(44.0).bind(choice()).legend("Stage").keep_case().options(&["Triode", "Pentode", "Standard"]), false));
        v.push((
            "concentric grey, stepped".into(),
            "knob.concentric",
            Place::new("Freq").bind(choice()).legend("kHz").keep_case().variant("grey").options(&["Off", "·36", "·7", "1·6", "3·2"]).inner(Place::new("Gain").bind(Binding::param("amp/bass"))),
            false,
        ));
    }
    if all || family == Some("meter") {
        v.push(("VU on GR, no data: rests at 0".into(), "meter.vu_ivory", Place::new("GR").bind(Binding::stream("comp/gr")).size(180.0), false));
        v.push(("ladder: gr (5 dB), reversed".into(), "meter.led_ladder", Place::new("GR").bind(level()).options(&["gr"]).reversed().size(200.0), false));
        v.push((
            "column with scale".into(),
            "meter.led_column",
            Place::new("In").bind(level()).options(&["floor=-30", "ceiling=6", "segments=12"]).scale(&["-30", "-18", "-6", "+6"]),
            false,
        ));
        v.push(("bar: colors=… per lamp".into(), "meter.led_bar", Place::new("Tail").bind(level()).options(&["segments=6", "colors=#6f7cff,#8a7cff,#a57cff,#c07cff,#db7cff,#ff7ad9"]), false));
        v.push(("VU: glass".into(), "meter.vu_amber", Place::new("GR").bind(Binding::stream("comp/gr")).size(180.0).options(&["glass"]), false));
        v.push(("bar, dark-ink scale".into(), "meter.led_bar", Place::new("Out").bind(level()).scale(&["-40", "-20", "0"]).variant("blue").ink(Ink::Dark), true));
    }
    if all || family == Some("button") {
        v.push((
            "bank reversed, long legends".into(),
            "button.square_bank",
            Place::new("Algo").bind(choice()).options(&["Hall", "Shimmer", "Spectral"]).reversed().variant("yellow"),
            false,
        ));
        v.push(("bank fitted as one".into(), "button.illuminated_bank", Place::new("Voice").bind(choice()).options(&["Clean", "Filtered", "Dark"]), false));
        v.push(("illuminated: momentary (TAP)".into(), "button.illuminated", Place::new("Tap").bind(Binding::param("delay/tap")).legend("Tap").variant("red").options(&["momentary"]), false));
        v.push(("push-in bank, dark ink".into(), "button.push_in_bank", Place::new("Ratio").bind(choice()).options(&["4", "8", "12"]).ink(Ink::Dark), true));
    }
    if all || family == Some("switch") {
        v.push((
            "footswitch: momentary, blue".into(),
            "switch.footswitch",
            Place::new("Tap").bind(Binding::param("delay/on")).variant("blue").options(&["momentary"]).legend("Tap"),
            false,
        ));
        v.push(("rotary: words, cream".into(), "switch.rotary", Place::new("Mode").bind(choice()).options(&["Opto", "Stereo", "Harm"]).variant("cream").legend("Mode"), false));
        v.push(("footswitch: color=#b77dff".into(), "switch.footswitch", Place::new("On").bind(Binding::param("delay/on")).options(&["color=#b77dff"]).legend("On"), false));
        v.push(("rotary: sweep 90, dark ink".into(), "switch.rotary", Place::new("Mode").bind(choice()).options(&["A", "B", "C"]).sweep(90.0).ink(Ink::Dark).legend("Mode"), true));
    }
    if all || family == Some("lever") {
        v.push(("pultec, dark ink".into(), "lever.pultec", Place::new("CPS").bind(choice()).options(&["20", "30", "60"]).ink(Ink::Dark).legend("CPS"), true));
    }
    v
}
