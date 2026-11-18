//! `closeup`: one rack panel of a track (its EQ, its compressor, …) in a
//! pane of its own, the close-up `studio-desktop` lays over its window.
//!
//! The pane hosts Studio's own `session_ui::closeup::RackWidget`, the
//! widget the window's close-up layer mounts: nothing redrawn. Its state
//! names the track and the panel (`session_ui::closeup::rack_state`,
//! `<GUID>/<panel>`). The rack's settings are seeded from the song as the
//! mixer's are, and its edits go to the engine as the window's do.
//!
//! The first slice: the window's chrome around it (the header, the tabs,
//! back), opening it from a strip's expand mark, and sharing the mixer
//! pane's settings in memory are later ones (studio#525).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use architect_workspace::paint::{Layer, LayerKey, PaintMount, PaintView, Scene};
use architect_workspace::{ViewCtx, ViewInput};
use blitz_traits::events::{MouseEventButton, MouseEventButtons, UiEvent};
use dioxus::prelude::*;
use session_ui::closeup::RackWidget;
use session_ui::song_model::StudioSession;
use session_ui::tone::Which;
use studio_view_link::{InScope, SceneLayer, Stats, input};

/// The panel's picture: the pane's one layer.
pub const SCENE: LayerKey = 1;

/// The rack panel in a pane: Studio's widget, painted each frame (its
/// analyser and meters move while the song plays), its picture crossing
/// only when it changed.
pub struct HostedRack {
    rack: Rc<RefCell<RackWidget>>,
    scope: InScope,
    layer: SceneLayer,
    /// The buttons held now: a move carries them, as Blitz's do.
    held: MouseEventButtons,
}

impl HostedRack {
    #[must_use]
    pub fn new(rack: Rc<RefCell<RackWidget>>, scope: InScope, stats: Arc<Mutex<Stats>>) -> Self {
        Self {
            rack,
            scope,
            layer: SceneLayer::new(SCENE, stats),
            held: MouseEventButtons::empty(),
        }
    }

    /// The scene the widget last painted, as it painted it.
    #[must_use]
    pub const fn last_scene(&self) -> Option<&Scene> {
        self.layer.last()
    }

    fn widget(&self, event: &UiEvent) {
        let rack = &self.rack;
        self.scope.run(|| rack.borrow_mut().event(event));
    }
}

impl PaintView for HostedRack {
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
            ViewInput::Focus(false) => {
                self.held = MouseEventButtons::empty();
            }
            _ => return false,
        }
        true
    }

    fn paint(&mut self, _: &ViewCtx, _: &mut Scene) {}

    fn animating(&self, _: &ViewCtx) -> bool {
        true
    }

    fn layers(&mut self, ctx: &ViewCtx) -> Option<Vec<(LayerKey, Layer)>> {
        let began = web_time::Instant::now();
        let (w, h) = studio_view_link::pane_size(ctx);
        let scene = {
            let rack = &self.rack;
            self.scope.run(|| rack.borrow_mut().paint_scene(w, h, 1.0))
        };
        Some(self.layer.frame(scene, (w, h), began))
    }
}

/// The racks' settings for `session`'s tracks, as the mixer seeds them.
#[must_use]
pub fn settings(session: &StudioSession) -> Rc<session_ui::tone::Shared> {
    let tone = Rc::new(session_ui::tone::Shared::default());
    let rows = session.planner.plan_strips(&session.planner.raw);
    tone.store.borrow_mut().seed(rows.as_slice());
    tone
}

/// The pane: the panel its state names, once the engine has a song.
#[component]
pub fn CloseupPane() -> Element {
    let song = studio_view_link::use_song();
    let state = architect_extensions::linked::use_pane_state().map(|(_, s)| s);
    let surface = studio_view_link::surface();
    let rack = state
        .as_deref()
        .and_then(session_ui::closeup::rack_of_state);
    match (song(), rack) {
        (Some((key, session)), Some((guid, which))) => rsx! {
            div { style: "position:relative; width:100%; height:100%; background:{surface};",
                Rack {
                    key: "{key}-{guid}-{which:?}",
                    session: crate::Song(session),
                    guid,
                    which,
                }
            }
        },
        (None, _) => studio_view_link::waiting(),
        (Some(_), None) => rsx! {
            div { style: "padding:16px; font-size:13px; opacity:0.7; color:#ccc; background:{surface}; height:100%;",
                "No rack panel to show: open it as `<track GUID>/<panel>` (eq, comp, …)."
            }
        },
    }
}

#[component]
fn Rack(session: crate::Song, guid: String, which: Which) -> Element {
    let rack = use_hook(|| {
        Rc::new(RefCell::new(RackWidget::new(
            settings(&session.0),
            guid.clone(),
            which,
            false,
        )))
    });
    let stats = use_hook(|| crate::pane_stats("closeup"));
    let mount = use_hook(|| {
        let scope = InScope::here();
        let (rack, stats) = (Rc::clone(&rack), Arc::clone(&stats));
        PaintMount::new(move |_| {
            HostedRack::new(Rc::clone(&rack), scope.clone(), Arc::clone(&stats))
        })
    });
    let surface = studio_view_link::surface();
    rsx! {
        {studio_view_link::studio_style()}
        div {
            class: studio_view_link::ROOT_CLASS,
            style: "position:absolute; top:0; left:0; right:0; bottom:0; overflow:hidden; \
                    background:{surface};",
            architect_extensions::linked::PaintPane { mount }
        }
    }
}
