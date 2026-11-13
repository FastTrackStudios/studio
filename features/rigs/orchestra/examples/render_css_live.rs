//! Render a MIDI file through the LIVE (reactive, exact-CSS) engine path → WAV,
//! for A/B against a real CSS-in-Kontakt export of the same MIDI.
//!
//! Unlike `render_css_test` (document/lookahead path), this drives the engine
//! the way a live plugin does: dispatch each CC / note-on / note-off in real
//! time and `render_offline` the gaps. Warms each note's articulation first
//! (offline cache). Channel 0 → the instrument.
//!
//! ```text
//! cargo run --release -p signal-sampler --example render_css_live -- css_test_full.mid css_test_full_live.wav
//! ```

use sampler::SamplerRig;

const ID: &str = "strings_1v";
const SR: u32 = 48_000;

fn read_vlq(d: &[u8], p: &mut usize) -> u32 {
    let mut v = 0u32;
    loop {
        let b = d[*p];
        *p += 1;
        v = (v << 7) | u32::from(b & 0x7f);
        if b & 0x80 == 0 {
            break;
        }
    }
    v
}

/// Minimal SMF parse → `(seconds, status, d1, d2)` channel events, in order.
fn parse_smf(d: &[u8]) -> Vec<(f64, u8, u8, u8)> {
    let div = f64::from(u16::from_be_bytes([d[12], d[13]]));
    let mut us_per_q = 500_000.0f64;
    let mut p = 14;
    while &d[p..p + 4] != b"MTrk" {
        let len = u32::from_be_bytes([d[p + 4], d[p + 5], d[p + 6], d[p + 7]]) as usize;
        p += 8 + len;
    }
    let track_len = u32::from_be_bytes([d[p + 4], d[p + 5], d[p + 6], d[p + 7]]) as usize;
    p += 8;
    let end = p + track_len;
    let mut sec = 0.0f64;
    let mut running = 0u8;
    let mut out = Vec::new();
    while p < end {
        let dt = u64::from(read_vlq(d, &mut p));
        sec += dt as f64 * (us_per_q / 1_000_000.0) / div;
        let mut status = d[p];
        if status & 0x80 != 0 {
            p += 1;
            running = status;
        } else {
            status = running;
        }
        match status {
            0xFF => {
                let meta = d[p];
                p += 1;
                let len = read_vlq(d, &mut p) as usize;
                if meta == 0x51 {
                    us_per_q = f64::from(d[p]).mul_add(65536.0, f64::from(d[p + 1]) * 256.0)
                        + f64::from(d[p + 2]);
                }
                p += len;
            }
            0xF0 | 0xF7 => {
                let len = read_vlq(d, &mut p) as usize;
                p += len;
            }
            s if (0x80..=0xEF).contains(&s) => {
                let d1 = d[p];
                let two = !matches!(s & 0xF0, 0xC0 | 0xD0);
                let d2 = if two { d[p + 1] } else { 0 };
                p += if two { 2 } else { 1 };
                out.push((sec, s, d1, d2));
            }
            _ => break,
        }
    }
    out
}

/// 32-bit integer PCM: a −190 dB floor, so a null test against Kontakt is
/// limited by the engine, never by the file (16-bit capped it near −98 dB).
fn write_wav(path: &str, samples: &[f32]) -> eyre::Result<()> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SR,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    for &s in samples {
        w.write_sample((f64::from(s.clamp(-1.0, 1.0)) * f64::from(i32::MAX)) as i32)?;
    }
    w.finalize()?;
    Ok(())
}

/// Host block size: the engine renders in plugin-sized blocks, as it would
/// under a DAW, so block-quantised behaviour shows up here too.
const BLOCK: usize = 128;

fn main() -> eyre::Result<()> {
    let inp = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "css_test_full.mid".into());
    let outp = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "css_test_full_live.wav".into());
    let events = parse_smf(&std::fs::read(&inp)?);
    eprintln!("parsed {} events from {inp}", events.len());

    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    // The orchestra feature's strings *definition* — loads CSS 1st Violins with
    // the exact engine settings (solo mic, arco-attack bloom, release overlap)
    // that match a real CSS-in-Kontakt render.
    // `--library woodwinds` (default strings) and `--section "Solo Flute"`:
    // any instrument of any library, by its patch folder name (default 1st
    // Violins).
    let all: Vec<String> = std::env::args().collect();
    let flag = |name: &str| {
        all.iter()
            .position(|a| a == name)
            .and_then(|i| all.get(i + 1).cloned())
    };
    let section = flag("--section").unwrap_or_else(|| "1st Violins".to_string());
    let lib_name = flag("--library").unwrap_or_else(|| "strings".to_string());
    let library = signal_orchestra::library(&lib_name)
        .ok_or_else(|| eyre::eyre!("unknown library {lib_name:?}"))?;
    signal_orchestra::load_instrument(&rig, ID, library, &section, "Mix")
        .map_err(|e| eyre::eyre!(e))?;
    let trace = std::env::args().any(|a| a == "--trace");
    rig.set_trace_enabled(ID, trace);
    // `--tunes a,b,…`: Kontakt's random short detune draws (cents), fitted
    // from its render, replayed so the rest of the null test is exact.
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--tunes") {
        let tunes: Vec<i8> = args
            .get(i + 1)
            .map(|s| s.split(',').filter_map(|v| v.trim().parse().ok()).collect())
            .unwrap_or_default();
        rig.force_random_tunes(ID, &tunes);
    }
    // `--attacks a,b,…`: loud first-note attack draws (index into the range).
    if let Some(i) = args.iter().position(|a| a == "--attacks") {
        let v: Vec<u8> = args
            .get(i + 1)
            .map(|s| s.split(',').filter_map(|v| v.trim().parse().ok()).collect())
            .unwrap_or_default();
        rig.force_random_attacks(ID, &v);
    }
    // `--variants a,b,…`: legato zone-2 body-variant draws (2..=4).
    if let Some(i) = args.iter().position(|a| a == "--variants") {
        let v: Vec<u8> = args
            .get(i + 1)
            .map(|s| s.split(',').filter_map(|v| v.trim().parse().ok()).collect())
            .unwrap_or_default();
        rig.force_random_variants(ID, &v);
    }

    // Render up to an ABSOLUTE frame in host-sized blocks. Each event lands on
    // round(sec * SR) exactly: no per-gap rounding that drifts across a file.
    let render_to = |rig: &SamplerRig, out: &mut Vec<f32>, frame: usize| -> eyre::Result<()> {
        let mut buf = vec![0.0f32; BLOCK * 2];
        while out.len() / 2 < frame {
            let n = (frame - out.len() / 2).min(BLOCK);
            buf[..n * 2].fill(0.0);
            rig.render_offline(&mut buf[..n * 2])?;
            out.extend_from_slice(&buf[..n * 2]);
        }
        Ok(())
    };
    let frame_of = |sec: f64| (sec * f64::from(SR)).round().max(0.0) as usize;

    let mut audio: Vec<f32> = Vec::new();
    for (sec, status, d1, d2) in &events {
        render_to(&rig, &mut audio, frame_of(*sec))?;
        match status & 0xF0 {
            0xB0 => rig.cc(ID, *d1, *d2),
            0x90 if *d2 > 0 => {
                rig.warm_note(ID, *d1);
                rig.note_on(ID, *d1, *d2);
            }
            0x90 | 0x80 => rig.note_off(ID, *d1),
            _ => {}
        }
    }
    let end = audio.len() / 2 + frame_of(8.0); // tail
    render_to(&rig, &mut audio, end)?;

    if trace {
        // Every voice the engine started: what plays, from where, how loud.
        for e in &rig.render_trace(ID).events {
            if let sampler::engine::trace::TraceKind::Transition {
                from,
                to,
                portamento,
            } = &e.kind
            {
                println!(
                    "{:9.4}s TRANSITION {from} -> {to} portamento {portamento}",
                    e.frame as f64 / f64::from(SR)
                );
            }
            if let sampler::engine::trace::TraceKind::NoteOff { note } = &e.kind {
                println!("{:9.4}s NOTEOFF    {note}", e.frame as f64 / f64::from(SR));
            }
            if let sampler::engine::trace::TraceKind::VoiceEnd { voice_id } = &e.kind {
                println!(
                    "{:9.4}s END        voice {voice_id}",
                    e.frame as f64 / f64::from(SR)
                );
            }
            if let sampler::engine::trace::TraceKind::VoiceSpawn(v) = &e.kind {
                print!("[v{}] ", v.voice_id);
                println!(
                    "{:9.4}s {:12} {:44} note {:3} root {:3} rate {:.6} gain {:.4} dyn {:4} start {:6} loop {}..{}",
                    e.frame as f64 / f64::from(SR),
                    v.voice_kind,
                    v.file,
                    v.note,
                    v.root_key,
                    v.rate,
                    v.gain,
                    v.dynamic,
                    v.start_frame,
                    v.loop_start,
                    v.loop_end
                );
            }
        }
    }
    write_wav(&outp, &audio)?;
    println!(
        "wrote {outp}  ({:.1}s, {} events)",
        audio.len() as f64 / 2.0 / f64::from(SR),
        events.len()
    );
    Ok(())
}
