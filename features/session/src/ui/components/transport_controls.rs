//! Transport Control Components
//!
//! Transport control bar with grid layout.
//! Copied from `FastTrackStudio` for exact styling match.

use crate::ui::prelude::*;
use lucide_dioxus::{
    Circle as RecordIcon, Mic as ArmIcon, Pause as PauseIcon, Play as PlayIcon,
    Repeat2 as LoopIcon, SkipBack as BackIcon, SkipForward as ForwardIcon,
};

/// Transport control bar component.
///
/// Provides arm, record, back, play/pause, loop, and forward controls.
/// All actions are handled via callbacks to keep the component domain-agnostic.
///
/// The six controls always lay out as a single 6-column grid. In the default
/// (desktop) mode each cell is a roomy `icon + label` row at `text-lg`. Set
/// `compact` for tight hosts (e.g. a narrow note pane): the icon stacks over a
/// small label so all six fit without clipping regardless of the frame width.
#[component]
pub fn TransportControlBar(
    is_playing: bool,
    is_looping: bool,
    is_recording: bool,
    is_armed: bool,
    /// Number of tracks currently record-armed in the active project (from
    /// a live `daw::service::Tracks` subscription, not this song's own
    /// arm-intent toggle — see `ARMED_TRACK_COUNT`). Always shown on the
    /// Record button so an operator can see at a glance whether the mics
    /// they expect to be hot actually are before rolling tape — "0 armed"
    /// is exactly as useful to see as "12 armed".
    #[props(default = 0)]
    armed_track_count: usize,
    on_play_pause: Callback<()>,
    on_loop_toggle: Callback<()>,
    on_record_toggle: Callback<()>,
    on_arm_toggle: Callback<()>,
    on_back: Callback<()>,
    on_forward: Callback<()>,
    /// Tight layout: stack a small label under a smaller icon so all six
    /// controls fit a narrow container. Defaults false (desktop row layout).
    #[props(default)]
    compact: bool,
    /// Show the Arm + Record controls. Recording environments want them;
    /// a playback-only surface (the browser session player) does not.
    /// Defaults true. When false the bar is a 4-column playback transport
    /// (Back / Play / Loop / Advance).
    #[props(default = true)]
    show_recording: bool,
    /// Icons without their words — a phone's transport, where the four
    /// shapes say enough and the width is better spent on the icons.
    #[props(default)]
    icons_only: bool,
    /// Record where Loop is: the four-button bar of a surface that records
    /// (Back / Play / Record / Advance) — a phone or a tablet in record
    /// mode, where arming is done elsewhere and looping is not wanted.
    #[props(default)]
    record_in_loop: bool,
    /// The performer screen's transport (worship / gig screen spec §4.6):
    /// Play two columns wide (green with dark words while playing), the
    /// skips worded "Section", Loop ringed amber when on, the cells
    /// apart with 8 px gaps and rounded. Off (the default), the bar is as
    /// it always was.
    #[props(default)]
    wide_play: bool,
    /// The label's size in px, over the layout's own (11 compact, 18 in a
    /// row). The performer screen reads 14 at arm's length.
    #[props(default)]
    label_px: Option<u32>,
    /// With `wide_play`: the song has no sections, so the skips step by
    /// song and say so ("Song": back restarts it, forward loads the next).
    #[props(default)]
    skip_song: bool,
    /// With `wide_play`: forward is armed (a first tap while playing asked
    /// to load the next song); it turns the primary blue and says "Load
    /// next?" until the second tap or the arm runs out.
    #[props(default)]
    forward_armed: bool,
) -> Element {
    let playing = is_playing;
    let looping = is_looping;
    let recording = is_recording;
    let armed = is_armed;

    // Play takes two columns when wide.
    let cols = (if show_recording { 6 } else { 4 }) + usize::from(wide_play);
    let icon = if icons_only {
        24
    } else if compact {
        20
    } else {
        28
    };
    // Each cell's look, inline and whole: its layout (icon over a small
    // word when compact, beside it otherwise) and its state's colours.
    // No hover colour: a touchscreen leaves a tapped button hovered, and
    // the tint it kept read as the button being greyed out for good.
    let label = label_px.unwrap_or(if compact { 11 } else { 18 });
    let base = if compact || wide_play {
        format!(
            "display:flex; flex-direction:column; align-items:center; justify-content:center; gap:4px; \
             padding:0 4px; text-align:center; line-height:1; font-size:{label}px;"
        )
    } else {
        format!(
            "display:flex; align-items:center; justify-content:center; gap:12px; padding:0; font-size:{label}px;"
        )
    };
    // Each cell is a real button (a role and a label for assistive
    // tech), reset to draw exactly as the old cells did.
    let cls = |state: Look| {
        let (fg, bg) = match (state, wide_play) {
            (Look::Idle, false) => ("#e5e7eb", "#0b0c0e"),
            (Look::Idle, true) => ("#e4e4e7", "#141418"),
            (Look::On, false) => ("#0b0c0e", "#e5e7eb"),
            (Look::On, true) => ("#e4e4e7", "#141418"),
            (Look::Playing, false) => ("#0b0c0e", "#e5e7eb"),
            (Look::Playing, true) => ("#04210f", "#22c55e"),
            (Look::Red, _) => ("#ffffff", "#dc2626"),
            (Look::RedWord, _) => ("#ef4444", "#0b0c0e"),
        };
        let edge = match (state, wide_play) {
            (_, false) => {
                "border:none; border-left:1px solid #2a2c31; border-radius:0;".to_string()
            }
            // Loop on: an amber ring, and the words say it.
            (Look::On, true) => "border:2px solid #f59e0b; border-radius:8px;".to_string(),
            (_, true) => "border:1px solid #2b2b31; border-radius:8px;".to_string(),
        };
        let weight = if wide_play { 650 } else { 500 };
        format!(
            "{base} font-weight:{weight}; cursor:pointer; color:{fg}; background:{bg}; {edge} \
             margin:0; min-width:0; font-family:inherit; user-select:none;"
        )
    };
    let play_look = if playing { Look::Playing } else { Look::Idle };
    let play_span = if wide_play {
        "grid-column: span 2; font-size: 20px; font-weight: 750; flex-direction: row; gap: 10px;"
    } else {
        ""
    };
    let gap = if wide_play {
        "gap:8px; background:transparent;"
    } else {
        "background:#0b0c0e;"
    };
    let (back_word, forward_word, back_aria, forward_aria) =
        skip_words(wide_play, skip_song, forward_armed);

    rsx! {
        div {
            // Layout-critical: state it inline so the controls always lay out
            // as one N-column row regardless of whether the Tailwind grid
            // utilities survived the consumer's CSS purge. Without this the
            // children collapse to block rows and overlap inside the caller's
            // fixed-height (`h-16 overflow-hidden`) frame.
            style: "display:grid; grid-template-columns:repeat({cols},minmax(0,1fr)); align-items:stretch; \
                    width:100%; height:100%; {gap} overflow:hidden;",

            // Arm + Record — recording environments only (playback surfaces
            // pass `show_recording: false`).
            if show_recording {
                // Arm Button — arms/disarms the selected tracks in the active song
                button {
                    "aria-label": "Arm",
                    "aria-pressed": "{armed}",
                    style: if armed { cls(Look::Red) } else { cls(Look::Idle) },
                    onclick: move |_| {
                        on_arm_toggle.call(());
                    },
                    ArmIcon { size: icon, color: "currentColor" }
                    if !icons_only { "Arm" }
                }

                // Record Button — toggles recording into the active song's project
                button {
                    "aria-label": "Record",
                    "aria-pressed": "{recording}",
                    style: if recording { cls(Look::Red) } else { cls(Look::RedWord) },
                    onclick: move |_| {
                        on_record_toggle.call(());
                    },
                    RecordIcon { size: icon, color: "currentColor" }
                    if recording {
                        "Recording ({armed_track_count} armed)"
                    } else {
                        "Record ({armed_track_count} armed)"
                    }
                }
            }

                        // Back Button — while playing too: a press mid-song is exactly
            // when back a section is wanted.
            button {
                "aria-label": back_aria,
                style: cls(Look::Idle),
                onclick: move |_| on_back.call(()),
                BackIcon { size: icon, color: "currentColor" }
                if !icons_only { "{back_word}" }
            }

            // Play/Pause Button
            button {
                "aria-label": if playing { "Pause" } else { "Play" },
                "aria-pressed": "{playing}",
                style: "{cls(play_look)} {play_span}",
                onclick: move |_| {
                    on_play_pause.call(());
                },
                if playing {
                    PauseIcon { size: icon, color: "currentColor" }
                } else {
                    PlayIcon { size: icon, color: "currentColor" }
                }
                if !icons_only {
                    if playing { "Pause" } else { "Play" }
                }
            }

            // Record, in Loop's place (see `record_in_loop`).
            if record_in_loop {
                button {
                    "aria-label": "Record",
                    "aria-pressed": "{recording}",
                    style: if recording { cls(Look::Red) } else { cls(Look::RedWord) },
                    onclick: move |_| {
                        on_record_toggle.call(());
                    },
                    // A plain round dot: Blitz draws no SVG `<circle>`,
                    // which is all the Circle icon is.
                    div {
                        style: format!(
                            "width:{dot}px; height:{dot}px; border-radius:50%; background:{fill}; flex:none;",
                            dot = icon * 7 / 10,
                            fill = if recording { "#ffffff" } else { "#ef4444" },
                        ),
                    }
                    if !icons_only {
                        if recording { "Recording" } else { "Record" }
                    }
                }
            } else {
            // Loop Button
            button {
                "aria-label": "Loop",
                "aria-pressed": "{looping}",
                style: if looping { cls(Look::On) } else { cls(Look::Idle) },
                onclick: move |_| {
                    on_loop_toggle.call(());
                },
                LoopIcon { size: icon, color: "currentColor" }
                if !icons_only { "Loop" }
            }
            }

                        // Advance Button — while playing too.
            button {
                "aria-label": forward_aria,
                style: if wide_play && forward_armed {
                    format!("{} background:#2563eb; color:#ffffff; border-color:#2563eb;", cls(Look::Idle))
                } else {
                    cls(Look::Idle)
                },
                onclick: move |_| on_forward.call(()),
                ForwardIcon { size: icon, color: "currentColor" }
                if !icons_only { "{forward_word}" }
            }
        }
    }
}

/// What a transport cell shows it is.
#[derive(Clone, Copy)]
enum Look {
    /// At rest.
    Idle,
    /// Switched on (looping; playing too, unless wide): light, with dark
    /// words — or, wide, ringed amber.
    On,
    /// Play while sounding: light like `On`, or green when wide.
    Playing,
    /// Recording, or armed: red.
    Red,
    /// Record, not recording: red words on the dark.
    RedWord,
}

/// The skip cells' words and labels: back, forward, back's label,
/// forward's label.
fn skip_words(
    wide_play: bool,
    skip_song: bool,
    forward_armed: bool,
) -> (&'static str, &'static str, &'static str, &'static str) {
    match (wide_play, skip_song, forward_armed) {
        (false, _, _) => ("Back", "Advance", "Back", "Advance"),
        (true, _, true) => (
            if skip_song { "Song" } else { "Section" },
            "Load next?",
            if skip_song {
                "Restart the song"
            } else {
                "Previous section"
            },
            "Load the next song: tap again",
        ),
        (true, true, false) => ("Song", "Song", "Restart the song", "Skip to the next song"),
        (true, false, false) => ("Section", "Section", "Previous section", "Next section"),
    }
}

#[cfg(test)]
mod skip_word_tests {
    use super::skip_words;

    #[test]
    fn a_song_without_sections_skips_by_song() {
        assert_eq!(skip_words(true, true, false).0, "Song");
        assert_eq!(skip_words(true, true, false).1, "Song");
        assert_eq!(skip_words(true, false, false).1, "Section");
        assert_eq!(skip_words(true, true, true).1, "Load next?");
        assert_eq!(
            skip_words(false, true, false).0,
            "Back",
            "the classic bar is as it was"
        );
    }
}
