//! `studio.keys`: the keys rig's screens in Architect panes.
//!
//! Nothing is redrawn: each pane is a vox client of the rig engine
//! (`studio_rig_link::EnginePane`) around the keys rig's own components
//! (`keys::ui`), with the state they share in one screen of Signal
//! (`KeysRigRemote`) provided around each: the live keys state, the
//! pointer kind, the popups, and what is open ([`shared`]: the engine or
//! layer zoomed into, the node the browser is pointed at), kept in step
//! between this process's panes.
//!
//! One extension with nine views: the keys screens have one view state
//! and no panel seam of their own yet, as the guitar's have
//! (`guitar::ui::panels`); they split when that seam exists
//! (`docs/design/signal-extensions.md`).
//!
//! A keys pane starts the keys rig when it opens, as Signal's keys screen
//! does: opening it is the intent to play it. The rig's controls as
//! actions are the engine's (`STUDIO_RIG_KEYS_*`).

pub mod shared;

use architect_extensions::linked::{Builder, Extension};
use dioxus::prelude::*;
use keys::proto::keys::KeysRigClient;
use keys::ui::{
    Browser, ControlView, ControllerStrip, Controllers, EngineView, KeysViewState, LayerView,
    MacroPanel, MidiPanel, PerformStrip, RoutingView, Zoom, held_notes,
};
use serde_json::{Value as Json, json};
use studio_rig_link::{EnginePane, Owner};

pub const MANIFEST: &str = include_str!("../extension.toml");

/// The extension's id.
pub const ID: &str = "studio.keys";

/// The keys rig's accent, as its own screens use it.
const ACCENT: &str = "#a78bfa";

pub struct Keys;

impl Extension for Keys {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        studio_rig_link::set_handle(ID, ext.handle());
        ext.view("mixer", MixerPane)
            .view("layers", LayersPane)
            .view("routing", RoutingPane)
            .view("midi", MidiPane)
            .view("macros", MacrosPane)
            .view("controller", ControllerPane)
            .view("piano", PianoPane)
            .view("browser", BrowserPane)
            .view("switches", SwitchesPane)
            .action("STUDIO_KEYS_STATE", |_| async {
                let now = shared::get();
                Ok(json!({
                    "link": studio_rig_link::state(),
                    "open": shared::zoom_json(&now.zoom),
                    "selection": shared::selection_json(&now.selection),
                }))
            })
            .action("STUDIO_KEYS_OPEN", |args| async move {
                let text = |key: &str| args.get(key).and_then(Json::as_str).map(str::to_owned);
                let zoom = match (text("engine"), text("layer")) {
                    (_, Some(layer)) => Zoom::Layer(layer),
                    (Some(engine), None) => Zoom::Engine(engine),
                    (None, None) => Zoom::Mixer,
                };
                shared::put(|s| s.zoom = zoom);
                Ok(shared::zoom_json(&shared::get().zoom))
            });
    }
}

/// A keys pane: `children` under the engine's clients and the keys rig's
/// shared state.
#[component]
fn KeysPane(children: Element) -> Element {
    rsx! {
        EnginePane { owner: Owner(ID),
            Providers { {children} }
        }
    }
}

/// What `KeysRigRemote` provides around its screens, around one pane.
#[component]
fn Providers(children: Element) -> Element {
    let (state, rig) = keys::ui::use_keys_state();
    // Whether a finger or a mouse plays the rig, and menus over the pane.
    let touch = signal_widgets::Touch::provide();
    signal_widgets::PopupHost::provide();
    let mut zoom = use_context_provider(|| Signal::new(shared::get().zoom));
    let mut selection = use_context_provider(|| Signal::new(shared::get().selection));
    use_context_provider(|| state);
    // In step with the process's other keys panes.
    use_effect(move || shared::put(|s| s.zoom = zoom()));
    use_effect(move || shared::put(|s| s.selection = selection()));
    use_future(move || async move {
        let mut changes = shared::subscribe();
        loop {
            if changes.changed().await.is_err() {
                return;
            }
            let now = shared::get();
            if *zoom.peek() != now.zoom {
                zoom.set(now.zoom);
            }
            if *selection.peek() != now.selection {
                selection.set(now.selection);
            }
        }
    });
    if rig.is_none() {
        return rsx! {
            div { style: "flex:1; display:flex; align-items:center; justify-content:center; padding:16px; \
                          font-size:13px; color:#8e8e98;",
                "This rig engine does not serve the keys rig."
            }
        };
    }
    rsx! {
        div {
            style: "position:relative; display:flex; flex-direction:column; flex:1; min-width:0; \
                    min-height:0; color:#e4e4e7; font-family:sans-serif; background:#08080a;",
            onpointerdown: move |e: PointerEvent| touch.observe(&e),
            {children}
            signal_widgets::PopupLayer {}
        }
    }
}

/// The mixer: every engine and its layers, the master, the macro band and
/// the keyboard.
#[component]
fn MixerPane() -> Element {
    rsx! { KeysPane { Mixer {} } }
}

#[component]
fn Mixer() -> Element {
    let state = use_context::<KeysViewState>();
    let midi = state.midi.read().clone();
    rsx! {
        ControlView {
            mixer: state.mixer.read().clone(),
            held: held_notes(&midi),
            controllers: Controllers::from_midi(&midi),
        }
    }
}

/// The engine or the layer opened (a card's open button in the mixer, or
/// `STUDIO_KEYS_OPEN`); the engines and their layers to open until one is.
#[component]
fn LayersPane() -> Element {
    rsx! { KeysPane { Layers {} } }
}

#[component]
fn Layers() -> Element {
    let state = use_context::<KeysViewState>();
    let mut zoom = use_context::<Signal<Zoom>>();
    let mixer = state.mixer.read().clone();
    let shown = zoom();
    match shown {
        Zoom::Layer(layer) => rsx! { LayerView { layer, zoom, mixer } },
        Zoom::Engine(name) if mixer.engines.iter().any(|e| e.name == name) => {
            let engine = mixer.engines.iter().find(|e| e.name == name).cloned();
            match engine {
                Some(engine) => rsx! { EngineView { engine, zoom } },
                None => rsx! {},
            }
        }
        Zoom::Mixer | Zoom::Engine(_) => rsx! {
            div { style: "flex:1; min-height:0; overflow-y:scroll; display:flex; flex-direction:column; \
                          gap:14px; padding:16px;",
                span { style: "font-size:13px; color:#8e8e98;", "Open an engine or one of its layers." }
                for engine in mixer.engines {
                    div { key: "{engine.name}", style: "display:flex; flex-direction:column; gap:6px;",
                        {
                            let name = engine.name.clone();
                            rsx! {
                                Open { label: engine.name.clone(), strong: true, onclick: move |()| zoom.set(Zoom::Engine(name.clone())) }
                            }
                        }
                        div { style: "display:flex; flex-wrap:wrap; gap:6px; padding-left:12px;",
                            for layer in engine.layers {
                                {
                                    let name = layer.name.clone();
                                    rsx! {
                                        Open { key: "{layer.name}", label: layer.name.clone(), strong: false, onclick: move |()| zoom.set(Zoom::Layer(name.clone())) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
    }
}

/// A row that opens an engine or a layer.
#[component]
fn Open(label: String, strong: bool, onclick: EventHandler<()>) -> Element {
    let weight = if strong { 750 } else { 600 };
    rsx! {
        button {
            style: "align-self:flex-start; height:34px; padding:0 14px; border-radius:8px; \
                    border:1px solid #3f3f46; background:transparent; color:#e4e4e7; font-size:13px; \
                    font-weight:{weight}; font-family:inherit; cursor:pointer;",
            onclick: move |_| onclick.call(()),
            "{label}"
        }
    }
}

/// The rig as it routes: engines, layers and modules to the master.
#[component]
fn RoutingPane() -> Element {
    rsx! { KeysPane { Routing {} } }
}

#[component]
fn Routing() -> Element {
    let state = use_context::<KeysViewState>();
    rsx! {
        div { style: "flex:1; min-height:0; overflow-y:scroll;",
            RoutingView { tree: state.tree.read().clone() }
        }
    }
}

/// The MIDI monitor and the port the rig listens to.
#[component]
fn MidiPane() -> Element {
    rsx! { KeysPane { Midi {} } }
}

#[component]
fn Midi() -> Element {
    let state = use_context::<KeysViewState>();
    rsx! {
        div { style: "flex:1; min-height:0; overflow-y:scroll;",
            MidiPanel { midi: state.midi.read().clone(), port: state.status.read().midi_port.clone() }
        }
    }
}

/// The rig's macros: the knobs over every engine.
#[component]
fn MacrosPane() -> Element {
    rsx! { KeysPane { Macros {} } }
}

#[component]
fn Macros() -> Element {
    let state = use_context::<KeysViewState>();
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    // Read again when the rig publishes its mixer (a preset, a stack).
    let macros = {
        let rig = rig.clone();
        use_resource(move || {
            let rig = rig.clone();
            let _published = state.mixer.read().engines.len();
            async move {
                match rig {
                    Some(rig) => rig.rig_macros().await.unwrap_or_default(),
                    None => Vec::new(),
                }
            }
        })
    };
    let shown = macros.read().clone().unwrap_or_default();
    rsx! {
        div { style: "flex:1; min-height:0; display:flex; flex-direction:column; justify-content:center; padding:8px;",
            if shown.is_empty() {
                span { style: "font-size:13px; color:#8e8e98; text-align:center;", "The rig playing has no macros." }
            } else {
                MacroPanel {
                    macros: shown,
                    accent: ACCENT.to_owned(),
                    on_change: move |(id, value): (String, f32)| {
                        let rig = rig.clone();
                        spawn(async move {
                            if let Some(rig) = rig {
                                let _ = rig.set_rig_global(id, value).await;
                            }
                        });
                    },
                }
            }
        }
    }
}

/// Where the wheels and pedals are sitting.
#[component]
fn ControllerPane() -> Element {
    rsx! { KeysPane { Controller {} } }
}

#[component]
fn Controller() -> Element {
    let state = use_context::<KeysViewState>();
    let controllers = Controllers::from_midi(&state.midi.read());
    rsx! {
        div { style: "flex:1; min-height:0; display:flex; align-items:center; justify-content:center; padding:8px;",
            ControllerStrip { controllers }
        }
    }
}

/// The keyboard: the keys held light up, a press plays the rig.
#[component]
fn PianoPane() -> Element {
    rsx! { KeysPane { Keyboard {} } }
}

#[component]
fn Keyboard() -> Element {
    let state = use_context::<KeysViewState>();
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let held = held_notes(&state.midi.read());
    let play = use_callback(move |(note, velocity): (u8, u32)| {
        let rig = rig.clone();
        spawn(async move {
            if let Some(rig) = rig {
                let _ = rig.trigger(u32::from(note), velocity).await;
            }
        });
    });
    rsx! {
        div { style: "flex:1; min-height:0; display:flex; flex-direction:column; justify-content:center; padding:8px;",
            signal_widgets::Piano {
                start_note: 21,
                end_note: 108,
                active_notes: held,
                show_labels: false,
                waterfall: false,
                accent_color: ACCENT.to_owned(),
                height: "120px",
                on_note_on: move |note: u8| play.call((note, 100)),
                on_note_off: move |note: u8| play.call((note, 0)),
            }
        }
    }
}

/// The browser, pointed at what is selected in the mixer.
#[component]
fn BrowserPane() -> Element {
    rsx! { KeysPane { Sounds {} } }
}

#[component]
fn Sounds() -> Element {
    let state = use_context::<KeysViewState>();
    rsx! {
        div { style: "flex:1; min-height:0; display:flex;", Browser { state } }
    }
}

/// The keys rig's switches: its stacks, a press plays one.
#[component]
fn SwitchesPane() -> Element {
    rsx! { KeysPane { Switches {} } }
}

#[component]
fn Switches() -> Element {
    let state = use_context::<KeysViewState>();
    rsx! {
        div { style: "flex:1; min-height:0; display:flex; flex-direction:column; justify-content:flex-end;",
            PerformStrip { perform: state.perform.read().clone() }
        }
    }
}
