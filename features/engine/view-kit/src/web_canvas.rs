//! A widget in a browser: dioxus-web, with each widget in a `<canvas>`.
//! The page around the widgets (the songs, the panels) is the app's
//! (studio-panels' `web_host`).
//!
//! dioxus-web has no custom-widget mechanism of its own (that is
//! dioxus-native's `CustomWidgetAttr`), so this is one: a canvas per
//! widget, drawn every animation frame with vello_hybrid's WebGL2 renderer
//! (keyflow-ui's `wasm-graphics` pattern), and the canvas's DOM events
//! turned into the same `UiEvent`s Blitz hands a widget. The widgets
//! themselves (the arrangement, the mixer, the chart) are unchanged: they
//! paint a scene and take events, and do not know which host they are in.
//!
//! Sizes: a widget paints at the canvas's CSS size (events arrive in CSS
//! pixels, so the hit test agrees with the picture), and the scene is
//! scaled by `devicePixelRatio` into the canvas's backing store, so it is
//! sharp on a high-density display.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::str::FromStr as _;

use anyrender::PaintScene as _;
use blitz_traits::events::{
    BlitzKeyEvent, BlitzPointerEvent, BlitzPointerId, Code, Key, KeyState, Location, Modifiers,
    MouseEventButton, MouseEventButtons, PointerCoords, PointerDetails, UiEvent,
};
use dioxus::prelude::*;
use vello::kurbo::Affine;
use wasm_bindgen::JsCast as _;
use wasm_bindgen::closure::Closure;

use crate::panel_input::{Button, PanelEvent, WHEEL_LINE};

/// What a canvas hosts: something that paints a scene and takes events.
pub trait Hosted {
    fn paint(&mut self, width: u32, height: u32, scale: f64) -> anyrender::Scene;
    fn event(&mut self, event: &UiEvent);
}

/// A hosted widget, shared between its canvas and whoever built it.
#[derive(Clone)]
pub struct HostedRef(pub Rc<RefCell<dyn Hosted>>);

impl PartialEq for HostedRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Where a canvas's node goes once mounted, for whoever gives it the
/// keyboard (the panel's `focus_node`).
#[derive(Clone)]
pub struct FocusSlot(pub Rc<RefCell<Option<Rc<MountedData>>>>);

impl FocusSlot {
    fn set(&self, node: Rc<MountedData>) {
        *self.0.borrow_mut() = Some(node);
    }
}

impl PartialEq for FocusSlot {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A canvas element slot, equal by identity.
#[derive(Clone)]
pub struct ElementSlot(pub Rc<RefCell<Option<web_sys::HtmlElement>>>);

impl PartialEq for ElementSlot {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// vello_gpu's (vello_hybrid's successor) WebGL2 renderer over one canvas.
struct Gpu {
    renderer: vello_gpu::WebGlRenderer,
    resources: vello_gpu::Resources,
    scene: vello_gpu::Scene,
    images: rustc_hash::FxHashMap<u64, vello_common::paint::ImageId>,
    size: (u32, u32),
}

impl Gpu {
    /// The renderer over `canvas`, or why there is none: a browser with
    /// hardware acceleration off gives no WebGL2, and the renderer would
    /// panic on that, so the context is asked for first.
    fn new(canvas: &web_sys::HtmlCanvasElement, width: u32, height: u32) -> Result<Self, NoWebGl2> {
        // vello_gpu hands back the image/glyph atlases (`Resources`) with
        // the renderer, and reports a missing context as an error rather
        // than a panic; both ways of having none read as no WebGL2.
        let (renderer, resources) = init_renderer(
            || webgl2_context(canvas),
            |_context| vello_gpu::WebGlRenderer::new(canvas).ok(),
        )?
        .ok_or(NoWebGl2)?;
        Ok(Self {
            renderer,
            resources,
            scene: Self::scene_at(width, height),
            images: rustc_hash::FxHashMap::default(),
            size: (width, height),
        })
    }

    fn scene_at(width: u32, height: u32) -> vello_gpu::Scene {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a canvas is well under 65k px"
        )]
        let (w, h) = (
            width.min(u32::from(u16::MAX)) as u16,
            height.min(u32::from(u16::MAX)) as u16,
        );
        vello_gpu::Scene::new(w, h)
    }

    fn resize(&mut self, width: u32, height: u32) {
        if self.size != (width, height) {
            self.size = (width, height);
            self.scene = Self::scene_at(width, height);
        }
    }

    /// Draw `scene` (in CSS pixels) scaled by `dpr` into the canvas.
    fn draw(&mut self, scene: anyrender::Scene, dpr: f64) {
        {
            let images = anyrender_vello_hybrid::WebGlImageManager::new(
                &mut self.renderer,
                &mut self.resources,
                &mut self.images,
            );
            let mut painter =
                anyrender_vello_hybrid::WebGlScenePainter::new(&mut self.scene, images);
            painter.append_scene(scene, Affine::scale(dpr));
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a canvas is well under 65k px"
        )]
        let size = vello_gpu::RenderSize {
            width: self.size.0.min(u32::from(u16::MAX)) as u16,
            height: self.size.1.min(u32::from(u16::MAX)) as u16,
        };
        if let Err(e) = self.renderer.render(
            &self.scene,
            &mut self.resources,
            &size,
            &vello_gpu::WebGlTextureBindings::new(),
            vello_common::peniko::Color::TRANSPARENT,
        ) {
            tracing::error!(error = ?e, "canvas render failed");
        }
        self.scene.reset();
    }
}

/// `canvas`'s WebGL2 context, asked for with the options the renderer asks
/// with (no antialiasing, a depth buffer) so the context it then gets is
/// this one, set up as it wants. `None`: the browser has none to give.
fn webgl2_context(canvas: &web_sys::HtmlCanvasElement) -> Option<js_sys::Object> {
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"antialias".into(), &wasm_bindgen::JsValue::FALSE).ok()?;
    js_sys::Reflect::set(&options, &"depth".into(), &wasm_bindgen::JsValue::TRUE).ok()?;
    canvas
        .get_context_with_context_options("webgl2", &options)
        .ok()
        .flatten()
}

/// Whether this browser draws with WebGL2 at all: asked of a canvas made
/// for the question, before the page shows anything drawn.
fn webgl2_offered() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.create_element("canvas").ok())
        .and_then(|el| el.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        .is_some_and(|canvas| webgl2_context(&canvas).is_some())
}

/// The DOM's modifier keys, as Blitz's.
fn modifiers(m: dioxus::prelude::Modifiers) -> Modifiers {
    let mut out = Modifiers::empty();
    if m.shift() {
        out |= Modifiers::SHIFT;
    }
    if m.ctrl() {
        out |= Modifiers::CONTROL;
    }
    if m.alt() {
        out |= Modifiers::ALT;
    }
    if m.meta() {
        out |= Modifiers::META;
    }
    out
}

fn mods(m: dioxus::prelude::Modifiers) -> crate::mousemap::Mods {
    crate::mousemap::Mods {
        shift: m.shift(),
        ctrl: m.ctrl(),
        alt: m.alt(),
    }
}

/// A DOM pointer event, as the widget reads one: coordinates relative to
/// the canvas (Blitz makes them relative to the node the same way).
fn pointer(e: &PointerData) -> BlitzPointerEvent {
    #[expect(clippy::cast_possible_truncation, reason = "a coordinate on a page")]
    let (x, y) = {
        let at = e.element_coordinates();
        (at.x as f32, at.y as f32)
    };
    let button = match e.trigger_button() {
        Some(dioxus::html::input_data::MouseButton::Auxiliary) => MouseEventButton::Auxiliary,
        Some(dioxus::html::input_data::MouseButton::Secondary) => MouseEventButton::Secondary,
        _ => MouseEventButton::Main,
    };
    // Which pointer, so a widget can tell fingers apart (two faders at
    // once) and a finger from a mouse (touch gestures).
    let id = match e.pointer_type().as_str() {
        "touch" => BlitzPointerId::Finger(u64::from(e.pointer_id().unsigned_abs())),
        "pen" => BlitzPointerId::Pen,
        _ => BlitzPointerId::Mouse,
    };
    let mut buttons = MouseEventButtons::empty();
    for held in e.held_buttons() {
        buttons |= match held {
            dioxus::html::input_data::MouseButton::Primary => MouseEventButtons::Primary,
            dioxus::html::input_data::MouseButton::Secondary => MouseEventButtons::Secondary,
            dioxus::html::input_data::MouseButton::Auxiliary => MouseEventButtons::Auxiliary,
            dioxus::html::input_data::MouseButton::Fourth => MouseEventButtons::Fourth,
            dioxus::html::input_data::MouseButton::Fifth => MouseEventButtons::Fifth,
            dioxus::html::input_data::MouseButton::Unknown => MouseEventButtons::empty(),
        };
    }
    BlitzPointerEvent {
        id,
        is_primary: e.is_primary(),
        coords: PointerCoords {
            page_x: x,
            page_y: y,
            screen_x: x,
            screen_y: y,
            client_x: x,
            client_y: y,
        },
        button,
        buttons,
        mods: modifiers(e.modifiers()),
        details: PointerDetails::default(),
        element: blitz_traits::events::Point { x, y },
        active_pointers: std::sync::Arc::default(),
    }
}

/// A DOM key event, as the widget reads one: by its W3C names (the DOM
/// and Blitz are on different `keyboard-types` versions).
fn key(e: &KeyboardData, state: KeyState) -> BlitzKeyEvent {
    BlitzKeyEvent {
        key: Key::from_str(&e.key().to_string()).unwrap_or(Key::Named(NamedKey::Unidentified)),
        code: Code::from_str(&e.code().to_string()).unwrap_or(Code::Unidentified),
        modifiers: modifiers(e.modifiers()),
        location: Location::Standard,
        is_auto_repeating: e.is_auto_repeating(),
        is_composing: false,
        state,
        text: None,
    }
}

fn panel_button(e: &PointerData) -> Button {
    match e.trigger_button() {
        Some(dioxus::html::input_data::MouseButton::Primary) => Button::Left,
        Some(dioxus::html::input_data::MouseButton::Auxiliary) => Button::Middle,
        _ => Button::Other,
    }
}

/// One widget in a canvas: painted every animation frame, fed the canvas's
/// events. `panel`, when given, hears the panel-level events too (the
/// zoom spring, the hand, the wheel), and `frame` runs once a frame before
/// the paint.
#[component]
pub fn WidgetCanvas(
    widget: HostedRef,
    panel: Option<EventHandler<PanelEvent>>,
    frame: Option<EventHandler<()>>,
    /// Where the canvas's node goes, for whoever gives it the focus.
    focus: Option<FocusSlot>,
    /// Where the canvas element goes, for the pointer shape.
    element: Option<ElementSlot>,
    /// Out of sight: painted once, so it is ready, and not again until
    /// shown.
    #[props(default)]
    hidden: bool,
) -> Element {
    let alive = use_hook(|| Rc::new(Cell::new(true)));
    let hiding = use_hook(|| Rc::new(Cell::new(hidden)));
    hiding.set(hidden);
    {
        let alive = Rc::clone(&alive);
        use_drop(move || alive.set(false));
    }
    let to_panel = move |event: PanelEvent| {
        if let Some(panel) = panel {
            panel.call(event);
        }
    };
    let (w1, w2, w3, w4, w5, w6) = (
        widget.clone(),
        widget.clone(),
        widget.clone(),
        widget.clone(),
        widget.clone(),
        widget.clone(),
    );
    rsx! {
        canvas {
            style: "position:absolute; left:0; top:0; width:100%; height:100%; \
                    display:block; outline:none; touch-action:none;",
            tabindex: "0",
            onmounted: move |event| {
                let data = event.data();
                // Only the canvas that reads the keyboard takes the focus.
                let takes_focus = focus.is_some();
                if let Some(slot) = &focus {
                    slot.set(Rc::clone(&data));
                }
                let Some(canvas) = data
                    .downcast::<web_sys::Element>()
                    .and_then(|el| el.clone().dyn_into::<web_sys::HtmlCanvasElement>().ok())
                else {
                    tracing::error!("the widget's node is not a canvas");
                    return;
                };
                if let Some(slot) = &element {
                    *slot.0.borrow_mut() = Some(canvas.clone().unchecked_into());
                }
                if takes_focus {
                    let _ = canvas.focus();
                }
                start_painting(canvas, widget.clone(), frame, Rc::clone(&alive), Rc::clone(&hiding));
            },
            onpointermove: move |e| {
                to_panel(PanelEvent::Pointer { x: e.client_coordinates().x, y: e.client_coordinates().y });
                w1.0.borrow_mut().event(&UiEvent::PointerMove(pointer(&e)));
            },
            onpointerdown: move |e| {
                // Keep the pointer through a drag that leaves the canvas.
                if let Some(target) = e.data().downcast::<web_sys::PointerEvent>()
                    .and_then(|p| p.target())
                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                {
                    let _ = target.set_pointer_capture(e.pointer_id());
                }
                to_panel(PanelEvent::Button { button: panel_button(&e), pressed: true });
                w2.0.borrow_mut().event(&UiEvent::PointerDown(pointer(&e)));
            },
            onpointerup: move |e| {
                to_panel(PanelEvent::Button { button: panel_button(&e), pressed: false });
                w3.0.borrow_mut().event(&UiEvent::PointerUp(pointer(&e)));
            },
            // The browser took the pointer back (a system gesture, the
            // page scrolling): what it was doing ends here.
            onpointercancel: move |e| {
                w6.0.borrow_mut().event(&UiEvent::PointerCancel(pointer(&e)));
            },
            oncontextmenu: move |e| e.prevent_default(),
            onwheel: move |e| {
                e.prevent_default();
                let delta = e.delta();
                let (dx, dy) = match delta {
                    dioxus::html::geometry::WheelDelta::Pixels(v) => (v.x, v.y),
                    dioxus::html::geometry::WheelDelta::Lines(v) => (v.x * WHEEL_LINE, v.y * WHEEL_LINE),
                    dioxus::html::geometry::WheelDelta::Pages(v) => (v.x * 400.0, v.y * 400.0),
                };
                // The DOM's positive delta scrolls down; the panel's (winit's)
                // positive delta moves the content down.
                to_panel(PanelEvent::Modifiers(mods(e.modifiers())));
                to_panel(PanelEvent::Wheel { dx: -dx, dy: -dy });
            },
            onkeydown: move |e| {
                if e.code() == dioxus::prelude::Code::KeyZ {
                    crate::tool::set_zoom_held(true);
                    to_panel(PanelEvent::ZKey { pressed: true, repeat: e.is_auto_repeating() });
                }
                to_panel(PanelEvent::Modifiers(mods(e.modifiers())));
                // The page must not scroll on space or the arrows, nor take
                // a shortcut the session means.
                if !matches!(e.key(), dioxus::prelude::Key::Named(dioxus::prelude::NamedKey::Tab)) {
                    e.prevent_default();
                }
                w4.0.borrow_mut().event(&UiEvent::KeyDown(key(&e, KeyState::Pressed)));
            },
            onkeyup: move |e| {
                if e.code() == dioxus::prelude::Code::KeyZ {
                    crate::tool::set_zoom_held(false);
                    to_panel(PanelEvent::ZKey { pressed: false, repeat: false });
                }
                to_panel(PanelEvent::Modifiers(mods(e.modifiers())));
                w5.0.borrow_mut().event(&UiEvent::KeyUp(key(&e, KeyState::Released)));
            },
        }
    }
}

/// A frame as drawn on a canvas: its scene, the backing store's size and
/// the device pixel ratio it was drawn at.
#[derive(PartialEq)]
struct Drawn {
    scene: anyrender::Scene,
    size: (u32, u32),
    dpr: f64,
}

/// The animation-frame loop for one canvas: size the backing store to the
/// canvas's CSS size times `devicePixelRatio`, run `frame`, paint.
fn start_painting(
    canvas: web_sys::HtmlCanvasElement,
    widget: HostedRef,
    frame: Option<EventHandler<()>>,
    alive: Rc<Cell<bool>>,
    hidden: Rc<Cell<bool>>,
) {
    let painted = Cell::new(false);
    // What the canvas shows now: a frame painting the same is not drawn.
    let mut shown: Option<Drawn> = None;
    let runtime = dioxus::core::Runtime::current();
    let scope = dioxus::core::current_scope_id();
    let gpu = Rc::new(RefCell::new(None::<Gpu>));
    let tick: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let again = Rc::clone(&tick);
    *tick.borrow_mut() = Some(Closure::new(move || {
        if !alive.get() {
            // Unmounted: stop, and let the closure go.
            again.borrow_mut().take();
            return;
        }
        let Some(window) = web_sys::window() else {
            return;
        };
        let dpr = window.device_pixel_ratio().max(1.0);
        let rect = canvas.get_bounding_client_rect();
        let (css_w, css_h) = (rect.width().max(1.0), rect.height().max(1.0));
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a canvas size"
        )]
        let (px_w, px_h) = ((css_w * dpr).round() as u32, (css_h * dpr).round() as u32);
        if canvas.width() != px_w || canvas.height() != px_h {
            canvas.set_width(px_w);
            canvas.set_height(px_h);
        }
        if let Some(frame) = frame {
            runtime.in_scope(scope, || frame.call(()));
        }
        // Hidden: painted once to be ready, then left until shown.
        if hidden.get() && painted.replace(true) {
            if let Some(next) = again.borrow().as_ref() {
                let _ = window.request_animation_frame(next.as_ref().unchecked_ref());
            }
            return;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a canvas size"
        )]
        let scene = widget.0.borrow_mut().paint(css_w as u32, css_h as u32, 1.0);
        let next = Drawn {
            scene,
            size: (px_w, px_h),
            dpr,
        };
        // Nothing changed: the canvas keeps what it shows (a WebGL canvas
        // is cleared only when drawn to or resized), and the scene is not
        // encoded and rasterized again — most frames of a page at rest.
        if shown.as_ref() == Some(&next) {
            if let Some(again) = again.borrow().as_ref() {
                let _ = window.request_animation_frame(again.as_ref().unchecked_ref());
            }
            return;
        }
        let mut gpu = gpu.borrow_mut();
        if gpu.is_none() {
            match Gpu::new(&canvas, px_w, px_h) {
                Ok(made) => *gpu = Some(made),
                Err(e) => {
                    // No panic, and no frame loop with nothing to draw
                    // on: the page says so (`WebDemo`'s check), and this
                    // canvas stays blank.
                    tracing::error!(error = %e, "the canvas cannot be drawn");
                    drop(gpu);
                    again.borrow_mut().take();
                    return;
                }
            }
        }
        let Some(gpu) = gpu.as_mut() else {
            return;
        };
        gpu.resize(px_w, px_h);
        gpu.draw(next.scene.clone(), dpr);
        shown = Some(next);
        if let Some(next) = again.borrow().as_ref() {
            let _ = window.request_animation_frame(next.as_ref().unchecked_ref());
        }
    }));
    if let (Some(window), Some(first)) = (web_sys::window(), tick.borrow().as_ref()) {
        let _ = window.request_animation_frame(first.as_ref().unchecked_ref());
    }
}

/// Why a canvas cannot be drawn: the browser gave it no WebGL2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoWebGl2;

impl std::fmt::Display for NoWebGl2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "this browser gave the page no WebGL2 (hardware acceleration is off or unsupported)",
        )
    }
}

impl std::error::Error for NoWebGl2 {}

/// A renderer over a canvas's WebGL2 context: `context` asks the browser
/// for it (`None` when there is none), `make` builds the renderer once it
/// is there. The renderer is only made once the context is known to
/// exist, so it is asked for with the options the page wants.
///
/// # Errors
///
/// [`NoWebGl2`]: the browser gave no WebGL2 context.
pub fn init_renderer<C, R>(
    context: impl FnOnce() -> Option<C>,
    make: impl FnOnce(C) -> R,
) -> Result<R, NoWebGl2> {
    context().map(make).ok_or(NoWebGl2)
}
