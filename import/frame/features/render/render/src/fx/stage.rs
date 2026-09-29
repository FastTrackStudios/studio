//! Running an [`EffectsPlan`] on the GPU around vello.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use document_proto::{NodeId, Scene};
use vello::{wgpu, AaConfig, RenderParams, Renderer};

use super::passes::{FxUniforms, Passes, LIGHT, TARGET};
use super::{flicker_level, pack_params, EmissiveDraw, Mode, ShaderRegion};
use crate::anyrender_renderer::{
    build_scene_primitives_and_effects, build_scene_primitives_with, paint_primitives_into_scene_with,
    AffineTransform2D, CollectOptions, PaintPrimitive, TextFontRef,
};

/// One frame to render through the effects stage.
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    pub doc: &'a Scene,
    /// The subtree to paint (usually a page).
    pub root: NodeId,
    /// Scene → target px.
    pub transform: kurbo::Affine,
    pub width: u32,
    pub height: u32,
    /// The colour under everything.
    pub base: peniko::Color,
    pub font: Option<TextFontRef<'a>>,
    pub mode: Mode,
    /// Seconds, for time-dependent shaders (`flicker`, custom ones).
    pub time_s: f64,
    /// Control positions (0..1) by control node, for a shader's `value`.
    pub values: Option<&'a HashMap<NodeId, f64>>,
    /// Shader nodes' live inputs this frame (see `CONTRACT.md`, "Inputs").
    pub inputs: Option<&'a super::FxInputs>,
    /// Painted last, over everything (a status banner), in target px.
    pub overlay: Option<&'a vello::Scene>,
    pub antialiasing: AaConfig,
}

impl<'a> Frame<'a> {
    /// `root` of `doc` at `width × height`, identity transform, black base,
    /// GPU effects at t = 0.
    #[must_use]
    pub fn new(doc: &'a Scene, root: NodeId, width: u32, height: u32) -> Self {
        Self {
            doc,
            root,
            transform: kurbo::Affine::IDENTITY,
            width,
            height,
            base: peniko::Color::from_rgba8(0, 0, 0, 255),
            font: None,
            mode: Mode::Gpu,
            time_s: 0.0,
            values: None,
            inputs: None,
            overlay: None,
            antialiasing: AaConfig::Area,
        }
    }
}

/// The effects stage's GPU state: pipelines (with the compiled-shader cache)
/// and intermediate textures, reused across frames.
pub struct Stage {
    passes: Passes,
    textures: HashMap<String, wgpu::Texture>,
    scene: vello::Scene,
    time_dependent: bool,
}

impl Stage {
    #[must_use]
    pub fn new(device: &wgpu::Device) -> Self {
        Self { passes: Passes::new(device), textures: HashMap::new(), scene: vello::Scene::new(), time_dependent: false }
    }

    /// Whether the last frame rendered had a time-dependent effect (a
    /// `flicker`, a custom shader reading `time_s`): hosts keep redrawing.
    #[must_use]
    pub fn time_dependent(&self) -> bool {
        self.time_dependent
    }

    /// Render `frame`; the result is an `Rgba8Unorm` texture (straight alpha,
    /// like vello's output) owned by the stage, valid until the next call.
    /// It can be sampled, copied from and used as a render attachment.
    ///
    /// # Errors
    /// When vello fails to render.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut Renderer,
        frame: &Frame<'_>,
    ) -> Result<wgpu::Texture, String> {
        let (w, h) = (frame.width.clamp(1, 8192), frame.height.clamp(1, 8192));
        let out = self.texture(device, "out", w, h, TARGET);
        let mut scene = std::mem::take(&mut self.scene);
        self.time_dependent = false;
        let result = match frame.mode {
            Mode::Fallback | Mode::Off => {
                let options = CollectOptions { emission_glow: frame.mode == Mode::Fallback };
                let prims = build_scene_primitives_with(frame.doc, frame.root, options);
                paint(&mut scene, &prims, frame, frame.overlay);
                vello_into(renderer, device, queue, &scene, &out, frame.base, frame.antialiasing)
            }
            Mode::Gpu => self.render_gpu(device, queue, renderer, frame, &mut scene, &out),
        };
        self.scene = scene;
        result.map(|()| out)
    }

    fn render_gpu(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut Renderer,
        frame: &Frame<'_>,
        scene: &mut vello::Scene,
        out: &wgpu::Texture,
    ) -> Result<(), String> {
        let (prims, plan) = build_scene_primitives_and_effects(frame.doc, frame.root);
        self.time_dependent = plan.time_dependent(|id| super::effect_source(frame.doc, id));
        // Shaders that fail to compile draw plainly: they stay in the main
        // segments.
        let regions: Vec<&ShaderRegion> =
            plan.shaders.iter().filter(|r| self.passes.effect(device, frame.doc, &r.shader).is_some()).collect();
        let (w, h) = (out.width(), out.height());
        let out_view = out.create_view(&wgpu::TextureViewDescriptor::default());
        let clear = peniko::Color::TRANSPARENT;
        let aa = frame.antialiasing;

        if regions.is_empty() {
            paint(scene, &prims, frame, frame.overlay);
            vello_into(renderer, device, queue, scene, out, frame.base, aa)?;
        } else {
            let scratch = self.texture(device, "scratch", w, h, TARGET);
            let scratch_view = scratch.create_view(&wgpu::TextureViewDescriptor::default());
            let mut cursor = 0;
            for (i, r) in regions.iter().enumerate() {
                let seg = slice_with_context(&prims, cursor..r.range.start);
                if i == 0 {
                    paint(scene, &seg, frame, None);
                    vello_into(renderer, device, queue, scene, out, frame.base, aa)?;
                } else if has_draws(&seg) {
                    paint(scene, &seg, frame, None);
                    vello_into(renderer, device, queue, scene, &scratch, clear, aa)?;
                    let mut enc = encoder(device);
                    self.passes.blit_over(device, &mut enc, &scratch_view, &out_view);
                    queue.submit([enc.finish()]);
                }
                let sub = slice_with_context(&prims, r.range.clone());
                paint(scene, &sub, frame, None);
                vello_into(renderer, device, queue, scene, &scratch, clear, aa)?;
                if let Some(pipeline) = self.passes.cached_effect(&r.shader) {
                    let (mut uniforms, box_px) = region_uniforms(r, frame, w, h);
                    let data = frame.inputs.and_then(|i| i.get(&r.node_id));
                    let (table, values) = super::pack_inputs(self.passes.effect_slots(&r.shader), &r.inputs, data);
                    uniforms.inputs = table;
                    let mut enc = encoder(device);
                    // The effect runs over the node's box only; the rest of
                    // the subtree (children overflowing it) draws plainly.
                    self.passes.effect_pass(device, &mut enc, pipeline, &uniforms, &values, &scratch_view, &out_view, Some(box_px));
                    self.passes.blit_over_in(device, &mut enc, &scratch_view, &out_view, &outside(box_px, w, h));
                    queue.submit([enc.finish()]);
                }
                cursor = r.range.end;
            }
            let tail = slice_with_context(&prims, cursor..prims.len());
            if has_draws(&tail) || frame.overlay.is_some() {
                paint(scene, &tail, frame, frame.overlay);
                vello_into(renderer, device, queue, scene, &scratch, clear, aa)?;
                let mut enc = encoder(device);
                self.passes.blit_over(device, &mut enc, &scratch_view, &out_view);
                queue.submit([enc.finish()]);
            }
        }

        self.bloom(device, queue, renderer, frame, scene, &plan.emissive, &out_view, w, h)
    }

    /// Emissive draws, bucketed by radius: each bucket renders at half
    /// resolution, blurs at a tight and a wide radius, and adds over `out`.
    #[allow(clippy::too_many_arguments)]
    fn bloom(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut Renderer,
        frame: &Frame<'_>,
        scene: &mut vello::Scene,
        emissive: &[EmissiveDraw],
        out_view: &wgpu::TextureView,
        w: u32,
        h: u32,
    ) -> Result<(), String> {
        let mut buckets: BTreeMap<i64, Vec<(&EmissiveDraw, f64)>> = BTreeMap::new();
        for d in emissive {
            let s = d.strength * d.flicker.as_ref().map_or(1.0, |p| flicker_level(frame.time_s, p));
            if s > 0.0 && d.radius > 0.0 {
                buckets.entry((d.radius * 2.0).round() as i64).or_default().push((d, s));
            }
        }
        if buckets.is_empty() {
            return Ok(());
        }
        let scale = frame.transform.determinant().abs().sqrt();
        let (ew, eh) = (w.div_ceil(2), h.div_ceil(2));
        let emit = self.texture(device, "emit", ew, eh, TARGET);
        let emit_view = emit.create_view(&wgpu::TextureViewDescriptor::default());
        let to_emit = kurbo::Affine::scale_non_uniform(f64::from(ew) / f64::from(w), f64::from(eh) / f64::from(h)) * frame.transform;
        let clip = kurbo::Rect::new(0.0, 0.0, f64::from(ew), f64::from(eh));
        for (key, draws) in buckets {
            let radius = key as f64 / 2.0;
            let max_s = draws.iter().map(|(_, s)| *s).fold(0.0, f64::max);
            scene.reset();
            for (d, s) in &draws {
                scene.push_layer(peniko::Fill::NonZero, peniko::Mix::Normal, (s / max_s) as f32, kurbo::Affine::IDENTITY, &clip);
                for (world, c) in &d.clips {
                    push_clip(scene, c, to_emit * to_kurbo(world));
                }
                paint_primitives_into_scene_with(scene, &d.primitives, to_emit * to_kurbo(&d.world), frame.font);
                for _ in &d.clips {
                    scene.pop_layer();
                }
                scene.pop_layer();
            }
            vello_into(renderer, device, queue, scene, &emit, peniko::Color::TRANSPARENT, frame.antialiasing)?;
            // Emission radius is a CSS-style blur radius (≈ 2σ), in scene px.
            let sigma = radius * 0.5 * scale * f64::from(ew) / f64::from(w);
            for (sigma, weight) in [(sigma * 0.3, 0.9), (sigma, 0.9)] {
                self.blur_add(device, queue, &emit_view, (ew, eh), sigma as f32, (max_s * weight) as f32, out_view);
            }
        }
        Ok(())
    }

    /// Blur the straight-alpha light in `src` by `sigma` texels (downsampling
    /// while it's wide) and add it over `dst` × `gain`.
    #[allow(clippy::too_many_arguments)]
    fn blur_add(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src: &wgpu::TextureView,
        (mut lw, mut lh): (u32, u32),
        mut sigma: f32,
        gain: f32,
        dst: &wgpu::TextureView,
    ) {
        const MAX_SIGMA: f32 = 6.0;
        let mut enc = encoder(device);
        let step = if sigma > MAX_SIGMA && lw > 8 && lh > 8 { 2 } else { 1 };
        (lw, lh, sigma) = (lw.div_ceil(step), lh.div_ceil(step), sigma / step as f32);
        let mut level_view =
            self.texture(device, &format!("lvl-{lw}x{lh}"), lw, lh, LIGHT).create_view(&wgpu::TextureViewDescriptor::default());
        self.passes.down(device, &mut enc, src, &level_view, step, true);
        while sigma > MAX_SIGMA && lw > 8 && lh > 8 {
            (lw, lh, sigma) = (lw.div_ceil(2), lh.div_ceil(2), sigma / 2.0);
            let next_view =
                self.texture(device, &format!("lvl-{lw}x{lh}"), lw, lh, LIGHT).create_view(&wgpu::TextureViewDescriptor::default());
            self.passes.down(device, &mut enc, &level_view, &next_view, 2, false);
            level_view = next_view;
        }
        let a = self.texture(device, &format!("blur-a-{lw}x{lh}"), lw, lh, LIGHT);
        let b = self.texture(device, &format!("blur-b-{lw}x{lh}"), lw, lh, LIGHT);
        let (a_view, b_view) = (
            a.create_view(&wgpu::TextureViewDescriptor::default()),
            b.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        self.passes.blur(device, &mut enc, &level_view, &a_view, [1.0, 0.0], sigma);
        self.passes.blur(device, &mut enc, &a_view, &b_view, [0.0, 1.0], sigma);
        self.passes.bloom(device, &mut enc, &b_view, src, dst, gain, BLOOM_INSIDE);
        queue.submit([enc.finish()]);
    }

    /// A cached texture by role and size (recreated when either changes).
    fn texture(&mut self, device: &wgpu::Device, key: &str, w: u32, h: u32, format: wgpu::TextureFormat) -> wgpu::Texture {
        if let Some(t) = self.textures.get(key)
            && t.width() == w
            && t.height() == h
            && t.format() == format
        {
            return t.clone();
        }
        if self.textures.len() > 64 {
            // Window resizes leave per-size levels behind.
            self.textures.clear();
        }
        let usage = if format == TARGET {
            wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
        } else {
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT
        };
        let t = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame-fx"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        });
        self.textures.insert(key.to_string(), t.clone());
        t
    }
}

/// How much of the bloom lands on the emitters themselves (the rest is the
/// halo around them): large lit areas (a backlit VU face) keep their detail.
const BLOOM_INSIDE: f32 = 0.2;

fn encoder(device: &wgpu::Device) -> wgpu::CommandEncoder {
    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame-fx") })
}

fn to_kurbo(t: &AffineTransform2D) -> kurbo::Affine {
    kurbo::Affine::new([t.a, t.c, t.b, t.d, t.tx, t.ty])
}

fn paint(scene: &mut vello::Scene, prims: &[PaintPrimitive], frame: &Frame<'_>, overlay: Option<&vello::Scene>) {
    scene.reset();
    paint_primitives_into_scene_with(scene, prims, frame.transform, frame.font);
    if let Some(o) = overlay {
        scene.append(o, None);
    }
}

fn vello_into(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &vello::Scene,
    target: &wgpu::Texture,
    base: peniko::Color,
    antialiasing: AaConfig,
) -> Result<(), String> {
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    renderer
        .render_to_texture(
            device,
            queue,
            scene,
            &view,
            &RenderParams { base_color: base, width: target.width(), height: target.height(), antialiasing_method: antialiasing },
        )
        .map_err(|e| format!("render_to_texture failed: {e}"))
}

/// The uniform block for one shader region, and the region's box in target
/// px (`[x, y, w, h]`, clamped to the target) — where its effect runs.
fn region_uniforms(r: &ShaderRegion, frame: &Frame<'_>, w: u32, h: u32) -> (FxUniforms, [u32; 4]) {
    let to_px = frame.transform * to_kurbo(&r.world);
    let to_local = if to_px.determinant().abs() > 1e-12 { to_px.inverse() } else { kurbo::Affine::IDENTITY };
    let rows = |t: kurbo::Affine| {
        let c = t.as_coeffs();
        [[c[0] as f32, c[2] as f32, c[4] as f32, 0.0], [c[1] as f32, c[3] as f32, c[5] as f32, 0.0]]
    };
    let value = r
        .control
        .and_then(|(id, default)| frame.values.and_then(|v| v.get(&id)).copied().or(default))
        .unwrap_or(0.0);
    let (params, param_mask) = pack_params(&r.params);
    let corners = [(0.0, 0.0), (r.width, 0.0), (0.0, r.height), (r.width, r.height)].map(|p| to_px * kurbo::Point::from(p));
    let (x0, y0) = corners.iter().fold((f64::INFINITY, f64::INFINITY), |(x, y), p| (x.min(p.x), y.min(p.y)));
    let (x1, y1) = corners.iter().fold((f64::NEG_INFINITY, f64::NEG_INFINITY), |(x, y), p| (x.max(p.x), y.max(p.y)));
    let clamp = |v: f64, hi: u32| if v.is_finite() { v.clamp(0.0, f64::from(hi)) as u32 } else { 0 };
    let (bx0, by0) = (clamp(x0.floor() - 1.0, w), clamp(y0.floor() - 1.0, h));
    let (bx1, by1) = (clamp(x1.ceil() + 1.0, w), clamp(y1.ceil() + 1.0, h));
    let box_px = [bx0, by0, bx1.saturating_sub(bx0), by1.saturating_sub(by0)];
    let uniforms = FxUniforms {
        time_s: frame.time_s as f32,
        value: value as f32,
        px_per_unit: to_px.determinant().abs().sqrt() as f32,
        param_mask,
        resolution: [w as f32, h as f32],
        node_size: [r.width as f32, r.height as f32],
        to_local: rows(to_local),
        to_px: rows(to_px),
        params,
        inputs: [[0; 4]; 8],
    };
    (uniforms, box_px)
}

/// The target outside `[x, y, w, h]`, as up to four bands.
fn outside([x, y, bw, bh]: [u32; 4], w: u32, h: u32) -> [[u32; 4]; 4] {
    let (x1, y1) = ((x + bw).min(w), (y + bh).min(h));
    [[0, 0, w, y], [0, y1, w, h.saturating_sub(y1)], [0, y, x, y1 - y], [x1, y, w.saturating_sub(x1), y1 - y]]
}

fn has_draws(prims: &[PaintPrimitive]) -> bool {
    prims.iter().any(|p| matches!(p, PaintPrimitive::Rect { .. } | PaintPrimitive::Path { .. } | PaintPrimitive::Text { .. }))
}

/// Open a `ClipStart`'s clip on `scene` under `transform` (other primitives:
/// an unbounded clip, so the caller's pops stay balanced).
fn push_clip(scene: &mut vello::Scene, clip: &PaintPrimitive, transform: kurbo::Affine) {
    let PaintPrimitive::ClipStart { x, y, width, height, corner_radii, .. } = clip else {
        scene.push_clip_layer(peniko::Fill::NonZero, kurbo::Affine::IDENTITY, &kurbo::Rect::new(-1e9, -1e9, 1e9, 1e9));
        return;
    };
    let rect = kurbo::Rect::new(*x, *y, x + width, y + height);
    let r = corner_radii;
    let rr = kurbo::RoundedRect::from_rect(rect, kurbo::RoundedRectRadii::new(r.top_left, r.top_right, r.bottom_right, r.bottom_left));
    scene.push_clip_layer(peniko::Fill::NonZero, transform, &rr);
}

/// `prims[range]` as a self-contained list: the groups, layers, clips and
/// masks open at `range.start` are reopened before it, and everything still
/// open at its end is closed.
pub(super) fn slice_with_context(prims: &[PaintPrimitive], range: Range<usize>) -> Vec<PaintPrimitive> {
    enum Open {
        One(usize),
        Mask { def: Option<Range<usize>>, apply: usize },
    }
    let range = range.start.min(prims.len())..range.end.min(prims.len());
    let mut stack: Vec<Open> = Vec::new();
    let mut last_def: Option<Range<usize>> = None;
    let mut i = 0;
    while i < range.start {
        match &prims[i] {
            PaintPrimitive::GroupStart { .. } | PaintPrimitive::LayerStart { .. } | PaintPrimitive::ClipStart { .. } => {
                stack.push(Open::One(i));
            }
            PaintPrimitive::GroupEnd { .. }
            | PaintPrimitive::LayerEnd { .. }
            | PaintPrimitive::ClipEnd { .. }
            | PaintPrimitive::MaskApplyEnd { .. } => {
                stack.pop();
            }
            PaintPrimitive::MaskDefStart { .. } => {
                let mut depth = 0_usize;
                let mut end = i;
                for (j, p) in prims.iter().enumerate().skip(i) {
                    match p {
                        PaintPrimitive::MaskDefStart { .. } => depth += 1,
                        PaintPrimitive::MaskDefEnd { .. } => {
                            depth -= 1;
                            if depth == 0 {
                                end = j;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                last_def = Some(i..end + 1);
                i = end + 1;
                continue;
            }
            PaintPrimitive::MaskApplyStart { .. } => stack.push(Open::Mask { def: last_def.take(), apply: i }),
            _ => {}
        }
        i += 1;
    }

    let mut out = Vec::new();
    for o in &stack {
        match o {
            Open::One(i) => out.push(prims[*i].clone()),
            Open::Mask { def, apply } => {
                if let Some(d) = def {
                    out.extend_from_slice(&prims[d.clone()]);
                }
                out.push(prims[*apply].clone());
            }
        }
    }
    out.extend_from_slice(&prims[range]);

    let mut closers = Vec::new();
    let mut in_def = 0_usize;
    for p in &out {
        match p {
            PaintPrimitive::MaskDefStart { .. } => in_def += 1,
            PaintPrimitive::MaskDefEnd { .. } => in_def = in_def.saturating_sub(1),
            _ if in_def > 0 => {}
            PaintPrimitive::GroupStart { node_id, .. } => closers.push(PaintPrimitive::GroupEnd { node_id: *node_id }),
            PaintPrimitive::LayerStart { node_id, .. } => closers.push(PaintPrimitive::LayerEnd { node_id: *node_id }),
            PaintPrimitive::ClipStart { node_id, .. } => closers.push(PaintPrimitive::ClipEnd { node_id: *node_id }),
            PaintPrimitive::MaskApplyStart { node_id, .. } => closers.push(PaintPrimitive::MaskApplyEnd { node_id: *node_id }),
            PaintPrimitive::GroupEnd { .. }
            | PaintPrimitive::LayerEnd { .. }
            | PaintPrimitive::ClipEnd { .. }
            | PaintPrimitive::MaskApplyEnd { .. } => {
                closers.pop();
            }
            _ => {}
        }
    }
    out.extend(closers.into_iter().rev());
    out
}
