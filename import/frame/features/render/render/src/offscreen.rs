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
        Ok(Self { device, queue, renderer })
    }

    /// Render `scene` at `width × height` over `base`.
    ///
    /// # Errors
    /// On GPU failures.
    pub fn render(&mut self, scene: &vello::Scene, width: u32, height: u32, base: peniko::Color) -> Result<Pixels, String> {
        let (width, height) = (width.clamp(1, 8192), height.clamp(1, 8192));
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame-offscreen-target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
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
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                scene,
                &view,
                &RenderParams { base_color: base, width, height, antialiasing_method: AaConfig::Msaa8 },
            )
            .map_err(|e| format!("render_to_texture failed: {e}"))?;

        let unpadded = width * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame-offscreen-readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame-offscreen-copy") });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(height) },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);

        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |v| {
            let _ = tx.send(v);
        });
        loop {
            let _ = self.device.poll(wgpu::PollType::Poll);
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
        let mut rgba = vec![0_u8; (unpadded * height) as usize];
        for row in 0..height as usize {
            let src = row * padded as usize;
            let dst = row * unpadded as usize;
            rgba[dst..dst + unpadded as usize].copy_from_slice(&mapped[src..src + unpadded as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(Pixels { width, height, rgba })
    }
}
