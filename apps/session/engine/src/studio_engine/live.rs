//! Live mode's roll-on: when the song playing reaches its end, the next one
//! of the set plays — on the frame the first one ends.
//!
//! The engine's, not a window's: it runs with no window (`studio engine`, a
//! show driven from the CLI), and a window only says whether the set is in
//! Live ([`set_live`], also `SESSION_LIVE` on the Action List). Shortly
//! before the current song's end the engine asks the set's audio engine to
//! hand over (`SetAudio::play_after`, `daw_standalone::schedule_handoff`):
//! both transports take a scheduled locate in the sync clock, so the next
//! song starts on the very frame the current one stops. When the moment has
//! passed the set follows ([`super::set::pick`]) — the next song is the
//! current one, its meters and keys — without touching its transport, which
//! is already playing. A song that cannot be handed over (no audio, not a
//! voice yet) falls back to what a window used to do: picked at its end and
//! played from its start.
//!
//! The decision is [`step`], pure; [`start`] runs it on a thread of its own
//! every few milliseconds while the set is in Live.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How long before a song's end its hand-over is scheduled: far enough
/// ahead that the audio thread has the locate before the moment, near
/// enough that a stop pressed before it is not overridden by a song that
/// starts by itself.
pub const LEAD: f64 = 0.5;

/// How often the roll-on looks.
const TICK: Duration = Duration::from_millis(10);

/// Where the set is, as the roll-on reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Now {
    /// The current song's playhead, seconds.
    pub at: f64,
    pub playing: bool,
    /// The current song's end (its span's end), when it has one.
    pub end: Option<f64>,
    /// The song after it, if the set goes on.
    pub next: Option<usize>,
}

/// What the roll-on has done about the current song so far.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pending {
    /// Nothing yet.
    Nothing,
    /// The hand-over to song `to` is scheduled for `end`.
    Scheduled { to: usize, end: f64 },
    /// It could not be scheduled: song `to` is picked at the end instead.
    AtTheEnd { to: usize },
    /// The song was past its end already when it became current (picked
    /// while a voice left playing ran on past it): nothing rolls on from it.
    Past,
}

/// What is pending for a song the moment it becomes current: [`Pending::Past`]
/// when its playhead is already past its end, else nothing. A song rolls on
/// only when it reaches its end while it is the current one.
#[must_use]
pub fn first_sight(now: Now) -> Pending {
    match now.end {
        Some(end) if now.at >= end => Pending::Past,
        _ => Pending::Nothing,
    }
}

/// What to do now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    Wait,
    /// Schedule the hand-over to song `to` at `end`.
    Schedule {
        to: usize,
        end: f64,
    },
    /// The hand-over to `to` has happened: the set follows (its transport
    /// is playing already).
    Follow {
        to: usize,
        end: f64,
    },
    /// The song ended with nothing scheduled: pick `to` and play it from
    /// its start.
    PickAndPlay {
        to: usize,
        end: f64,
    },
    /// Stopped before the moment: the hand-over to `to` is called off, so
    /// it does not start by itself.
    CallOff {
        to: usize,
    },
}

/// The roll-on's decision, from where the set is and what is pending.
#[must_use]
pub fn step(now: Now, pending: Pending) -> Step {
    let (Some(end), Some(next)) = (now.end, now.next) else {
        return Step::Wait;
    };
    let ended = now.at >= end;
    match pending {
        // Handed over: once the moment has passed (the playhead read is
        // the current song's, stopped at its end), the set follows.
        Pending::Scheduled { to, end } if ended => Step::Follow { to, end },
        Pending::Scheduled { to, .. } if !now.playing => Step::CallOff { to },
        Pending::Scheduled { .. } => Step::Wait,
        Pending::AtTheEnd { to } if ended && now.playing => Step::PickAndPlay { to, end },
        Pending::AtTheEnd { .. } | Pending::Past => Step::Wait,
        Pending::Nothing if !now.playing => Step::Wait,
        Pending::Nothing if ended => Step::PickAndPlay { to: next, end },
        Pending::Nothing if end - now.at <= LEAD => Step::Schedule { to: next, end },
        Pending::Nothing => Step::Wait,
    }
}

/// Whether the set is in Live (what a window's mode says; `SESSION_LIVE`).
static LIVE: AtomicBool = AtomicBool::new(false);

/// Put the set in Live (`true`) or take it out: the roll-on runs only in
/// Live, so working on a song in another mode never jumps away from it.
pub fn set_live(on: bool) {
    // A window driving an engine in another process: the roll-on is that
    // engine's.
    if super::link::linked().is_some() {
        let args = format!(r#"{{"on":{on}}}"#);
        if let Err(e) = super::link::run("SESSION_LIVE", args.as_bytes()) {
            tracing::warn!(error = %e, "live: the engine did not take Live");
        }
        return;
    }
    let was = LIVE.swap(on, Ordering::AcqRel);
    if on && !was {
        start();
    }
    if on != was {
        tracing::info!(
            live.on = on,
            "live: roll-on {}",
            if on { "on" } else { "off" }
        );
    }
}

/// Whether the set is in Live.
#[must_use]
pub fn is_live() -> bool {
    LIVE.load(Ordering::Acquire)
}

/// The roll-on, on a thread of its own, while the set is in Live; started
/// once, by [`set_live`].
fn start() {
    static RUNNING: AtomicBool = AtomicBool::new(false);
    if RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("live-roll-on".into())
        .spawn(|| {
            let mut pending = (String::new(), Pending::Nothing);
            loop {
                std::thread::sleep(TICK);
                if !is_live() {
                    pending = (String::new(), Pending::Nothing);
                    continue;
                }
                tick(&mut pending);
            }
        });
    if let Err(e) = spawned {
        RUNNING.store(false, Ordering::Release);
        tracing::warn!(error = %e, "live: no thread for the roll-on");
    }
}

/// One look: where the set is, what to do, done. `pending` is what has
/// been done about which song (its project).
fn tick(pending: &mut (String, Pending)) {
    // Playing together, only the leader rolls on; this engine follows it
    // into the next song.
    if crate::collab::following() {
        return;
    }
    let Some((project, next_project, end, next)) = super::set::peek(|view| {
        let setlist = view.setlist.as_ref()?;
        let song = setlist.current()?;
        let next = setlist.next();
        let next_project = next
            .and_then(|n| setlist.songs.get(n))
            .map(|s| s.project.clone());
        let end = (song.span.1 > song.span.0).then_some(song.span.1);
        Some((song.project.clone(), next_project, end, next))
    }) else {
        return;
    };
    let Some((at, playing)) = crate::stream_set::transport_of(&project) else {
        return;
    };
    let now = Now {
        at,
        playing,
        end,
        next,
    };
    if pending.0 != project {
        *pending = (project.clone(), first_sight(now));
    }
    match step(now, pending.1) {
        Step::Wait => {}
        Step::Schedule { to, end } => {
            let Some(to_project) = next_project else {
                return;
            };
            pending.1 = match crate::open::play_after(&project, &to_project, end) {
                Ok(moment) => {
                    tracing::info!(
                        live.from = %project,
                        live.to = %to_project,
                        live.end_s = end,
                        live.moment_us = moment,
                        "live: the next song is handed the set on the frame this one ends"
                    );
                    Pending::Scheduled { to, end }
                }
                Err(why) => {
                    tracing::warn!(live.from = %project, live.to = %to_project, error = %why, "live: no hand-over; the next song is picked at the end");
                    Pending::AtTheEnd { to }
                }
            };
        }
        Step::Follow { to, end } => {
            super::set::pick(to, end);
            *pending = (String::new(), Pending::Nothing);
        }
        Step::CallOff { to } => {
            if let Some(to_project) = next_project
                && let Err(why) = crate::open::call_off_handoff(&project, &to_project)
            {
                tracing::warn!(live.to = %to_project, error = %why, "live: the hand-over could not be called off");
            }
            tracing::info!(
                setlist.next = to,
                "live: stopped before the end; the next song stays put"
            );
            pending.1 = Pending::Nothing;
        }
        Step::PickAndPlay { to, end } => {
            if super::set::pick(to, end).is_some() {
                tracing::info!(
                    setlist.next = to,
                    "live: the song ended; the next one plays"
                );
                crate::engine::transport(crate::engine::Move::PlayFrom, 0.0);
            }
            *pending = (String::new(), Pending::Nothing);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now(at: f64, playing: bool) -> Now {
        Now {
            at,
            playing,
            end: Some(180.0),
            next: Some(3),
        }
    }

    #[test]
    fn nothing_happens_far_from_the_end_or_stopped() {
        assert_eq!(step(now(10.0, true), Pending::Nothing), Step::Wait);
        assert_eq!(step(now(179.9, false), Pending::Nothing), Step::Wait);
    }

    #[test]
    fn the_hand_over_is_scheduled_just_before_the_end() {
        assert_eq!(
            step(now(180.0 - LEAD, true), Pending::Nothing),
            Step::Schedule { to: 3, end: 180.0 }
        );
        // Once only.
        let pending = Pending::Scheduled { to: 3, end: 180.0 };
        assert_eq!(step(now(179.8, true), pending), Step::Wait);
    }

    #[test]
    fn the_set_follows_once_the_moment_has_passed() {
        let pending = Pending::Scheduled { to: 3, end: 180.0 };
        // The song handed from stops at its end: stopped there, it is over.
        assert_eq!(
            step(now(180.0, false), pending),
            Step::Follow { to: 3, end: 180.0 }
        );
    }

    #[test]
    fn a_stop_before_the_moment_calls_the_hand_over_off() {
        let pending = Pending::Scheduled { to: 3, end: 180.0 };
        assert_eq!(step(now(179.7, false), pending), Step::CallOff { to: 3 });
    }

    #[test]
    fn a_song_that_could_not_be_handed_over_is_picked_at_its_end() {
        let pending = Pending::AtTheEnd { to: 3 };
        assert_eq!(step(now(179.9, true), pending), Step::Wait);
        assert_eq!(
            step(now(180.01, true), pending),
            Step::PickAndPlay { to: 3, end: 180.0 }
        );
        // Ended with nothing scheduled at all (a tick missed the lead).
        assert_eq!(
            step(now(180.2, true), Pending::Nothing),
            Step::PickAndPlay { to: 3, end: 180.0 }
        );
    }

    #[test]
    fn a_song_picked_past_its_end_does_not_roll_on() {
        // A song left playing runs on past its end; picked again, it is
        // not taken for one that just ended.
        let picked = now(240.0, true);
        assert_eq!(first_sight(picked), Pending::Past);
        assert_eq!(step(picked, Pending::Past), Step::Wait);
        assert_eq!(first_sight(now(12.0, true)), Pending::Nothing);
    }

    #[test]
    fn the_last_song_and_a_song_without_an_end_roll_nowhere() {
        let last = Now {
            next: None,
            ..now(181.0, true)
        };
        assert_eq!(step(last, Pending::Nothing), Step::Wait);
        let endless = Now {
            end: None,
            ..now(181.0, true)
        };
        assert_eq!(step(endless, Pending::Nothing), Step::Wait);
    }
}
