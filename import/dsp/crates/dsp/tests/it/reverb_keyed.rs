//! Non-Linear keyed by hits: on a drum sum full of bleed its onset detector
//! (+12 dB over a 150 ms follower) never fires, so the lane is silent.
//! Keyed (`set_keyed`, `trigger_at`), each hit the host hands it opens the
//! window on its frame: a gated burst per hit, silence between.

use dsp::reverb::AlgorithmType;
use dsp::reverb::chain::ReverbChain;
use dsp_core::audio::{AudioConfig, Processor as _};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;
const FRAMES: usize = 48_000 * 3;
const HITS: [usize; 2] = [24_000, 84_000];

fn chain() -> ReverbChain {
    let mut chain = ReverbChain::new();
    chain.set_algorithm(AlgorithmType::NonLinear);
    chain.update(AudioConfig {
        sample_rate: SR,
        max_buffer_size: BLOCK,
    });
    // A gated burst a quarter-second long (0.1 + 1.9 × 0.08 s), all wet.
    chain.params.decay = 0.08;
    chain.params.extra_a = 0.3;
    chain.mix = 1.0;
    chain.update_params();
    chain.reset();
    chain
}

/// A snare sum: loud bleed throughout (the kit, at -10 dBFS) and the snare's
/// hits only ~6 dB over it, never +12 dB over its slow follower.
fn snare_sum() -> Vec<f64> {
    let mut x = 0x1234_5678_u32;
    let mut out: Vec<f64> = (0..FRAMES)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            0.3 * (f64::from(x >> 8) / 8_388_608.0 - 1.0)
        })
        .collect();
    for &h in &HITS {
        for (i, s) in out.iter_mut().enumerate().skip(h).take(4_800) {
            let t = (i - h) as f64;
            *s += 0.6 * (-t / 1_200.0).exp() * (t * 200.0 * std::f64::consts::TAU / SR).sin();
        }
    }
    out
}

fn render(chain: &mut ReverbChain, keyed: bool) -> Vec<f64> {
    chain.set_keyed(keyed);
    let mut l = snare_sum();
    let mut r = l.clone();
    for (b, (l, r)) in l.chunks_mut(BLOCK).zip(r.chunks_mut(BLOCK)).enumerate() {
        if keyed {
            for &h in &HITS {
                if (b * BLOCK..(b + 1) * BLOCK).contains(&h) {
                    chain.trigger_at(h - b * BLOCK);
                }
            }
        }
        chain.process(l, r);
    }
    l
}

fn peak(x: &[f64]) -> f64 {
    x.iter().fold(0.0, |m, s| m.max(s.abs()))
}

#[test]
fn on_a_sum_full_of_bleed_its_own_detector_stays_shut() {
    // The bleed starting from silence is an onset: one window at the start,
    // then never again, the snare's hits included.
    let out = render(&mut chain(), false);
    assert!(
        peak(&out[14_000..]) < 1e-9,
        "unkeyed it never opens on the hits: {}",
        peak(&out[14_000..])
    );
}

#[test]
fn each_hit_opens_a_gated_burst_when_keyed() {
    let mut c = chain();
    let out = render(&mut c, true);
    let window = 12_000; // 0.25 s
    assert!(peak(&out[..HITS[0]]) < 1e-12, "silent before the first hit");
    for &h in &HITS {
        let burst = peak(&out[h..h + window]);
        assert!(burst > 0.05, "a burst on the hit at {h}: {burst}");
        let after = peak(&out[h + window + 500..(h + window + 20_000).min(FRAMES)]);
        assert!(after < 1e-9, "shut after the window at {h}: {after}");
    }
    // A burst starts on its hit's frame, not before it.
    assert!(peak(&out[HITS[1] - 2_000..HITS[1]]) < 1e-9);
    // Unkeyed again, the onset detector is back: it opens on the bleed's
    // start, as a fresh one does.
    c.reset();
    let out = render(&mut c, false);
    assert!(peak(&out[..12_000]) > 0.05 && peak(&out[14_000..]) < 1e-9);
}
