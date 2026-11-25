use super::*;

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

/// A bell at 1 kHz that rides down to −12 dB when its detector hears level,
/// at a fixed −40 dB threshold so the ride does not wait on learning.
fn ducking_band(external_key: bool) -> FtsEq {
    let mut eq = FtsEq::new(SR);
    eq.prepare(SR, BLOCK as u32);
    eq.set_canonical_band(
        0,
        BandConfig {
            used: true,
            enabled: true,
            freq_hz: 1_000.0,
            gain_db: 0.0,
            q: 1.0,
            shape: 0,
            ..BandConfig::default()
        },
        BandDynamics {
            range_db: -12.0,
            threshold_db: -40.0,
            auto: false,
            attack_pct: 10.0,
            external_key,
            ..BandDynamics::default()
        },
    );
    eq
}

/// Run a second of near-silent programme with a loud 1 kHz key beside it,
/// and say where the band's gain ended up.
fn gain_after_a_loud_key(eq: &mut FtsEq, keyed: bool) -> f64 {
    let quiet = 1.0e-4;
    let mut phase = 0.0f64;
    for _ in 0..(SR as usize / BLOCK) {
        let mut l = [quiet; BLOCK];
        let mut r = [quiet; BLOCK];
        let mut key = [0.0; BLOCK];
        for k in &mut key {
            *k = 0.5 * phase.sin();
            phase += std::f64::consts::TAU * 1_000.0 / SR;
        }
        let key = keyed.then_some((&key[..], &key[..]));
        eq.process_keyed(&mut l, &mut r, key);
    }
    eq.live_dyn_gain_db(0).unwrap_or(0.0)
}

#[test]
fn a_keyed_band_rides_on_the_side_chain() {
    let mut eq = ducking_band(true);
    let g = gain_after_a_loud_key(&mut eq, true);
    assert!(g < -6.0, "a loud key should duck a keyed band: {g:.2} dB");
}

#[test]
fn a_band_that_is_not_keyed_ignores_the_side_chain() {
    let mut eq = ducking_band(false);
    let g = gain_after_a_loud_key(&mut eq, true);
    assert!(
        g > -1.0,
        "an unkeyed band hears only its own quiet input: {g:.2} dB"
    );
}

#[test]
fn a_keyed_band_with_no_key_routed_hears_its_own_input() {
    let mut eq = ducking_band(true);
    let g = gain_after_a_loud_key(&mut eq, false);
    assert!(
        g > -1.0,
        "with no key, a keyed band hears its own quiet input: {g:.2} dB"
    );
}
