//! The arrangement installed into the window: its panel on dioxus-native
//! and in a browser, handed to the views that show it
//! (`crate::dock::ArrangementViews`). Call [`install`] once at start.

#[cfg(feature = "web")]
use std::cell::RefCell;
#[cfg(any(feature = "native", feature = "web"))]
use std::rc::Rc;

use dioxus::prelude::*;

/// Install the arrangement's views for every window of this process.
pub fn install() {
    crate::dock::install_arrangement(crate::dock::ArrangementViews {
        #[cfg(feature = "native")]
        arrangement: || rsx! { Arrangement {} },
        #[cfg(feature = "web")]
        web_arrangement: |engine| rsx! { WebArrangement { engine } },
    });
}

/// The arrangement panel, on dioxus-native: the widget as a Blitz custom
/// widget, winit's window events translated into [`crate::panel`]'s.
///
/// No props: the session comes from context ([`crate::studio::StudioSession`]), so the
/// same panel renders in any tile of any window.
#[cfg(feature = "native")]
#[component]
pub fn Arrangement() -> Element {
    use crate::panel::{Button, PanelEvent, WHEEL_LINE};
    let window = dioxus_native::use_window();
    let win = window.clone();
    // The pointer's shapes, the painted glyphs among them (`window_cursor`).
    let sink: crate::tool::Sink = crate::window_cursor::use_cursor_sink();
    let applier = use_hook(|| Rc::new(crate::engine::Applier::start()));
    let engine: crate::panel::Engine = {
        let applier = Rc::clone(&applier);
        Rc::new(move |edit| match applier.as_ref() {
            Some(applier) => applier.send(edit),
            None => tracing::debug!(?edit, "no engine to carry out the edit"),
        })
    };
    let (panel, widget) = crate::panel::use_arrangement_panel(
        Some(sink),
        engine,
        dioxus_native_dom::CustomWidgetAttr::new,
    );
    let transport = crate::engine::Transport::shared();
    let events = panel.clone();
    dioxus_native::use_window_event(move |event, _| {
        let panel = &events;
        match event {
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                // By the physical key, not the character: with Shift held
                // the character is "Z", and Shift+z is a zoom of its own.
                if event.physical_key
                    == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ)
                {
                    panel.handle(PanelEvent::ZKey {
                        pressed: event.state.is_pressed(),
                        repeat: event.repeat,
                    });
                }
            }
            winit::event::WindowEvent::ModifiersChanged(state) => {
                let keys = state.state();
                panel.handle(PanelEvent::Modifiers(crate::mousemap::Mods {
                    shift: keys.shift_key(),
                    ctrl: keys.control_key(),
                    alt: keys.alt_key(),
                }));
            }
            winit::event::WindowEvent::PointerMoved { position, .. } => {
                let (x, y) = crate::studio::css_point(&*win, position.x, position.y);
                panel.handle(PanelEvent::Pointer { x, y });
            }
            winit::event::WindowEvent::PointerButton {
                state,
                button,
                position,
                ..
            } => {
                // Where it is pressed, first: a finger has not moved there
                // beforehand, so the last position is where the previous
                // one lifted.
                let (x, y) = crate::studio::css_point(&*win, position.x, position.y);
                panel.handle(PanelEvent::Pointer { x, y });
                // Mouse buttons 5 (forward) and 4 (back): the take under the
                // mouse ranked up and down (the window reads where the
                // mouse is).
                let rank = match button {
                    winit::event::ButtonSource::Mouse(winit::event::MouseButton::Forward) => {
                        Some("SESSION_TAKE_UPRANK_MOUSE")
                    }
                    winit::event::ButtonSource::Mouse(winit::event::MouseButton::Back) => {
                        Some("SESSION_TAKE_DOWNRANK_MOUSE")
                    }
                    _ => None,
                };
                if let Some(id) = rank {
                    if state.is_pressed()
                        && let Err(e) = crate::actions::run(id, b"")
                    {
                        tracing::warn!(rank.error = %e, "mouse button: the take could not be ranked");
                    }
                    return;
                }
                let button = match button {
                    winit::event::ButtonSource::Mouse(winit::event::MouseButton::Middle) => {
                        Button::Middle
                    }
                    winit::event::ButtonSource::Mouse(winit::event::MouseButton::Left) => {
                        Button::Left
                    }
                    winit::event::ButtonSource::Mouse(_) => Button::Other,
                    _ => Button::Left,
                };
                panel.handle(PanelEvent::Button {
                    button,
                    pressed: state.is_pressed(),
                });
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => {
                        (f64::from(*x) * WHEEL_LINE, f64::from(*y) * WHEEL_LINE)
                    }
                    winit::event::MouseScrollDelta::PixelDelta(at) => {
                        crate::studio::css_delta(&*win, at.x, at.y)
                    }
                    // winit's delta is non-exhaustive: nothing else scrolls.
                    _ => (0.0, 0.0),
                };
                panel.handle(PanelEvent::Wheel { dx, dy });
            }
            // A pinch, on a touchscreen (see `touch::recognize_gestures`)
            // or a trackpad.
            winit::event::WindowEvent::PinchGesture { delta, .. } => {
                panel.handle(PanelEvent::Pinch { by: *delta });
            }
            winit::event::WindowEvent::RedrawRequested => {
                let play_at = transport.map_or(0.0, |t| t.read().0);
                panel.handle(PanelEvent::Frame { play_at });
            }
            _ => {}
        }
    });

    let colors = daw_ui::studio::lanes::Colors::from_theme(&daw_ui::theming::Theme::dark());
    let surface = colors.surface.clone();
    let mounted = Rc::clone(&panel.mounted);
    let focus_node = Rc::clone(&panel.focus_node);
    rsx! {
        div {
            // Filling the positioned tile it is in. Absolute rather than
            // `height:100%`: a percentage of a flex item's height does not
            // resolve in Blitz, and the panel came out zero tall.
            style: "position:absolute; top:0; left:0; right:0; bottom:0; overflow:hidden; \
                    background:{surface};",
            onmounted: move |event| {
                *mounted.borrow_mut() = Some(event.data());
            },
            object {
                style: "position:absolute; left:0; top:0; width:100%; height:100%;",
                // `<object>` is not in Blitz's default-focusable list, and
                // the arrangement takes the keyboard (a rename is a field
                // inside it).
                tabindex: "0",
                data: widget.clone(),
                // The keyboard is the arrangement's from the start: with
                // nothing focused, every shortcut waited for a first click.
                onmounted: move |event| {
                    let node = event.data();
                    *focus_node.borrow_mut() = Some(Rc::clone(&node));
                    async move {
                        let _ = node.set_focus(true).await;
                    }
                },
            }
            crate::panel::PanelChrome { panel: panel.clone() }
        }
    }
}

#[cfg(feature = "web")]
impl crate::web_host::Hosted for crate::widget::ArrangementWidget {
    fn paint(&mut self, width: u32, height: u32, scale: f64) -> anyrender::Scene {
        self.paint_scene(width, height, scale)
    }
    fn event(&mut self, event: &blitz_traits::events::UiEvent) {
        crate::widget::ArrangementWidget::event(self, event);
    }
}

#[cfg(feature = "web")]
/// The arrangement panel in a browser: [`crate::panel`]'s behaviour, the
/// widget in a canvas, the shared chrome over it.
#[component]
pub fn WebArrangement(engine: crate::web_engine::EngineRef) -> Element {
    let element = use_hook(|| Rc::new(RefCell::new(None::<web_sys::HtmlElement>)));
    let sink: crate::tool::Sink = {
        let element = Rc::clone(&element);
        // A browser canvas takes no painted cursor here: each glyph's
        // nearest CSS shape.
        Rc::new(move |shape: crate::window_cursor::Shape| {
            if let Some(el) = element.borrow().as_ref() {
                let _ = el.style().set_property("cursor", shape.icon().name());
            }
        })
    };
    let to_engine: crate::panel::Engine = {
        let engine = engine.clone();
        Rc::new(move |edit| engine.send(edit))
    };
    let (panel, widget) = crate::panel::use_arrangement_panel(Some(sink), to_engine, |w| {
        crate::web_host::HostedRef(Rc::new(RefCell::new(w)))
    });
    let colors = daw_ui::studio::lanes::Colors::from_theme(&daw_ui::theming::Theme::dark());
    let surface = colors.surface.clone();
    let mounted = Rc::clone(&panel.mounted);
    let (on_panel, on_frame) = (panel.clone(), panel.clone());
    let focus = crate::web_host::FocusSlot(Rc::clone(&panel.focus_node));
    let playhead = engine.clone();
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; right:0; bottom:0; overflow:hidden; \
                    background:{surface};",
            onmounted: move |event| {
                *mounted.borrow_mut() = Some(event.data());
            },
            crate::web_host::WidgetCanvas {
                widget,
                panel: move |e| on_panel.handle(e),
                frame: move |()| on_frame.handle(crate::panel_input::PanelEvent::Frame { play_at: playhead.position() }),
                focus,
                element: Some(crate::web_host::ElementSlot(Rc::clone(&element))),
            }
            crate::panel::PanelChrome { panel: panel.clone() }
        }
    }
}
