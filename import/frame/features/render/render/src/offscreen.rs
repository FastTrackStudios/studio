//! Headless GPU rendering: a vello scene → RGBA pixels.
//!
//! One [`Offscreen`] holds the device and renderer, so rendering many frames
//! (parity runs, surface snapshots, previews) pays GPU setup once.

use vello::wgpu;
use vello::{AaConfig, RenderParams, Renderer, RendererOptions};

pub struct Offscreen {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    /// The effects stage, created on first use.
    fx: Option<crate::fx::Stage>,
    /// Bands the last [`Offscreen::render`] took.
    last_bands: u32,
}

/// What a render target is cleared to before vello draws: vello writes
/// every pixel of a frame it renders, and none when a buffer overflows, so
/// a row of nothing but this is an overflow. (No render leaves a whole row
/// this colour: almost transparent, but not black.)
pub const SENTINEL: [u8; 4] = [3, 1, 4, 1];

/// Rows per band multiple: vello's tile height, so bands split on tiles.
const BAND_ALIGN: u32 = 16;

/// The usages a [`render_checked`] target needs.
pub const CHECKED_TARGET_USAGES: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
    .union(wgpu::TextureUsages::COPY_SRC)
    .union(wgpu::TextureUsages::COPY_DST)
    .union(wgpu::TextureUsages::STORAGE_BINDING)
    .union(wgpu::TextureUsages::TEXTURE_BINDING);

/// Whether `rgba` is entirely [`SENTINEL`] (vello drew nothing).
#[must_use]
pub fn is_untouched(rgba: &[u8]) -> bool {
    rgba.as_chunks::<4>().0.iter().all(|p| *p == SENTINEL)
}

/// Render `scene` into `target` (an `Rgba8Unorm` texture with
/// [`CHECKED_TARGET_USAGES`]) like `Renderer::render_to_texture`, but never
/// leave it blank.
///
/// vello's GPU buffers (per-tile commands, tiles, segments, blend spill)
/// have fixed sizes that vello 0.10 cannot grow, and a frame that overflows
/// any of them renders *nothing*: coarse rasterisation flags the failure
/// and the fine stage skips every tile, with no error returned. A 6000 px
/// page overflows the per-tile command buffer on its own. So the target is
/// cleared to [`SENTINEL`] first; if its top row is still all sentinel
/// after the render, the frame is rendered again in horizontal bands
/// (halved until each fits — the viewport-sized buffers shrink with the
/// band) copied into place.
///
/// Returns the number of bands drawn (1 when the frame fit whole).
///
/// # Errors
/// On GPU failures, and when the scene overflows even the thinnest band
/// (its path data alone is too large).
pub fn render_checked(
    renderer: &mut vello::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &vello::Scene,
    target: &wgpu::Texture,
    base: peniko::Color,
    antialiasing: AaConfig,
) -> Result<u32, String> {
    let (width, height) = (target.width(), target.height());
    if render_band(renderer, device, queue, scene, target, 0, base, antialiasing)? {
        return Ok(1);
    }
    let mut bands = 0;
    // Bands still to draw, (top, height), top first; halved on overflow.
    let half = |h: u32| (h / 2).div_ceil(BAND_ALIGN) * BAND_ALIGN;
    let mut todo = vec![(half(height), height - half(height)), (0, half(height))];
    while let Some((top, h)) = todo.pop() {
        let band = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame-band"),
            size: wgpu::Extent3d { width, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: target.format(),
            usage: CHECKED_TARGET_USAGES,
            view_formats: &[],
        });
        if render_band(renderer, device, queue, scene, &band, top, base, antialiasing)? {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame-band-copy") });
            encoder.copy_texture_to_texture(
                band.as_image_copy(),
                wgpu::TexelCopyTextureInfo { texture: target, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: top, z: 0 }, aspect: wgpu::TextureAspect::All },
                wgpu::Extent3d { width, height: h, depth_or_array_layers: 1 },
            );
            queue.submit([encoder.finish()]);
            bands += 1;
            continue;
        }
        let split = half(h);
        if split == 0 || split >= h {
            return Err(format!(
                "the scene overflows vello's GPU buffers even in a {width}×{h} band; simplify it or render it smaller"
            ));
        }
        todo.push((top + split, h - split));
        todo.push((top, split));
    }
    Ok(bands)
}

/// Render rows `top..` of `scene` into `target`; `false` when vello's
/// buffers overflowed (its first row came back untouched).
#[allow(clippy::too_many_arguments)]
fn render_band(
    renderer: &mut vello::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &vello::Scene,
    target: &wgpu::Texture,
    top: u32,
    base: peniko::Color,
    antialiasing: AaConfig,
) -> Result<bool, String> {
    let shifted;
    let scene = if top == 0 {
        scene
    } else {
        let mut s = vello::Scene::new();
        s.append(scene, Some(kurbo::Affine::translate((0.0, -f64::from(top)))));
        shifted = s;
        &shifted
    };
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    clear_to_sentinel(device, queue, &view);
    renderer
        .render_to_texture(
            device,
            queue,
            scene,
            &view,
            &RenderParams { base_color: base, width: target.width(), height: target.height(), antialiasing_method: antialiasing },
        )
        .map_err(|e| format!("render_to_texture failed: {e}"))?;
    Ok(!is_untouched(&read_rows(device, queue, target, 0, 1)?))
}

/// Fill `view` with [`SENTINEL`].
fn clear_to_sentinel(device: &wgpu::Device, queue: &wgpu::Queue, view: &wgpu::TextureView) {
    let c = |v: u8| f64::from(v) / 255.0;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame-sentinel") });
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("frame-sentinel"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color { r: c(SENTINEL[0]), g: c(SENTINEL[1]), b: c(SENTINEL[2]), a: c(SENTINEL[3]) }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    }));
    queue.submit([encoder.finish()]);
}

/// Copy rows `top..top + rows` of an `Rgba8Unorm` texture back to the CPU
/// (straight RGBA8, no padding).
fn read_rows(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture, top: u32, rows: u32) -> Result<Vec<u8>, String> {
    let width = texture.width();
    let unpadded = width * 4;
    let padded = unpadded.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("frame-offscreen-readback"),
        size: u64::from(padded) * u64::from(rows),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame-offscreen-copy") });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: top, z: 0 }, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(rows) },
        },
        wgpu::Extent3d { width, height: rows, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |v| {
        let _ = tx.send(v);
    });
    loop {
        let _ = device.poll(wgpu::PollType::Poll);
        match rx.try_recv() {
            Ok(result) => {
                result.map_err(|e| format!("map_async failed: {e}"))?;
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => std::thread::sleep(std::time::Duration::from_millis(1)),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return Err("map_async channel disconnected".into()),
        }
    }
    let mapped = slice.get_mapped_range();
    let mut rgba = vec![0_u8; (unpadded * rows) as usize];
    for row in 0..rows as usize {
        let src = row * padded as usize;
        let dst = row * unpadded as usize;
        rgba[dst..dst + unpadded as usize].copy_from_slice(&mapped[src..src + unpadded as usize]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(rgba)
}

/// Straight RGBA8 pixels, row-major, no padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Offscreen {
    /// # Errors
    /// When no GPU adapter/device is available.
    pub fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .map_err(|e| format!("request_adapter failed: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("frame-offscreen"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::default(),
        }))
        .map_err(|e| format!("request_device failed: {e}"))?;
        let renderer = Renderer::new(
            &device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: [AaConfig::Area, AaConfig::Msaa8].into_iter().collect(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .map_err(|e| format!("failed to create renderer: {e}"))?;
        Ok(Self { device, queue, renderer, fx: None, last_bands: 0 })
    }

    /// Render `scene` at `width × height` over `base`, splitting it into
    /// bands when it overflows vello's buffers (see [`render_checked`]).
    /// [`Self::last_bands`] tells how many bands the frame took.
    ///
    /// # Errors
    /// On GPU failures, and when the scene overflows vello's buffers even
    /// in the thinnest band (never a blank frame).
    pub fn render(&mut self, scene: &vello::Scene, width: u32, height: u32, base: peniko::Color) -> Result<Pixels, String> {
        let (width, height) = (width.clamp(1, 8192), height.clamp(1, 8192));
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame-offscreen-target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: CHECKED_TARGET_USAGES,
            view_formats: &[],
        });
        self.last_bands = render_checked(&mut self.renderer, &self.device, &self.queue, scene, &texture, base, AaConfig::Msaa8)?;
        self.read_back(&texture)
    }

    /// How many bands the last [`Self::render`] drew (1 when the frame fit
    /// vello's buffers whole).
    #[must_use]
    pub const fn last_bands(&self) -> u32 {
        self.last_bands
    }

    /// Render a document through the effects stage ([`crate::fx`]), in
    /// `frame.mode`. Antialiasing is MSAA 8×, like [`Self::render`].
    ///
    /// # Errors
    /// On GPU failures.
    pub fn render_with_effects(&mut self, frame: &crate::fx::Frame<'_>) -> Result<Pixels, String> {
        let frame = crate::fx::Frame { antialiasing: AaConfig::Msaa8, ..*frame };
        let stage = self.fx.get_or_insert_with(|| crate::fx::Stage::new(&self.device));
        let texture = stage.render(&self.device, &self.queue, &mut self.renderer, &frame)?;
        self.read_back(&texture)
    }

    /// Copy an `Rgba8Unorm` texture back to the CPU.
    fn read_back(&self, texture: &wgpu::Texture) -> Result<Pixels, String> {
        let rgba = read_rows(&self.device, &self.queue, texture, 0, texture.height())?;
        Ok(Pixels { width: texture.width(), height: texture.height(), rgba })
    }
}
