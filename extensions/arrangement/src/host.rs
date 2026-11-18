//! The arrangement widget in an Architect pane: the third host of
//! `session_ui::panel` (after dioxus-native's `studio::Arrangement` and the
//! browser's `web_host::WebArrangement`).
//!
//! Nothing here draws. Each frame the pane's paint view does what the
//! other hosts do on theirs: tells the panel the frame and where the play
//! cursor is (`PanelEvent::Frame`, from the same `engine::Transport` the
//! desktop window reads), and asks the widget for its scene
//! (`ArrangementWidget::paint_layers`: the picture, and the play cursor
//! apart), which become the pane's two layers. A layer that paints the
//! same as last frame sends nothing (the web host's rule), so a song
//! playing in a still view moves the cursor's layer alone.
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

/// The pane's picture but the play cursor: what crosses only when the
/// view changes.
pub const SCENE: LayerKey = 1;
/// The play cursor over it: all that crosses while the song plays and the
/// view stands still.
pub const CURSOR: LayerKey = 2;
/// The track panel's mute and solo, lit, from here up: parts the engine's
/// values show (`ArrangementWidget::paint_layers`), one key each.
pub const BUTTONS: LayerKey = 1000;

/// The widget hosted in a pane.
pub struct Hosted {
    widget: Rc<RefCell<ArrangementWidget>>,
    panel: ArrangementPanel,
    scope: InScope,
    layer: SceneLayer,
    cursor: SceneLayer,
    /// The mute and solo parts, as last sent.
    buttons: studio_view_link::ValueParts,
    /// The engine's values light the buttons
    /// (`studio_view_link::engine_lights_buttons`), and when that was last
    /// looked at.
    lit_by_values: bool,
    looked: Option<web_time::Instant>,
    /// Set by a test: the buttons lit by values or not, whatever the
    /// table on this machine says.
    forced: Option<bool>,
    /// The buttons held now: a move carries them, as Blitz's do.
    held: MouseEventButtons,
    /// The fingers down, the first the primary one (`input::touch`).
    fingers: Vec<u64>,
    /// Where the play cursor is now, seconds: the engine's transport (the
    /// same `engine::Transport` the desktop window reads), or a test's.
    play_at: Box<dyn Fn() -> f64>,
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
            cursor: SceneLayer::new(CURSOR, Arc::default()),
            buttons: studio_view_link::ValueParts::default(),
            lit_by_values: false,
            looked: None,
            forced: None,
            held: MouseEventButtons::empty(),
            fingers: Vec::new(),
            play_at: Box::new(|| {
                // Read each frame: the transport is there once the link is.
                session_ui::engine::Transport::shared().map_or(0.0, |t| t.read().0)
            }),
        }
    }

    /// The tracks' mute and solo lit by the engine's values (`on`) or
    /// drawn in the picture, whatever this machine's table says: a test's
    /// own table.
    #[must_use]
    pub const fn with_buttons_lit_by_values(mut self, on: bool) -> Self {
        self.forced = Some(on);
        self
    }

    /// The play cursor from `play_at` rather than the engine's transport:
    /// a measurement driving the pane as playback does.
    #[must_use]
    pub fn with_play_at(mut self, play_at: impl Fn() -> f64 + 'static) -> Self {
        self.play_at = Box::new(play_at);
        self
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
            // A finger: to the panel as the window's touch comes (its
            // place, a press of the left button), to the widget as a
            // finger, which its gestures (a swipe across the track panel,
            // a scroll, a scrub) tell from a mouse.
            ViewInput::Touch { id, x, y, phase } => {
                let Some(event) = input::touch(&mut self.fingers, *id, (*x, *y), phase) else {
                    return false;
                };
                self.panel(self.pointer_at(*x, *y));
                match &event {
                    UiEvent::PointerDown(_) => self.panel(PanelEvent::Button {
                        button: session_ui::panel::Button::Left,
                        pressed: true,
                    }),
                    UiEvent::PointerUp(_) | UiEvent::PointerCancel(_) => {
                        self.panel(PanelEvent::Button {
                            button: session_ui::panel::Button::Left,
                            pressed: false,
                        });
                    }
                    _ => {}
                }
                self.widget(&event);
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
        let play_at = (self.play_at)();
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
        // Looked at every half second: the engine's table, once it is
        // there, stays.
        if let Some(on) = self.forced {
            self.lit_by_values = on;
        } else if self
            .looked
            .is_none_or(|at| began.duration_since(at).as_millis() >= 500)
        {
            self.looked = Some(began);
            self.lit_by_values = studio_view_link::engine_lights_buttons_here();
        }
        let buttons = self.lit_by_values;
        // The panel's shape, as asked (`STUDIO_ARRANGEMENT_PANEL`): set as
        // the toolbar sets it, the widget drawing it from the next paint.
        let asked = crate::ASKED_COMPACT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(compact) = asked {
            let mut shape = self.panel.shape;
            let cell = std::rc::Rc::clone(&self.panel.compact);
            self.scope.run(|| {
                cell.set(compact);
                if *shape.peek() != compact {
                    shape.set(compact);
                }
            });
        }
        crate::COMPACT.store(
            self.panel.compact.get(),
            std::sync::atomic::Ordering::Release,
        );
        let (scene, cursor, parts) = {
            let widget = &self.widget;
            self.scope
                .run(|| widget.borrow_mut().paint_layers(w, h, 1.0, buttons))
        };
        let mut layers = self.cursor.frame(cursor, (w, h), began);
        layers.splice(0..0, self.layer.frame(scene, (w, h), began));
        layers.extend(self.buttons.layers(
            &parts,
            BUTTONS,
            (1, 1),
            architect_workspace::paint::peniko::kurbo::Affine::IDENTITY,
        ));
        Some(layers)
    }
}
