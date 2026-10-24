use super::*;
use crate::modulation::trem::fts_modulation::{TriggerMode, tempo::SyncDivision};
use dsp_core::time::{Flavour, MusicalTime, NoteValue};

const EIGHTH: MusicalTime = MusicalTime::new(NoteValue::Eighth, Flavour::Straight);

fn chains(profile: &Profile, controls: &Controls) -> (ChorusChain, TremChain, WahChain) {
    let (mut chorus, mut trem, mut wah) = (ChorusChain::new(), TremChain::new(), WahChain::new());
    apply(profile, controls, &mut chorus, &mut trem, &mut wah);
    (chorus, trem, wah)
}

/// Every circuit whose movement comes from a trigger clock: the tremolos,
/// and the wahs a pattern sweeps (an envelope wah moves when you play).
fn clocked() -> impl Iterator<Item = &'static Profile> {
    PROFILES.iter().filter(|p| match p.voicing.circuit {
        Circuit::Tremolo { .. } => true,
        Circuit::Wah { source, .. } => source != WahSource::Envelope,
        Circuit::Delay { .. } => false,
    })
}

#[test]
fn the_clock_is_the_one_the_controls_choose() {
    for profile in clocked() {
        for (clock, mode, division) in [
            (Clock::Free, TriggerMode::Free, SyncDivision::FreeHz),
            (
                Clock::Synced(EIGHTH),
                TriggerMode::Sync,
                SyncDivision::Beat(0.5),
            ),
        ] {
            let controls = Controls {
                clock,
                ..Controls::default()
            };
            let (_, trem, wah) = chains(profile, &controls);
            let trigger = match profile.voicing.circuit {
                Circuit::Tremolo { .. } => &trem.modulator.trigger,
                _ => &wah.modulator.trigger,
            };
            assert_eq!(
                (trigger.mode, trigger.division),
                (mode, division),
                "{} on {clock:?}",
                profile.id,
            );
        }
    }
}

#[test]
fn every_clocked_circuit_draws_movement_free_and_synced() {
    for profile in clocked() {
        for clock in [Clock::Free, Clock::Synced(EIGHTH)] {
            let controls = Controls {
                depth: 1.0,
                clock,
                ..Controls::default()
            };
            let mut out = [0.0f64; 180];
            shape(profile, &controls, &mut out);
            let (lo, hi) = out
                .iter()
                .fold((f64::MAX, f64::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
            assert!(
                hi - lo > 0.1,
                "{} on {clock:?} draws a still line: {lo}..{hi}",
                profile.id
            );
        }
    }
}
