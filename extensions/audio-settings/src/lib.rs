//! `studio.audio-settings`: one settings surface for Studio's two engines.
//!
//! - [`engines`]: both engines' audio read and set (the Studio engine
//!   through `DAW_AUDIO`, the rig engine through its `AudioSettings`
//!   service), and the actions over it.
//! - `audio`: each engine's device, sample rate and buffer, then the rig's
//!   own Audio page (its interface, outputs and test tones).
//! - `midi`: what the rig's switches are learned to, then the rig's own
//!   MIDI page (controllers and what arrives from them).
//! - `instrument`: the rig's own Guitar page (the guitars, their tone,
//!   gates and level).
//! - `settings`: all of it on one page, drawn in Architect's Settings
//!   (its `[[sections]]` names this view).
//!
//! The rig's pages are Signal's Setup tabs (`guitar::ui::panels::Setup`),
//! not redrawn.

pub mod engines;

use architect_extensions::linked::{Builder, Extension};
use dioxus::prelude::*;
use guitar::ui::panels::SetupTab;
use serde_json::Value as Json;
use studio_rig_link::{Owner, RigPane};

use crate::engines::{Change, Target};

pub const MANIFEST: &str = include_str!("../extension.toml");

/// The extension's id.
pub const ID: &str = "studio.audio-settings";

/// The setting that keeps one output for both engines.
pub const LINK_KEY: &str = "studio.audio-settings.link_engines";

pub struct AudioSettings;

impl Extension for AudioSettings {
    fn manifest(&self) -> &'static str {
        MANIFEST
    }

    fn build(&self, ext: &mut Builder) {
        studio_rig_link::set_handle(ID, ext.handle());
        ext.view("settings", SettingsPage)
            .view("audio", AudioPane)
            .view("midi", MidiPane)
            .view("instrument", InstrumentPane)
            .action("STUDIO_AUDIO_SETTINGS_STATE", |_| async {
                Ok(engines::state().await)
            })
            .action("STUDIO_AUDIO_SETTINGS_SET", |args| async move {
                let target = Target::parse(args.get("engine").and_then(Json::as_str))?;
                let change = Change::from_args(&args)?;
                engines::set(target, &change).await?;
                Ok(engines::state().await)
            });
    }
}

const PAGE: &str = "width:100%; height:100%; min-height:0; display:flex; flex-direction:column; \
                    overflow-y:scroll; background:#0f0f12; color:#e4e4e7; \
                    font-family: Inter, -apple-system, sans-serif;";
const INK_2: &str = "#a1a1aa";
const INK_3: &str = "#71717a";
const RULE: &str = "#26262c";

/// All of it on one page: the engines' devices, then the rig's Setup
/// (its Guitar, Audio and MIDI tabs).
#[component]
fn SettingsPage() -> Element {
    rsx! {
        div { style: PAGE,
            Engines {}
            div { style: "flex:1; min-height:520px; display:flex; border-top:1px solid {RULE};",
                RigPane { owner: Owner(ID),
                    guitar::ui::panels::Setup { tab: SetupTab::Audio }
                }
            }
        }
    }
}

#[component]
fn AudioPane() -> Element {
    rsx! { SettingsPage {} }
}

#[component]
fn MidiPane() -> Element {
    rsx! {
        div { style: PAGE,
            div { style: "flex:1; min-height:0; display:flex;",
                RigPane { owner: Owner(ID),
                    div { style: "flex:1; min-width:0; min-height:0; display:flex; flex-direction:column;",
                        Learn {}
                        guitar::ui::panels::Setup { tab: SetupTab::Midi }
                    }
                }
            }
        }
    }
}

#[component]
fn InstrumentPane() -> Element {
    rsx! {
        div { style: PAGE,
            div { style: "flex:1; min-height:0; display:flex;",
                RigPane { owner: Owner(ID),
                    guitar::ui::panels::Setup { tab: SetupTab::Guitar }
                }
            }
        }
    }
}

/// The sample rates offered (0: the device's own).
const RATES: [u32; 5] = [0, 44_100, 48_000, 88_200, 96_000];
/// The buffers offered, frames (0: the backend's default).
const BUFFERS: [u32; 7] = [0, 32, 64, 128, 256, 512, 1024];

/// Each engine's device, sample rate and buffer.
#[component]
fn Engines() -> Element {
    let mut state = use_signal(|| None::<Json>);
    let mut problem = use_signal(String::new);
    let refresh = use_callback(move |()| {
        spawn(async move {
            state.set(Some(engines::state().await));
        });
    });
    use_hook(|| refresh.call(()));
    let set = use_callback(move |(target, change): (Target, Change)| {
        spawn(async move {
            match engines::set(target, &change).await {
                Ok(()) => problem.set(String::new()),
                Err(why) => problem.set(why),
            }
            refresh.call(());
        });
    });
    let Some(now) = state() else {
        return rsx! {
            div { style: "padding:16px; font-size:13px; color:{INK_3};", "Reading the engines' audio…" }
        };
    };
    let link = now.get("link").and_then(Json::as_bool).unwrap_or(true);
    let studio = now.get("studio").cloned().unwrap_or(Json::Null);
    let rig = now.get("rig").cloned().unwrap_or(Json::Null);
    let text = |v: &Json, path: &[&str]| -> String {
        path.iter()
            .try_fold(v, |v, key| v.get(key))
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let number = |v: &Json, path: &[&str]| -> u32 {
        path.iter()
            .try_fold(v, |v, key| v.get(key))
            .and_then(Json::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0)
    };
    let names = |v: &Json, path: &[&str]| -> Vec<String> {
        path.iter()
            .try_fold(v, |v, key| v.get(key))
            .and_then(Json::as_array)
            .map(|all| {
                all.iter()
                    .filter_map(|d| d.get("name").and_then(Json::as_str))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    let rig_inputs = names(&rig, &["devices", "inputs"]);
    let rig_input = text(&rig, &["prefs", "input_device"]);
    // The chosen input's channels (two when the engine does not say).
    let input_channels = rig
        .get("devices")
        .and_then(|d| d.get("inputs"))
        .and_then(Json::as_array)
        .and_then(|all| {
            all.iter().find(|d| {
                d.get("name")
                    .and_then(Json::as_str)
                    .is_some_and(|n| !rig_input.is_empty() && n.contains(rig_input.as_str()))
            })
        })
        .and_then(|d| d.get("channels"))
        .and_then(Json::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(2)
        .clamp(1, 32);
    rsx! {
        div { style: "flex-shrink:0; display:flex; flex-direction:column; gap:18px; padding:16px 18px;",
            if !problem().is_empty() {
                div { style: "font-size:13px; color:#f87171;", "{problem}" }
            }
            Engine {
                title: "Studio engine",
                detail: "Songs and sets",
                error: text(&studio, &["error"]),
                note: String::new(),
                Row { label: "Output",
                    Devices {
                        names: names(&studio, &["devices"]),
                        current: text(&studio, &["device", "output_device"]),
                        on_pick: move |name: String| set.call((Target::Studio, Change { output_device: Some(name), ..Change::default() })),
                    }
                }
                Row { label: "Sample rate",
                    Numbers {
                        options: RATES.to_vec(),
                        current: number(&studio, &["device", "sample_rate"]),
                        zero: "Device",
                        unit: "Hz",
                        on_pick: move |rate: u32| set.call((Target::Studio, Change { sample_rate: Some(rate), ..Change::default() })),
                    }
                }
                Row { label: "Buffer",
                    Numbers {
                        options: BUFFERS.to_vec(),
                        current: number(&studio, &["device", "buffer_size"]),
                        zero: "Default",
                        unit: "",
                        on_pick: move |frames: u32| set.call((Target::Studio, Change { buffer_size: Some(frames), ..Change::default() })),
                    }
                }
            }
            Engine {
                title: "Rig engine",
                detail: "Guitar and keys",
                error: text(&rig, &["error"]),
                note: if rig.get("running").and_then(Json::as_bool).unwrap_or(false) {
                    "Audio is running: a change reopens the device.".to_owned()
                } else {
                    text(&rig, &["audio_error"])
                },
                Row { label: "Output",
                    Devices {
                        names: names(&rig, &["devices", "outputs"]),
                        current: text(&rig, &["prefs", "output_device"]),
                        on_pick: move |name: String| set.call((Target::Rig, Change { output_device: Some(name), ..Change::default() })),
                    }
                }
                Row { label: "Input",
                    Devices {
                        names: rig_inputs,
                        current: rig_input.clone(),
                        on_pick: move |name: String| set.call((Target::Rig, Change { input_device: Some(name), ..Change::default() })),
                    }
                }
                Row { label: "Input channel",
                    Numbers {
                        options: (1..=input_channels).collect::<Vec<u32>>(),
                        current: number(&rig, &["prefs", "input_channel"]).saturating_add(1),
                        zero: "",
                        unit: "",
                        on_pick: move |channel: u32| set.call((Target::Rig, Change { input_channel: Some(channel.saturating_sub(1)), ..Change::default() })),
                    }
                }
                Row { label: "Sample rate",
                    Numbers {
                        options: RATES.to_vec(),
                        current: number(&rig, &["prefs", "sample_rate"]),
                        zero: "Device",
                        unit: "Hz",
                        on_pick: move |rate: u32| set.call((Target::Rig, Change { sample_rate: Some(rate), ..Change::default() })),
                    }
                }
                Row { label: "Buffer",
                    Numbers {
                        options: BUFFERS.to_vec(),
                        current: number(&rig, &["prefs", "buffer_size"]),
                        zero: "Default",
                        unit: "",
                        on_pick: move |frames: u32| set.call((Target::Rig, Change { buffer_size: Some(frames), ..Change::default() })),
                    }
                }
            }
            div { style: "display:flex; align-items:center; gap:10px;",
                Chip {
                    label: if link { "One output for both engines: on" } else { "One output for both engines: off" },
                    on: link,
                    onclick: move |()| {
                        spawn(async move {
                            let _ = studio_rig_link::set_setting(ID, LINK_KEY, Json::Bool(!link)).await;
                            refresh.call(());
                        });
                    },
                }
                span { style: "font-size:12px; color:{INK_3};",
                    "On: the Studio engine takes the rig's output device and sample rate when they change."
                }
            }
        }
    }
}

/// One engine's block: its name, what it plays, why it does not answer.
#[component]
fn Engine(
    title: String,
    detail: String,
    error: String,
    note: String,
    children: Element,
) -> Element {
    rsx! {
        div { style: "display:flex; flex-direction:column; gap:8px;",
            div { style: "display:flex; align-items:baseline; gap:10px;",
                span { style: "font-size:15px; font-weight:750; color:#fafafa;", "{title}" }
                span { style: "font-size:12px; color:{INK_3};", "{detail}" }
                if !note.is_empty() {
                    span { style: "font-size:12px; color:{INK_2};", "{note}" }
                }
            }
            if error.is_empty() {
                {children}
            } else {
                div { style: "font-size:13px; color:{INK_3};", "Not answering: {error}" }
            }
        }
    }
}

/// A labelled row of choices.
#[component]
fn Row(label: String, children: Element) -> Element {
    rsx! {
        div { style: "display:flex; align-items:flex-start; gap:12px;",
            span { style: "flex-shrink:0; width:110px; padding-top:7px; font-size:12px; font-weight:650; \
                           letter-spacing:0.04em; color:{INK_2};",
                "{label}"
            }
            div { style: "flex:1; min-width:0; display:flex; flex-wrap:wrap; gap:6px;", {children} }
        }
    }
}

/// The devices to choose from, the system's default first. `current` is
/// part of a name, as the engines keep it.
#[component]
fn Devices(names: Vec<String>, current: String, on_pick: EventHandler<String>) -> Element {
    let known = names
        .iter()
        .any(|n| !current.is_empty() && n.contains(current.as_str()));
    rsx! {
        Chip { label: "System default", on: current.is_empty(), onclick: move |()| on_pick.call(String::new()) }
        for name in names {
            Chip {
                key: "{name}",
                label: name.clone(),
                on: !current.is_empty() && name.contains(current.as_str()),
                onclick: move |()| on_pick.call(name.clone()),
            }
        }
        // Kept, but not plugged in now: shown so it is not lost silently.
        if !current.is_empty() && !known {
            Chip { label: format!("{current} (not connected)"), on: true, onclick: move |()| {} }
        }
    }
}

/// Numbers to choose from; `zero` names the 0 choice.
#[component]
fn Numbers(
    options: Vec<u32>,
    current: u32,
    zero: &'static str,
    unit: &'static str,
    on_pick: EventHandler<u32>,
) -> Element {
    rsx! {
        for value in options {
            Chip {
                key: "{value}",
                label: if value == 0 { zero.to_owned() } else if unit.is_empty() { value.to_string() } else { format!("{value} {unit}") },
                on: value == current,
                onclick: move |()| on_pick.call(value),
            }
        }
    }
}

/// One choice: filled when it is the one in use.
#[component]
fn Chip(label: String, on: bool, onclick: EventHandler<()>) -> Element {
    let (background, ink, edge) = if on {
        ("#fafafa", "#0f0f12", "#fafafa")
    } else {
        ("transparent", "#d4d4d8", "#3f3f46")
    };
    rsx! {
        button {
            style: "height:32px; padding:0 12px; border-radius:8px; border:1px solid {edge}; \
                    background:{background}; color:{ink}; font-size:13px; font-weight:600; \
                    font-family:inherit; cursor:pointer; white-space:nowrap;",
            onclick: move |_| onclick.call(()),
            "{label}"
        }
    }
}

/// What the rig's switches are learned to: each target with what it
/// answers to, a press to learn it from the next pedal or pad, another to
/// forget it.
#[component]
fn Learn() -> Element {
    let state = try_use_context::<guitar::ui::RigViewState>();
    let rig = use_hook(try_consume_context::<guitar::proto::rig::RigClient>);
    let (Some(state), Some(rig)) = (state, rig) else {
        return rsx! {};
    };
    let perf = state.perf.read().clone();
    let learn = perf.learn.clone();
    let mut targets: Vec<(String, String)> = perf
        .stacks
        .iter()
        .enumerate()
        .map(|(i, stack)| (format!("stack:{i}"), stack.name.clone()))
        .collect();
    for (target, name) in [
        ("tap", "Tap tempo"),
        ("fx", "FX toggle"),
        ("boost", "Boost"),
        ("tuner", "Tuner"),
    ] {
        targets.push((target.to_owned(), name.to_owned()));
    }
    rsx! {
        div { style: "flex-shrink:0; display:flex; flex-direction:column; gap:6px; padding:14px 18px; \
                      border-bottom:1px solid {RULE};",
            div { style: "display:flex; align-items:baseline; gap:10px;",
                span { style: "font-size:15px; font-weight:750; color:#fafafa;", "MIDI learn" }
                span { style: "font-size:12px; color:{INK_3};",
                    "Press Learn, then the pedal or pad that should do it."
                }
            }
            for (target, name) in targets {
                {
                    let bound = learn.binding(&target).map(str::to_owned);
                    let waiting = learn.learning.as_deref() == Some(target.as_str());
                    let (rig_learn, rig_forget) = (rig.clone(), rig.clone());
                    let (t_learn, t_forget) = (target.clone(), target.clone());
                    rsx! {
                        div { key: "{target}", style: "display:flex; align-items:center; gap:10px; min-height:34px;",
                            span { style: "width:150px; font-size:13px; font-weight:650; color:#e4e4e7;", "{name}" }
                            span { style: "flex:1; min-width:0; font-size:13px; color:{INK_2};",
                                if waiting { "Waiting for a pedal or pad…" } else { {bound.clone().unwrap_or_else(|| "Not learned".to_owned())} }
                            }
                            Chip {
                                label: if waiting { "Cancel" } else { "Learn" },
                                on: waiting,
                                onclick: move |()| {
                                    let (rig, target) = (rig_learn.clone(), t_learn.clone());
                                    spawn(async move {
                                        let _ = if waiting { rig.midi_learn_cancel().await } else { rig.midi_learn(target).await };
                                    });
                                },
                            }
                            if bound.is_some() {
                                Chip {
                                    label: "Forget",
                                    on: false,
                                    onclick: move |()| {
                                        let (rig, target) = (rig_forget.clone(), t_forget.clone());
                                        spawn(async move {
                                            let _ = rig.midi_unlearn(target).await;
                                        });
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
