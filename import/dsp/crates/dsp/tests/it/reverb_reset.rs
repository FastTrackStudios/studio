//! A reset chain sounds as a fresh one: nothing it heard or ramped
//! before the reset reaches what it plays after.
//!
//! A host resets a plugin before a render and expects the render to be
//! the one a new instance would give (Studio's LISTEN reuses warm copies
//! and compares them with a full render). The chain is set up the way a
//! plugin sets it up: built, `update`d, then its parameters moved and
//! `update_params`ed — so the smoothers have somewhere to ramp to.

use dsp::reverb::AlgorithmType;
use dsp::reverb::chain::ReverbChain;
use dsp_core::audio::{AudioConfig, Processor as _};
use dsp_core::golden::signal::{noise, widen};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

const ALGORITHMS: &[AlgorithmType] = &[
    AlgorithmType::Room,
    AlgorithmType::Hall,
    AlgorithmType::Plate,
    AlgorithmType::Spring,
    AlgorithmType::Cloud,
    AlgorithmType::Bloom,
    AlgorithmType::Shimmer,
    AlgorithmType::Chorale,
    AlgorithmType::Magneto,
    AlgorithmType::NonLinear,
    AlgorithmType::Swell,
    AlgorithmType::Reflections,
    AlgorithmType::Velvet,
    AlgorithmType::FreeVerb,
    AlgorithmType::Random,
];

fn built(algo: AlgorithmType) -> ReverbChain {
    let mut chain = ReverbChain::new();
    chain.set_algorithm(algo);
    chain.update(AudioConfig {
        sample_rate: SR,
        max_buffer_size: BLOCK,
    });
    // A preset's values, away from the constructor's.
    chain.params.decay = 0.8;
    chain.params.size = 0.3;
    chain.params.damping = 0.6;
    chain.params.modulation = 0.4;
    chain.mix = 0.35;
    chain.width = 0.7;
    chain.predelay_ms = 12.0;
    chain.update_params();
    chain
}

fn play(chain: &mut ReverbChain, frames: usize, seed: u64) -> Vec<f64> {
    let mut l = widen(&noise(frames / 4, seed));
    let mut r = widen(&noise(frames / 4, seed + 1));
    l.resize(frames, 0.0);
    r.resize(frames, 0.0);
    let mut out = Vec::with_capacity(frames * 2);
    for (l, r) in l.chunks_mut(BLOCK).zip(r.chunks_mut(BLOCK)) {
        chain.process(l, r);
        out.extend(l.iter().zip(r.iter()).flat_map(|(&a, &b)| [a, b]));
    }
    out
}

#[test]
fn a_reset_chain_sounds_as_a_fresh_one() {
    let mut off = Vec::new();
    for &algo in ALGORITHMS {
        let mut warm = built(algo);
        let _ = play(&mut warm, 96_000, 3);
        warm.reset();
        let mut fresh = built(algo);
        fresh.reset();
        let a = play(&mut warm, 48_000, 21);
        let b = play(&mut fresh, 48_000, 21);
        let worst = a
            .iter()
            .zip(&b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f64::max);
        if worst > 0.0 {
            off.push(format!("{algo:?} {:.1} dBFS", 20.0 * worst.log10()));
        }
    }
    assert!(off.is_empty(), "reset differs from fresh: {off:?}");
}
