//! Realtime cost of one reverb instance: 10 s of stereo noise at 48 kHz in
//! 1024-frame blocks through each engine, reported as % of one core.
//!
//! Run: `cargo bench -p dsp --features reverb --bench reverb_cost`

use std::time::Instant;

use dsp::reverb::AlgorithmType;
use dsp::reverb::chain::ReverbChain;
use dsp_core::audio::{AudioConfig, Processor as _};

const SR: f64 = 48_000.0;
const BLOCK: usize = 1024;
const SECONDS: usize = 10;
const RUNS: usize = 3;

fn noise(n: usize, seed: u32) -> Vec<f64> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            f64::from(s) / f64::from(u32::MAX) - 0.5
        })
        .collect()
}

/// Best-of-`RUNS` wall time to render the noise, as % of realtime.
fn cost(algo: AlgorithmType, variant: usize) -> f64 {
    let config = AudioConfig {
        sample_rate: SR,
        max_buffer_size: BLOCK,
    };
    let frames = 48_000 * SECONDS;
    let in_l = noise(frames, 1);
    let in_r = noise(frames, 2);
    let mut best = f64::INFINITY;
    for _ in 0..RUNS {
        let mut c = ReverbChain::new();
        c.set_algorithm_variant(algo, variant);
        c.mix = 1.0;
        c.update(config);
        let mut l = vec![0.0; BLOCK];
        let mut r = vec![0.0; BLOCK];
        let start = Instant::now();
        for (bl, br) in in_l.chunks(BLOCK).zip(in_r.chunks(BLOCK)) {
            let n = bl.len();
            l[..n].copy_from_slice(bl);
            r[..n].copy_from_slice(br);
            c.process(&mut l[..n], &mut r[..n]);
        }
        std::hint::black_box(&l);
        best = best.min(start.elapsed().as_secs_f64());
    }
    best / SECONDS as f64 * 100.0
}

fn main() {
    for (name, algo, variant) in [
        ("Hall/Concert", AlgorithmType::Hall, 0),
        ("Hall/Cathedral", AlgorithmType::Hall, 1),
        ("Hall/Arena", AlgorithmType::Hall, 2),
        ("Room", AlgorithmType::Room, 0),
        ("Plate", AlgorithmType::Plate, 0),
    ] {
        println!("{name:>16}: {:6.2} % of one core", cost(algo, variant));
    }
}
