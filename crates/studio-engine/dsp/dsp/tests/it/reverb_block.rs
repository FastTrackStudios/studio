//! The block paths are the sample loop, to the bit.
//!
//! The reverbs run a stage at a time over a block where the sample loop
//! ran every stage per sample (`ReverbAlgorithm::process_block`,
//! `ModulatedAllpass::process`, `Fdn::process`). Same arithmetic, same
//! order per object: these hold them to it over ragged blocks, through
//! silence and back, with and without the paths' preconditions (short
//! delays, vintage reads, jitter) so the fallbacks are crossed too.

use dsp::reverb::algorithm::{AlgorithmParams, AlgorithmType, ReverbAlgorithm};
use dsp::reverb::algorithms;
use dsp::reverb::primitives::fdn::{Fdn, MixMatrix};
use dsp::reverb::primitives::modulated_allpass::ModulatedAllpass;
use dsp_core::golden::signal::{noise, widen};

const SR: f64 = 48_000.0;
/// Every block length the paths cut at, and around them.
const BLOCKS: &[usize] = &[1, 7, 32, 33, 64, 5, 128, 31, 512, 2];

/// Bursts of noise and stretches of silence.
fn signal(len: usize, seed: u64) -> Vec<f64> {
    let mut x = widen(&noise(len, seed));
    for (i, v) in x.iter_mut().enumerate() {
        if (i / 3000) % 3 == 2 {
            *v = 0.0;
        } else {
            *v *= 0.5;
        }
    }
    x
}

fn assert_same(name: &str, want: &[f64], got: &[f64]) {
    assert_eq!(want.len(), got.len(), "{name}");
    if let Some(i) = want
        .iter()
        .zip(got)
        .position(|(a, b)| a.to_bits() != b.to_bits())
    {
        panic!(
            "{name}: first difference at sample {i}: {} vs {}",
            want[i], got[i]
        );
    }
    assert!(want.iter().any(|v| v.abs() > 1e-6), "{name}: silent");
}

/// Run `buf` through `f` in ragged blocks.
fn ragged(buf: &mut [f64], mut f: impl FnMut(&mut [f64])) {
    let (mut at, mut i) = (0, 0);
    while at < buf.len() {
        let n = BLOCKS[i % BLOCKS.len()].min(buf.len() - at);
        f(&mut buf[at..at + n]);
        at += n;
        i += 1;
    }
}

fn params(modulation: f64, size: f64) -> AlgorithmParams {
    AlgorithmParams {
        decay: 0.6,
        size,
        diffusion: 0.7,
        damping: 0.4,
        modulation,
        tone: 0.2,
        extra_a: 0.5,
        extra_b: 0.5,
        ..AlgorithmParams::default()
    }
}

fn built(
    algo: AlgorithmType,
    variant: usize,
    p: &AlgorithmParams,
    vintage: bool,
) -> Box<dyn ReverbAlgorithm> {
    let mut a = algorithms::create(algo, variant, SR);
    a.set_params(p);
    a.set_vintage(vintage);
    a.reset();
    a
}

#[test]
fn every_staged_algorithm_is_its_sample_loop() {
    let len = 24_000;
    let in_l = signal(len, 3);
    let in_r = signal(len, 4);
    for (algo, variant) in [
        (AlgorithmType::Room, 0),
        (AlgorithmType::Hall, 0),
        (AlgorithmType::Hall, 1),
    ] {
        for (modulation, size) in [(0.2, 0.5), (1.0, 0.0), (0.0, 1.0)] {
            for vintage in [false, true] {
                let p = params(modulation, size);
                let name =
                    format!("{algo:?}/{variant} mod {modulation} size {size} vintage {vintage}");
                let mut a = built(algo, variant, &p, vintage);
                let (mut want_l, mut want_r) = (in_l.clone(), in_r.clone());
                for (l, r) in want_l.iter_mut().zip(want_r.iter_mut()) {
                    (*l, *r) = a.tick(*l, *r);
                }
                let mut b = built(algo, variant, &p, vintage);
                let (mut got_l, mut got_r) = (in_l.clone(), in_r.clone());
                let (mut at, mut i) = (0, 0);
                while at < len {
                    let n = BLOCKS[i % BLOCKS.len()].min(len - at);
                    b.process_block(&mut got_l[at..at + n], &mut got_r[at..at + n]);
                    at += n;
                    i += 1;
                }
                assert_same(&format!("{name}, left"), &want_l, &got_l);
                assert_same(&format!("{name}, right"), &want_r, &got_r);
            }
        }
    }
}

#[test]
fn a_modulated_allpass_block_is_its_sample_loop() {
    let input = signal(20_000, 9);
    // Delays either side of the block path's reach, modulated hard enough
    // to cross whole samples inside blocks.
    for (delay, depth, rate) in [
        (400, 6.0, 0.7),
        (40, 4.0, 3.0),
        (20, 2.0, 1.0),
        (1200, 30.0, 0.2),
    ] {
        let make = || {
            let mut ap = ModulatedAllpass::new();
            ap.set_sample_rate(SR);
            ap.sample_delay = delay;
            ap.feedback = 0.6;
            ap.set_modulation(rate, depth, SR);
            ap.set_phase(0.37);
            ap.reset();
            ap
        };
        let mut a = make();
        let want: Vec<f64> = input.iter().map(|&x| a.tick(x)).collect();
        let mut b = make();
        let mut got = input.clone();
        ragged(&mut got, |block| b.process(block));
        assert_same(&format!("delay {delay} depth {depth}"), &want, &got);
    }
}

#[test]
fn a_network_block_is_its_sample_loop() {
    let input = signal(30_000, 11);
    let long = [1549usize, 1877, 2237, 2663, 3109, 3571, 4019, 4507];
    let short = [17usize, 23, 29, 41, 53, 61, 73, 89];
    for (name, delays, jitter, vintage) in [
        ("long", &long[..], 0.0, false),
        ("short", &short[..], 0.0, false),
        ("long, jitter", &long[..], 1.5, false),
        ("long, vintage", &long[..], 0.0, true),
        ("four lines", &long[..4], 0.0, false),
    ] {
        let make = || {
            let mut f = Fdn::new(delays, MixMatrix::Householder);
            f.set_decay(0.85);
            f.set_damping(8000.0, SR);
            f.set_loop_allpass(0.6);
            f.set_t60(3.0, 1.2, SR);
            f.set_rotation(0.5, 0.1, SR);
            f.set_jitter(jitter, SR);
            f.set_vintage_reads(vintage, SR);
            f.reset();
            f
        };
        let mut a = make();
        let want: Vec<f64> = input.iter().map(|&x| a.tick(x)).collect();
        let mut b = make();
        let mut got = input.clone();
        ragged(&mut got, |block| b.process(block));
        assert_same(name, &want, &got);
    }
}
