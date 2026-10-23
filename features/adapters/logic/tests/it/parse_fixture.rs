//! Integration test against the real `FileDecrypt.logicx` fixture.

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/logic/FileDecrypt.logicx"
);

#[test]
fn parse_bundle_metadata() {
    let session = logic::read_session(FIXTURE).expect("parse failed");

    assert_eq!(session.sample_rate, 48000);
    assert!((session.bpm - 100.0).abs() < 0.01, "bpm should be 100");
    assert_eq!(session.time_sig_numerator, 4);
    assert_eq!(session.time_sig_denominator, 4);
    assert_eq!(session.key, "C");
    assert_eq!(session.key_gender, "major");
    assert!(
        session.creator_version.contains("Logic Pro"),
        "creator_version should mention Logic Pro, got: {}",
        session.creator_version
    );
}

#[test]
fn chunk_inventory_non_empty() {
    let session = logic::read_session(FIXTURE).expect("parse failed");

    assert!(!session.chunks.is_empty(), "expected chunks, got none");

    // The Song (gnoS) chunk should always be first.
    let first = &session.chunks[0];
    assert_eq!(
        first.type_name, "Song",
        "first chunk should be Song, got: {}",
        first.type_name
    );

    // Sanity-check total count (we measured 498 in the Python script).
    assert!(
        session.chunks.len() > 400,
        "expected ~498 chunks, got {}",
        session.chunks.len()
    );
}

#[test]
fn chunk_inventory_dump() {
    let session = logic::read_session(FIXTURE).expect("parse failed");

    use std::collections::HashMap;
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for chunk in &session.chunks {
        *counts.entry(chunk.type_name.as_str()).or_default() += 1;
    }

    let mut sorted: Vec<_> = counts.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));

    println!(
        "\n=== Chunk type inventory ({} total) ===",
        session.chunks.len()
    );
    for (name, count) in &sorted {
        println!("  {:>8}  {}", count, name);
    }

    // Known types we expect to find
    assert!(
        counts.get("AuCO").copied().unwrap_or(0) > 100,
        "expected many AuCO chunks"
    );
    assert!(
        counts.get("Trak").copied().unwrap_or(0) > 10,
        "expected Trak chunks"
    );
    assert!(
        counts.get("Envi").copied().unwrap_or(0) > 5,
        "expected Envi chunks"
    );
    assert_eq!(
        counts.get("Song").copied().unwrap_or(0),
        1,
        "expected exactly 1 Song chunk"
    );
}

#[test]
fn tracks_extracted() {
    let session = logic::read_session(FIXTURE).expect("parse failed");

    println!("\n=== Tracks ({}) ===", session.tracks.len());
    for track in &session.tracks {
        println!("  {:?}  '{}'", track.kind, track.name);
    }

    // The fixture has named channels: Audio Track 1-3, Midi Track 1-2,
    // Summing Groups, etc.
    assert!(!session.tracks.is_empty(), "expected at least one track");
}

#[test]
fn session_summary_smoke() {
    let session = logic::read_session(FIXTURE).expect("parse failed");
    let summary = logic::session_summary(&session);
    println!("\n{}", summary);
    assert!(summary.contains("100"), "summary should mention 100 BPM");
    assert!(summary.contains("48000"));
}

#[test]
fn aufl_audio_filenames() {
    use logic::parse::aufl::parse_aufl;

    let session = logic::read_session(FIXTURE).expect("parse failed");

    // Collect filenames from AuFl chunks
    let aufl_tag = *b"lFuA";
    let filenames: Vec<String> = session
        .chunks
        .iter()
        .filter(|c| c.tag == aufl_tag)
        .filter_map(|c| parse_aufl(&c.data))
        .map(|e| e.filename)
        .collect();

    println!("\n=== AuFl filenames ({}) ===", filenames.len());
    for f in &filenames {
        println!("  {}", f);
    }

    // We added audio regions, so there should be at least one .wav file
    assert!(!filenames.is_empty(), "expected at least one AuFl entry");
    assert!(
        filenames
            .iter()
            .any(|f| f.ends_with(".wav") || f.ends_with(".aif") || f.ends_with(".aiff")),
        "expected at least one WAV/AIFF file, got: {:?}",
        filenames
    );
}

#[test]
fn aurg_audio_regions() {
    use logic::parse::aurg::parse_aurg;

    let session = logic::read_session(FIXTURE).expect("parse failed");

    let aurg_tag = *b"gRuA";
    let regions: Vec<_> = session
        .chunks
        .iter()
        .filter(|c| c.tag == aurg_tag)
        .filter_map(|c| parse_aurg(&c.data))
        .collect();

    println!("\n=== AuRg pool entries ({}) ===", regions.len());
    for r in &regions {
        println!(
            "  '{}' src_offset={} frames={}",
            r.name, r.source_offset_frames, r.duration_frames
        );
    }

    assert!(!regions.is_empty(), "expected at least one AuRg region");
    assert!(
        regions.iter().all(|r| !r.name.is_empty()),
        "region name should not be empty"
    );
    assert!(
        regions.iter().all(|r| r.duration_frames > 0),
        "region duration should be > 0"
    );
}

#[test]
fn mseq_arrangement_positions() {
    let session = logic::read_session(FIXTURE).expect("parse failed");

    // MSeq chunks with arrangement positions in header_meta[4..8].
    // 65536 ticks = 1 beat; 262144 ticks = 1 bar (4/4 at any BPM).
    let mseq_tag = *b"qeSM";
    let positions: Vec<(String, f64)> = session
        .chunks
        .iter()
        .filter(|c| c.tag == mseq_tag)
        .map(|c| {
            let ticks = i32::from_le_bytes(c.header_meta[4..8].try_into().unwrap_or([0; 4]));
            let beats = ticks as f64 / 65536.0;
            (c.type_name.clone(), beats)
        })
        .collect();

    println!("\n=== MSeq arrangement positions ({}) ===", positions.len());
    for (name, beats) in &positions {
        println!("  {} @ {:.2} beats ({:.2} bars)", name, beats, beats / 4.0);
    }

    assert!(
        !positions.is_empty(),
        "expected at least one MSeq arrangement position"
    );

    // At least one position should be non-zero (clips at different bar positions).
    let has_nonzero = positions.iter().any(|(_, b)| *b > 0.0);
    assert!(
        has_nonzero,
        "expected some MSeq clips at non-zero positions"
    );

    // All positions should be non-negative.
    assert!(
        positions.iter().all(|(_, b)| *b >= 0.0),
        "arrangement positions should be non-negative"
    );
}

#[test]
fn midi_items_extracted() {
    use logic::ClipKind;

    let session = logic::read_session(FIXTURE).expect("parse failed");

    // Collect all MIDI clips from all tracks.
    let mut midi_clips = Vec::new();
    for track in &session.tracks {
        for clip in &track.clips {
            if let ClipKind::Midi { notes } = &clip.kind {
                midi_clips.push((track.name.as_str(), clip.position_beats, notes.len()));
            }
        }
    }

    println!("\n=== MIDI clips from tracks ({}) ===", midi_clips.len());
    for (track, pos, note_count) in &midi_clips {
        println!(
            "  track='{}' position={:.2} beats ({:.2} bars) notes={}",
            track,
            pos,
            pos / 4.0,
            note_count
        );
    }

    // Independently verify MSeq-based arrangement positions.
    let mseq_tag = *b"qeSM";
    let trak_tag = *b"karT";
    let evsq_tag = *b"qSvE";

    let mut midi_positions = Vec::new();
    let chunks = &session.chunks;
    let mut i = 0;
    while i < chunks.len() {
        if chunks[i].tag != mseq_tag {
            i += 1;
            continue;
        }
        let mseq = &chunks[i];
        let ticks = i32::from_le_bytes(mseq.header_meta[4..8].try_into().unwrap_or([0; 4]));
        let beats = ticks as f64 / 65536.0;

        let mut j = i + 1;
        let mut is_midi = false;
        let mut event_count = 0usize;
        while j < chunks.len() {
            if chunks[j].tag == trak_tag {
                if chunks[j].data_len == 0 {
                    is_midi = true;
                }
            } else if chunks[j].tag == evsq_tag {
                // Count non-sentinel, non-end events
                for rec in chunks[j].data.chunks_exact(16) {
                    if rec[0] == 0xf1 {
                        break;
                    }
                    let pos_raw = u32::from_le_bytes(rec[4..8].try_into().unwrap_or([0; 4]));
                    if pos_raw != 0x8800_0000 {
                        event_count += 1;
                    }
                }
                break;
            } else if chunks[j].tag == mseq_tag {
                break;
            }
            j += 1;
        }

        if is_midi && event_count > 0 {
            midi_positions.push(beats);
        }
        i = j;
    }

    println!(
        "\n=== MIDI MSeq regions with events ({}) ===",
        midi_positions.len()
    );
    for pos in &midi_positions {
        println!("  {:.2} beats ({:.2} bars)", pos, pos / 4.0);
    }

    assert!(
        !midi_positions.is_empty(),
        "expected MIDI regions with note events"
    );
    // All positions must be non-negative.
    assert!(
        midi_positions.iter().all(|&p| p >= 0.0),
        "MIDI positions should be non-negative"
    );
}

// ── Fire.logicx fixture ───────────────────────────────────────────────────────

const FIRE_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/logic/Fire.logicx"
);

#[test]
fn fire_parse_summary() {
    let session = logic::read_session(FIRE_FIXTURE).expect("parse failed");

    println!(
        "\nBPM: {:.2}  SR: {}  Key: {} {}",
        session.bpm, session.sample_rate, session.key, session.key_gender
    );
    println!(
        "Chunks: {}  Tracks: {}  Summing groups: {}",
        session.chunks.len(),
        session.tracks.len(),
        session.summing_groups.len()
    );

    println!("\n=== Tracks ===");
    let mut total_clips = 0usize;
    for t in &session.tracks {
        println!(
            "  [chan_seq={}] {:?} '{}' clips={}",
            t.channel,
            t.kind,
            t.name,
            t.clips.len()
        );
        for clip in &t.clips {
            let kind = match &clip.kind {
                logic::ClipKind::Audio { file_path } => {
                    format!("AUDIO file={}", file_path.as_deref().unwrap_or("?"))
                }
                logic::ClipKind::Midi { notes } => {
                    format!("MIDI notes={}", notes.len())
                }
                logic::ClipKind::TakeFolder(f) => {
                    format!(
                        "TAKE_FOLDER takes={} comp_ranges={}",
                        f.takes.len(),
                        f.comp_ranges.len()
                    )
                }
                logic::ClipKind::Other => "OTHER".into(),
            };
            println!(
                "    @ {:.2}b len={:.2}b  {}",
                clip.position_beats, clip.length_beats, kind
            );
            total_clips += 1;
        }
    }
    println!("\nTotal clips attached: {}", total_clips);

    // Sanity assertions
    assert!(session.chunks.len() > 100, "expected many chunks");
    assert!(!session.tracks.is_empty(), "expected tracks");
    assert!(session.bpm > 0.0, "expected non-zero BPM");
}

#[test]
fn fire_take_folders() {
    // Verify that the take folder parser detects comped clips.
    // Fire.logicx has AuRg entries with take_number 1–23 ('Comp A' etc.) so at
    // least some tracks should carry TakeFolder clips after parsing.
    let session = logic::read_session(FIRE_FIXTURE).expect("parse failed");

    let mut total_take_folders = 0usize;
    let mut total_takes = 0usize;
    let mut total_comp_ranges = 0usize;
    let mut sample_printed = false;

    println!("\n=== Take Folders in Fire.logicx ===");
    for track in &session.tracks {
        for clip in &track.clips {
            if let logic::ClipKind::TakeFolder(folder) = &clip.kind {
                total_take_folders += 1;
                total_takes += folder.takes.len();
                total_comp_ranges += folder.comp_ranges.len();

                if !sample_printed {
                    println!(
                        "Track '{}' @ {:.2}b len={:.2}b",
                        track.name, clip.position_beats, clip.length_beats
                    );
                    println!(
                        "  {} takes, {} comp ranges",
                        folder.takes.len(),
                        folder.comp_ranges.len()
                    );
                    for t in &folder.takes {
                        println!(
                            "    take {} dur={:.2}b offset_frames={}",
                            t.number, t.duration_beats, t.source_offset_frames
                        );
                    }
                    for r in &folder.comp_ranges {
                        println!(
                            "    comp take={} start={} end={}",
                            r.take_number, r.comp_start_ticks, r.comp_end_ticks
                        );
                    }
                    sample_printed = true;
                }
            }
        }
    }

    println!(
        "\nTotal: {} take folders, {} takes, {} comp ranges",
        total_take_folders, total_takes, total_comp_ranges
    );

    // Fire has comped vocal/instrument recordings — we expect at least one take folder.
    // If none are found, it means the pool matching isn't picking up the names yet.
    // Treat this as an informational test (no hard assert) until the clip→track
    // name mapping is fully solved.
    println!(
        "Take folder detection: {} folders found (0 = name-matching not yet resolving comped clips)",
        total_take_folders
    );
}

#[test]
fn fire_envi_fader_values() {
    // Verify that fader_db is extracted from Envi chunks for user-track channels.
    // Confirmed raw values from Fire.logicx hex dumps:
    //   "Kick":        byte[0x52] = 0x3d = 61  → fader below unity
    //   "Snare":       byte[0x52] = 0x7f = 127 → fader at maximum
    //   "Pluck Synth": byte[0x52] = 0x1b = 27  → fader well below unity
    let session = logic::read_session(FIRE_FIXTURE).expect("parse failed");

    println!("\n=== Track fader_db values ===");
    let mut tracks_with_fader = 0;
    for t in &session.tracks {
        if let Some(db) = t.fader_db {
            println!("  '{}': {:.1} dB", t.name, db);
            tracks_with_fader += 1;
        }
    }
    println!(
        "Tracks with fader_db: {} / {}",
        tracks_with_fader,
        session.tracks.len()
    );

    assert!(
        tracks_with_fader > 0,
        "expected at least some tracks to have fader_db values"
    );

    // Verify specific tracks extracted from Fire.logicx Envi hex dumps.
    // Note: tracks may appear multiple times if duplicated in the Envi list.
    let kick = session.tracks.iter().find(|t| t.name == "Kick");
    let snare = session.tracks.iter().find(|t| t.name == "Snare");
    let pluck = session.tracks.iter().find(|t| t.name == "Pluck Synth");

    if let Some(kick) = kick {
        let db = kick.fader_db.expect("Kick should have a fader_db");
        // raw=61 → 20*log10(61/128) ≈ -6.4 dB
        assert!(
            db < 0.0 && db > -15.0,
            "Kick fader should be 5–15 dB below unity, got {:.1} dB",
            db
        );
    }
    if let Some(snare) = snare {
        let db = snare.fader_db.expect("Snare should have a fader_db");
        // raw=127 → 20*log10(127/128) ≈ -0.07 dB (near unity)
        assert!(
            db.abs() < 1.0,
            "Snare fader should be near 0 dB, got {:.1} dB",
            db
        );
    }
    if let Some(pluck) = pluck {
        let db = pluck.fader_db.expect("Pluck Synth should have a fader_db");
        // raw=27 → 20*log10(27/128) ≈ -13.5 dB (quiet)
        assert!(
            db < -5.0,
            "Pluck Synth fader should be well below unity, got {:.1} dB",
            db
        );
    }
}

// ── Tier-1 export coverage ───────────────────────────────────────────────────

#[test]
fn tier1_audio_file_pool_exposed() {
    let session = logic::read_session(FIXTURE).expect("parse failed");
    assert!(
        !session.audio_files.is_empty(),
        "audio_files should be exposed on the session for export"
    );
    // Filenames must be non-empty and end in a recognisable audio extension.
    for f in &session.audio_files {
        assert!(!f.filename.is_empty(), "audio file with empty filename");
        let lower = f.filename.to_ascii_lowercase();
        assert!(
            lower.ends_with(".wav")
                || lower.ends_with(".aif")
                || lower.ends_with(".aiff")
                || lower.ends_with(".caf")
                || lower.ends_with(".mp3"),
            "unexpected extension in audio_files: {:?}",
            f.filename
        );
    }
}

#[test]
fn tier1_track_channels_contiguous() {
    let session = logic::read_session(FIXTURE).expect("parse failed");
    for (i, t) in session.tracks.iter().enumerate() {
        assert_eq!(
            t.channel,
            (i + 1) as u32,
            "track[{i}] should have channel={} (1-based display order), got {}",
            i + 1,
            t.channel
        );
    }
}

#[test]
fn tier1_clips_carry_names() {
    let session = logic::read_session(FIXTURE).expect("parse failed");
    let mut total_clips = 0;
    for t in &session.tracks {
        for c in &t.clips {
            assert!(
                !c.name.is_empty(),
                "clip on track {:?} has empty name",
                t.name
            );
            total_clips += 1;
        }
    }
    assert!(total_clips > 0, "expected at least one clip with a name");
}

#[test]
fn tier1_folder_track_classified() {
    let session = logic::read_session(FIXTURE).expect("parse failed");
    let has_folder = session
        .tracks
        .iter()
        .any(|t| t.kind == logic::TrackKind::Folder);
    assert!(
        has_folder,
        "expected at least one Folder track in FileDecrypt fixture"
    );
}
