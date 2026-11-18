//! The mixer widget in an Architect pane: the mixer's third host (after
//! dioxus-native's `mixer_panel::Mixer` and the browser's `WebMixer`).
//!
//! Nothing here draws. Each frame the pane asks the widget for its scene
//! (`MixerWidget::paint_scene`, which catches up with the engine — its
//! meters, a fold, the racks switch — as it does in the window), and that
//! scene becomes the pane's one layer, crossing only when it changed.
//! Pointer and keys go to the widget as Blitz `UiEvent`s; the wheel runs
//! along the strips, as the window's does.

use std::rc::Rc;
use std::sync::{Arc, Mutex};

use architect_workspace::paint::{Layer, LayerKey, PaintView, Scene};
use architect_workspace::{ViewCtx, ViewInput};
use blitz_traits::events::{KeyState, MouseEventButton, MouseEventButtons, UiEvent};
use session_ui::mixer_panel::HostedMixer;
use studio_view_link::{InScope, SceneLayer, Stats, input};

/// The pane's one layer: the widget's scene as it paints it.
pub const SCENE: LayerKey = 1;

pub struct Hosted {
    mixer: Rc<HostedMixer>,
    scope: InScope,
    layer: SceneLayer,
    /// The buttons held now: a move carries them, as Blitz's do.
    held: MouseEventButtons,
    /// The pane's width, for the wheel's end.
    width: f64,
}

impl Hosted {
    #[must_use]
    pub fn new(mixer: Rc<HostedMixer>, scope: InScope, stats: Arc<Mutex<Stats>>) -> Self {
        Self {
            mixer,
            scope,
            layer: SceneLayer::new(SCENE, stats),
            held: MouseEventButtons::empty(),
            width: 0.0,
        }
    }

    /// The scene the widget last painted, as it painted it.
    #[must_use]
    pub const fn last_scene(&self) -> Option<&Scene> {
        self.layer.last()
    }

    fn widget(&self, event: &UiEvent) {
        let widget = &self.mixer.widget;
        self.scope.run(|| widget.borrow_mut().event(event));
    }
}

impl PaintView for Hosted {
    fn input(&mut self, _: &ViewCtx, event: &ViewInput) -> bool {
        match event {
            ViewInput::PointerDown { x, y, button, mods } => {
                let (b, bs, _) = input::mouse(button);
                self.held |= bs;
                let e = input::pointer((*x, *y), b, self.held, input::modifiers(mods));
                self.widget(&UiEvent::PointerDown(e));
            }
            ViewInput::PointerMove { x, y, mods } => {
                let e = input::pointer(
                    (*x, *y),
                    MouseEventButton::Main,
                    self.held,
                    input::modifiers(mods),
                );
                self.widget(&UiEvent::PointerMove(e));
            }
            ViewInput::PointerUp { x, y, button, mods } => {
                let (b, bs, _) = input::mouse(button);
                self.held.remove(bs);
                let e = input::pointer((*x, *y), b, self.held, input::modifiers(mods));
                self.widget(&UiEvent::PointerUp(e));
            }
            // Architect's wheel is signed as the web's; the mixer's is
            // winit's, as the browser host turns it.
            ViewInput::Wheel { dx, dy, .. } => {
                self.mixer
                    .wheel(-f64::from(*dx), -f64::from(*dy), self.width);
            }
            ViewInput::Key {
                key: name,
                down,
                modifiers: mods,
            } => {
                let state = if *down {
                    KeyState::Pressed
                } else {
                    KeyState::Released
                };
                let e = input::key(name, mods, state);
                self.widget(&if *down {
                    UiEvent::KeyDown(e)
                } else {
                    UiEvent::KeyUp(e)
                });
            }
            ViewInput::Focus(false) => {
                self.held = MouseEventButtons::empty();
            }
            _ => return false,
        }
        true
    }

    fn paint(&mut self, _: &ViewCtx, _: &mut Scene) {}

    // Every frame, as the window's mixer redraws while the transport
    // moves (its meters): a frame that paints the same scene sends
    // nothing.
    fn animating(&self, _: &ViewCtx) -> bool {
        true
    }

    fn layers(&mut self, ctx: &ViewCtx) -> Option<Vec<(LayerKey, Layer)>> {
        let began = web_time::Instant::now();
        let (w, h) = studio_view_link::pane_size(ctx);
        self.width = f64::from(w);
        let scene = {
            let widget = &self.mixer.widget;
            self.scope
                .run(|| widget.borrow_mut().paint_scene(w, h, 1.0))
        };
        Some(self.layer.frame(scene, (w, h), began))
    }
}
