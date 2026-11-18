//! The mixer widget in an Architect pane: the mixer's third host (after
//! dioxus-native's `mixer_panel::Mixer` and the browser's `WebMixer`).
//!
//! Nothing here draws. Each frame the pane asks the widget for its picture
//! with the meters' moving parts apart (`MixerWidget::paint_layers`, which
//! catches up with the engine — a fold, the racks switch — as it does in
//! the window): the picture is the pane's layer, crossing only when it
//! changed; the meters' parts are layers the host moves by the engine's
//! values (`engine.meter.<track>.*`, written by the `daw` extension into
//! its shared table), so a playing meter crosses nothing; the caps go over
//! them.
//! Pointer and keys go to the widget as Blitz `UiEvent`s; the wheel runs
//! along the strips, as the window's does.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use anyrender::recording::RenderCommand;
use architect_scene::anim::{Animation, Input, Linear, Name, Property};

use architect_workspace::paint::{Layer, LayerKey, PaintView, Scene};
use architect_workspace::{ViewCtx, ViewInput};
use blitz_traits::events::{KeyState, MouseEventButton, MouseEventButtons, UiEvent};
use session_ui::mixer_panel::HostedMixer;
use studio_view_link::{InScope, SceneLayer, Stats, input};

/// The picture: the strips with their meters fully lit.
pub const SCENE: LayerKey = 1;
/// Over the meters' parts: the caps, their readouts, the clip latches.
pub const ABOVE: LayerKey = 2;
/// The meters' parts, from here up, one key each.
pub const PARTS: LayerKey = 1000;

pub struct Hosted {
    mixer: Rc<HostedMixer>,
    scope: InScope,
    layer: SceneLayer,
    /// Each part's commands as last sent, by key: the same commands keep
    /// the same content, so a part crosses once.
    sent: HashMap<LayerKey, Arc<Vec<RenderCommand>>>,
    /// The layers last given, with the picture and what is over the
    /// meters they were made for: the parts only change with the layout,
    /// which the picture shows, so the same two keep them.
    kept: Option<(
        Arc<Vec<RenderCommand>>,
        Vec<RenderCommand>,
        Vec<(LayerKey, Layer)>,
    )>,
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
            sent: HashMap::new(),
            kept: None,
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

impl Hosted {
    /// `commands` as layer `key`'s content: what it had when they are the
    /// same, so nothing crosses.
    fn content(&mut self, key: LayerKey, commands: &[RenderCommand]) -> Arc<Vec<RenderCommand>> {
        match self.sent.get(&key) {
            Some(was) if was.as_slice() == commands => Arc::clone(was),
            _ => {
                let now = Arc::new(commands.to_vec());
                self.sent.insert(key, Arc::clone(&now));
                now
            }
        }
    }
}

/// A part's move as the host's animation of it.
fn animation(m: &session_ui::overlay::ValueMove) -> Option<Animation> {
    use session_ui::overlay::ValueProperty;
    let name = Name::new(&m.name).ok()?;
    let property = match m.property {
        ValueProperty::Y => Property::Y,
        ValueProperty::Opacity => Property::Opacity,
        ValueProperty::Visible => Property::Visible,
    };
    let mut linear = Linear::new(m.scale, m.offset);
    linear.min = m.min;
    linear.max = m.max;
    Some(Animation::linear(property, Input::Value(name), linear))
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
        let (scene, apart) = {
            let widget = &self.mixer.widget;
            self.scope
                .run(|| widget.borrow_mut().paint_layers(w, h, 1.0))
        };
        let mut layers = self.layer.frame(scene, (w, h), began);
        if let (Some((picture, above, kept)), Some((_, now))) = (&self.kept, layers.first())
            && Arc::ptr_eq(picture, &now.commands)
            && *above == apart.above.commands
        {
            return Some(kept.clone());
        }
        let picture = layers.first().map(|(_, l)| Arc::clone(&l.commands));
        let mut keep = Vec::with_capacity(apart.layers.len() + 1);
        for (i, part) in apart.layers.iter().enumerate() {
            let key = PARTS + LayerKey::try_from(i).unwrap_or(LayerKey::MAX - PARTS);
            let mut layer = Layer::new(self.content(key, &part.scene.commands))
                .animated(part.moves.iter().filter_map(animation).collect::<Vec<_>>());
            let at = part.transform.unwrap_or(apart.transform);
            layer.transform = at;
            layer.clip = part.clip.map(|clip| at.transform_rect_bbox(clip));
            layer.z = if part.over { 3 } else { 1 };
            keep.push(key);
            layers.push((key, layer));
        }
        let mut above = Layer::new(self.content(ABOVE, &apart.above.commands));
        above.transform = apart.transform;
        above.z = 2;
        keep.push(ABOVE);
        layers.push((ABOVE, above));
        self.sent.retain(|key, _| keep.contains(key));
        if let Some(picture) = picture {
            self.kept = Some((picture, apart.above.commands.clone(), layers.clone()));
        }
        Some(layers)
    }
}
