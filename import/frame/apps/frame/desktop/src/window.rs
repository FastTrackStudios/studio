//! The winit + vello window around a [`Player`].

use std::sync::Arc;
use std::time::{Duration, Instant};

use document_proto::{NodeId, Scene};
use vello::wgpu::{self, PresentMode};
use vello::{AaConfig, RenderParams, Renderer, RendererOptions};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use crate::Args;
use crate::input::{self, View};
use crate::player::{Player, banner, font};
use crate::source::{self, Source, Watch};

pub fn run(args: Args) -> Result<(), String> {
    let (surface, error) = match args.source.load() {
        Ok(s) => (s, None),
        Err(e) => {
            eprintln!("{e}");
            (source::placeholder(), Some(e))
        }
    };
    let player = Player::new(surface_memory::Memory::demo(), surface)?;
    let now = Instant::now();
    let watch = args.source.path().map(|p| Watch::new(source::stamp(p), now));
    if let Some(p) = args.source.path() {
        eprintln!("frame: watching {} for changes", p.display());
    }
    let mut app = App {
        source: args.source,
        watch,
        player,
        error: None,
        banner: None,
        stats: args.stats.then(Stats::default),
        size: args.size,
        start: now,
        modifiers: ModifiersState::default(),
        cursor: None,
        context: vello::util::RenderContext::new(),
        gpu: None,
        scene: vello::Scene::new(),
    };
    app.set_error(error);
    let event_loop = EventLoop::new().map_err(|e| format!("event loop: {e}"))?;
    event_loop.run_app(&mut app).map_err(|e| format!("event loop: {e}"))
}

struct Gpu {
    window: Arc<Window>,
    surface: vello::util::RenderSurface<'static>,
    renderer: Renderer,
}

/// Frame-time accounting for `--stats`, printed every couple of seconds.
#[derive(Default)]
struct Stats {
    frames: u32,
    busy_ms: f64,
    since: Option<Instant>,
}

impl Stats {
    fn add(&mut self, busy: Duration, now: Instant) {
        let since = *self.since.get_or_insert(now);
        self.frames += 1;
        self.busy_ms += busy.as_secs_f64() * 1000.0;
        let wall = now - since;
        if wall >= Duration::from_secs(2) {
            eprintln!(
                "frame: {:.1} fps, avg frame {:.2} ms ({} frames)",
                f64::from(self.frames) / wall.as_secs_f64(),
                self.busy_ms / f64::from(self.frames),
                self.frames
            );
            *self = Self { since: Some(now), ..Self::default() };
        }
    }
}

struct App {
    source: Source,
    watch: Option<Watch>,
    player: Player,
    error: Option<String>,
    /// The error banner scene, rebuilt when the error or width changes.
    banner: Option<(Scene, NodeId, f64)>,
    stats: Option<Stats>,
    size: (u32, u32),
    start: Instant,
    modifiers: ModifiersState,
    /// Cursor, physical window px.
    cursor: Option<(f64, f64)>,
    context: vello::util::RenderContext,
    gpu: Option<Gpu>,
    scene: vello::Scene,
}

impl App {
    fn t(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    fn scale_factor(&self) -> f64 {
        self.gpu.as_ref().map_or(1.0, |g| g.window.scale_factor())
    }

    fn physical_size(&self) -> (f64, f64) {
        self.gpu
            .as_ref()
            .map_or((1.0, 1.0), |g| (f64::from(g.surface.config.width), f64::from(g.surface.config.height)))
    }

    fn view(&self) -> View {
        let (w, h) = self.physical_size();
        View::fit(self.player.content(), w, h)
    }

    fn redraw(&self) {
        if let Some(g) = &self.gpu {
            g.window.request_redraw();
        }
    }

    fn set_error(&mut self, error: Option<String>) {
        self.error = error;
        self.banner = None;
    }

    fn reload(&mut self) {
        let result = self.source.load().and_then(|s| self.player.swap(s));
        match result {
            Ok(()) => {
                eprintln!("frame: reloaded");
                self.set_error(None);
            }
            Err(e) => {
                eprintln!("{e}");
                self.set_error(Some(e));
            }
        }
        self.redraw();
    }

    fn create_gpu(&mut self, event_loop: &ActiveEventLoop) -> Result<Gpu, String> {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_inner_size(LogicalSize::new(self.size.0, self.size.1))
                        .with_title(match self.source.path() {
                            Some(p) => format!("frame — {}", p.display()),
                            None => "frame — demo surface".into(),
                        }),
                )
                .map_err(|e| format!("window: {e}"))?,
        );
        let size = window.inner_size();
        let surface = pollster::block_on(self.context.create_surface(
            window.clone(),
            size.width.max(1),
            size.height.max(1),
            PresentMode::AutoVsync,
        ))
        .map_err(|e| format!("gpu surface: {e}"))?;
        let renderer = Renderer::new(
            &self.context.devices[surface.dev_id].device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: [AaConfig::Area].into_iter().collect(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .map_err(|e| format!("renderer: {e}"))?;
        Ok(Gpu { window, surface, renderer })
    }

    /// Cursor in surface coordinates and in logical window px.
    fn cursor(&self) -> Option<((f64, f64), (f64, f64))> {
        let (x, y) = self.cursor?;
        let s = self.scale_factor();
        Some((self.view().to_surface(x, y), (x / s, y / s)))
    }

    fn render(&mut self) {
        let started = Instant::now();
        let t = self.t();
        self.player.tick(t);
        let view = self.view();
        let (width, _) = self.physical_size();
        let scale = self.scale_factor();
        if self.banner.as_ref().is_some_and(|b| b.2 != width) {
            self.banner = None;
        }
        if self.banner.is_none() {
            self.banner = self
                .error
                .as_deref()
                .and_then(|e| banner(&self.source.short_error(e), width / scale))
                .map(|(s, p)| (s, p, width));
        }

        self.scene.reset();
        self.player.paint(&mut self.scene, &view, t);
        if let Some((b, page, _)) = &self.banner {
            render::paint_scene_into(&mut self.scene, b, *page, kurbo::Affine::scale(scale), Some(font()));
        }

        let Some(gpu) = &mut self.gpu else { return };
        let device = &self.context.devices[gpu.surface.dev_id];
        let (w, h) = (gpu.surface.config.width, gpu.surface.config.height);
        let params = RenderParams {
            base_color: self.player.background(),
            width: w,
            height: h,
            antialiasing_method: AaConfig::Area,
        };
        if let Err(e) = gpu.renderer.render_to_texture(
            &device.device,
            &device.queue,
            &self.scene,
            &gpu.surface.target_view,
            &params,
        ) {
            eprintln!("frame: render failed: {e}");
            return;
        }
        let Ok(texture) = gpu.surface.surface.get_current_texture() else {
            // Outdated/lost surface (mid-resize): skip; the next frame reconfigures.
            return;
        };
        let mut encoder =
            device.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame blit") });
        gpu.surface.blitter.copy(
            &device.device,
            &mut encoder,
            &gpu.surface.target_view,
            &texture.texture.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        device.queue.submit([encoder.finish()]);
        texture.present();
        let _ = device.device.poll(wgpu::PollType::Poll);
        if let Some(stats) = &mut self.stats {
            stats.add(started.elapsed(), Instant::now());
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        match self.create_gpu(event_loop) {
            Ok(gpu) => {
                gpu.window.request_redraw();
                self.gpu = Some(gpu);
                eprintln!(
                    "frame: drag knobs/faders (Shift = fine), wheel, double-click or Ctrl/Cmd-click to reset, \
                     arrows/Page/Home/End on the last-clicked control, Esc quits"
                );
            }
            Err(e) => {
                eprintln!("frame: {e}");
                event_loop.exit();
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if let (Some(watch), Some(path)) = (&mut self.watch, self.source.path())
            && watch.due(now)
            && watch.observe(source::stamp(path), now)
        {
            self.reload();
        }
        if self.player.animating() {
            self.redraw();
        }
        event_loop.set_control_flow(match &self.watch {
            Some(w) => ControlFlow::WaitUntil(w.deadline()),
            None => ControlFlow::Wait,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.gpu.as_ref().is_none_or(|g| g.window.id() != id) {
            return;
        }
        let t = self.t();
        let mods = input::modifiers(self.modifiers);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => return self.render(),
            WindowEvent::Resized(size) => {
                if let Some(g) = &mut self.gpu {
                    self.context.resize_surface(&mut g.surface, size.width.max(1), size.height.max(1));
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => self.banner = None,
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::Focused(false) => self.player.cancel(t),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x, position.y));
                if let Some((at, _)) = self.cursor() {
                    self.player.pointer_move(at, mods, t);
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => match state {
                ElementState::Pressed => {
                    if let Some((at, logical)) = self.cursor() {
                        self.player.pointer_down(at, logical, mods, t);
                    }
                }
                ElementState::Released => self.player.pointer_up(t),
            },
            WindowEvent::MouseWheel { delta, .. } => {
                self.player.wheel(input::wheel_notches(delta, self.scale_factor()), mods, t);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if event.logical_key == WinitKey::Named(NamedKey::Escape) {
                    event_loop.exit();
                } else if let Some(key) = input::key(&event.logical_key) {
                    self.player.key(key, mods, t);
                }
            }
            _ => return,
        }
        self.redraw();
    }
}
