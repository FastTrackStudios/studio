//! Render a generated control surface to PNG.
//!
//! ```text
//! render_surface --out surface.png [--theme stage-dark|daylight]
//!                [--markup surface.fm] [--catalog catalog.json] [--set amp/gain=0.8 ...]
//!                [--stream ID=V[,V2…] ...] [--level -12] [--demo-feeds]
//!                [--scale 2] [--fx gpu|fallback|off] [--time SECONDS]
//! ```
//! Builds a surface from the demo engine catalog (or `--markup` +
//! `--catalog`), applies the given parameter values (normalized) and stream
//! frames, and renders one frame.
//!
//! `--stream id=v[,v2…]` feeds one frame to any stream — a level in dBFS
//! per channel (`plate/in=-12,-14`), gain reduction (`la2a/gr=-6`), an LFO
//! scalar (`chorus/lfo=0.8`) — and repeats. `--level DB` feeds every level
//! stream in the catalog (DB on the first channel, DB − 4 on the rest)
//! unless `--stream` names streams explicitly; then it only fills in the
//! level streams none of them named.
//!
//! `--demo-feeds` feeds every other feed of the catalog (levels, gain
//! reduction, spectra, histories, scopes, envelopes, tuner, scalars) from
//! `feed-memory`'s demo generators at `--time` seconds.

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
    let mut demo_feeds = false;
    let mut streams: Vec<StreamFrame> = Vec::new();
    let mut scale = 1.0_f64;
    let mut markup: Option<String> = None;
    let mut catalog_path: Option<String> = None;
    let mut fx = render::fx::Mode::Gpu;
    let mut time_s = 0.0_f64;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = args.next(),
            "--markup" => markup = args.next(),
            "--catalog" => catalog_path = args.next(),
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
            "--stream" => {
                let kv = args.next().ok_or("--stream needs id=v[,v2…]")?;
                let (k, v) = kv.split_once('=').ok_or("--stream needs id=v[,v2…]")?;
                let values = v.split(',').map(|x| x.trim().parse::<f64>().map_err(|e| format!("{kv}: {e}"))).collect::<Result<Vec<_>, _>>()?;
                streams.push(StreamFrame { id: EntryId::new(k), values });
            }
            "--demo-feeds" => demo_feeds = true,
            "--level" => level = Some(args.next().ok_or("--level needs dB")?.parse::<f64>().map_err(|e| e.to_string())?),
            "--fx" => fx = args.next().ok_or("--fx needs gpu|fallback|off")?.parse()?,
            "--time" => time_s = args.next().ok_or("--time needs seconds")?.parse::<f64>().map_err(|e| e.to_string())?,
            "--scale" => scale = args.next().ok_or("--scale needs a factor")?.parse::<f64>().map_err(|e| e.to_string())?,
            other => return Err(format!("unknown arg: {other}")),
        }
    }
    let out = out.ok_or("--out <png> is required")?;

    let catalog = match &catalog_path {
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))?;
            serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?
        }
        None => surface_memory::demo_catalog(),
    };
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
    let levels = level_frames(rt.catalog(), level, &streams);
    if demo_feeds {
        let named: Vec<&EntryId> = streams.iter().chain(&levels).map(|f| &f.id).collect();
        for f in demo_frames(rt.catalog(), time_s, &named) {
            rt.apply_frame(f, 0.0);
        }
    }
    for f in levels {
        rt.apply_frame(f, 0.0);
    }
    for f in streams {
        if rt.catalog().stream(&f.id).is_none() {
            eprintln!("render_surface: warning: no stream `{}` in the catalog", f.id.0);
        }
        rt.apply_frame(f, 0.0);
    }
    let frame = rt.frame(0.0);

    let page = frame.pages()[0];
    let root = frame.node(page).and_then(|p| p.children.first().copied()).ok_or("empty surface")?;
    let size = frame.node(root).map(|n| n.size).ok_or("no root")?;
    let (w, h) = ((size.width * scale).ceil() as u32, (size.height * scale).ceil() as u32);

    let bg = frame
        .node(root)
        .and_then(|n| n.fills.iter().find_map(|p| match &p.kind {
            document_proto::PaintKind::Solid { color } => Some(*color),
            _ => None,
        }))
        .unwrap_or(theme.background);
    let base = peniko::Color::from_rgba8((bg.r * 255.0) as u8, (bg.g * 255.0) as u8, (bg.b * 255.0) as u8, 255);
    let font = include_bytes!("../../assets/inter/Inter-Variable.ttf");
    let values = control_positions(&rt);
    let px = render::offscreen::Offscreen::new()?.render_with_effects(&render::fx::Frame {
        transform: kurbo::Affine::scale(scale),
        base,
        font: Some(render::TextFontRef { bytes: font, index: 0 }),
        mode: fx,
        time_s,
        values: Some(&values),
        ..render::fx::Frame::new(&frame, page, w, h)
    })?;
    image::RgbaImage::from_raw(px.width, px.height, px.rgba)
        .ok_or("bad pixel buffer")?
        .save(&out)
        .map_err(|e| format!("writing {out}: {e}"))?;
    println!("{{\"ok\":true,\"out\":\"{out}\",\"width\":{w},\"height\":{h},\"controls\":{}}}", rt.surface().controls.len());
    Ok(())
}

/// `--level`'s frames: `db` (and `db − 4` on further channels) on every
/// level stream of the catalog that no explicit `--stream` names.
#[cfg(feature = "anyrender")]
fn level_frames(catalog: &surface_proto::Catalog, level: Option<f64>, explicit: &[surface_proto::StreamFrame]) -> Vec<surface_proto::StreamFrame> {
    let Some(db) = level else { return Vec::new() };
    catalog
        .streams
        .iter()
        .filter(|s| !explicit.iter().any(|f| f.id == s.id))
        .filter_map(|s| match s.kind {
            surface_proto::StreamKind::Level { channels } => Some(surface_proto::StreamFrame {
                id: s.id.clone(),
                values: (0..channels.max(1)).map(|c| if c == 0 { db } else { db - 4.0 }).collect(),
            }),
            _ => None,
        })
        .collect()
}

/// `--demo-feeds`' frames: every feed of the catalog not in `skip`, from
/// the demo generators at `t` seconds.
#[cfg(feature = "anyrender")]
fn demo_frames(catalog: &surface_proto::Catalog, t: f64, skip: &[&surface_proto::EntryId]) -> Vec<surface_proto::StreamFrame> {
    feed_memory::frames(catalog.streams.iter().filter(|s| !skip.contains(&&s.id)), t)
}

/// Every control's position, for effect shaders' `value`.
#[cfg(feature = "anyrender")]
fn control_positions(rt: &surface_runtime::Runtime) -> std::collections::HashMap<document_proto::NodeId, f64> {
    rt.surface().controls.iter().map(|(id, c)| (*id, rt.position(*id, c))).collect()
}

#[cfg(all(test, feature = "anyrender"))]
mod tests {
    use surface_proto::{Catalog, EntryId, StreamFrame, StreamInfo, StreamKind};

    fn stream(id: &str, kind: StreamKind) -> StreamInfo {
        StreamInfo { id: EntryId::new(id), name: id.into(), kind, rate_hz: 30.0 }
    }

    #[test]
    fn level_feeds_every_level_stream_an_explicit_stream_leaves_alone() {
        let catalog = Catalog {
            streams: vec![
                stream("plate/in", StreamKind::Level { channels: 2 }),
                stream("plate/out", StreamKind::Level { channels: 1 }),
                stream("plate/gr", StreamKind::GainReduction),
            ],
            ..Catalog::default()
        };
        let ids = |fs: &[StreamFrame]| fs.iter().map(|f| (f.id.0.clone(), f.values.clone())).collect::<Vec<_>>();
        assert_eq!(
            ids(&super::level_frames(&catalog, Some(-12.0), &[])),
            [("plate/in".to_string(), vec![-12.0, -16.0]), ("plate/out".to_string(), vec![-12.0])]
        );
        let explicit = [StreamFrame { id: EntryId::new("plate/in"), values: vec![-3.0] }];
        assert_eq!(ids(&super::level_frames(&catalog, Some(-12.0), &explicit)), [("plate/out".to_string(), vec![-12.0])]);
        assert!(super::level_frames(&catalog, None, &[]).is_empty());
    }

    #[test]
    fn demo_feeds_fill_every_feed_not_named() {
        let catalog = Catalog { streams: feed_memory::demo_feeds(), ..Catalog::default() };
        let skip = EntryId::new("demo/gr");
        let frames = super::demo_frames(&catalog, 1.5, &[&skip]);
        assert_eq!(frames.len(), catalog.streams.len() - 1);
        assert!(frames.iter().all(|f| f.id != skip));
        for f in &frames {
            assert_eq!(f.values.len(), catalog.stream(&f.id).unwrap().kind.frame_len(), "{}", f.id);
        }
    }
}
