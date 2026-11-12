//! Studio's island (studio#445): Session's song progress bar and transport
//! controls (`transport_ui`, the same components Session draws), as a WASI
//! component a share link's now-playing view runs in the page's sandbox.
//!
//! Its two views are mounted by the page's player at the top of the view
//! (`progress`) and under the chart (`transport`). Each is given, as its
//! embed input's `props`, where the song is: `time`, `playing`,
//! `duration`, `title`, and its `sections` (`[{name, start, end, color}]`,
//! seconds). It asks the page only `PLAYHEAD_SEEK` (`{t}`),
//! `PLAYHEAD_PLAY`, `PLAYHEAD_PAUSE`, `PLAYHEAD_NEXT`, `PLAYHEAD_PREVIOUS`.
//! Built for `wasm32-wasip2`; empty anywhere else.
#![cfg(target_arch = "wasm32")]

use architect_extension_wasi::{Extension, embed, host};
use dioxus::prelude::*;
use serde_json::{Value, json};
use transport_ui::{ProgressSection, SongProgressBar, TransportControlBar};

/// Where the song is, as the page gave it.
#[derive(Clone, Debug, Default, PartialEq)]
struct Playhead {
    time: f64,
    playing: bool,
    duration: f64,
    sections: Vec<(String, f64, f64, String)>,
}

fn number(v: &Value, key: &str) -> f64 {
    v.get(key)
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .unwrap_or(0.0)
}

fn playhead() -> Playhead {
    let props = embed().map_or(Value::Null, |e| e.props);
    let sections = props
        .get("sections")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|s| {
                    (
                        s.get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        number(s, "start"),
                        number(s, "end"),
                        s.get("color")
                            .and_then(Value::as_str)
                            .unwrap_or("#64748b")
                            .to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    Playhead {
        time: number(&props, "time"),
        playing: props
            .get("playing")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        duration: number(&props, "duration"),
        sections,
    }
}

/// The colour of a song without sections: one slate fill the whole length.
const WHOLE_SONG: &str = "#94a3b8";

/// Asks the page `action` (one of its five intents).
fn ask(action: &str, args: &Value) {
    let _ = host::run(action, args);
}

/// `m:ss` of `seconds`.
fn clock(seconds: f64) -> String {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::as_conversions
    )]
    let whole = seconds.max(0.0).floor() as u64;
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// The song's sections as the bar draws them: a part of its length each.
/// A song without sections is one unnamed section the whole length, so
/// the bar still shows how much has played (the bar darkens what has not,
/// over its sections' colours: with none, there would be nothing to see).
fn sections(p: &Playhead) -> Vec<ProgressSection> {
    if p.duration <= 0.0 {
        return Vec::new();
    }
    if p.sections.is_empty() {
        return vec![ProgressSection {
            start_percent: 0.0,
            end_percent: 100.0,
            color: WHOLE_SONG.to_owned(),
            name: String::new(),
            short_name: String::new(),
            comment: None,
        }];
    }
    p.sections
        .iter()
        .map(|(name, start, end, color)| ProgressSection {
            start_percent: (start / p.duration * 100.0).clamp(0.0, 100.0),
            end_percent: (end / p.duration * 100.0).clamp(0.0, 100.0),
            color: color.clone(),
            name: name.clone(),
            short_name: name.chars().take(6).collect(),
            comment: None,
        })
        .collect()
}

/// The top of the now-playing view: Session's progress bar (sections as
/// played, the playhead, a tap on a section seeks to it), and the time.
#[component]
fn Progress() -> Element {
    let p = playhead();
    let progress = if p.duration > 0.0 {
        (p.time / p.duration * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let starts: Vec<f64> = p.sections.iter().map(|s| s.1).collect();
    let left = (p.duration - p.time).max(0.0);
    rsx! {
        div { class: "studio-island-progress", style: "display:flex; flex-direction:column; gap:4px; width:100%;",
            SongProgressBar {
                progress,
                sections: sections(&p),
                height: Some("2.75rem".to_owned()),
                on_section_click: move |i: usize| {
                    if let Some(t) = starts.get(i) {
                        ask("PLAYHEAD_SEEK", &json!({ "t": t }));
                    }
                },
            }
            div { style: "display:flex; justify-content:space-between; font-size:12px; opacity:0.7; font-variant-numeric:tabular-nums;",
                span { "{clock(p.time)}" }
                span { "-{clock(left)}" }
            }
        }
    }
}

/// Under the chart: Session's transport (previous / play / pause / next),
/// what a recording on a page can do: no record, no loop.
#[component]
fn Transport() -> Element {
    let p = playhead();
    let playing = p.playing;
    rsx! {
        div { class: "studio-island-transport", style: "height:3.5rem; width:100%;",
            TransportControlBar {
                is_playing: playing,
                is_looping: false,
                is_recording: false,
                is_armed: false,
                show_recording: false,
                show_loop: false,
                icons_only: true,
                wide_play: true,
                skip_song: true,
                on_play_pause: move |()| ask(if playing { "PLAYHEAD_PAUSE" } else { "PLAYHEAD_PLAY" }, &Value::Null),
                on_back: move |()| ask("PLAYHEAD_PREVIOUS", &Value::Null),
                on_forward: move |()| ask("PLAYHEAD_NEXT", &Value::Null),
                on_loop_toggle: move |()| {},
                on_record_toggle: move |()| {},
                on_arm_toggle: move |()| {},
            }
        }
    }
}

fn build() -> Extension {
    Extension::new()
        .view("progress", Progress)
        .view("transport", Transport)
}

architect_extension_wasi::export!(build);
