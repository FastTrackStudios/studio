//! Session's performer view: the shared performer screen (app-shell's
//! `PerformerScreen`, worship / gig screen spec B1) mounted over Session's
//! own transport, setlist and chart — build issue B3 (STA-360).
//!
//! Session gives the screen three things and draws nothing of its own:
//!
//! - [`SessionPlayback`], the `PlaybackModel` adapter over the transport's
//!   reading ([`crate::progress::use_reading`]) and its moves
//!   ([`crate::engine::transport`]);
//! - the four panel bodies, registered through app-shell's
//!   [`performer_registry`] (the one registry function Signal's gig mount
//!   calls too — no fork): the now band over Session's `ProgressBar`, the
//!   navigator as the setlist, the chart, and the dock with Session's
//!   `TransportButtons` over the shared nudge row;
//! - the rig, when there is one: a `RigClient` the host provides
//!   (`app_shell::ui::provide_rig`). Without one (no Signal ever paired)
//!   the transport spans the dock and the app bar offers **Pair a rig**;
//!   the live peer link to Signal comes with the connect-anywhere work
//!   (C3, C7).
//!
//! The view is the iPad's: a touchscreen with room for the wide layout.
//! There the now band carries the clock, bar.beat, BPM and key, and the
//! transport's ⋯ holds Go to start / Go to end — the iPad audit's fix 7,
//! which the classic bottom bar used to drop at those widths.

use dioxus::prelude::*;

use app_shell::ui::performer::{PerformerBodies, SESSION_SCOPE, performer_registry};
use app_shell::ui::playback::{PlaybackModel, PlaybackView, SectionView, SongView};
use app_shell::ui::{
    NowBand, PerformDock, PerformerScreen, RigCrumb, SyncPill, TransportNudgeRow, provide_playback,
    rig_half, use_panel_registry,
};

use crate::engine::{Move, Reading, Transport, transport};
use crate::progress::{Song, Step, back_to, forward_to, step_song};
use crate::song_model::StudioSession;

/// The performer view's fill round the screen and under the system's
/// insets: the desk (`#0c0c0f`), not the bars' `#17181b`.
pub const DESK: &str = app_shell::tokens::DESK;

/// Whether a window gets the performer view for Performance: a finger on
/// a screen with room for the wide layout (the 11-inch iPad either way up
/// is 1194 x 834 / 834 x 1194; a phone keeps its compact shell).
#[must_use]
pub fn wants_performer(touch: bool, form: crate::compact::Form) -> bool {
    touch && !form.compact()
}

/// Session's playback, as the performer screen reads and drives it.
#[derive(Clone)]
pub struct SessionPlayback {
    session: StudioSession,
    song: Option<Song>,
    reading: Signal<Reading>,
    setlist: Option<Signal<crate::setlist::Setlist>>,
    pick: Option<crate::record_view::PickSong>,
}

impl SessionPlayback {
    /// The adapter over the song in context. A hook: call it once, at the
    /// top of the view.
    pub fn use_new() -> Self {
        let session: StudioSession = use_context();
        let song = use_hook(|| Song::of(&session));
        let reading = crate::progress::use_reading();
        let setlist = try_use_context::<Signal<crate::setlist::Setlist>>();
        let pick = try_use_context::<crate::record_view::PickSong>();
        Self {
            session,
            song,
            reading,
            setlist,
            pick,
        }
    }

    fn here(&self) -> f64 {
        Transport::shared().map_or_else(|| self.reading.peek().at, |t| t.read().0)
    }

    fn start(&self) -> f64 {
        self.song.as_ref().map_or(0.0, |s| s.start)
    }

    /// The song up's sections, in song seconds.
    fn sections(&self) -> Vec<SectionView> {
        let start = self.start();
        let mut parts: Vec<_> = self
            .session
            .project
            .sections
            .iter()
            .filter(|r| r.lane == crate::ruler::SECTIONS_ROW as u32)
            .collect();
        parts.sort_by(|a, b| a.start.total_cmp(&b.start));
        parts
            .into_iter()
            .map(|r| SectionView {
                name: r.name.clone(),
                colour: r.color.clone().unwrap_or_else(|| "#4b5563".to_owned()),
                start: r.start - start,
                end: r.end - start,
            })
            .collect()
    }
}

impl PlaybackModel for SessionPlayback {
    fn view(&self) -> PlaybackView {
        let r = (self.reading)();
        let (start, end) = self.song.as_ref().map_or((0.0, 0.0), |s| (s.start, s.end));
        let key = crate::arrangement::key_at(&self.session.project, r.at)
            .map(|k| crate::transport_bar::short_key(&k))
            .unwrap_or_default();
        let (bar, beat) = crate::ruler::song_bar_beat(
            &self.session.project.tempo,
            crate::ruler::song_start_of(&self.session.project),
            r.at,
        );
        let bpm = if r.bpm.is_finite() && r.bpm > 0.0 {
            r.bpm.round() as u32
        } else {
            0
        };
        let (songs, current) = match self.setlist {
            Some(list) => {
                let list = list.read();
                let songs = list
                    .songs
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let up = i == list.at;
                        SongView {
                            title: s.name.clone(),
                            key: if up { key.clone() } else { String::new() },
                            bpm: if up { bpm } else { 0 },
                            colour: s.color.clone(),
                            sections: if up { self.sections() } else { Vec::new() },
                        }
                    })
                    .collect::<Vec<_>>();
                let current = (list.at < songs.len()).then_some(list.at);
                (songs, current)
            }
            // A song on its own (no set): it is the whole set.
            None => (
                vec![SongView {
                    title: "This song".to_owned(),
                    key,
                    bpm,
                    colour: "#7dd3fc".to_owned(),
                    sections: self.sections(),
                }],
                Some(0),
            ),
        };
        PlaybackView {
            songs,
            current,
            position: (r.at - start).max(0.0),
            duration: (end - start).max(0.0),
            playing: r.playing,
            looping: r.looping.then(|| {
                self.song
                    .as_ref()
                    .and_then(|s| s.current(r.at))
                    .and_then(|i| self.sections().get(i).map(|s| s.name.clone()))
                    .unwrap_or_else(|| "Loop".to_owned())
            }),
            bar_beat: Some(format!("{bar}.{beat}")),
            ..PlaybackView::default()
        }
    }

    fn step_song(&self, by: i32) {
        step_song(self.pick, self.setlist, by as isize);
    }

    fn load_song(&self, index: usize) {
        if let Some(crate::record_view::PickSong(pick)) = self.pick {
            pick.call(index);
        }
    }

    fn seek(&self, seconds: f64) {
        transport(Move::Seek, self.start() + seconds);
    }

    fn play_pause(&self) {
        transport(Move::PlayStop, 0.0);
    }

    fn section_step(&self, by: i32) {
        let Some(song) = self.song.as_ref() else {
            return;
        };
        let at = self.here();
        let step = if by < 0 {
            back_to(song, at)
        } else {
            forward_to(song, at)
        };
        match step {
            Step::Seek(to) => transport(Move::Seek, to),
            Step::Song => step_song(self.pick, self.setlist, by.signum() as isize),
        }
    }

    fn set_loop(&self, on: bool) {
        if self.reading.peek().looping != on {
            transport(Move::ToggleLoop, 0.0);
        }
    }

    // Session's engine plays at the record's rate and its level is the
    // mixer's: the Speed and Level cells show 100% and change nothing yet.
    fn set_speed(&self, _rate: f64) {}

    fn set_level(&self, _level: f64) {}

    fn can_go_to(&self) -> bool {
        true
    }

    fn go_to_start(&self) {
        transport(Move::Seek, self.start());
    }

    fn go_to_end(&self) {
        let end = self.song.as_ref().map_or(0.0, |s| s.end);
        transport(Move::Seek, end);
    }
}

// ── The panel bodies ───────────────────────────────────────────────────────

fn now() -> Element {
    rsx! {
        NowBand {
            progress: rsx! { crate::progress::ProgressBar { height: "44px".to_owned() } },
        }
    }
}

fn setlist() -> Element {
    let pick = try_use_context::<crate::record_view::PickSong>();
    let has_set = try_use_context::<Signal<crate::setlist::Setlist>>().is_some();
    if !has_set {
        return rsx! {
            div { style: "display: flex; align-items: center; justify-content: center; flex: 1; color: #8e8e98; font-size: 16px;",
                "No set open"
            }
        };
    }
    rsx! {
        div { style: "display: flex; flex-direction: column; flex: 1; min-height: 0; overflow-y: scroll;",
            crate::navigator::Navigator {
                on_pick: move |i: usize| if let Some(crate::record_view::PickSong(p)) = pick { p.call(i) },
            }
        }
    }
}

fn chart() -> Element {
    #[cfg(feature = "native")]
    return rsx! { crate::chart_panel::Chart { paged: true } };
    #[cfg(all(feature = "web", not(feature = "native")))]
    return rsx! { crate::chart_panel::WebChart { paged: true } };
    #[cfg(not(any(feature = "native", feature = "web")))]
    rsx! {}
}

fn dock() -> Element {
    rsx! {
        PerformDock {
            rig: rig_half(),
            transport: rsx! {
                crate::progress::TransportButtons { height: 60 }
                TransportNudgeRow {}
            },
        }
    }
}

/// The bodies Session hands the shared registry.
pub const BODIES: PerformerBodies = PerformerBodies {
    now,
    setlist,
    chart,
    dock,
};

/// The performer view, over the song in context (`StudioSession`) and the
/// set (`Signal<Setlist>`), with the rig the host provides, if any.
#[component]
pub fn SessionPerformer(
    /// The app bar's own items (the views menu, Back), before the rig's
    /// crumb and the sync pill.
    #[props(default)]
    bar: Option<Element>,
    /// The status and home-indicator insets. A page under
    /// `viewport-fit=cover` already pads them out (`SAFE_AREA`), so 0.
    #[props(default)]
    inset_top: u32,
    #[props(default)] inset_bottom: u32,
) -> Element {
    provide_playback(SessionPlayback::use_new());
    use_panel_registry(|| performer_registry(BODIES));
    rsx! {
        div { style: "position: relative; display: flex; flex: 1; min-width: 0; min-height: 0; background: {DESK};",
            PerformerScreen {
                scope: SESSION_SCOPE,
                inset_top,
                inset_bottom,
                app_bar: rsx! {
                    div { style: "display: flex; align-items: center; gap: 12px; flex: 1; min-width: 0; padding: 0 12px;",
                        if let Some(bar) = bar { {bar} } else {
                            span { style: "font-size: 18px; font-weight: 750; color: #e4e4e7;", "Session" }
                        }
                        span { style: "flex: 1;" }
                        RigCrumb {}
                        SyncPill {}
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compact::Form;

    #[test]
    fn the_ipad_gets_the_performer_view_and_a_phone_does_not() {
        let ipad = Form::of(1194.0, 834.0);
        let upright = Form::of(834.0, 1194.0);
        let phone = Form::of(390.0, 844.0);
        assert!(wants_performer(true, ipad));
        assert!(wants_performer(true, upright));
        assert!(!wants_performer(true, phone), "a phone keeps its shell");
        assert!(
            !wants_performer(false, ipad),
            "a mouse keeps the classic view"
        );
    }

    #[test]
    fn session_registers_through_the_shared_registry_function() {
        let reg = performer_registry(BODIES);
        for (id, region) in app_shell::ui::performer::ids::ALL {
            assert_eq!(reg.get(id).map(|p| p.region), Some(region), "{id}");
        }
    }
}
