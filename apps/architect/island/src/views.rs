//! The island's two views, drawn the same by the component and the web
//! bundle: Session's progress bar and transport (`transport_ui`), given
//! where the song is and a way to ask the page.

use dioxus::prelude::*;
use serde_json::{Value, json};
use transport_ui::{ProgressSection, SongProgressBar, TransportControlBar};

/// Where the song is, as the page gave it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Playhead {
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

/// Where the song is, from the props the page gives a view.
pub fn playhead_of(props: &Value) -> Playhead {
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
        time: number(props, "time"),
        playing: props
            .get("playing")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        duration: number(props, "duration"),
        sections,
    }
}

/// The colour of a song without sections: one slate fill the whole length.
const WHOLE_SONG: &str = "#94a3b8";

/// How a view asks the page one of its five intents: the action and
/// its arguments.
pub type Ask = Callback<(String, Value)>;

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
/// played, the playhead, a tap on a section seeks to it).
#[component]
pub fn Progress(p: Playhead, ask: Ask) -> Element {
    let progress = if p.duration > 0.0 {
        (p.time / p.duration * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let starts: Vec<f64> = p.sections.iter().map(|s| s.1).collect();
    // The bar alone: no time under it (a share page's view spends its
    // height on the chart).
    rsx! {
        div { class: "studio-island-progress", style: "display:flex; flex-direction:column; width:100%;",
            SongProgressBar {
                progress,
                sections: sections(&p),
                height: Some("2.75rem".to_owned()),
                on_section_click: move |i: usize| {
                    if let Some(t) = starts.get(i) {
                        ask.call(("PLAYHEAD_SEEK".to_owned(), json!({ "t": t })));
                    }
                },
            }
        }
    }
}

/// Under the chart: Session's transport (previous / play / pause / next)
/// as one joined control, what a recording on a page can do: no record,
/// no loop.
#[component]
pub fn Transport(p: Playhead, ask: Ask) -> Element {
    let playing = p.playing;
    rsx! {
        div { class: "studio-island-transport", style: "height:3.25rem; width:100%;",
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
                joined: true,
                on_play_pause: move |()| ask.call(((if playing { "PLAYHEAD_PAUSE" } else { "PLAYHEAD_PLAY" }).to_owned(), Value::Null)),
                on_back: move |()| ask.call(("PLAYHEAD_PREVIOUS".to_owned(), Value::Null)),
                on_forward: move |()| ask.call(("PLAYHEAD_NEXT".to_owned(), Value::Null)),
                on_loop_toggle: move |()| {},
                on_record_toggle: move |()| {},
                on_arm_toggle: move |()| {},
            }
        }
    }
}
