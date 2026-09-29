//! Render a generated control surface to PNG.
//!
//! ```text
//! render_surface --out surface.png [--theme stage-dark|daylight]
//!                [--markup surface.fm] [--set amp/gain=0.8 ...] [--level -12] [--scale 2]
//! ```
//! Builds a surface from the demo engine catalog, applies the given
//! parameter values (normalized) and a meter level, and renders one frame.

#[cfg(not(feature = "anyrender"))]
fn main() {
    eprintln!("render_surface requires --features anyrender");
    std::process::exit(2);
}

#[cfg(feature = "anyrender")]
fn main() {
    if let Err(err) = run() {
        eprintln!("render_surface error: {err}");
        std::process::exit(1);
    }
}

#[cfg(feature = "anyrender")]
fn run() -> Result<(), String> {
    use surface_proto::{EntryId, ParamAddr, ParamValue, StreamFrame};
    use surface_runtime::Runtime;
    use surface_widgets::{Options, Theme, surface_for_catalog};

    let mut out = None;
    let mut theme = Theme::stage_dark();
    let mut sets = Vec::new();
    let mut level = None;
    let mut scale = 1.0_f64;
    let mut markup: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = args.next(),
            "--markup" => markup = args.next(),
            "--theme" => {
                theme = match args.next().as_deref() {
                    Some("daylight") => Theme::daylight(),
                    _ => Theme::stage_dark(),
                }
            }
            "--set" => {
                let kv = args.next().ok_or("--set needs addr=value")?;
                let (k, v) = kv.split_once('=').ok_or("--set needs addr=value")?;
                sets.push(ParamValue { addr: ParamAddr::new(k), value: v.parse::<f64>().map_err(|e| format!("{kv}: {e}"))? });
            }
            "--level" => level = Some(args.next().ok_or("--level needs dB")?.parse::<f64>().map_err(|e| e.to_string())?),
            "--scale" => scale = args.next().ok_or("--scale needs a factor")?.parse::<f64>().map_err(|e| e.to_string())?,
            other => return Err(format!("unknown arg: {other}")),
        }
    }
    let out = out.ok_or("--out <png> is required")?;

    let catalog = surface_memory::demo_catalog();
    let surface = match &markup {
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))?;
            let scene = document_text::parse(&text, Default::default()).map_err(|e| format!("{path}:{e}"))?;
            surface_runtime::Surface::from_scene(scene)
        }
        None => surface_for_catalog(&catalog, &Options { theme: theme.clone(), ..Options::default() }),
    };
    let mut rt = Runtime::new(surface, catalog);
    rt.apply_values(sets);
    if let Some(db) = level {
        rt.apply_frame(StreamFrame { id: EntryId::new("out_level"), values: vec![db, db - 4.0] }, 0.0);
    }
    let frame = rt.frame(0.0);

    let page = frame.pages()[0];
    let root = frame.node(page).and_then(|p| p.children.first().copied()).ok_or("empty surface")?;
    let size = frame.node(root).map(|n| n.size).ok_or("no root")?;
    let (w, h) = ((size.width * scale).ceil() as u32, (size.height * scale).ceil() as u32);

    let mut scene = vello::Scene::new();
    let font = include_bytes!("../../assets/inter/Inter-Variable.ttf");
    render::paint_scene_into(
        &mut scene,
        &frame,
        page,
        kurbo::Affine::scale(scale),
        Some(render::TextFontRef { bytes: font, index: 0 }),
    );
    let bg = frame
        .node(root)
        .and_then(|n| n.fills.iter().find_map(|p| match &p.kind {
            document_proto::PaintKind::Solid { color } => Some(*color),
            _ => None,
        }))
        .unwrap_or(theme.background);
    let base = peniko::Color::from_rgba8((bg.r * 255.0) as u8, (bg.g * 255.0) as u8, (bg.b * 255.0) as u8, 255);
    let px = render::offscreen::Offscreen::new()?.render(&scene, w, h, base)?;
    image::RgbaImage::from_raw(px.width, px.height, px.rgba)
        .ok_or("bad pixel buffer")?
        .save(&out)
        .map_err(|e| format!("writing {out}: {e}"))?;
    println!("{{\"ok\":true,\"out\":\"{out}\",\"width\":{w},\"height\":{h},\"controls\":{}}}", rt.surface().controls.len());
    Ok(())
}
