//! The arrangement widget in an Architect pane: the third host of
//! `session_ui::panel` (after dioxus-native's `studio::Arrangement` and the
//! browser's `web_host::WebArrangement`).
//!
//! Nothing here draws. Each frame the pane's paint view does what the
//! other hosts do on theirs: tells the panel the frame and where the play
//! cursor is (`PanelEvent::Frame`, from the same `engine::Transport` the
//! desktop window reads), and asks the widget for its scene
//! (`ArrangementWidget::paint_scene`), which becomes the pane's layer. A
//! frame that paints the same scene sends nothing (the web host's rule).
//! Input arrives as Architect's `ViewInput` and goes where Blitz and winit
//! send it in the desktop window: pointer and keys to the widget as Blitz
//! `UiEvent`s, and the panel's own events (the pointer's place, buttons,
//! the wheel, the `z` zoom spring, the modifiers) to the panel.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use architect_workspace::paint::{Layer, LayerKey, PaintView, Scene};
use architect_workspace::{ViewCtx, ViewInput};
use blitz_traits::events::{KeyState, MouseEventButton, MouseEventButtons, UiEvent};
use dioxus::prelude::{ReadableExt as _, WritableExt as _};
use session_ui::panel::{ArrangementPanel, PanelEvent};
use session_ui::widget::ArrangementWidget;
pub use studio_view_link::{InScope, Stats};
use studio_view_link::{SceneLayer, input};

/// The pane's one layer: the widget's scene as it paints it.
pub const SCENE: LayerKey = 1;

/// The widget hosted in a pane.
pub struct Hosted {
    widget: Rc<RefCell<ArrangementWidget>>,
    panel: ArrangementPanel,
    scope: InScope,
    layer: SceneLayer,
    /// The buttons held now: a move carries them, as Blitz's do.
    held: MouseEventButtons,
}

impl Hosted {
    #[must_use]
    pub fn new(
        widget: Rc<RefCell<ArrangementWidget>>,
        panel: ArrangementPanel,
        scope: InScope,
        stats: Arc<Mutex<Stats>>,
    ) -> Self {
        Self {
            widget,
            panel,
            scope,
            layer: SceneLayer::new(SCENE, stats),
            held: MouseEventButtons::empty(),
        }
    }

    fn panel(&self, event: PanelEvent) {
        let panel = &self.panel;
        self.scope.run(|| panel.handle(event));
    }

    /// The pointer at `(x, y)` in the pane, for the panel: in the window,
    /// where it has measured itself (`ArrangementPanel::rect`), as the
    /// other hosts hand it.
    fn pointer_at(&self, x: f32, y: f32) -> PanelEvent {
        let rect = self.scope.run(|| *self.panel.rect.peek());
        PanelEvent::Pointer {
            x: rect.0 + f64::from(x),
            y: rect.1 + f64::from(y),
        }
    }

    fn widget(&self, event: &UiEvent) {
        let widget = &self.widget;
        self.scope.run(|| widget.borrow_mut().event(event));
    }
}

impl PaintView for Hosted {
    fn input(&mut self, _: &ViewCtx, event: &ViewInput) -> bool {
        match event {
            ViewInput::PointerDown { x, y, button, mods } => {
                let (b, bs, p) = input::mouse(button);
                self.held |= bs;
                self.panel(PanelEvent::Modifiers(input::panel_mods(mods)));
                self.panel(self.pointer_at(*x, *y));
                self.panel(PanelEvent::Button {
                    button: p,
                    pressed: true,
                });
                let e = input::pointer((*x, *y), b, self.held, input::modifiers(mods));
                self.widget(&UiEvent::PointerDown(e));
            }
            ViewInput::PointerMove { x, y, mods } => {
                self.panel(self.pointer_at(*x, *y));
                let e = input::pointer(
                    (*x, *y),
                    MouseEventButton::Main,
                    self.held,
                    input::modifiers(mods),
                );
                self.widget(&UiEvent::PointerMove(e));
            }
            ViewInput::PointerUp { x, y, button, mods } => {
                let (b, bs, p) = input::mouse(button);
                self.held.remove(bs);
                self.panel(PanelEvent::Button {
                    button: p,
                    pressed: false,
                });
                let e = input::pointer((*x, *y), b, self.held, input::modifiers(mods));
                self.widget(&UiEvent::PointerUp(e));
            }
            ViewInput::Wheel { dx, dy, mods, .. } => {
                // Architect's wheel is signed as the web's (positive scrolls
                // down); the panel's is winit's, as the web host turns it.
                self.panel(PanelEvent::Modifiers(input::panel_mods(mods)));
                self.panel(PanelEvent::Wheel {
                    dx: -f64::from(*dx),
                    dy: -f64::from(*dy),
                });
            }
            ViewInput::Key {
                key: name,
                down,
                modifiers: mods,
            } => {
                if name.eq_ignore_ascii_case("z") {
                    session_ui::tool::set_zoom_held(*down);
                    self.panel(PanelEvent::ZKey {
                        pressed: *down,
                        repeat: false,
                    });
                }
                self.panel(PanelEvent::Modifiers(input::panel_mods(mods)));
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

    // Every frame, as the other hosts draw: the panel follows the play
    // cursor and springs its zoom on its frames. A frame that paints the
    // same scene sends nothing.
    fn animating(&self, _: &ViewCtx) -> bool {
        true
    }

    fn layers(&mut self, ctx: &ViewCtx) -> Option<Vec<(LayerKey, Layer)>> {
        let began = web_time::Instant::now();
        // Read each frame: the transport is there once the link is.
        let play_at = session_ui::engine::Transport::shared().map_or(0.0, |t| t.read().0);
        self.panel(PanelEvent::Frame { play_at });
        let (w, h) = studio_view_link::pane_size(ctx);
        // The panel's rectangle is the pane: it fills it, and the pane is
        // where its pointer is (pane-local, as [`Hosted::pointer_at`] hands
        // it). Told here, as the pane is measured, rather than read back
        // from the panel's node as the other hosts do: a process view's
        // nodes are in the host's document, which this process cannot
        // measure, and the scrollbars were laid out in a zero-sized panel.
        {
            let mut rect = self.panel.rect;
            let next = (0.0, 0.0, f64::from(w), f64::from(h));
            self.scope.run(|| {
                if *rect.peek() != next {
                    rect.set(next);
                }
            });
        }
        let scene = {
            let widget = &self.widget;
            self.scope
                .run(|| widget.borrow_mut().paint_scene(w, h, 1.0))
        };
        Some(self.layer.frame(scene, (w, h), began))
    }
}
