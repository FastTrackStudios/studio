//! Stretch markers on a real kit, three ways, measured and written out to
//! listen to.
//!
//! ```text
//! cargo run --release -p stretch --example stretch_ab -- [MEDIA] [FROM_S] [SECONDS] [OUT]
//! ```
//!
//! `MEDIA` is a song's Media folder with a kit recorded as Session names
//! it (Unbreakable's by default), `FROM_S`/`SECONDS` the excerpt (60, 12),
//! `OUT` where the renders go. The kit's hits are read off its triggers;
//! every hit is moved as a quantize would move it (up to ±20 ms) and the
//! decays between stretched to fit — every mic through the same map, as
//! the engine renders a kit's takes — by:
//!
//! - `wsola`: the engine's renderer before this crate's map (WSOLA per
//!   stretch, `dsp::pitch::wsola::stretch_planar`);
//! - `map`: [`dsp::stretch::map`];
//! - `rubberband`: Rubber Band R3 with the same map as a key-frame map
//!   (`RUBBERBAND`, default `rubberband-r3` on the PATH), as the bar.
//!
//! Each render is scored (per mic, then the kit):
//!
//! - **attack**: how alike each hit's first 10 ms is to the recording's
//!   (correlation; 1 is the recording);
//! - **coherence**: how aligned pairs of mics of one source stay in the
//!   stretched decays, against how aligned they were recorded (1 = as
//!   recorded);
//! - **pre-echo**: energy in the 10 ms before each hit against the
//!   recording's there, dB (0 = none added);
//! - **clicks**: the largest jump in the high band against its median, dB;
//! - **tone**: the stretched decays' spectrum against the recording's
//!   decays, mean |dB| over third-octave bands (0 = same colour).

#![allow(
    clippy::allow_attributes,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::too_many_lines,
    clippy::missing_panics_doc,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::suboptimal_flops
)]

use std::path::{Path, PathBuf};

use dsp::stretch::map::{Anchor, MapConfig};

const SR: f64 = 48_000.0;
const TAKE: &str = "260907_1321-01";

/// The kit's mics (file prefix, name) and triggers.
const MICS: [(&str, &str); 8] = [
    ("65-In", "kick-in"),
    ("66-Out", "kick-out"),
    ("69-Top", "snare-top"),
    ("70-Bottom", "snare-bottom"),
    ("73-T1", "tom-1"),
    ("76-T4", "tom-4"),
    ("83-OHs", "overheads"),
    ("86-Rooms", "rooms"),
];
const TRIGGERS: [&str; 6] = [
    "67-Trig",
    "71-Trig",
    "78-T1 Trig",
    "79-T2 Trig",
    "80-T3 Trig",
    "81-T4 Trig",
];

/// Pairs of mics that hear one source: their alignment is what a stretch
/// must keep.
const PAIRS: [(&str, &str); 4] = [
    ("kick-in", "kick-out"),
    ("snare-top", "snare-bottom"),
    ("snare-top", "overheads"),
    ("tom-1", "overheads"),
];

/// A take, planar.
type Planar = Vec<Vec<f32>>;

fn read(media: &Path, prefix: &str, from: usize, len: usize) -> Planar {
    let path = media.join(format!("{prefix}-{TAKE}.wav"));
    let mut reader = hound::WavReader::open(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let spec = reader.spec();
    let channels = usize::from(spec.channels);
    let scale = 1.0 / f64::from(1_u32 << (spec.bits_per_sample - 1));
    reader.seek(u32::try_from(from).unwrap()).unwrap();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i32>()
            .take(len * channels)
            .map(|s| (f64::from(s.unwrap()) * scale) as f32)
            .collect(),
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .take(len * channels)
            .map(Result::unwrap)
            .collect(),
    };
    (0..channels)
        .map(|c| samples.iter().skip(c).step_by(channels).copied().collect())
        .collect()
}

fn write(path: &Path, planar: &Planar) {
    let spec = hound::WavSpec {
        channels: u16::try_from(planar.len()).unwrap(),
        sample_rate: SR as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..planar[0].len() {
        for ch in planar {
            w.write_sample(ch[i]).unwrap();
        }
    }
    w.finalize().unwrap();
}

/// The kit's hits, frames: where any trigger crosses half its loudest,
/// 40 ms apart at least, merged within 15 ms.
fn hits(triggers: &[Planar]) -> Vec<usize> {
    let mut all: Vec<usize> = Vec::new();
    for t in triggers {
        let x = &t[0];
        let peak = x.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        if peak < 0.05 {
            continue;
        }
        let mut last: Option<usize> = None;
        for (i, v) in x.iter().enumerate() {
            if v.abs() > peak * 0.5 && last.is_none_or(|l| i - l > 1_920) {
                all.push(i);
                last = Some(i);
            }
        }
    }
    all.sort_unstable();
    let mut merged: Vec<usize> = Vec::new();
    for h in all {
        if merged.last().is_none_or(|&l| h - l > 720) {
            merged.push(h);
        }
    }
    merged
}

/// The map a quantize makes: each hit's lead (5 ms before it) moved by
/// up to ±20 ms, pseudo-randomly; never closer than 20 ms to the last.
fn map(hits: &[usize], len: usize) -> Vec<Anchor> {
    let mut anchors = vec![Anchor { out: 0.0, src: 0.0 }];
    let mut seed = 0x9e37_79b9_u32;
    for &h in hits {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let shift = (f64::from(seed % 1_000) / 1_000.0 - 0.5) * 2.0 * 0.020 * SR;
        let src = h as f64 - 0.005 * SR;
        let last = anchors.last().unwrap();
        let out = (src + shift).max(last.out + 0.020 * SR);
        if src > last.src + 0.020 * SR && out < len as f64 - 0.05 * SR {
            anchors.push(Anchor { out, src });
        }
    }
    let last = *anchors.last().unwrap();
    anchors.push(Anchor {
        out: len as f64,
        src: last.src + (len as f64 - last.out),
    });
    anchors
}

/// The map a quantize's gap-closing makes (`arrange_edit::draw_out_between`):
/// every other gap between hits widened by `slow` of its tail — each
/// slice's first 30 ms as played, the next 30 ms at 0.85, and its far
/// tail slowed to `slow`, the next hit pushed on by what that adds.
fn drawn_out(hits: &[usize], len: usize, slow: f64) -> Vec<Anchor> {
    let (keep, body) = (0.030 * SR, 0.030 * SR);
    let mut anchors = vec![Anchor { out: 0.0, src: 0.0 }];
    let mut shift = 0.0;
    for (k, w) in hits.windows(2).enumerate() {
        let (a, b) = (w[0] as f64 - 0.005 * SR, w[1] as f64 - 0.005 * SR);
        let gap = b - a;
        if a <= anchors.last().unwrap().src || a + shift > len as f64 - 0.2 * SR {
            continue;
        }
        anchors.push(Anchor {
            out: a + shift,
            src: a,
        });
        if k % 2 == 1 || gap < keep + body + 0.020 * SR {
            continue;
        }
        let tail = gap - keep - body;
        anchors.push(Anchor {
            out: a + shift + keep,
            src: a + keep,
        });
        anchors.push(Anchor {
            out: a + shift + keep + body / 0.85,
            src: a + keep + body,
        });
        shift += body / 0.85 - body + tail / slow - tail;
        // The tail ends where the next hit's lead starts.
    }
    let last = *anchors.last().unwrap();
    if last.out < len as f64 {
        anchors.push(Anchor {
            out: len as f64,
            src: last.src + (len as f64 - last.out),
        });
    }
    anchors.retain(|a| a.out <= len as f64);
    anchors
}

/// The engine's renderer before `dsp::stretch::map`: WSOLA per stretch, a
/// 2 ms joint (`daw-standalone`'s `stretch_render`, as it was).
fn wsola(input: &Planar, anchors: &[Anchor], len: usize) -> Planar {
    let joint = 96;
    let grain = 1024;
    let mut out = vec![vec![0.0_f32; len + joint]; input.len()];
    let last = anchors.len() - 2;
    for (k, w) in anchors.windows(2).enumerate() {
        let (o0, o1) = (w[0].out.round() as usize, w[1].out.round() as usize);
        let n = o1.saturating_sub(o0);
        if n == 0 {
            continue;
        }
        let more = if k < last { joint } else { 0 };
        let (s0, s1) = (w[0].src, w[1].src);
        let rate = (s1 - s0) / n as f64;
        let from = s0.round() as usize;
        let read = |ch: &Vec<f32>, from: usize, n: usize| -> Vec<f32> {
            (from..from + n)
                .map(|i| ch.get(i).copied().unwrap_or(0.0))
                .collect()
        };
        let piece: Planar = if (rate - 1.0).abs() <= 1e-4 {
            input.iter().map(|c| read(c, from, n + more)).collect()
        } else {
            let span = (s1 - s0).round().max(1.0) as usize;
            let segment: Planar = input.iter().map(|c| read(c, from, span)).collect();
            dsp::pitch::wsola::stretch_planar(&segment, 0, rate, n + more, grain)
        };
        for (dest, p) in out.iter_mut().zip(&piece) {
            for (i, s) in p.iter().enumerate() {
                let gain = if k > 0 && i < joint {
                    (i as f32 + 0.5) / joint as f32
                } else if i >= n {
                    1.0 - ((i - n) as f32 + 0.5) / joint as f32
                } else {
                    1.0
                };
                if let Some(slot) = dest.get_mut(o0 + i) {
                    *slot += s * gain;
                }
            }
        }
    }
    for ch in &mut out {
        ch.truncate(len);
    }
    out
}

/// Rubber Band R3 through the CLI, the map as its key frames.
fn rubberband(
    dir: &Path,
    name: &str,
    input: &Planar,
    anchors: &[Anchor],
    len: usize,
) -> Option<Planar> {
    let exe = std::env::var("RUBBERBAND").unwrap_or_else(|_| "rubberband-r3".to_owned());
    let src = dir.join(format!("{name}.src.wav"));
    let map = dir.join(format!("{name}.map"));
    let dst = dir.join(format!("{name}.rb.wav"));
    write(&src, input);
    let lines: String = anchors
        .iter()
        .map(|a| format!("{} {}\n", a.src.round() as i64, a.out.round() as i64))
        .collect();
    std::fs::write(&map, lines).ok()?;
    let ok = std::process::Command::new(&exe)
        .args(["-q", "-3", "--timemap"])
        .arg(&map)
        .args(["-D", &format!("{:.6}", len as f64 / SR)])
        .arg(&src)
        .arg(&dst)
        .status()
        .ok()?
        .success();
    if !ok {
        return None;
    }
    let mut reader = hound::WavReader::open(&dst).ok()?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels);
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(Result::unwrap).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / f64::from(1_u32 << (spec.bits_per_sample - 1));
            reader
                .samples::<i32>()
                .map(|s| (f64::from(s.unwrap()) * scale) as f32)
                .collect()
        }
    };
    let mut planar: Planar = (0..channels)
        .map(|c| samples.iter().skip(c).step_by(channels).copied().collect())
        .collect();
    for ch in &mut planar {
        ch.resize(len, 0.0);
    }
    let _ = std::fs::remove_file(&src);
    Some(planar)
}

// ── Scores ───────────────────────────────────────────────────────────

fn correlation(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum();
    let e = |s: &[f32]| s.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
    let (ea, eb) = (e(a), e(b));
    if ea < 1e-12 || eb < 1e-12 {
        return f64::NAN;
    }
    dot / (ea * eb).sqrt()
}

fn mono(p: &Planar) -> Vec<f32> {
    let n = p.len() as f32;
    (0..p[0].len())
        .map(|i| p.iter().map(|c| c[i]).sum::<f32>() / n)
        .collect()
}

fn slice(x: &[f32], from: f64, len: usize) -> &[f32] {
    let a = (from.round().max(0.0) as usize).min(x.len());
    &x[a..(a + len).min(x.len())]
}

/// Mean attack correlation over the hits.
fn attack(src: &[f32], out: &[f32], anchors: &[Anchor]) -> f64 {
    let n = (0.010 * SR) as usize;
    let lead = 0.005 * SR;
    let v: Vec<f64> = anchors[1..anchors.len() - 1]
        .iter()
        .map(|a| correlation(slice(src, a.src + lead, n), slice(out, a.out + lead, n)))
        .filter(|c| c.is_finite())
        .collect();
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

/// The stretched decays, as (out from, src from, len) — each segment's
/// middle, where only the stretch is heard.
fn decays(anchors: &[Anchor]) -> Vec<(f64, f64, usize, f64)> {
    anchors
        .windows(2)
        .filter_map(|w| {
            let n = w[1].out - w[0].out;
            let m = w[1].src - w[0].src;
            let rate = m / n;
            ((rate - 1.0).abs() > 0.02 && n > 0.06 * SR).then(|| {
                let skip = 0.025 * SR;
                (
                    w[0].out + skip,
                    w[0].src + skip,
                    (n - 2.0 * skip).max(0.0) as usize,
                    rate,
                )
            })
        })
        .collect()
}

/// How aligned two mics stay in the decays, against as recorded: the
/// correlation at the recorded best lag (±2 ms), out over in, mean.
fn coherence(src: (&[f32], &[f32]), out: (&[f32], &[f32]), anchors: &[Anchor]) -> f64 {
    let reach = (0.002 * SR) as isize;
    let mut v = Vec::new();
    for (o, s, n, rate) in decays(anchors) {
        let m = (n as f64 * rate) as usize;
        let best = |a: &[f32], b: &[f32]| -> (isize, f64) {
            (-reach..=reach)
                .map(|lag| {
                    let (a, b) = if lag >= 0 {
                        (
                            &a[..a.len().saturating_sub(lag as usize)],
                            &b[lag as usize..],
                        )
                    } else {
                        (
                            &a[(-lag) as usize..],
                            &b[..b.len().saturating_sub((-lag) as usize)],
                        )
                    };
                    (lag, correlation(a, b))
                })
                .filter(|(_, c)| c.is_finite())
                .max_by(|x, y| x.1.abs().total_cmp(&y.1.abs()))
                .unwrap_or((0, f64::NAN))
        };
        let (lag, recorded) = best(slice(src.0, s, m), slice(src.1, s, m));
        let (a, b) = (slice(out.0, o, n), slice(out.1, o, n));
        let played = if lag >= 0 {
            correlation(
                &a[..a.len().saturating_sub(lag as usize)],
                &b[(lag as usize).min(b.len())..],
            )
        } else {
            correlation(
                &a[((-lag) as usize).min(a.len())..],
                &b[..b.len().saturating_sub((-lag) as usize)],
            )
        };
        if recorded.is_finite() && played.is_finite() && recorded.abs() > 0.2 {
            v.push(played / recorded);
        }
    }
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

fn energy(x: &[f32]) -> f64 {
    x.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() + 1e-12
}

/// Energy in the 10 ms before each hit's attack, out against recorded, dB.
fn pre_echo(src: &[f32], out: &[f32], anchors: &[Anchor]) -> f64 {
    let n = (0.010 * SR) as usize;
    let v: Vec<f64> = anchors[1..anchors.len() - 1]
        .iter()
        .map(|a| {
            10.0 * (energy(slice(out, a.out - n as f64, n))
                / energy(slice(src, a.src - n as f64, n)))
            .log10()
        })
        .collect();
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

/// The largest jump in the high band (first difference twice) against
/// its median, dB.
fn clicks(out: &[f32]) -> f64 {
    let d: Vec<f64> = out
        .windows(3)
        .map(|w| f64::from(w[2] - 2.0 * w[1] + w[0]).abs())
        .collect();
    let mut sorted = d.clone();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2].max(1e-9);
    let max = sorted.last().copied().unwrap_or(0.0);
    20.0 * (max / median).log10()
}

/// Third-octave band energies of `x` (from 50 Hz).
fn bands(x: &[f32]) -> Vec<f64> {
    use realfft::RealFftPlanner;
    let n = 4096;
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(n);
    let mut acc = vec![0.0_f64; n / 2 + 1];
    let window: Vec<f32> = (0..n)
        .map(|i| 0.5 - 0.5 * (core::f32::consts::TAU * i as f32 / n as f32).cos())
        .collect();
    let mut at = 0;
    while at + n <= x.len() {
        let mut frame: Vec<f32> = x[at..at + n]
            .iter()
            .zip(&window)
            .map(|(a, w)| a * w)
            .collect();
        let mut spec = fft.make_output_vec();
        fft.process(&mut frame, &mut spec).unwrap();
        for (a, c) in acc.iter_mut().zip(&spec) {
            *a += f64::from(c.norm_sqr());
        }
        at += n / 2;
    }
    let mut out = Vec::new();
    let mut lo = 50.0_f64;
    while lo < 18_000.0 {
        let hi = lo * 2f64.powf(1.0 / 3.0);
        let (a, b) = (
            (lo / SR * n as f64) as usize,
            (hi / SR * n as f64).ceil() as usize,
        );
        out.push(acc[a..b.min(acc.len())].iter().sum::<f64>() + 1e-12);
        lo = hi;
    }
    out
}

/// The decays' colour against the recording's: mean |dB| per band, the
/// overall level taken out.
fn tone(src: &[f32], out: &[f32], anchors: &[Anchor]) -> f64 {
    let (mut s, mut o): (Vec<f32>, Vec<f32>) = (Vec::new(), Vec::new());
    for (od, sd, n, rate) in decays(anchors) {
        o.extend_from_slice(slice(out, od, n));
        s.extend_from_slice(slice(src, sd, (n as f64 * rate) as usize));
    }
    if s.len() < 8192 || o.len() < 8192 {
        return f64::NAN;
    }
    let (bs, bo) = (bands(&s), bands(&o));
    let (ts, to): (f64, f64) = (bs.iter().sum(), bo.iter().sum());
    let d: Vec<f64> = bs
        .iter()
        .zip(&bo)
        .filter(|(a, _)| **a / ts > 1e-5)
        .map(|(a, b)| (10.0 * ((b / to) / (a / ts)).log10()).abs())
        .collect();
    d.iter().sum::<f64>() / d.len().max(1) as f64
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let media = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| {
        "/run/media/Development/fts-real/crescendum-2026-10-08/unbreakable/Media".to_owned()
    }));
    let from_s: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60.0);
    let secs: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(12.0);
    let out_dir = PathBuf::from(
        args.get(4)
            .cloned()
            .unwrap_or_else(|| "stretch-ab".to_owned()),
    );
    std::fs::create_dir_all(&out_dir).unwrap();
    let (from, len) = ((from_s * SR) as usize, (secs * SR) as usize);

    let triggers: Vec<Planar> = TRIGGERS
        .iter()
        .map(|t| read(&media, t, from, len))
        .collect();
    let hits = hits(&triggers);
    // MODE: `quantize` (hits moved ±20 ms) or `tail:<rate>` (gaps drawn
    // out, far tails at that rate — what the quantize does in a fill).
    let mode = std::env::var("MODE").unwrap_or_else(|_| "quantize".to_owned());
    let anchors = match mode.strip_prefix("tail:") {
        Some(rate) => drawn_out(&hits, len, rate.parse().unwrap_or(0.25)),
        None => map(&hits, len),
    };
    let decays = decays(&anchors);
    println!(
        "{} hits, {} anchors, {} stretched decays (rates {:.2}–{:.2})",
        hits.len(),
        anchors.len(),
        decays.len(),
        decays.iter().map(|d| d.3).fold(f64::INFINITY, f64::min),
        decays.iter().map(|d| d.3).fold(0.0, f64::max),
    );

    let mut config = MapConfig::for_sample_rate(SR);
    if let Ok(attack) = std::env::var("ATTACK_MS") {
        config.attack = (attack.parse::<f64>().unwrap_or(20.0) * SR / 1_000.0) as usize;
    }
    let algorithms = ["wsola", "map", "rubberband"];
    let mut kit: Vec<Vec<(String, Planar)>> = vec![Vec::new(); algorithms.len()];
    let mut sources: Vec<(String, Planar)> = Vec::new();
    println!(
        "\n{:<13} {:<11} {:>7} {:>9} {:>8} {:>7}",
        "mic", "engine", "attack", "pre-echo", "clicks", "tone"
    );
    for (prefix, name) in MICS {
        let input = read(&media, prefix, from, len);
        let started = std::time::Instant::now();
        let renders: Vec<Option<Planar>> = vec![
            Some(wsola(&input, &anchors, len)),
            Some(dsp::stretch::map::render(
                &input.iter().map(Vec::as_slice).collect::<Vec<_>>(),
                &anchors,
                len,
                &config,
            )),
            rubberband(&out_dir, name, &input, &anchors, len),
        ];
        let _ = started;
        let src = mono(&input);
        for (k, (alg, render)) in algorithms.iter().zip(renders).enumerate() {
            let Some(render) = render else {
                println!("{name:<13} {alg:<11} (not rendered)");
                continue;
            };
            let out = mono(&render);
            println!(
                "{name:<13} {alg:<11} {:>7.3} {:>8.1}dB {:>6.1}dB {:>6.2}dB",
                attack(&src, &out, &anchors),
                pre_echo(&src, &out, &anchors),
                clicks(&out),
                tone(&src, &out, &anchors),
            );
            let dir = out_dir.join(alg);
            std::fs::create_dir_all(&dir).unwrap();
            write(&dir.join(format!("{name}.wav")), &render);
            kit[k].push((name.to_owned(), render));
        }
        sources.push((name.to_owned(), input));
    }

    println!("\ncoherence (1 = as recorded)");
    for (a, b) in PAIRS {
        let get =
            |set: &[(String, Planar)], n: &str| mono(&set.iter().find(|(k, _)| k == n).unwrap().1);
        let (sa, sb) = (get(&sources, a), get(&sources, b));
        let line: Vec<String> = algorithms
            .iter()
            .zip(&kit)
            .filter(|(_, set)| set.iter().any(|(k, _)| k == a) && set.iter().any(|(k, _)| k == b))
            .map(|(alg, set)| {
                format!(
                    "{alg} {:.3}",
                    coherence((&sa, &sb), (&get(set, a), &get(set, b)), &anchors)
                )
            })
            .collect();
        println!("  {a} / {b}: {}", line.join("  "));
    }

    // The kit summed, each way, and the recording: to listen to.
    let sum = |set: &[(String, Planar)]| -> Planar {
        let mut out = vec![vec![0.0_f32; len]; 2];
        for (_, p) in set {
            for (c, o) in out.iter_mut().enumerate() {
                let ch = &p[c.min(p.len() - 1)];
                for (x, y) in o.iter_mut().zip(ch) {
                    *x += y * 0.25;
                }
            }
        }
        out
    };
    write(&out_dir.join("kit-recorded.wav"), &sum(&sources));
    for (alg, set) in algorithms.iter().zip(&kit) {
        if !set.is_empty() {
            write(&out_dir.join(format!("kit-{alg}.wav")), &sum(set));
        }
    }
    println!("\nrenders: {}", out_dir.display());
}
