#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

fn backend(dir: &Path, script: &str) -> std::path::PathBuf {
    let path = dir.join("mock backend");
    fs::write(&path, format!("#!/bin/sh\nset -eu\n{script}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[test]
fn cli_runs_audio_separator_and_preserves_paths() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("a mix.wav");
    fs::write(&input, b"input").unwrap();
    let out = dir.path().join("output dir");
    let executable = backend(
        dir.path(),
        r#"
        test "$1 $2 $3 $4" = "--model_filename demo.ckpt --output_format WAV"
        test "$5" = "--output_dir"
        test "$7" = "--single_stem=Vocals"
        test -f "$8"
        printf audio > "$6/Vocals.wav"
        "#,
    );
    let result = Command::new(env!("CARGO_BIN_EXE_stem-seperation"))
        .args([
            "audio-separator",
            "--model",
            "demo.ckpt",
            "--single-stem",
            "Vocals",
        ])
        .arg(&input)
        .arg("--output")
        .arg(&out)
        .arg("--executable")
        .arg(executable)
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result);
    assert_eq!(fs::read(out.join("Vocals.wav")).unwrap(), b"audio");
}

#[test]
fn failed_inference_cleans_up_and_leaves_existing_outputs_alone() {
    use stem_seperation::{Backend, Job};

    let dir = tempfile::tempdir().unwrap();
    let executable = backend(dir.path(), "exit 3");
    let input = dir.path().join("mix.wav");
    fs::write(&input, b"input").unwrap();
    let output = dir.path().join("stems");
    let job = Job {
        input: &input,
        output_dir: &output,
        model: "htdemucs",
    };
    let demucs = Backend::Demucs {
        device: None,
        two_stems: None,
    };

    assert!(demucs.separate(executable.as_os_str(), &job).is_err());
    assert!(!output.exists());

    fs::create_dir(&output).unwrap();
    fs::write(output.join("existing.wav"), b"original").unwrap();
    let error = demucs.separate(executable.as_os_str(), &job).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(output.join("existing.wav")).unwrap(), b"original");
}
