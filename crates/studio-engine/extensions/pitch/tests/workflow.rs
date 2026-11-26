use std::f32::consts::TAU;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use pitch_core::midi_to_hz;
use pitch_ext::write_wav_mono;
use serde_json::{Value as Json, json};

fn add_vocal_note(buf: &mut [f32], sr: f32, start_s: f32, dur_s: f32, midi: f64) {
    let f0 = midi_to_hz(midi) as f32;
    let start = (start_s * sr).round() as usize;
    let len = (dur_s * sr).round() as usize;
    for i in 0..len {
        let idx = start + i;
        if idx >= buf.len() {
            break;
        }
        let t = i as f32 / sr;
        let fade = (t / 0.015).min(1.0) * ((dur_s - t).max(0.0) / 0.015).min(1.0);
        // Harmonic-rich vocal-like tone (fundamental + 2nd + 3rd harmonics).
        let s = 0.55 * (TAU * f0 * t).sin()
            + 0.25 * (TAU * 2.0 * f0 * t).sin()
            + 0.12 * (TAU * 3.0 * f0 * t).sin();
        buf[idx] += 0.6 * fade * s;
    }
}

#[test]
fn process_extension_analyze_correct_and_resynthesize() {
    let dir = std::env::temp_dir().join(format!("fts-pitch-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let sr = 48_000_u32;
    let mut audio = vec![0.0_f32; (sr as usize) * 2];
    // Note 1: A4 (MIDI 69) sharp by +35 cents -> 69.35
    add_vocal_note(&mut audio, sr as f32, 0.15, 0.65, 69.35);
    // Note 2: C5 (MIDI 72) flat by -30 cents -> 71.70
    add_vocal_note(&mut audio, sr as f32, 1.00, 0.65, 71.70);

    let in_wav = dir.join("vocal_raw.wav");
    let out_wav = dir.join("vocal_tuned.wav");
    write_wav_mono(&in_wav, sr, &audio).unwrap();

    let bin = env!("CARGO_BIN_EXE_fts-pitch");
    let mut child = Command::new(bin)
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn fts-pitch serve");

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

    // 1. State lists swappable detector, segmenter, and synthesizer backends.
    let state = call(1, "PITCH_STATE", Json::Null);
    assert_eq!(state["detectors"][0]["id"], "fts.pitch.yin");
    assert_eq!(state["segmenters"][0]["id"], "fts.segment.contour");
    assert_eq!(state["synthesizers"][0]["id"], "fts.synth.td-ola");

    // 2. Analyze raw vocal WAV.
    let doc = call(2, "PITCH_ANALYZE", json!({ "wav": in_wav }));
    let notes = doc["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 2, "expected 2 notes, got {notes:#?}");
    assert!((notes[0]["detected_midi"].as_f64().unwrap() - 69.35).abs() < 0.15);
    assert!((notes[1]["detected_midi"].as_f64().unwrap() - 71.70).abs() < 0.15);

    // 3. Snap to C major (root_pc = 0 -> contains A4=69 and C5=72).
    let corrected = call(
        3,
        "PITCH_CORRECT_SCALE",
        json!({ "doc": doc, "root_pc": 0, "mode": "major", "strength": 1.0 }),
    );
    assert!(
        corrected["notes"][0]["pitch_delta_semitones"]
            .as_f64()
            .unwrap()
            < -0.2
    );
    assert!(
        corrected["notes"][1]["pitch_delta_semitones"]
            .as_f64()
            .unwrap()
            > 0.2
    );

    // 4. Incrementally render to cached WAV.
    let cache = call(
        4,
        "PITCH_RENDER_SPAN",
        json!({
            "source_wav": in_wav,
            "doc": corrected,
            "out_wav": out_wav
        }),
    );
    assert_eq!(cache["synthesizer_id"], "fts.synth.td-ola");
    assert!(out_wav.exists());

    // 5. Re-analyze rendered WAV and verify notes landed on A4 (69.0) and C5 (72.0)!
    let after = call(5, "PITCH_ANALYZE", json!({ "wav": out_wav }));
    let tuned_notes = after["notes"].as_array().unwrap();
    assert_eq!(
        tuned_notes.len(),
        2,
        "expected 2 tuned notes, got {tuned_notes:#?}"
    );
    let n0 = tuned_notes[0]["detected_midi"].as_f64().unwrap();
    let n1 = tuned_notes[1]["detected_midi"].as_f64().unwrap();
    assert!((n0 - 69.0).abs() < 0.2, "note 0 expected ~69.0, got {n0}");
    assert!((n1 - 72.0).abs() < 0.2, "note 1 expected ~72.0, got {n1}");

    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success());
    let _ = std::fs::remove_dir_all(&dir);
}
