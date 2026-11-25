use std::f32::consts::TAU;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{Value as Json, json};

fn write_wav(path: &Path, sr: u32, samples: &[f32]) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for &s in samples {
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
}

/// Synthesize a realistic decaying drum hit (fast attack, exponential decay).
fn add_drum_hit(buf: &mut [f32], sr: f32, at_s: f32, amp: f32, freq: f32, decay_s: f32) {
    let start = (at_s * sr).round() as usize;
    let len = (decay_s * 5.0 * sr) as usize;
    for i in 0..len {
        let idx = start + i;
        if idx >= buf.len() {
            break;
        }
        let t = i as f32 / sr;
        let env = (1.0 - (-t / 0.0008).exp()) * (-t / decay_s).exp();
        buf[idx] += amp * env * (TAU * freq * t).sin();
    }
}

/// Synthesize a sharp contact-trigger spike.
fn add_trigger_spike(buf: &mut [f32], sr: f32, at_s: f32, amp: f32) {
    let start = (at_s * sr).round() as usize;
    for i in 0..40 {
        let idx = start + i;
        if idx >= buf.len() {
            break;
        }
        let t = i as f32 / sr;
        buf[idx] += amp * (-t / 0.0003).exp();
    }
}

#[test]
fn process_extension_stdio_rpc_detect_and_hybrid_edit() {
    let dir = std::env::temp_dir().join(format!("fts-rhythm-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let sr = 48_000_u32;
    let len = (sr as usize) * 3;
    let mut kick_mic = vec![0.0_f32; len];
    let mut kick_trig = vec![0.0_f32; len];
    let mut snare_mic = vec![0.0_f32; len];
    let mut snare_trig = vec![0.0_f32; len];

    // 4-beat groove: kicks at 0.50, 1.50, 2.50; snares at 1.02 (20ms late), 2.00
    // with realistic bleed of kick into snare mic and snare into kick mic.
    for &t in &[0.50_f32, 1.50, 2.50] {
        add_drum_hit(&mut kick_mic, sr as f32, t, 0.85, 65.0, 0.08);
        add_trigger_spike(&mut kick_trig, sr as f32, t, 0.9);
        add_drum_hit(&mut snare_mic, sr as f32, t, 0.08, 65.0, 0.06); // kick bleed
    }
    for &t in &[1.02_f32, 2.00] {
        add_drum_hit(&mut snare_mic, sr as f32, t, 0.80, 195.0, 0.06);
        add_trigger_spike(&mut snare_trig, sr as f32, t, 0.9);
        add_drum_hit(&mut kick_mic, sr as f32, t, 0.06, 195.0, 0.05); // snare bleed
    }

    let kick_mic_path = dir.join("kick_mic.wav");
    let kick_trig_path = dir.join("kick_trig.wav");
    let snare_mic_path = dir.join("snare_mic.wav");
    let snare_trig_path = dir.join("snare_trig.wav");
    write_wav(&kick_mic_path, sr, &kick_mic);
    write_wav(&kick_trig_path, sr, &kick_trig);
    write_wav(&snare_mic_path, sr, &snare_mic);
    write_wav(&snare_trig_path, sr, &snare_trig);

    // Spawn the actual out-of-process extension binary.
    let bin = env!("CARGO_BIN_EXE_fts-rhythm");
    let mut child = Command::new(bin)
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn fts-rhythm serve");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let mut call = |id: u32, action: &str, params: Json| -> Json {
        let req = json!({ "id": id, "action": action, "params": params });
        writeln!(stdin, "{req}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let resp: Json = serde_json::from_str(&line).unwrap();
        assert!(resp.get("error").is_none(), "rpc error: {resp}");
        resp["result"].clone()
    };

    // 1. Query capabilities / swappable detectors.
    let state = call(1, "RHYTHM_STATE", Json::Null);
    let detectors = state["detectors"].as_array().unwrap();
    assert!(detectors.iter().any(|d| d["id"] == "fts.hits.kit-fusion"));
    assert!(detectors.iter().any(|d| d["id"] == "fts.hits.envelope-gate"));

    // 2. Detect with kit-fusion (trigger+mic calibrated, bleed suppressed).
    let detected = call(
        2,
        "RHYTHM_DETECT",
        json!({
            "detector": "fts.hits.kit-fusion",
            "pieces": [
                { "name": "Kick", "mic_wav": kick_mic_path, "trigger_wav": kick_trig_path },
                { "name": "Snare", "mic_wav": snare_mic_path, "trigger_wav": snare_trig_path }
            ]
        }),
    );
    let hits = detected["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 5, "expected 3 kicks + 2 snares, got {hits:#?}");

    // 3. Move one individual snare hit by hand (1.02 -> 1.00): pure slip + crossfade.
    let snare0_id = hits
        .iter()
        .find(|h| h["piece"] == "Snare")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let plan = call(
        3,
        "RHYTHM_PLACE_HITS",
        json!({
            "hits": hits,
            "moves": [{ "id": snare0_id, "target_time": 1.00, "mode": "auto" }],
            "span": [0.0, 3.0]
        }),
    );
    assert_eq!(plan["audit"]["attacks_protected"], true);
    assert_eq!(plan["audit"]["unclosed_gaps"], 0);
    assert_eq!(plan["audit"]["tail_stretch_segments"], 0);

    let group_path = dir.join("phase-group.wav");
    let render_path = dir.join("edited.wav");
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&group_path, spec).unwrap();
    for &sample in &snare_mic {
        writer.write_sample(sample).unwrap();
        writer.write_sample(sample * 0.5).unwrap();
    }
    writer.finalize().unwrap();
    let render = call(5, "RHYTHM_RENDER_PLAN", json!({
        "plan": plan,
        "input_wav": group_path,
        "output_wav": render_path,
    }));
    assert_eq!(render["frames"], len);
    assert_eq!(render["channels"], 2);
    let mut rendered = hound::WavReader::open(render_path).unwrap();
    let samples: Vec<_> = rendered.samples::<f32>().collect::<Result<_, _>>().unwrap();
    assert!(samples.chunks_exact(2).all(|frame| frame[0] * 0.5 == frame[1]));
    assert!(samples[2 * 48_000..2 * 48_400].iter().any(|s| s.abs() > 0.1));

    let quantized = call(6, "RHYTHM_QUANTIZE_PLAN", json!({
        "hits": [
            {"id":"a", "piece":"Kick", "at":0.535},
            {"id":"b", "piece":"Snare", "at":0.541}
        ],
        "bpm":120, "span":[0,3],
        "pinned_moves":[{"id":"b","target_time":0.506}]
    }));
    let moves = quantized["moves"].as_array().unwrap();
    assert!((moves[0]["target_time"].as_f64().unwrap() - 0.5).abs() < 1e-9);
    assert_eq!(moves[1]["target_time"], 0.506);

    // 4. Swap detector to envelope-gate on the exact same WAVs.
    let gate_out = call(
        4,
        "RHYTHM_DETECT",
        json!({
            "detector": "fts.hits.envelope-gate",
            "threshold_db": -16.0,
            "pieces": [
                { "name": "Kick", "mic_wav": kick_mic_path },
                { "name": "Snare", "mic_wav": snare_mic_path }
            ]
        }),
    );
    assert_eq!(gate_out["detector_id"], "fts.hits.envelope-gate");
    assert!(!gate_out["hits"].as_array().unwrap().is_empty());

    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success());
    let _ = std::fs::remove_dir_all(&dir);
}
