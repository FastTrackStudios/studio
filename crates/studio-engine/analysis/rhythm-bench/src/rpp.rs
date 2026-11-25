#![allow(dead_code)]
//! Minimal REAPER `.comped.RPP` parser & active-comp-lane drum kit renderer.
//!
//! Reads REAPER Fixed Item Lanes (`LANESOLO` + item `YPOS`) so only the active
//! comped performance is rendered for each kit mic, applying `POSITION`,
//! `LENGTH`, `SOFFS`, `VOLPAN`, and linear `FADEIN`/`FADEOUT` crossfades.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct RppItem {
    pub position: f64,
    pub length: f64,
    pub soffs: f64,
    pub muted: bool,
    pub lane: usize,
    pub fade_in: f64,
    pub fade_out: f64,
    pub vol: f64,
    pub file: PathBuf,
}

#[derive(Clone, Debug)]
pub struct RppTrack {
    pub project_index: usize,
    pub name: String,
    pub folder_path: String,
    pub lane_solo: Vec<u32>,
    pub items: Vec<RppItem>,
}

#[derive(Clone, Debug)]
pub struct CompedDrumSession {
    pub song_name: String,
    pub rpp_path: PathBuf,
    pub bpm: f64,
    pub time_sig: (u32, u32),
    pub tracks: HashMap<String, RppTrack>,
}

#[derive(Clone, Debug)]
pub struct RenderedKit {
    pub song_name: String,
    pub bpm: f64,
    pub time_sig: (u32, u32),
    pub sample_rate: f64,
    pub duration_secs: f64,
    /// Mono 48 kHz (or session SR) active-comp audio keyed by canonical track key:
    /// `kick_in`, `kick_out`, `kick_trig`, `snare_top`, `snare_bottom`, `snare_trig`,
    /// `t1`..`t4`, `t1_trig`..`t4_trig`, `ohs`, `hihat`, `ride`, `rooms`, `drum_bus`.
    pub tracks: HashMap<String, Vec<f32>>,
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1].to_owned()
    } else {
        t.to_owned()
    }
}

fn lane_is_active(lane_solo: &[u32], lane: usize) -> bool {
    let any_solo = lane_solo.iter().any(|&w| w != 0);
    if !any_solo {
        return true;
    }
    let word = lane / 32;
    let bit = lane % 32;
    lane_solo
        .get(word)
        .is_some_and(|&mask| ((mask >> bit) & 1) != 0)
}

fn classify_drum_track(folder_stack: &[String], name: &str) -> Option<&'static str> {
    let under_live_drums = folder_stack.iter().any(|f| f == "Drums");
    if !under_live_drums {
        return None;
    }
    let parent = folder_stack.last().map(String::as_str).unwrap_or("");
    match (parent, name) {
        ("Kick", "In") => Some("kick_in"),
        ("Kick", "Out") => Some("kick_out"),
        ("Kick", "Trig") => Some("kick_trig"),
        ("Snare", "Top") => Some("snare_top"),
        ("Snare", "Bottom") => Some("snare_bottom"),
        ("Snare", "Trig") => Some("snare_trig"),
        ("Toms", "T1") => Some("t1"),
        ("Toms", "T2") => Some("t2"),
        ("Toms", "T3") => Some("t3"),
        ("Toms", "T4") => Some("t4"),
        ("Trig", "T1 Trig") | ("Toms", "T1 Trig") => Some("t1_trig"),
        ("Trig", "T2 Trig") | ("Toms", "T2 Trig") => Some("t2_trig"),
        ("Trig", "T3 Trig") | ("Toms", "T3 Trig") => Some("t3_trig"),
        ("Trig", "T4 Trig") | ("Toms", "T4 Trig") => Some("t4_trig"),
        ("Cymbals", "OHs") | ("Drums", "OHs") => Some("ohs"),
        ("Cymbals", "Hi Hat") | ("Drums", "Hi Hat") => Some("hihat"),
        ("Cymbals", "Ride") | ("Drums", "Ride") => Some("ride"),
        ("Drums", "Rooms") | ("Cymbals", "Rooms") => Some("rooms"),
        _ => None,
    }
}

pub fn parse_comped_rpp(rpp_path: &Path) -> Result<CompedDrumSession, String> {
    let text = std::fs::read_to_string(rpp_path)
        .map_err(|e| format!("read {}: {e}", rpp_path.display()))?;
    let base_dir = rpp_path.parent().unwrap_or_else(|| Path::new("."));
    let song_name = rpp_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .trim_end_matches(".comped.RPP")
        .trim_end_matches(".RPP")
        .to_owned();

    let mut bpm = 120.0_f64;
    let mut time_sig = (4_u32, 4_u32);
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("TEMPO ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 3 {
                if let Ok(b) = parts[0].parse::<f64>() {
                    bpm = b;
                }
                if let (Ok(n), Ok(d)) = (parts[1].parse::<u32>(), parts[2].parse::<u32>()) {
                    time_sig = (n, d);
                }
            }
            break;
        }
    }

    let mut folder_stack: Vec<String> = Vec::new();
    let mut tracks: HashMap<String, RppTrack> = HashMap::new();

    for (project_index, raw_track) in text.split("  <TRACK ").skip(1).enumerate() {
        let header = raw_track.split("    <ITEM").next().unwrap_or(raw_track);
        let mut name = String::new();
        let mut isbus = (0_i32, 0_i32);
        let mut lane_solo: Vec<u32> = Vec::new();

        for line in header.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("NAME ") {
                name = unquote(rest);
            } else if let Some(rest) = t.strip_prefix("ISBUS ") {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.len() >= 2 {
                    isbus = (
                        parts[0].parse().unwrap_or(0),
                        parts[1].parse().unwrap_or(0),
                    );
                }
            } else if let Some(rest) = t.strip_prefix("LANESOLO ") {
                lane_solo = rest
                    .split_whitespace()
                    .filter_map(|s| s.parse::<u32>().ok())
                    .collect();
            }
        }

        if let Some(key) = classify_drum_track(&folder_stack, &name) {
            let mut items = Vec::new();
            for raw_item in raw_track.split("    <ITEM").skip(1) {
                let mut pos = 0.0_f64;
                let mut len = 0.0_f64;
                let mut soffs = 0.0_f64;
                let mut muted = false;
                let mut lane = 0_usize;
                let mut fade_in = 0.0_f64;
                let mut fade_out = 0.0_f64;
                let mut vol = 1.0_f64;
                let mut file_rel: Option<String> = None;

                for line in raw_item.lines() {
                    let t = line.trim();
                    if t == ">" && file_rel.is_some() {
                        // End of first SOURCE block; don't overwrite with secondary takes
                        break;
                    }
                    if let Some(rest) = t.strip_prefix("POSITION ") {
                        pos = rest.trim().parse().unwrap_or(0.0);
                    } else if let Some(rest) = t.strip_prefix("LENGTH ") {
                        len = rest.trim().parse().unwrap_or(0.0);
                    } else if let Some(rest) = t.strip_prefix("SOFFS ") {
                        soffs = rest.trim().parse().unwrap_or(0.0);
                    } else if let Some(rest) = t.strip_prefix("MUTE ") {
                        muted = rest.split_whitespace().next().unwrap_or("0") != "0";
                    } else if let Some(rest) = t.strip_prefix("YPOS ") {
                        let parts: Vec<&str> = rest.split_whitespace().collect();
                        if parts.len() >= 2 {
                            let top: f64 = parts[0].parse().unwrap_or(0.0);
                            let h: f64 = parts[1].parse().unwrap_or(1.0);
                            if h > 1e-6 {
                                lane = (top / h).round().max(0.0) as usize;
                            }
                        }
                    } else if let Some(rest) = t.strip_prefix("FADEIN ") {
                        let parts: Vec<&str> = rest.split_whitespace().collect();
                        if parts.len() >= 2 {
                            fade_in = parts[1].parse().unwrap_or(0.0);
                        }
                    } else if let Some(rest) = t.strip_prefix("FADEOUT ") {
                        let parts: Vec<&str> = rest.split_whitespace().collect();
                        if parts.len() >= 2 {
                            fade_out = parts[1].parse().unwrap_or(0.0);
                        }
                    } else if let Some(rest) = t.strip_prefix("VOLPAN ") {
                        let parts: Vec<&str> = rest.split_whitespace().collect();
                        if let Some(v) = parts.first() {
                            vol = v.parse().unwrap_or(1.0);
                        }
                    } else if let Some(rest) = t.strip_prefix("FILE ")
                        && file_rel.is_none()
                    {
                        file_rel = Some(unquote(rest));
                    }
                }

                if !muted && len > 0.0 && lane_is_active(&lane_solo, lane)
                    && let Some(rel) = file_rel
                {
                    let p = PathBuf::from(&rel);
                    let full = if p.is_absolute() { p } else { base_dir.join(p) };
                    items.push(RppItem {
                        position: pos,
                        length: len,
                        soffs,
                        muted,
                        lane,
                        fade_in,
                        fade_out,
                        vol,
                        file: full,
                    });
                }
            }
            if !items.is_empty() {
                tracks.insert(
                    key.to_owned(),
                    RppTrack {
                        project_index,
                        name: name.clone(),
                        folder_path: folder_stack.join("/"),
                        lane_solo,
                        items,
                    },
                );
            }
        }

        // Update REAPER folder nesting stack AFTER classifying this track.
        if isbus.0 == 1 && isbus.1 > 0 {
            folder_stack.push(name);
        } else if isbus.0 == 2 && isbus.1 < 0 {
            let pops = (-isbus.1) as usize;
            for _ in 0..pops {
                folder_stack.pop();
            }
        }
    }

    Ok(CompedDrumSession {
        song_name,
        rpp_path: rpp_path.to_path_buf(),
        bpm,
        time_sig,
        tracks,
    })
}

/// Read a contiguous mono slice `[start_frame, start_frame + frame_count)` from a WAV file.
fn read_wav_slice_mono(
    path: &Path,
    start_frame: usize,
    frame_count: usize,
) -> Result<(Vec<f32>, f64), String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let spec = reader.spec();
    let sr = f64::from(spec.sample_rate);
    let ch = spec.channels.max(1) as usize;
    let total_frames = (reader.len() as usize) / ch;
    if start_frame >= total_frames {
        return Ok((Vec::new(), sr));
    }
    let avail = (total_frames - start_frame).min(frame_count);
    reader
        .seek(start_frame as u32)
        .map_err(|e| format!("seek {}: {e}", path.display()))?;

    let mut out = Vec::with_capacity(avail);
    let inv_ch = 1.0 / (ch as f32);

    match spec.sample_format {
        hound::SampleFormat::Float => {
            let mut iter = reader.samples::<f32>();
            for _ in 0..avail {
                let mut sum = 0.0_f32;
                for _ in 0..ch {
                    let s = iter
                        .next()
                        .transpose()
                        .map_err(|e| format!("read {}: {e}", path.display()))?
                        .unwrap_or(0.0);
                    sum += s;
                }
                out.push(sum * inv_ch);
            }
        }
        hound::SampleFormat::Int => {
            let scale = (1_i64 << (spec.bits_per_sample.saturating_sub(1))) as f32;
            let inv = inv_ch / scale;
            let mut iter = reader.samples::<i32>();
            for _ in 0..avail {
                let mut sum = 0.0_f32;
                for _ in 0..ch {
                    let s = iter
                        .next()
                        .transpose()
                        .map_err(|e| format!("read {}: {e}", path.display()))?
                        .unwrap_or(0);
                    sum += s as f32;
                }
                out.push(sum * inv);
            }
        }
    }

    Ok((out, sr))
}

/// Render one active-comp track without collapsing stereo sources.
pub fn render_track_channels(
    track: &RppTrack,
    rate: u32,
    frames: usize,
) -> Result<(Vec<f32>, usize), String> {
    let open = |path: &Path| {
        hound::WavReader::open(path).map_err(|e| format!("open {}: {e}", path.display()))
    };
    let first = track.items.first().ok_or("empty comp track")?;
    let channels = usize::from(open(&first.file)?.spec().channels);
    if channels == 0 {
        return Err("comp source has no channels".into());
    }
    let mut audio = vec![0.0_f32; frames * channels];
    let samples = |s: f64| (s * f64::from(rate)).round().max(0.0) as usize;
    for item in &track.items {
        let mut reader = open(&item.file)?;
        let spec = reader.spec();
        if spec.sample_rate != rate || usize::from(spec.channels) != channels {
            return Err(format!("inconsistent source format in {}", item.file.display()));
        }
        let start = samples(item.position).min(frames);
        let count = samples(item.length).min(frames - start);
        let offset = samples(item.soffs);
        let available = (reader.duration() as usize).saturating_sub(offset).min(count);
        if available == 0 {
            continue;
        }
        reader.seek(offset as u32).map_err(|e| e.to_string())?;
        let scale = (1_u64 << spec.bits_per_sample.saturating_sub(1)) as f32;
        let input: Box<dyn Iterator<Item = Result<f32, hound::Error>>> = match spec.sample_format {
            hound::SampleFormat::Float => Box::new(reader.samples::<f32>()),
            hound::SampleFormat::Int => Box::new(reader.samples::<i32>().map(move |s| s.map(|x| x as f32 / scale))),
        };
        let fade_in = samples(item.fade_in);
        let fade_out = samples(item.fade_out);
        for (index, value) in input.take(available * channels).enumerate() {
            let frame = index / channels;
            let fade = |distance: usize, length: usize| (distance as f32 / length.max(1) as f32).min(1.0);
            let gain_in = if fade_in == 0 { 1.0 } else { fade(frame, fade_in) };
            let gain_out = if fade_out == 0 { 1.0 } else { fade(available - 1 - frame, fade_out) };
            audio[start * channels + index] += value.map_err(|e| e.to_string())?
                * item.vol as f32 * gain_in * gain_out;
        }
    }
    Ok((audio, channels))
}

pub fn render_comped_kit(session: &CompedDrumSession) -> Result<RenderedKit, String> {
    let mut max_end = 0.0_f64;
    let mut sample_rate = 48_000.0_f64;
    let mut probed_sr = false;

    for track in session.tracks.values() {
        for item in &track.items {
            max_end = max_end.max(item.position + item.length);
            if !probed_sr
                && let Ok(r) = hound::WavReader::open(&item.file)
            {
                sample_rate = f64::from(r.spec().sample_rate);
                probed_sr = true;
            }
        }
    }

    let total_samples = (max_end * sample_rate).ceil() as usize;
    let mut rendered: HashMap<String, Vec<f32>> = HashMap::new();

    for (key, track) in &session.tracks {
        let mut buf = vec![0.0_f32; total_samples];
        for item in &track.items {
            let dst_start = (item.position * sample_rate).round().max(0.0) as usize;
            let len_samples = (item.length * sample_rate).round().max(0.0) as usize;
            let src_start = (item.soffs * sample_rate).round().max(0.0) as usize;
            if len_samples == 0 || dst_start >= total_samples {
                continue;
            }
            let (slice, _sr) = read_wav_slice_mono(&item.file, src_start, len_samples)?;
            let copy_len = slice.len().min(total_samples - dst_start);
            let fi = (item.fade_in * sample_rate).round().max(0.0) as usize;
            let fo = (item.fade_out * sample_rate).round().max(0.0) as usize;
            let vol = item.vol as f32;

            for i in 0..copy_len {
                let mut g = vol;
                if fi > 0 && i < fi {
                    g *= (i as f32) / (fi as f32);
                }
                let from_end = copy_len - 1 - i;
                if fo > 0 && from_end < fo {
                    g *= (from_end as f32) / (fo as f32);
                }
                buf[dst_start + i] += slice[i] * g;
            }
        }
        rendered.insert(key.clone(), buf);
    }

    // Build a balanced mono acoustic drum_bus (excluding trigger tracks) for bus-only models.
    let bus_keys = [
        ("kick_in", 0.9_f32),
        ("kick_out", 0.5),
        ("snare_top", 0.9),
        ("snare_bottom", 0.4),
        ("t1", 0.6),
        ("t2", 0.6),
        ("t3", 0.6),
        ("t4", 0.6),
        ("ohs", 0.8),
        ("hihat", 0.5),
        ("ride", 0.5),
        ("rooms", 0.5),
    ];
    let mut bus = vec![0.0_f32; total_samples];
    let mut weight_sum = 0.0_f32;
    for (k, w) in bus_keys {
        if let Some(src) = rendered.get(k) {
            weight_sum += w;
            for (dst, &s) in bus.iter_mut().zip(src.iter()) {
                *dst += s * w;
            }
        }
    }
    if weight_sum > 0.0 {
        let peak = bus.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        let norm = if peak > 0.95 { 0.95 / peak } else { 1.0 / weight_sum.max(1.0) };
        for s in &mut bus {
            *s *= norm;
        }
    }
    rendered.insert("drum_bus".to_owned(), bus);

    Ok(RenderedKit {
        song_name: session.song_name.clone(),
        bpm: session.bpm,
        time_sig: session.time_sig,
        sample_rate,
        duration_secs: (total_samples as f64) / sample_rate,
        tracks: rendered,
    })
}
