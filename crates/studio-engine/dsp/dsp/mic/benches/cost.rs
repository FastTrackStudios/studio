//! Realtime cost of one FTS Mic instance (one modelled mic): 10 s of
//! capsule-pair noise at 48 kHz in 1024-frame blocks, as % of one core.
//!
//! The models are private: the bench reads `$FTS_MIC_MODELS/48000/LD-47K.micm`
//! (else `~/.local/share/fts/mic-models/...`) and says so when it is absent.
//!
//! Run: `cargo bench -p dsp --features mic --bench mic_cost`

use std::path::PathBuf;
use std::time::Instant;

use dsp::mic::{MicChain, MicModel};

const SR: usize = 48_000;
const BLOCK: usize = 1024;
const SECONDS: usize = 10;
const RUNS: usize = 3;

fn model_path() -> PathBuf {
    std::env::var_os("FTS_MIC_MODELS")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/fts/mic-models"))
        })
        .unwrap_or_default()
        .join("48000/LD-47K.micm")
}

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

fn main() {
    let path = model_path();
    let Ok(bytes) = std::fs::read(&path) else {
        println!("no mic model at {} — skipped", path.display());
        return;
    };
    let model = MicModel::from_bytes(&bytes).expect("model parses");
    let taps = model.taps;
    let frames = SR * SECONDS;
    let front = noise(frames, 1);
    let rear = noise(frames, 2);
    let mut out = vec![0.0; BLOCK];
    let mut best = f64::INFINITY;
    for _ in 0..RUNS {
        let mut chain = MicChain::new(model.clone());
        let start = Instant::now();
        for (f, r) in front.chunks(BLOCK).zip(rear.chunks(BLOCK)) {
            for ((o, &a), &b) in out.iter_mut().zip(f).zip(r) {
                *o = chain.process(a, b);
            }
            std::hint::black_box(&out);
        }
        best = best.min(start.elapsed().as_secs_f64());
    }
    println!(
        "mic ({taps} taps): {:6.2} % of one core",
        best / SECONDS as f64 * 100.0
    );
}
