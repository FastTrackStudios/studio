//! A frame surface running live inside a host's renderer.
//!
//! [`LiveSurface`] is what a host with a GPU reaches for: it loads a page
//! from `.fm` markup (plus its catalog), watches both files and swaps a new
//! design in place when either changes — values and a drag in progress
//! survive — runs the surface's controls, and renders through the effects
//! stage on the host's own wgpu device into a texture the host composites.
//!
//! Coordinates in are the host's element pixels (logical); the surface is
//! fitted into the element uniformly and centred, like the desktop player
//! fits it into its window.
//!
//! Edits come out as [`EngineCall`]s ([`LiveSurface::take_calls`]); with no
//! engine answering, each edit is echoed back so the control follows the
//! hand. A host with an engine feeds the engine's values in with
//! [`LiveSurface::apply_values`].

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use document_proto::{NodeId, PaintKind, Scene};
use surface_proto::{Catalog, ParamAddr, ParamValue, Source as Binding};
use surface_runtime::{Bounds, EngineCall, Input, Runtime, Surface};
use vello::wgpu;
use vello::{AaConfig, Renderer, RendererOptions};

pub use render::fx::Mode;
pub use surface_runtime::EngineCall as Call;
pub use surface_runtime::Modifiers;

/// How often to look at the files.
const POLL: Duration = Duration::from_millis(250);
/// Max gap and travel between two presses of a double-click.
const DOUBLE_CLICK_MS: f64 = 350.0;
const DOUBLE_CLICK_PX: f64 = 4.0;
/// Under a page with no solid fill.
const DARK: peniko::Color = peniko::Color::from_rgb8(0x0e, 0x0f, 0x12);

/// Fit of the surface's content into the element: surface → element px.
#[derive(Debug, Clone, Copy, PartialEq)]
struct View {
    scale: f64,
    tx: f64,
    ty: f64,
}

impl View {
    fn fit(content: Bounds, width: f64, height: f64) -> Self {
        if content.width <= 0.0 || content.height <= 0.0 || width <= 0.0 || height <= 0.0 {
            return Self { scale: 1.0, tx: -content.x, ty: -content.y };
        }
        let scale = (width / content.width).min(height / content.height);
        Self {
            scale,
            tx: (width - content.width * scale) / 2.0 - content.x * scale,
            ty: (height - content.height * scale) / 2.0 - content.y * scale,
        }
    }

    fn to_surface(self, x: f64, y: f64) -> (f64, f64) {
        ((x - self.tx) / self.scale, (y - self.ty) / self.scale)
    }

    fn affine(self) -> kurbo::Affine {
        kurbo::Affine::new([self.scale, 0.0, 0.0, self.scale, self.tx, self.ty])
    }
}

/// The GPU half, on the host's device.
struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    stage: render::fx::Stage,
    /// The host's texture, which the frame is copied into.
    target: Option<(wgpu::Texture, (u32, u32))>,
}

/// One surface, live.
pub struct LiveSurface {
    markup: PathBuf,
    catalog_path: Option<PathBuf>,
    stamps: (Option<u64>, Option<u64>),
    next_check: Instant,
    rt: Runtime,
    page: NodeId,
    content: Bounds,
    background: peniko::Color,
    mode: Mode,
    gpu: Option<Gpu>,
    born: Instant,
    /// Element px → surface, as of the last render (logical px).
    view: View,
    cursor: Option<(f64, f64)>,
    last_press: Option<(f64, f64, f64)>,
    dragging: Option<ParamAddr>,
    calls: Vec<EngineCall>,
    /// Something changed since the last render.
    dirty: bool,
    time_dependent: bool,
    error: Option<String>,
}

fn stamp(path: &Path) -> Option<u64> {
    document_io::fingerprint(path)
}

fn load(markup: &Path, catalog: Option<&Path>) -> Result<(Surface, Catalog), String> {
    let scene = document_io::load_surface(markup).map_err(|e| format!("{}: {e}", markup.display()))?;
    let catalog = match catalog {
        Some(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            serde_json::from_str::<Catalog>(&text).map_err(|e| format!("{}: {e}", p.display()))?
        }
        None => Catalog::default(),
    };
    Ok((Surface::from_scene(scene), catalog))
}

fn bounds(b: &render::LayoutBox) -> Bounds {
    Bounds { x: b.x, y: b.y, width: b.width, height: b.height }
}

fn union(boxes: impl Iterator<Item = Bounds>) -> Bounds {
    boxes
        .reduce(|a, b| {
            let (x, y) = (a.x.min(b.x), a.y.min(b.y));
            Bounds { x, y, width: (a.x + a.width).max(b.x + b.width) - x, height: (a.y + a.height).max(b.y + b.height) - y }
        })
        .unwrap_or_default()
}

fn solid_fill(scene: &Scene, id: NodeId) -> Option<peniko::Color> {
    scene.node(id)?.fills.iter().rev().find(|p| p.visible).and_then(|p| match p.kind {
        PaintKind::Solid { color: c } => {
            let u8 = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            Some(peniko::Color::from_rgb8(u8(c.r), u8(c.g), u8(c.b)))
        }
        _ => None,
    })
}

impl LiveSurface {
    /// Open `markup` (a `.fm` file with its `.fm.assets/`) against the
    /// catalog at `catalog` (its params' ranges and defaults).
    ///
    /// # Errors
    /// When either file cannot be read, or the surface has no page.
    pub fn open(markup: impl Into<PathBuf>, catalog: Option<PathBuf>) -> Result<Self, String> {
        let markup = markup.into();
        let (surface, cat) = load(&markup, catalog.as_deref())?;
        let stamps = (stamp(&markup), catalog.as_deref().and_then(stamp));
        let mut me = Self {
            markup,
            catalog_path: catalog,
            stamps,
            next_check: Instant::now() + POLL,
            rt: Runtime::new(Surface::from_scene(Scene::new("")), Catalog::default()),
            page: NodeId::default(),
            content: Bounds::default(),
            background: DARK,
            mode: Mode::Gpu,
            gpu: None,
            born: Instant::now(),
            view: View { scale: 1.0, tx: 0.0, ty: 0.0 },
            cursor: None,
            last_press: None,
            dragging: None,
            calls: Vec::new(),
            dirty: true,
            time_dependent: false,
            error: None,
        };
        me.install(Runtime::new(surface, cat))?;
        Ok(me)
    }

    /// Lay out a runtime and make it the one running.
    fn install(&mut self, mut rt: Runtime) -> Result<(), String> {
        rt.set_text_measure(std::sync::Arc::new(|data: &document_proto::TextData, text: &str| {
            render::text_advance(render::FontBook::bundled(), &render::fonts::text_font(data), text, data.font_size, data.letter_spacing)
        }));
        let scene = &rt.surface().scene;
        let page = *scene.pages().first().ok_or("surface has no <Page>")?;
        let boxes = render::build_scene_layout_boxes(scene, page);
        let tops = scene.node(page).map(|p| p.children.clone()).unwrap_or_default();
        self.content = union(tops.iter().filter_map(|id| boxes.get(id)).map(bounds));
        self.background = tops.first().and_then(|id| solid_fill(scene, *id)).unwrap_or(DARK);
        rt.set_boxes(boxes.iter().map(|(id, b)| (*id, bounds(b))).collect::<Vec<_>>());
        self.page = page;
        self.rt = rt;
        self.dirty = true;
        Ok(())
    }

    /// The effects mode (GPU by default).
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.dirty = true;
    }

    /// The surface's content size (its page's frames), surface units.
    #[must_use]
    pub fn content_size(&self) -> (f64, f64) {
        (self.content.width, self.content.height)
    }

    /// Why the last reload failed, while it stays failed.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Look at the files (at most every 250 ms) and swap in a changed
    /// design, keeping values and a drag on a control that is still there.
    /// True when something was swapped in.
    pub fn poll_reload(&mut self) -> bool {
        let now = Instant::now();
        if now < self.next_check {
            return false;
        }
        self.next_check = now + POLL;
        let stamps = (stamp(&self.markup), self.catalog_path.as_deref().and_then(stamp));
        if stamps == self.stamps || stamps.0.is_none() {
            return false;
        }
        self.stamps = stamps;
        match load(&self.markup, self.catalog_path.as_deref()) {
            Ok((surface, catalog)) => {
                if surface.scene.pages().is_empty() {
                    self.error = Some("surface has no <Page>".into());
                    return false;
                }
                let placeholder = Runtime::new(Surface::from_scene(Scene::new("")), Catalog::default());
                let mut rt = std::mem::replace(&mut self.rt, placeholder);
                if catalog != *rt.catalog() {
                    // New params (a regenerated page): start from the old
                    // values where the addresses carry over.
                    let values = rt.values();
                    rt = Runtime::new(surface, catalog);
                    rt.apply_values(values);
                } else {
                    rt.swap_surface(surface);
                }
                match self.install(rt) {
                    Ok(()) => {
                        self.error = None;
                        true
                    }
                    Err(e) => {
                        self.error = Some(e);
                        false
                    }
                }
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    /// Values from the engine (normalized 0..1). A param being dragged
    /// keeps the hand's value.
    pub fn apply_values(&mut self, values: impl IntoIterator<Item = ParamValue>) {
        let dragging = self.dragging.clone();
        self.rt.apply_values(values.into_iter().filter(|v| dragging.as_ref() != Some(&v.addr)));
        self.dirty = true;
    }

    /// The calls the surface made since the last take (param edits,
    /// actions), for the host to send.
    pub fn take_calls(&mut self) -> Vec<EngineCall> {
        std::mem::take(&mut self.calls)
    }

    /// Values from the engine in their own units (Hz, dB, 0..1…), by
    /// address; each is normalized through the surface catalog's range.
    /// Addresses the catalog does not know are ignored.
    pub fn apply_real(&mut self, values: impl IntoIterator<Item = (String, f64)>) {
        let normalized: Vec<ParamValue> = values
            .into_iter()
            .filter_map(|(addr, real)| {
                let addr = ParamAddr::new(addr);
                let info = self.rt.catalog().param(&addr)?;
                Some(ParamValue { value: info.range.to_normalized(real), addr })
            })
            .collect();
        self.apply_values(normalized);
    }

    /// The param edits made since the last take, as (address, value in the
    /// param's own units) — the catalog's range turns the surface's
    /// normalized value back into Hz, dB, 0..1…
    pub fn take_edits(&mut self) -> Vec<(String, f64)> {
        let calls = std::mem::take(&mut self.calls);
        calls
            .into_iter()
            .filter_map(|c| match c {
                EngineCall::SetParam { addr, value, .. } => {
                    let real = self.rt.catalog().param(&addr).map_or(value, |p| p.range.to_real(value));
                    Some((addr.to_string(), real))
                }
                _ => None,
            })
            .collect()
    }

    /// The params the surface's catalog knows, by address.
    #[must_use]
    pub fn addresses(&self) -> Vec<String> {
        self.rt.catalog().params.iter().map(|p| p.addr.to_string()).collect()
    }

    /// Whether the host should render again: something changed, a control
    /// is animating, or a shader reads the clock.
    #[must_use]
    pub fn needs_redraw(&self) -> bool {
        self.dirty || self.time_dependent || self.rt.animating()
    }

    fn t(&self) -> f64 {
        self.born.elapsed().as_secs_f64() * 1000.0
    }

    fn dispatch(&mut self, calls: Vec<EngineCall>) {
        if calls.is_empty() {
            return;
        }
        // No engine answers here: echo the edits so the controls follow.
        let echoed: Vec<ParamValue> = calls
            .iter()
            .filter_map(|c| match c {
                EngineCall::SetParam { addr, value, .. } => Some(ParamValue { addr: addr.clone(), value: *value }),
                _ => None,
            })
            .collect();
        self.rt.apply_values(echoed);
        self.calls.extend(calls);
        self.dirty = true;
    }

    fn param_of(&self, id: NodeId) -> Option<ParamAddr> {
        let c = self.rt.surface().controls.get(&id)?;
        match c.binding.as_ref().map(|b| &b.source) {
            Some(Binding::Param { addr }) => Some(addr.clone()),
            _ => None,
        }
    }

    // ── input, in element px (logical) ──────────────────────────────────

    pub fn pointer_down(&mut self, x: f64, y: f64, mods: Modifiers) {
        let t = self.t();
        let at = self.view.to_surface(x, y);
        self.cursor = Some(at);
        let double = self.last_press.is_some_and(|(lx, ly, lt)| t - lt <= DOUBLE_CLICK_MS && (x - lx).hypot(y - ly) <= DOUBLE_CLICK_PX);
        self.last_press = if double { None } else { Some((x, y, t)) };
        let hit = self.rt.hit(at.0, at.1);
        self.dragging = hit.and_then(|id| self.param_of(id));
        let mut calls = self.rt.pointer(Input::Down { x: at.0, y: at.1, t, mods }, t);
        if double {
            calls.extend(self.rt.pointer(Input::DoubleClick, t));
        }
        self.dispatch(calls);
        self.dirty = true;
    }

    pub fn pointer_move(&mut self, x: f64, y: f64, mods: Modifiers) {
        let t = self.t();
        let at = self.view.to_surface(x, y);
        self.cursor = Some(at);
        let calls = self.rt.pointer(Input::Move { x: at.0, y: at.1, t, mods }, t);
        if self.dragging.is_some() || !calls.is_empty() {
            self.dirty = true;
        }
        self.dispatch(calls);
    }

    pub fn pointer_up(&mut self) {
        let t = self.t();
        let (x, y) = self.cursor.unwrap_or_default();
        let calls = self.rt.pointer(Input::Up { x, y, t }, t);
        self.dragging = None;
        self.dispatch(calls);
        self.dirty = true;
    }

    pub fn cancel(&mut self) {
        let t = self.t();
        let calls = self.rt.pointer(Input::Cancel { t }, t);
        self.dragging = None;
        self.dispatch(calls);
    }

    /// Wheel over the control under the cursor (notches: + is up).
    pub fn wheel(&mut self, x: f64, y: f64, notches: f64, mods: Modifiers) {
        let t = self.t();
        let at = self.view.to_surface(x, y);
        self.cursor = Some(at);
        let Some(id) = self.rt.hit(at.0, at.1) else { return };
        let calls = self.rt.feed(id, Input::Wheel { notches, mods }, t);
        self.dispatch(calls);
    }

    // ── drawing ─────────────────────────────────────────────────────────

    /// Take the host's device (once it has one).
    ///
    /// # Errors
    /// When vello cannot build its pipelines on it.
    pub fn attach_gpu(&mut self, device: wgpu::Device, queue: wgpu::Queue) -> Result<(), String> {
        let renderer = Renderer::new(
            &device,
            RendererOptions { use_cpu: false, antialiasing_support: [AaConfig::Area].into_iter().collect(), num_init_threads: None, pipeline_cache: None },
        )
        .map_err(|e| format!("renderer: {e}"))?;
        let stage = render::fx::Stage::new(&device);
        self.gpu = Some(Gpu { device, queue, renderer, stage, target: None });
        self.dirty = true;
        Ok(())
    }

    /// The host's device went away (drop the textures).
    pub fn detach_gpu(&mut self) {
        self.gpu = None;
    }

    /// Whether a device is attached.
    #[must_use]
    pub fn has_gpu(&self) -> bool {
        self.gpu.is_some()
    }

    /// Render at `width × height` physical px (`scale` physical px per
    /// element px) into the texture this keeps for the host, and return
    /// it with whether it was (re)made — a new texture must be registered
    /// again. `Rgba8Unorm`, straight alpha, usable as a texture binding and
    /// a copy source.
    ///
    /// # Errors
    /// With no device attached, or when vello fails.
    pub fn render(&mut self, width: u32, height: u32, scale: f64) -> Result<(wgpu::Texture, bool), String> {
        let t = self.t();
        let calls = self.rt.poll(t);
        self.dispatch(calls);
        self.rt.set_clock(t / 1000.0);
        let (w, h) = (width.max(1), height.max(1));
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let physical = View::fit(self.content, f64::from(w), f64::from(h));
        self.view = View { scale: physical.scale / scale, tx: physical.tx / scale, ty: physical.ty / scale };
        let doc = self.rt.frame(t);
        let values = self.rt.surface().controls.iter().map(|(id, c)| (*id, self.rt.position(*id, c))).collect();
        let inputs = self.rt.shader_inputs();
        let gpu = self.gpu.as_mut().ok_or("no GPU attached")?;
        let frame = render::fx::Frame {
            transform: physical.affine(),
            base: self.background,
            fonts: Some(render::FontBook::bundled()),
            mode: self.mode,
            time_s: t / 1000.0,
            values: Some(&values),
            inputs: Some(&inputs),
            overlay: None,
            antialiasing: AaConfig::Area,
            ..render::fx::Frame::new(&doc, self.page, w, h)
        };
        let rendered = gpu.stage.render(&gpu.device, &gpu.queue, &mut gpu.renderer, &frame)?;
        self.time_dependent = gpu.stage.time_dependent();
        let fresh = !gpu.target.as_ref().is_some_and(|(_, size)| *size == (w, h));
        if fresh {
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("frame-live target"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            gpu.target = Some((texture, (w, h)));
        }
        let (target, _) = gpu.target.as_ref().expect("target made above");
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame-live copy") });
        encoder.copy_texture_to_texture(rendered.as_image_copy(), target.as_image_copy(), wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
        gpu.queue.submit(Some(encoder.finish()));
        self.dirty = false;
        Ok((target.clone(), fresh))
    }
}

/// Where a design lives: `<dir>/<name>.fm` with `<dir>/<plugin>.catalog.json`
/// beside it, as frame's plugin-faces generator writes them.
#[must_use]
pub fn face_paths(dir: &Path, name: &str) -> (PathBuf, Option<PathBuf>) {
    let markup = dir.join(format!("{name}.fm"));
    let catalog = std::fs::read_dir(dir)
        .ok()
        .and_then(|rd| rd.filter_map(Result::ok).map(|e| e.path()).find(|p| p.to_string_lossy().ends_with(".catalog.json")));
    (markup, catalog)
}
