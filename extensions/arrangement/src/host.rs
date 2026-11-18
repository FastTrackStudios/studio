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
use std::str::FromStr as _;
use std::sync::Arc;

use anyrender::recording::RenderCommand;
use architect_workspace::paint::{Layer, LayerKey, PaintView, Scene};
use architect_workspace::{Button, ViewCtx, ViewInput};
use blitz_traits::events::{
    BlitzKeyEvent, BlitzPointerEvent, BlitzPointerId, Code, Key, KeyState, Location, Modifiers,
    MouseEventButton, MouseEventButtons, PointerCoords, PointerDetails, UiEvent,
};
use session_ui::panel::{ArrangementPanel, Button as PanelButton, PanelEvent};
use session_ui::widget::ArrangementWidget;

/// The pane's one layer: the widget's scene as it paints it.
pub const SCENE: LayerKey = 1;

/// What the paint view needs to run the panel's code inside the view's
/// Dioxus runtime (its signals are set there, as on the other hosts).
#[derive(Clone)]
pub struct InScope {
    pub runtime: Rc<dioxus::core::Runtime>,
    pub scope: dioxus::core::ScopeId,
}

impl InScope {
    fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        self.runtime.in_scope(self.scope, f)
    }
}

/// The widget hosted in a pane.
pub struct Hosted {
    widget: Rc<RefCell<ArrangementWidget>>,
    panel: ArrangementPanel,
    scope: InScope,
    /// The scene last painted, and the same commands as the layer shows
    /// them: a frame that paints the same keeps the layer's content, so
    /// nothing crosses.
    last: Option<Scene>,
    content: Arc<Vec<RenderCommand>>,
    /// The buttons held now: a move carries them, as Blitz's do.
    held: MouseEventButtons,
    /// What the shared state reports (`STUDIO_ARRANGEMENT_STATE`).
    stats: Arc<std::sync::Mutex<Stats>>,
}

/// How the pane has been painting.
#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub frames: u64,
    /// Frames whose scene differed from the last (and so crossed).
    pub changed: u64,
    pub size: (u32, u32),
    pub commands: usize,
    /// The view's work a frame (the panel's frame and the widget's
    /// paint), the latest few hundred, in milliseconds.
    pub work_ms: std::collections::VecDeque<f64>,
}

impl Hosted {
    #[must_use]
    pub fn new(
        widget: Rc<RefCell<ArrangementWidget>>,
        panel: ArrangementPanel,
        scope: InScope,
        stats: Arc<std::sync::Mutex<Stats>>,
    ) -> Self {
        Self {
            widget,
            panel,
            scope,
            last: None,
            content: Arc::new(Vec::new()),
            held: MouseEventButtons::empty(),
            stats,
        }
    }

    fn panel(&self, event: PanelEvent) {
        let panel = &self.panel;
        self.scope.run(|| panel.handle(event));
    }

    fn widget(&self, event: &UiEvent) {
        let widget = &self.widget;
        self.scope.run(|| widget.borrow_mut().event(event));
    }
}

/// Architect's modifier names as Blitz's (`Mod` is the platform's command
/// key: Ctrl here, ⌘ on a Mac).
fn modifiers(mods: &[&str]) -> Modifiers {
    let mut out = Modifiers::empty();
    for m in mods {
        out |= match *m {
            "Shift" => Modifiers::SHIFT,
            "Alt" => Modifiers::ALT,
            "Ctrl" => Modifiers::CONTROL,
            "Mod" if cfg!(target_os = "macos") => Modifiers::META,
            "Mod" => Modifiers::CONTROL,
            "Meta" => Modifiers::META,
            _ => Modifiers::empty(),
        };
    }
    out
}

fn panel_mods(mods: &[&str]) -> session_ui::mousemap::Mods {
    let m = modifiers(mods);
    session_ui::mousemap::Mods {
        shift: m.contains(Modifiers::SHIFT),
        ctrl: m.contains(Modifiers::CONTROL),
        alt: m.contains(Modifiers::ALT),
    }
}

const fn mouse(button: &Button) -> (MouseEventButton, MouseEventButtons, PanelButton) {
    match button {
        Button::Primary => (
            MouseEventButton::Main,
            MouseEventButtons::Primary,
            PanelButton::Left,
        ),
        Button::Middle => (
            MouseEventButton::Auxiliary,
            MouseEventButtons::Auxiliary,
            PanelButton::Middle,
        ),
        Button::Secondary => (
            MouseEventButton::Secondary,
            MouseEventButtons::Secondary,
            PanelButton::Other,
        ),
    }
}

/// A pointer event at `(x, y)` in the pane, as Blitz hands one to a
/// widget (coordinates relative to it).
fn pointer(
    (x, y): (f32, f32),
    button: MouseEventButton,
    buttons: MouseEventButtons,
    mods: Modifiers,
) -> BlitzPointerEvent {
    BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
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
        mods,
        details: PointerDetails::default(),
        element: blitz_traits::events::Point { x, y },
        active_pointers: std::sync::Arc::default(),
    }
}

/// A key as Blitz hands one to a widget: by its W3C names. A letter's code
/// is its key's (`z` is `KeyZ`), which is what the arrangement's keys read.
fn key(name: &str, mods: &[&str], state: KeyState) -> BlitzKeyEvent {
    let key = Key::from_str(name).unwrap_or_else(|_| Key::Character(name.to_owned()));
    let code = match name.chars().next() {
        Some(c) if name.chars().count() == 1 && c.is_ascii_alphabetic() => {
            Code::from_str(&format!("Key{}", c.to_ascii_uppercase())).unwrap_or(Code::Unidentified)
        }
        Some(c) if name.chars().count() == 1 && c.is_ascii_digit() => {
            Code::from_str(&format!("Digit{c}")).unwrap_or(Code::Unidentified)
        }
        _ if name == " " => Code::Space,
        _ => Code::from_str(name).unwrap_or(Code::Unidentified),
    };
    BlitzKeyEvent {
        key,
        code,
        modifiers: modifiers(mods),
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state,
        text: None,
    }
}

impl PaintView for Hosted {
    fn input(&mut self, _: &ViewCtx, event: &ViewInput) -> bool {
        match event {
            ViewInput::PointerDown { x, y, button, mods } => {
                let (b, bs, p) = mouse(button);
                self.held |= bs;
                self.panel(PanelEvent::Modifiers(panel_mods(mods)));
                self.panel(PanelEvent::Pointer {
                    x: f64::from(*x),
                    y: f64::from(*y),
                });
                self.panel(PanelEvent::Button {
                    button: p,
                    pressed: true,
                });
                let e = pointer((*x, *y), b, self.held, modifiers(mods));
                self.widget(&UiEvent::PointerDown(e));
            }
            ViewInput::PointerMove { x, y, mods } => {
                self.panel(PanelEvent::Pointer {
                    x: f64::from(*x),
                    y: f64::from(*y),
                });
                let e = pointer((*x, *y), MouseEventButton::Main, self.held, modifiers(mods));
                self.widget(&UiEvent::PointerMove(e));
            }
            ViewInput::PointerUp { x, y, button, mods } => {
                let (b, bs, p) = mouse(button);
                self.held.remove(bs);
                self.panel(PanelEvent::Button {
                    button: p,
                    pressed: false,
                });
                let e = pointer((*x, *y), b, self.held, modifiers(mods));
                self.widget(&UiEvent::PointerUp(e));
            }
            ViewInput::Wheel { dx, dy, mods, .. } => {
                // Architect's wheel is signed as the web's (positive scrolls
                // down); the panel's is winit's, as the web host turns it.
                self.panel(PanelEvent::Modifiers(panel_mods(mods)));
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
                self.panel(PanelEvent::Modifiers(panel_mods(mods)));
                let state = if *down {
                    KeyState::Pressed
                } else {
                    KeyState::Released
                };
                let e = key(name, mods, state);
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
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a pane's size in logical pixels"
        )]
        let (w, h) = (
            ctx.geometry.width.max(1.0) as u32,
            ctx.geometry.height.max(1.0) as u32,
        );
        let scene = {
            let widget = &self.widget;
            self.scope
                .run(|| widget.borrow_mut().paint_scene(w, h, 1.0))
        };
        let mut stats = self
            .stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        stats.frames += 1;
        stats.size = (w, h);
        if self.last.as_ref() != Some(&scene) {
            stats.changed += 1;
            stats.commands = scene.commands.len();
            self.content = Arc::new(scene.commands.clone());
            self.last = Some(scene);
        }
        let work = began.elapsed().as_secs_f64() * 1000.0;
        stats.work_ms.push_back(work);
        if stats.work_ms.len() > 600 {
            stats.work_ms.pop_front();
        }
        Some(vec![(SCENE, Layer::new(Arc::clone(&self.content)))])
    }
}
