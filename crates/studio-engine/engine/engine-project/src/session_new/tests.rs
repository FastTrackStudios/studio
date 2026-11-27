use super::*;

fn arg() -> NewArg {
    NewArg {
        title: None,
        key: None,
        tempo: None,
        form: None,
        from: None,
        to: None,
        template: None,
    }
}

/// A clean folder of the test's own.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("session-new-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A mono 16-bit WAV of `seconds` of silence at 48 kHz.
fn wav(path: &Path, seconds: f64) {
    let rate = 48_000u32;
    let frames = (seconds * f64::from(rate)) as u32;
    let data = frames * 2;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    b.resize(b.len() + data as usize, 0);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(path, b).unwrap();
}

#[test]
fn a_section_is_its_kind_and_bars() {
    assert_eq!(
        section("Verse 8"),
        Ok(Section {
            kind: "Verse".into(),
            bars: 8
        })
    );
    assert_eq!(
        section(" Pre-Chorus 2 "),
        Ok(Section {
            kind: "Pre-Chorus".into(),
            bars: 2
        })
    );
}

#[test]
fn a_section_without_bars_is_its_kinds_length() {
    assert_eq!(section("Intro").map(|s| s.bars), Ok(4));
    assert_eq!(section("outro").map(|s| s.bars), Ok(4));
    assert_eq!(section("Chorus").map(|s| s.bars), Ok(8));
}

#[test]
fn a_section_of_no_bars_is_refused() {
    assert!(section("Verse 0").is_err());
    assert!(section("  ").is_err());
}

#[test]
fn keys_are_named_as_musicians_write_them() {
    let named = |t: &str| key_named(t).map(|k| engine_domain::key::format_key(&k));
    assert_eq!(named("Eb"), Ok("Eb major".into()));
    assert_eq!(named("C#m"), Ok("C# minor".into()));
    assert_eq!(named("d minor"), Ok("D minor".into()));
    assert_eq!(named("G mixolydian"), Ok("G mixolydian".into()));
    assert!(named("H").is_err());
    assert!(named("Eb sideways").is_err());
}

#[test]
fn from_scratch_takes_what_it_is_given() {
    let start = start_of(NewArg {
        title: Some(" Morning Song ".into()),
        key: Some("Eb".into()),
        tempo: Some(96.0),
        form: Some(vec!["Intro".into(), "Verse 8".into()]),
        to: Some("/tmp/Morning Song.session".into()),
        ..arg()
    });
    assert_eq!(
        start,
        Ok(Start::Scratch(Scratch {
            title: Some("Morning Song".into()),
            key: Some("Eb".into()),
            bpm: Some(96.0),
            form: vec![
                Section {
                    kind: "Intro".into(),
                    bars: 4
                },
                Section {
                    kind: "Verse".into(),
                    bars: 8
                },
            ],
            to: Some(PathBuf::from("/tmp/Morning Song.session")),
        }))
    );
}

#[test]
fn nothing_asked_is_an_untitled_song() {
    let start = start_of(arg()).unwrap();
    assert_eq!(start, Start::Scratch(Scratch::default()));
    assert_eq!(start.title(), "Untitled");
}

#[test]
fn what_is_not_a_song_is_refused() {
    assert!(
        start_of(NewArg {
            tempo: Some(5.0),
            ..arg()
        })
        .is_err()
    );
    assert!(
        start_of(NewArg {
            key: Some("Q".into()),
            ..arg()
        })
        .is_err()
    );
    assert!(
        start_of(NewArg {
            to: Some("/tmp/x.RPP".into()),
            ..arg()
        })
        .is_err()
    );
    assert!(
        start_of(NewArg {
            template: Some("Band".into()),
            ..arg()
        })
        .is_err()
    );
}

#[test]
fn a_folder_is_named_for_itself_at_120_unless_told() {
    let dir = scratch_dir("named").join("Night Drive");
    std::fs::create_dir_all(&dir).unwrap();
    let start = start_of(NewArg {
        from: Some(dir.display().to_string()),
        ..arg()
    });
    assert_eq!(
        start,
        Ok(Start::Folder {
            dir: dir.clone(),
            title: "Night Drive".into(),
            bpm: 120.0,
            key: None,
        })
    );
    assert!(
        start_of(NewArg {
            from: Some(dir.display().to_string()),
            form: Some(vec!["Verse".into()]),
            ..arg()
        })
        .is_err(),
        "a folder has the form it has"
    );
}

#[test]
fn a_folder_already_made_into_a_session_is_refused() {
    let dir = scratch_dir("made");
    std::fs::write(dir.join("Song.session"), b"").unwrap();
    let refused = start_of(NewArg {
        from: Some(dir.display().to_string()),
        title: Some("Song".into()),
        ..arg()
    });
    assert!(refused.is_err_and(|e| e.contains("Song.session")));
}

#[test]
fn a_reaper_project_is_taken_as_it_is_once() {
    let dir = scratch_dir("rpp");
    let rpp = dir.join("Waltz.RPP");
    std::fs::write(&rpp, "<REAPER_PROJECT\n>\n").unwrap();
    let from = || Some(rpp.display().to_string());
    assert_eq!(
        start_of(NewArg {
            from: from(),
            ..arg()
        }),
        Ok(Start::Rpp(rpp.clone()))
    );
    assert!(
        start_of(NewArg {
            from: from(),
            tempo: Some(90.0),
            ..arg()
        })
        .is_err(),
        "a project keeps its own tempo"
    );
    std::fs::write(dir.join("Waltz.session"), b"").unwrap();
    assert!(
        start_of(NewArg {
            from: from(),
            ..arg()
        })
        .is_err_and(|e| e.contains("Waltz.session"))
    );
}

#[test]
fn a_file_that_is_not_a_project_is_refused() {
    let dir = scratch_dir("notrpp");
    let file = dir.join("notes.txt");
    std::fs::write(&file, "hi").unwrap();
    assert!(
        start_of(NewArg {
            from: Some(file.display().to_string()),
            ..arg()
        })
        .is_err()
    );
}

#[test]
fn every_audio_file_in_a_folder_is_a_stem() {
    let dir = scratch_dir("stems");
    wav(&dir.join("Bass.wav"), 2.0);
    wav(&dir.join("Drums/Kick.wav"), 1.5);
    std::fs::write(dir.join("notes.txt"), "not audio").unwrap();
    wav(&dir.join(".hidden/Ghost.wav"), 1.0);
    let found = stems(&dir).unwrap();
    assert_eq!(
        found,
        vec![
            Stem {
                file: PathBuf::from("Bass.wav"),
                seconds: 2.0
            },
            Stem {
                file: PathBuf::from("Drums/Kick.wav"),
                seconds: 1.5
            },
        ]
    );
}

#[test]
fn a_folder_with_no_audio_is_refused() {
    let dir = scratch_dir("empty");
    std::fs::write(dir.join("notes.txt"), "not audio").unwrap();
    assert!(stems(&dir).is_err());
}

#[test]
fn the_stems_project_has_a_track_per_stem_playing_it() {
    let text = stems_project(
        &[
            Stem {
                file: PathBuf::from("Bass.wav"),
                seconds: 2.0,
            },
            Stem {
                file: PathBuf::from("Drums/Kick.flac"),
                seconds: 1.5,
            },
        ],
        96.0,
    );
    let project = dawfile_reaper::parse_project_text(&text).unwrap();
    let tracks: Vec<(String, usize, f64)> = project
        .tracks
        .iter()
        .map(|t| {
            (
                t.name.clone(),
                t.items.len(),
                t.items.first().map_or(0.0, |i| i.length),
            )
        })
        .collect();
    assert_eq!(
        tracks,
        vec![("Bass".into(), 1, 2.0), ("Kick".into(), 1, 1.5)]
    );
    assert!(text.contains("Drums/Kick.flac"), "{text}");
    assert!(text.contains("TEMPO 96 4 4"), "{text}");
}

#[test]
fn a_song_from_scratch_is_not_saved_over_another() {
    let dir = scratch_dir("over");
    let to = dir.join("Song.session");
    std::fs::write(&to, b"").unwrap();
    let refused = start_of(NewArg {
        to: Some(to.display().to_string()),
        ..arg()
    });
    assert!(refused.is_err_and(|e| e.contains("Song.session")));
}

#[test]
fn a_title_with_a_slash_is_refused() {
    let refused = start_of(NewArg {
        title: Some("AC/DC".into()),
        ..arg()
    });
    assert!(refused.is_err_and(|e| e.contains("AC/DC")));
}

#[cfg(unix)]
#[test]
fn a_link_to_a_folder_is_not_followed() {
    let dir = scratch_dir("link");
    wav(&dir.join("Bass.wav"), 1.0);
    std::os::unix::fs::symlink(&dir, dir.join("loop")).unwrap();
    let found = stems(&dir).unwrap();
    assert_eq!(
        found,
        vec![Stem {
            file: PathBuf::from("Bass.wav"),
            seconds: 1.0
        }]
    );
}

#[test]
fn an_ogg_stem_is_a_vorbis_source() {
    let text = stems_project(
        &[Stem {
            file: PathBuf::from("Pad.ogg"),
            seconds: 1.0,
        }],
        120.0,
    );
    assert!(text.contains("<SOURCE VORBIS"), "{text}");
}

#[test]
fn a_session_is_named_for_its_file() {
    let refused = start_of(NewArg {
        title: Some("Morning Song".into()),
        to: Some("/tmp/morning.session".into()),
        ..arg()
    });
    assert!(refused.is_err_and(|e| e.contains("Morning Song.session")));
    let named = start_of(NewArg {
        to: Some("/tmp/Evening Song.session".into()),
        ..arg()
    });
    assert!(
        matches!(&named, Ok(Start::Scratch(s)) if s.title.as_deref() == Some("Evening Song")),
        "{named:?}"
    );
}
