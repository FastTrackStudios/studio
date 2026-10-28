//! Every parameter moving at once, for measurement.
//!
//! The mixer's controls are drawn live so that a mute can change
//! without the mixer being re-recorded. Whether that is actually cheap
//! is a claim, and a claim about a renderer is worth exactly as much as
//! the frame that proves it.
//!
//! So this drives every parameter on every track — mutes and solos
//! toggling, arms flipping, faders sweeping, pans crossing — at rates
//! chosen to be worse than a person can produce. If the live pass holds
//! its frame rate against that, it holds against anything a user does.
//!
//! Deliberately NOT random. A stress test that cannot be replayed is a
//! stress test that cannot be bisected: the same `t` gives the same
//! session state on every run and every machine, so two measurements
//! are of the same work.

use daw_proto::Track;

/// Drive every track's parameters to where they are at `t`.
///
/// `t` runs 0..1 over the phase. Each parameter moves at its own rate
/// and each track is offset from the next, so no two controls are ever
/// in step — a mixer where every fader moves together is one long
/// gradient the GPU can predict, and predicting it is exactly what a
/// stress test must not let it do.
pub fn drive(tracks: &mut [Track], t: f64) {
    for (i, track) in tracks.iter_mut().enumerate() {
        let n = crate::num::coord(i);
        // Phase-shifted per track, by an irrational-ish step so the
        // pattern does not repeat across any run of tracks.
        let phase = n * 0.6180339887;

        // Faders sweep their whole range twice per phase.
        let sweep = (std::f64::consts::TAU * (t * 2.0 + phase)).sin();
        track.volume = ((sweep + 1.0) / 2.0).clamp(0.0, 1.0) * 2.0;

        // Pan crosses at a different rate, so a strip's two continuous
        // controls are never in step with each other either.
        track.pan = (std::f64::consts::TAU * (t * 3.0 + phase)).cos();

        // The toggles switch far faster than a person can click: about
        // twelve times a phase, each track on its own beat.
        track.muted = toggles(t, phase, 12.0);
        track.soloed = toggles(t, phase * 1.7, 9.0);
        track.armed = toggles(t, phase * 2.3, 7.0);
    }
}

/// Meter levels for `n` tracks at `t`.
///
/// Separate from [`drive`] because meters are not track state: they
/// arrive on their own subscription, indexed by project track, and the
/// engine publishes them at about 30 Hz whatever the frame rate is.
///
/// Driven per FRAME here anyway, which is four times faster than the
/// engine will ever move them. A stress test measures the case that
/// cannot happen so that the case that can is covered by it.
#[must_use]
pub fn meters(n: usize, t: f64) -> Vec<daw_proto::TrackLevels> {
    (0..n)
        .map(|i| {
            let phase = crate::num::coord(i) * 0.6180339887;
            // Fast, and out of step with the fader on the same strip:
            // a meter that tracked its own fader would let the GPU
            // predict both from one gradient.
            let wave = (std::f64::consts::TAU * (t * 7.0 + phase)).sin();
            let peak = crate::mcp::f64_to_f32(((wave + 1.0) / 2.0).clamp(0.0, 1.0));
            daw_proto::TrackLevels {
                peak_left: peak,
                // The right channel lags, so the two are never equal
                // and `max` actually has to choose.
                peak_right: peak * 0.8,
                hold_left: peak,
                hold_right: peak,
            }
        })
        .collect()
}

/// A square wave: on for half of each cycle.
fn toggles(t: f64, phase: f64, rate: f64) -> bool {
    (t * rate + phase).fract() < 0.5
}
