#[cfg(not(feature = "anyrender"))]
fn main() {
    eprintln!("render_png requires --features anyrender");
    std::process::exit(2);
}

#[cfg(feature = "anyrender")]
fn main() {
    if let Err(err) = run() {
        eprintln!("render_png error: {err}");
        std::process::exit(1);
    }
}

#[cfg(feature = "anyrender")]
fn run() -> Result<(), String> {
    use anyrender_vello::VelloScenePainter;
    use frame_proto::FrameDocument;
    use frame_ui::{build_layout_boxes, TextFontRef};
    use image::RgbaImage;
    use kurbo::Affine;
    use peniko::Color;
    use vello::wgpu;
    use vello::{AaConfig, RenderParams, Renderer, RendererOptions, Scene};

    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .ok_or_else(|| "usage: render_png <input_json> --out <output_png> [--root FIGMA_ID] [--width PX] [--height PX]".to_string())?;

    let mut out_path: Option<String> = None;
    let mut root_figma_id: Option<String> = None;
    let mut width_override: Option<u32> = None;
    let mut height_override: Option<u32> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out_path = args.next(),
            "--root" => root_figma_id = args.next(),
            "--width" => {
                width_override = Some(
                    args.next()
                        .ok_or_else(|| "--width requires value".to_string())?
                        .parse::<u32>()
                        .map_err(|e| format!("invalid --width value: {e}"))?,
                )
            }
            "--height" => {
                height_override = Some(
                    args.next()
                        .ok_or_else(|| "--height requires value".to_string())?
                        .parse::<u32>()
                        .map_err(|e| format!("invalid --height value: {e}"))?,
                )
            }
            _ => return Err(format!("unknown arg: {arg}")),
        }
    }

    let out_path = out_path.ok_or_else(|| "--out <output_png> is required".to_string())?;
    let bytes = std::fs::read(&input).map_err(|e| format!("failed reading {input}: {e}"))?;
    let export_bbox = extract_export_bbox(&bytes, root_figma_id.as_deref())?;
    let base_color = extract_export_base_color(&bytes, root_figma_id.as_deref())?.unwrap_or(Color::WHITE);
    let doc = FrameDocument::from_source_bytes(&bytes)
        .map_err(|e| format!("failed importing source document from {input}: {e}"))?;
    let root = match root_figma_id {
        Some(figma_id) => doc
            .get_by_figma_id(&figma_id)
            .map(|n| n.id)
            .ok_or_else(|| format!("root figma id not found: {figma_id}"))?,
        None => doc.pages.first().copied().unwrap_or(doc.root),
    };

    let layout_boxes = build_layout_boxes(&doc, root);
    let subtree = doc.walk_subtree(root);

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for node_id in subtree {
        if let Some(b) = layout_boxes.get(&node_id).copied() {
            if b.width <= 0.0 || b.height <= 0.0 {
                continue;
            }
            min_x = min_x.min(b.x);
            min_y = min_y.min(b.y);
            max_x = max_x.max(b.x + b.width);
            max_y = max_y.max(b.y + b.height);
        }
    }
    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        if let Some((x, y, w, h)) = export_bbox {
            min_x = x;
            min_y = y;
            max_x = x + w;
            max_y = y + h;
        } else {
            min_x = 0.0;
            min_y = 0.0;
            max_x = 1.0;
            max_y = 1.0;
        }
    }

    let (bbox_width, bbox_height) = export_bbox
        .map(|(_, _, w, h)| (w.ceil().max(1.0) as u32, h.ceil().max(1.0) as u32))
        .unwrap_or((1, 1));
    let natural_width = (max_x - min_x).ceil().max(1.0) as u32;
    let natural_height = (max_y - min_y).ceil().max(1.0) as u32;
    let width = width_override.unwrap_or(natural_width).clamp(1, 8192);
    let height = height_override.unwrap_or(natural_height).clamp(1, 8192);
    let width = if width_override.is_some() {
        width
    } else {
        width.max(bbox_width)
    };
    let height = if height_override.is_some() {
        height
    } else {
        height.max(bbox_height)
    };

    let mut scene = Scene::new();
    let scene_transform = Affine::translate((-min_x, -min_y));
    let text_font_bytes = load_default_text_font_bytes();
    let text_font_ref = text_font_bytes
        .as_ref()
        .map(|bytes| TextFontRef { bytes, index: 0 });
    {
        let mut painter = VelloScenePainter::new(&mut scene);
        frame_ui::paint_into_scene_with(&mut painter, &doc, root, scene_transform, text_font_ref);
    }

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .map_err(|e| format!("request_adapter failed: {e}"))?;

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("render_png-device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::default(),
    }))
    .map_err(|e| format!("request_device failed: {e}"))?;

    let mut renderer = Renderer::new(
        &device,
        RendererOptions {
            use_cpu: false,
            antialiasing_support: [AaConfig::Area, AaConfig::Msaa8].into_iter().collect(),
            num_init_threads: None,
            pipeline_cache: None,
        },
    )
    .map_err(|e| format!("failed to create renderer: {e}"))?;

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("render_png-target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let target_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    renderer
        .render_to_texture(
            &device,
            &queue,
            &scene,
            &target_view,
            &RenderParams {
                base_color,
                width,
                height,
                antialiasing_method: AaConfig::Msaa8,
            },
        )
        .map_err(|e| format!("render_to_texture failed: {e}"))?;

    let bytes_per_pixel = 4u32;
    let unpadded_bytes_per_row = width * bytes_per_pixel;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(256) * 256;
    let output_buffer_size = padded_bytes_per_row as u64 * height as u64;

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("render_png-readback"),
        size: output_buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("render_png-copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    let buffer_slice = output_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |v| {
        let _ = tx.send(v);
    });
    loop {
        let _ = device.poll(wgpu::PollType::Poll);
        match rx.try_recv() {
            Ok(result) => {
                result.map_err(|e| format!("map_async failed: {e}"))?;
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err("map_async channel disconnected".to_string());
            }
        }
    }

    let mapped = buffer_slice.get_mapped_range();
    let mut pixels = vec![0_u8; (width * height * bytes_per_pixel) as usize];
    for row in 0..height as usize {
        let src_offset = row * padded_bytes_per_row as usize;
        let dst_offset = row * unpadded_bytes_per_row as usize;
        let src = &mapped[src_offset..src_offset + unpadded_bytes_per_row as usize];
        let dst = &mut pixels[dst_offset..dst_offset + unpadded_bytes_per_row as usize];
        dst.copy_from_slice(src);
    }
    drop(mapped);
    output_buffer.unmap();

    let image = RgbaImage::from_raw(width, height, pixels)
        .ok_or_else(|| "failed to build image buffer from render output".to_string())?;
    if let Some(parent) = std::path::Path::new(&out_path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed creating output dir {}: {e}", parent.display()))?;
    }
    image
        .save(&out_path)
        .map_err(|e| format!("failed writing PNG {out_path}: {e}"))?;

    println!(
        "{{\"ok\":true,\"input\":\"{}\",\"out\":\"{}\",\"width\":{},\"height\":{}}}",
        input, out_path, width, height
    );
    Ok(())
}

#[cfg(feature = "anyrender")]
fn load_default_text_font_bytes() -> Option<Vec<u8>> {
    // Prefer vendored Inter for parity with Figma defaults.
    if let Some(bytes) = load_vendored_inter() {
        return Some(bytes);
    }

    let candidates = [
        "/System/Library/Fonts/SFNS.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            return Some(bytes);
        }
    }
    None
}

#[cfg(feature = "anyrender")]
fn load_vendored_inter() -> Option<Vec<u8>> {
    let bytes = include_bytes!("../../assets/inter/Inter-Variable.ttf");
    if bytes.is_empty() {
        None
    } else {
        Some(bytes.to_vec())
    }
}

#[cfg(feature = "anyrender")]
fn extract_export_bbox(
    bytes: &[u8],
    root_figma_id: Option<&str>,
) -> Result<Option<(f64, f64, f64, f64)>, String> {
    let json: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let nodes = match json.get("nodes").and_then(|n| n.as_array()) {
        Some(nodes) if !nodes.is_empty() => nodes,
        _ => return Ok(None),
    };

    let target = if let Some(root_id) = root_figma_id {
        nodes.iter().find(|node| node.get("id").and_then(|v| v.as_str()) == Some(root_id))
    } else {
        nodes.first()
    }
    .unwrap_or(&nodes[0]);

    let bbox = target
        .get("absoluteBoundingBox")
        .ok_or_else(|| "export node missing absoluteBoundingBox".to_string())?;
    let x = bbox
        .get("x")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "invalid absoluteBoundingBox.x".to_string())?;
    let y = bbox
        .get("y")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "invalid absoluteBoundingBox.y".to_string())?;
    let w = bbox
        .get("width")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "invalid absoluteBoundingBox.width".to_string())?;
    let h = bbox
        .get("height")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "invalid absoluteBoundingBox.height".to_string())?;
    if w <= 0.0 || h <= 0.0 {
        return Ok(None);
    }
    Ok(Some((x, y, w, h)))
}

#[cfg(feature = "anyrender")]
fn extract_export_base_color(
    bytes: &[u8],
    root_figma_id: Option<&str>,
) -> Result<Option<peniko::Color>, String> {
    let json: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let nodes = match json.get("nodes").and_then(|n| n.as_array()) {
        Some(nodes) if !nodes.is_empty() => nodes,
        _ => return Ok(None),
    };

    let target = if let Some(root_id) = root_figma_id {
        nodes.iter().find(|node| node.get("id").and_then(|v| v.as_str()) == Some(root_id))
    } else {
        nodes.first()
    }
    .unwrap_or(&nodes[0]);

    let fills = match target.get("fills").and_then(|v| v.as_array()) {
        Some(fills) => fills,
        None => return Ok(None),
    };
    let solid = match fills.iter().find(|fill| fill.get("type").and_then(|v| v.as_str()) == Some("SOLID")) {
        Some(fill) => fill,
        None => return Ok(None),
    };
    let color = match solid.get("color").and_then(|v| v.as_object()) {
        Some(color) => color,
        None => return Ok(None),
    };
    let r = color.get("r").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let g = color.get("g").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let b = color.get("b").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let a = solid.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0);

    let to_u8 = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    Ok(Some(peniko::Color::from_rgba8(
        to_u8(r),
        to_u8(g),
        to_u8(b),
        to_u8(a),
    )))
}
