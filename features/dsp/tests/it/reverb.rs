//! The Hall engines' output, pinned: a cost optimisation of the network,
//! its allpasses or the delay lines under them must not change the sound.

use dsp::reverb::AlgorithmType;
use dsp::reverb::chain::ReverbChain;
use dsp_core::assert_golden;
use dsp_core::audio::{AudioConfig, Processor as _};
use dsp_core::golden::signal::{noise, widen};

const SR: f64 = 48_000.0;
const BLOCK: usize = 1024;

/// Two seconds of wet-only output: half a second of stereo noise, then the
/// tail ringing out — the network, its modulated allpasses and the
/// diffuser all heard, the decay included.
fn render(variant: usize) -> Vec<f64> {
    let frames = 96_000;
    let burst = 24_000;
    let mut in_l = widen(&noise(burst, 7));
    let mut in_r = widen(&noise(burst, 11));
    in_l.resize(frames, 0.0);
    in_r.resize(frames, 0.0);
    let mut chain = ReverbChain::new();
    chain.set_algorithm_variant(AlgorithmType::Hall, variant);
    chain.mix = 1.0;
    chain.update(AudioConfig {
        sample_rate: SR,
        max_buffer_size: BLOCK,
    });
    let mut out = Vec::with_capacity(frames * 2);
    for (l, r) in in_l.chunks_mut(BLOCK).zip(in_r.chunks_mut(BLOCK)) {
        chain.process(l, r);
        out.extend(l.iter().zip(r.iter()).flat_map(|(&a, &b)| [a, b]));
    }
    out
}

#[test]
fn hall_concert_renders_as_recorded() {
    assert_golden!(dsp_core::golden!(), "reverb/hall_concert", &render(0));
}

#[test]
fn hall_cathedral_renders_as_recorded() {
    assert_golden!(dsp_core::golden!(), "reverb/hall_cathedral", &render(1));
}

#[test]
fn hall_arena_renders_as_recorded() {
    assert_golden!(dsp_core::golden!(), "reverb/hall_arena", &render(2));
}
