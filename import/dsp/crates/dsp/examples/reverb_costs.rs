//! What each reverb Studio's songs use costs a 512-frame block, and whether
//! a change moved its output.
//!
//! `cargo run --release -p dsp --features reverb --example reverb_costs --
//! [--seconds S] [--dump FILE | --compare FILE] [ALGORITHM...]`
//!
//! Each algorithm runs as Signal's reverb block sets it up (its parameter
//! defaults, 25 % mix) over a stereo test signal: drum-like hits in bars of
//! playing and bars of silence, so the tail rings out into nothing as it
//! does between a song's sections. It prints the best µs a block of seven
//! runs after a warm-up. `--dump` writes every output (f64 LE); `--compare`
//! nulls a run against such a file: bit for bit, or the largest difference
//! in dB below the output's peak.

use std::io::{Read, Write};
use std::time::Instant;

use dsp::reverb::AlgorithmType;
use dsp::reverb::chain::ReverbChain;
use dsp_core::audio::{AudioConfig, Processor as _};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

/// Hits for two bars, then two bars of silence, at 120 BPM.
fn signal(frames: usize, seed: u32) -> Vec<f64> {
    let mut s = seed;
    let mut noise = move || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        f64::from(s) / f64::from(u32::MAX) * 2.0 - 1.0
    };
    let beat = (SR * 0.5) as usize;
    let mut env = 0.0f64;
    let decay = (-1.0 / (0.06 * SR)).exp();
    (0..frames)
        .map(|i| {
            let bar = i / (4 * beat);
            let playing = bar % 4 < 2;
            if playing && i % (beat / 2) == 0 {
                env = 0.8;
            }
            env *= decay;
            if playing { env * noise() } else { 0.0 }
        })
        .collect()
}

fn chain(algo: AlgorithmType) -> ReverbChain {
    let mut c = ReverbChain::new();
    c.set_algorithm(algo);
    c.update(AudioConfig {
        sample_rate: SR,
        max_buffer_size: BLOCK,
    });
    // Signal's reverb block defaults.
    c.params.decay = 0.5;
    c.params.size = 0.5;
    c.params.diffusion = 0.7;
    c.params.damping = 0.4;
    c.params.modulation = 0.2;
    c.params.tone = 0.0;
    c.predelay_ms = 10.0;
    c.mix = 0.25;
    c.width = 1.0;
    c.input_hp_freq = 20.0;
    c.input_lp_freq = 20_000.0;
    c.params.extra_a = 0.5;
    c.params.extra_b = 0.5;
    c.update_params();
    c.reset();
    c
}

fn render(c: &mut ReverbChain, in_l: &[f64], in_r: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut out_l = in_l.to_vec();
    let mut out_r = in_r.to_vec();
    for (l, r) in out_l.chunks_mut(BLOCK).zip(out_r.chunks_mut(BLOCK)) {
        c.process(l, r);
    }
    (out_l, out_r)
}

fn main() {
    let mut seconds = 20.0f64;
    let mut dump = None;
    let mut compare = None;
    let mut names = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--seconds" => seconds = args.next().unwrap().parse().unwrap(),
            "--dump" => dump = args.next(),
            "--compare" => compare = args.next(),
            _ => names.push(a.to_lowercase()),
        }
    }
    if names.is_empty() {
        names = ["room", "hall", "plate", "nonlinear"]
            .map(String::from)
            .to_vec();
    }
    let frames = (SR * seconds) as usize;
    let in_l = signal(frames, 1);
    let in_r = signal(frames, 7);
    let reference = compare.map(|p| {
        let mut bytes = Vec::new();
        std::fs::File::open(p)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
            .chunks_exact(8)
            .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
            .collect::<Vec<f64>>()
    });
    let mut dumped = Vec::new();
    let mut offset = 0;
    let blocks = frames.div_ceil(BLOCK) as f64;
    for name in &names {
        let algo = match name.as_str() {
            "room" => AlgorithmType::Room,
            "hall" => AlgorithmType::Hall,
            "plate" => AlgorithmType::Plate,
            "nonlinear" => AlgorithmType::NonLinear,
            other => panic!("unknown algorithm {other}"),
        };
        // The output compared is a fresh chain's.
        let (out_l, out_r) = render(&mut chain(algo), &in_l, &in_r);
        let mut times = Vec::new();
        for _ in 0..7 {
            let mut c = chain(algo);
            let start = Instant::now();
            let out = render(&mut c, &in_l, &in_r);
            times.push(start.elapsed().as_secs_f64());
            std::hint::black_box(out);
        }
        times.sort_by(f64::total_cmp);
        let mut line = format!("{name:<10} {:>8.1} µs a block", times[0] * 1e6 / blocks);
        let mine: Vec<f64> = out_l.iter().chain(&out_r).copied().collect();
        if let Some(r) = reference.as_ref() {
            let theirs = &r[offset..offset + mine.len()];
            let peak = theirs.iter().fold(0.0f64, |m, x| m.max(x.abs()));
            let (mut max, mut same) = (0.0f64, true);
            for (a, b) in mine.iter().zip(theirs) {
                same &= a.to_bits() == b.to_bits();
                max = max.max((a - b).abs());
            }
            line += &if same {
                "  bit for bit".to_string()
            } else {
                format!(
                    "  max diff {max:.3e} ({:.1} dB re peak)",
                    20.0 * (max.max(1e-300) / peak).log10()
                )
            };
        }
        println!("{line}");
        offset += mine.len();
        dumped.extend(mine);
    }
    if let Some(p) = dump {
        let mut f = std::fs::File::create(p).unwrap();
        for x in dumped {
            f.write_all(&x.to_le_bytes()).unwrap();
        }
    }
}
