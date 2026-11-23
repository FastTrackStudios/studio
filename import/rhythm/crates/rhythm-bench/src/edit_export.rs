use std::{fmt::Write as _, path::Path};

use rhythm_editing::{
    HitMove, HybridPlan, HybridPolicy, RecordedHit, plan_hybrid, render::render_group,
};
use rhythm_hits::DetectorRegistry;

use crate::{
    BenchMode, Cli,
    rpp::{CompedDrumSession, parse_comped_rpp, render_comped_kit, render_track_channels},
    run_rust_detector,
};

const TRACKS: [(&str, &str); 18] = [
    ("kick_in", "Kick In"),
    ("kick_out", "Kick Out"),
    ("snare_top", "Snare Top"),
    ("snare_bottom", "Snare Bottom"),
    ("t1", "T1"),
    ("t2", "T2"),
    ("t3", "T3"),
    ("t4", "T4"),
    ("ohs", "Overheads"),
    ("hihat", "Hi Hat"),
    ("ride", "Ride"),
    ("rooms", "Rooms"),
    ("kick_trig", "Kick Trigger"),
    ("snare_trig", "Snare Trigger"),
    ("t1_trig", "T1 Trigger"),
    ("t2_trig", "T2 Trigger"),
    ("t3_trig", "T3 Trigger"),
    ("t4_trig", "T4 Trigger"),
];

pub(crate) fn run(path: &Path, out: &Path, cli: &Cli) -> Result<(), String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let session = parse_comped_rpp(&path)?;
    let project = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if project
        .split_once("\n  <TEMPOENVEX\n")
        .is_some_and(|(_, env)| {
            env.lines()
                .take_while(|line| *line != "  >")
                .any(|line| line.trim_start().starts_with("PT "))
        })
    {
        return Err("project export currently requires a constant tempo".into());
    }
    if !session.bpm.is_finite() || session.bpm <= 0.0 {
        return Err("invalid project tempo".into());
    }
    let kit = render_comped_kit(&session)?;
    let (detected, _) = run_rust_detector(
        &DetectorRegistry::default(),
        "fts.hits.kit-fusion",
        BenchMode::WithTriggers,
        &kit,
        cli,
    )?;
    let mut hits: Vec<_> = detected
        .hits
        .into_iter()
        .map(|hit| RecordedHit {
            id: hit.id,
            piece: hit.piece,
            at: hit.at,
            peak_db: hit.peak_db,
            ghost: hit.ghost,
        })
        .collect();
    hits.sort_by(|a, b| a.at.total_cmp(&b.at));
    let step = 60.0 / session.bpm / f64::from(cli.quantize_subdivision);
    let (moves, plan) = protected_plan(&hits, step, kit.duration_secs)?;
    let rate = kit.sample_rate as u32;
    let frames = (kit.duration_secs * kit.sample_rate).round() as usize;
    drop(kit);
    write_project(&session, out, &plan, rate, frames)?;
    write_full_copy(&project, &session, out, frames as f64 / f64::from(rate))?;
    let report = serde_json::json!({
        "source_project": path, "bpm": session.bpm, "subdivision": cli.quantize_subdivision,
        "hits": hits, "moves": moves, "plan": plan,
    });
    std::fs::write(
        out.join("edit-plan.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("Wrote {} moves to {}", moves.len(), out.display());
    Ok(())
}

#[derive(Clone)]
struct EditGroup {
    range: std::ops::Range<usize>,
    shift: f64,
}

fn grid_groups(hits: &[RecordedHit], step: f64, duration: f64) -> Vec<EditGroup> {
    let mut groups = Vec::new();
    let mut start = 0;
    while let Some(first) = hits.get(start) {
        let cell = (first.at / step).round();
        let len = hits[start..]
            .partition_point(|hit| (hit.at / step).round() == cell || hit.at - first.at <= 0.012);
        let end = start + len;
        let shift = (cell * step - first.at).clamp(-first.at, duration - hits[end - 1].at);
        groups.push(EditGroup {
            range: start..end,
            shift: if shift.abs() >= 0.001 { shift } else { 0.0 },
        });
        start = end;
    }
    groups
}

fn protected_plan(
    hits: &[RecordedHit],
    step: f64,
    duration: f64,
) -> Result<(Vec<HitMove>, HybridPlan), String> {
    let mut groups = grid_groups(hits, step, duration);
    loop {
        let moves: Vec<_> = groups
            .iter()
            .filter(|group| group.shift != 0.0)
            .flat_map(|group| {
                hits[group.range.clone()].iter().map(|hit| HitMove {
                    id: hit.id.clone(),
                    target_time: hit.at + group.shift,
                    mode: Default::default(),
                })
            })
            .collect();
        let plan = plan_hybrid(hits, &moves, (0.0, duration), HybridPolicy::default());
        let mut offending = vec![false; groups.len()];
        for (bad, group) in offending.iter_mut().zip(&groups) {
            *bad = hits[group.range.clone()].iter().any(|hit| {
                let want = hit.at + group.shift;
                let index = plan
                    .segments
                    .partition_point(|segment| segment.timeline_end <= want);
                !plan.segments.get(index).is_some_and(|segment| {
                    matches!(segment.kind, rhythm_editing::SegmentKind::Slip)
                        && segment.timeline_start <= want
                        && (segment.source_start + want - segment.timeline_start - hit.at).abs()
                            < 1e-6
                })
            });
        }
        if !offending.iter().any(|&bad| bad)
            && plan.audit.warp_segments == 0
            && plan.audit.max_stretch_ratio <= HybridPolicy::default().max_tail_stretch_ratio
        {
            return Ok((moves, plan));
        }
        // Merge edits across a seam that would cut or stretch an attack.
        // A shared offset leaves that part of the comp continuous.
        if groups.len() < 2 {
            return Err("could not build an attack-preserving quantization plan".into());
        }
        if !offending.iter().any(|&bad| bad)
            && let Some(warp) = plan
                .segments
                .iter()
                .find(|segment| matches!(segment.kind, rhythm_editing::SegmentKind::Warp { .. }) || matches!(segment.kind, rhythm_editing::SegmentKind::TailStretch { stretch_ratio } if stretch_ratio > HybridPolicy::default().max_tail_stretch_ratio))
        {
            let index =
                groups.partition_point(|group| hits[group.range.start].at <= warp.source_start);
            offending[index.saturating_sub(1)] = true;
        }
        if offending[groups.len() - 1] {
            offending[groups.len() - 2] = true;
        }
        for index in (0..groups.len() - 1).rev() {
            if offending[index] {
                let end = groups.remove(index + 1).range.end;
                groups[index].range.end = end;
                groups[index].shift = groups[index].shift.clamp(
                    -hits[groups[index].range.start].at,
                    duration - hits[end - 1].at - 1e-6,
                );
            }
        }
    }
}

fn write_project(
    session: &CompedDrumSession,
    out: &Path,
    plan: &HybridPlan,
    rate: u32,
    frames: usize,
) -> Result<(), String> {
    let mut layout = Vec::new();
    let mut channels = Vec::new();
    for (key, name) in TRACKS {
        if let Some(track) = session.tracks.get(key) {
            let (samples, count) = render_track_channels(track, rate, frames)?;
            layout.push((key, name, count));
            channels.push(samples);
        }
    }
    let width: usize = layout.iter().map(|(_, _, n)| n).sum();
    let mut source = Vec::with_capacity(frames * width);
    for frame in 0..frames {
        for ((_, _, count), track) in layout.iter().zip(&channels) {
            source.extend_from_slice(&track[frame * count..(frame + 1) * count]);
        }
    }
    drop(channels);
    let audio = render_group(plan, &source, width, rate)?;
    drop(source);
    std::fs::create_dir(out).map_err(|e| format!("create {}: {e}", out.display()))?;
    let (num, den) = session.time_sig;
    let mut rpp = format!(
        "<REAPER_PROJECT 0.1 \"7.0\" 0\n  TEMPO {} {num} {den} 0\n  SAMPLERATE {rate} 1 0\n",
        session.bpm
    );
    let length = frames as f64 / f64::from(rate);
    let mut offset = 0;
    for (key, name, count) in layout {
        let filename = format!("{key}.wav");
        let spec = hound::WavSpec {
            channels: count as u16,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut wav =
            hound::WavWriter::create(out.join(&filename), spec).map_err(|e| e.to_string())?;
        for frame in audio.chunks_exact(width) {
            for &sample in &frame[offset..offset + count] {
                wav.write_sample(sample).map_err(|e| e.to_string())?;
            }
        }
        wav.finalize().map_err(|e| e.to_string())?;
        offset += count;
        if key.ends_with("_trig") {
            continue;
        }
        writeln!(rpp, "  <TRACK\n    NAME \"{name}\"\n    <ITEM\n      POSITION 0\n      LENGTH {length:.12}\n      GROUPID 1\n      FADEIN 0 0 0 1 0 0\n      FADEOUT 0 0 0 1 0 0\n      NAME \"{name}\"\n      SOFFS 0\n      PLAYRATE 1 1 0 -1 0 0.0025\n      <SOURCE WAVE\n        FILE \"{filename}\"\n      >\n    >\n  >").unwrap();
    }
    rpp.push_str(">\n");
    std::fs::write(
        out.join(format!("{}.drums.quantized.RPP", session.song_name)),
        rpp,
    )
    .map_err(|e| e.to_string())
}

fn write_full_copy(
    project: &str,
    session: &CompedDrumSession,
    out: &Path,
    duration: f64,
) -> Result<(), String> {
    let folder = session
        .rpp_path
        .parent()
        .ok_or("missing source project folder")?;
    let mut chunks = project.split("  <TRACK ");
    let mut updated = String::with_capacity(project.len());
    relocate_media(chunks.next().unwrap_or_default(), folder, &mut updated);
    for (index, chunk) in chunks.enumerate() {
        updated.push_str("  <TRACK ");
        let key = TRACKS.iter().find_map(|(key, _)| {
            session
                .tracks
                .get(*key)
                .filter(|track| track.project_index == index)
                .map(|_| *key)
        });
        if let Some(key) = key {
            replace_track_items(chunk, key, duration, folder, &mut updated)?;
        } else {
            relocate_media(chunk, folder, &mut updated);
        }
    }
    std::fs::write(
        out.join(format!("{}.quantized.RPP", session.song_name)),
        updated,
    )
    .map_err(|e| e.to_string())
}

fn relocate_media(text: &str, folder: &Path, out: &mut String) {
    for line in text.split_inclusive('\n') {
        if let Some(value) = line.trim_start().strip_prefix("FILE \"")
            && let Some((reference, suffix)) = value.split_once('"')
            && Path::new(reference).is_relative()
        {
            let indent = &line[..line.len() - line.trim_start().len()];
            writeln!(
                out,
                "{indent}FILE \"{}\"{}",
                folder.join(reference).display(),
                suffix.trim_end_matches('\n')
            )
            .unwrap();
        } else {
            out.push_str(line);
        }
    }
}

fn replace_track_items(
    chunk: &str,
    key: &str,
    duration: f64,
    folder: &Path,
    out: &mut String,
) -> Result<(), String> {
    let mut in_item = false;
    let mut added = false;
    for line in chunk.split_inclusive('\n') {
        if in_item {
            if line.trim_end() == "    >" {
                in_item = false;
            }
            continue;
        }
        if line.starts_with("    <ITEM") {
            in_item = true;
            if !added {
                writeln!(out, "    <ITEM\n      POSITION 0\n      LENGTH {duration:.12}\n      GROUPID 2147000000\n      FADEIN 0 0 0 1 0 0\n      FADEOUT 0 0 0 1 0 0\n      NAME \"{key} quantized\"\n      SOFFS 0\n      PLAYRATE 1 1 0 -1 0 0.0025\n      <SOURCE WAVE\n        FILE \"{key}.wav\"\n      >\n    >").unwrap();
                added = true;
            }
        } else if ![
            "FIXEDLANES",
            "LANESOLO",
            "LANEREC",
            "LANENAME",
            "ITEMLANES",
            "LINKEDLANE",
        ]
        .iter()
        .any(|field| {
            line.strip_prefix("    ")
                .is_some_and(|s| s.starts_with(field))
        }) {
            relocate_media(line, folder, out);
        }
    }
    if in_item || !added {
        return Err(format!("could not replace the old items on {key}"));
    }
    Ok(())
}
