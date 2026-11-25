use std::path::PathBuf;

use rhythm_hits::DetectorRegistry;
use serde::Serialize;

use crate::metrics::greedy_match;
use crate::rpp::{CompedDrumSession, parse_comped_rpp, render_comped_kit};
use crate::{BenchMode, Cli, run_rust_detector};

#[derive(Default, Serialize)]
struct Scores {
    reference: usize,
    predicted: usize,
    matches_50ms: usize,
    matches_70ms: usize,
    f1_50ms: f64,
    f1_70ms: f64,
}

impl Scores {
    fn add(
        &mut self,
        reference: usize,
        predicted: usize,
        matches_50ms: usize,
        matches_70ms: usize,
    ) {
        self.reference += reference;
        self.predicted += predicted;
        self.matches_50ms += matches_50ms;
        self.matches_70ms += matches_70ms;
        let count = (self.reference + self.predicted).max(1) as f64;
        self.f1_50ms = 2.0 * self.matches_50ms as f64 / count;
        self.f1_70ms = 2.0 * self.matches_70ms as f64 / count;
    }

    fn score(reference: &[f64], predicted: &[f64]) -> Self {
        let mut scores = Self::default();
        scores.add(
            reference.len(),
            predicted.len(),
            greedy_match(reference, predicted, 0.050).len(),
            greedy_match(reference, predicted, 0.070).len(),
        );
        scores
    }

    fn accumulate(&mut self, song: &Self) {
        self.add(
            song.reference,
            song.predicted,
            song.matches_50ms,
            song.matches_70ms,
        );
    }
}

#[derive(Serialize)]
struct Song {
    song: String,
    session_bpm: f64,
    median_predicted_bpm: Option<f64>,
    median_grid_bpm: Option<f64>,
    beats: Scores,
    downbeats: Scores,
    duration_secs: f64,
    elapsed_secs: f64,
}

fn median_bpm(beats: &[f64]) -> Option<f64> {
    let mut bpms: Vec<_> = beats
        .windows(2)
        .filter_map(|w| {
            let interval = w[1] - w[0];
            (interval > 0.0).then(|| 60.0 / interval)
        })
        .collect();
    bpms.sort_by(f64::total_cmp);
    bpms.get(bpms.len() / 2).copied()
}

pub(crate) fn run(cli: &Cli, registry: &DetectorRegistry, paths: &[PathBuf]) -> Result<(), String> {
    if !registry
        .get(&cli.detector)
        .is_some_and(|d| d.info().supports_beats)
    {
        return Err(format!("{} does not provide beat events", cli.detector));
    }
    let mut songs = Vec::new();
    let mut beats = Scores::default();
    let mut downbeats = Scores::default();
    for path in paths {
        let session = parse_comped_rpp(path)?;
        let kit = render_comped_kit(&session)?;
        let (grid_beats, grid_downbeats) = read_grid(&session, kit.duration_secs)?;
        let (output, elapsed_secs) =
            run_rust_detector(registry, &cli.detector, BenchMode::BusOnly, &kit, cli)?;
        let predicted: Vec<_> = output.beats.iter().map(|b| b.at).collect();
        let predicted_downbeats: Vec<_> = output
            .beats
            .iter()
            .filter(|b| b.downbeat)
            .map(|b| b.at)
            .collect();
        let song = Song {
            song: kit.song_name,
            session_bpm: kit.bpm,
            median_predicted_bpm: median_bpm(&predicted),
            median_grid_bpm: median_bpm(&grid_beats),
            beats: Scores::score(&grid_beats, &predicted),
            downbeats: Scores::score(&grid_downbeats, &predicted_downbeats),
            duration_secs: kit.duration_secs,
            elapsed_secs,
        };
        println!(
            "{} | beat F1@70ms {:.3} | downbeat F1@70ms {:.3}",
            song.song, song.beats.f1_70ms, song.downbeats.f1_70ms
        );
        beats.accumulate(&song.beats);
        downbeats.accumulate(&song.downbeats);
        songs.push(song);
    }
    let duration: f64 = songs.iter().map(|s| s.duration_secs).sum();
    let elapsed: f64 = songs.iter().map(|s| s.elapsed_secs).sum();
    let report = serde_json::json!({
        "engine": cli.detector,
        "mode": "bus-only",
        "reference": "REAPER project beat grid",
        "song_count": songs.len(),
        "cpu_rtf": elapsed / duration,
        "beats": beats,
        "downbeats": downbeats,
        "songs": songs,
    });
    if let Some(path) = &cli.out {
        let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| format!("write {}: {e}", path.display()))?;
    }
    Ok(())
}

fn read_grid(session: &CompedDrumSession, duration: f64) -> Result<(Vec<f64>, Vec<f64>), String> {
    let text = std::fs::read_to_string(&session.rpp_path).map_err(|e| e.to_string())?;
    let mut markers = vec![(0.0, session.bpm, Some(session.time_sig))];
    if let Some((_, envelope)) = text.split_once("\n  <TEMPOENVEX\n") {
        for line in envelope.lines().take_while(|line| *line != "  >") {
            let Some(point) = line.trim().strip_prefix("PT ") else {
                continue;
            };
            let mut fields = point.split_whitespace();
            let time: f64 = fields
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or("invalid tempo marker time")?;
            let bpm: f64 = fields
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or("invalid tempo marker BPM")?;
            if !time.is_finite() || time < 0.0 || !bpm.is_finite() || bpm <= 0.0 {
                return Err("invalid tempo marker".into());
            }
            if fields.next() != Some("1") {
                return Err("beat-grid scoring requires step tempo markers".into());
            }
            let meter = fields
                .next()
                .and_then(|s| s.parse::<u32>().ok())
                .filter(|&s| s != 0)
                .map(|s| (s & 0xffff, s >> 16));
            markers.push((time, bpm, meter));
        }
    }

    let mut beats = Vec::new();
    let mut downbeats = Vec::new();
    let mut at: f64 = 0.0;
    let mut beat_in_bar = 0;
    let mut meter = session.time_sig;
    for (index, &(start, bpm, new_meter)) in markers.iter().enumerate() {
        if let Some(next_meter) = new_meter {
            meter = next_meter;
            beat_in_bar = 0;
            at = start;
        }
        if meter.0 == 0 || meter.1 == 0 || !bpm.is_finite() || bpm <= 0.0 {
            return Err("invalid project tempo or time signature".into());
        }
        let end = markers
            .get(index + 1)
            .map_or(duration, |marker| marker.0.min(duration));
        let interval = 60.0 / bpm * 4.0 / f64::from(meter.1);
        while at < end - 1e-8 {
            beats.push(at);
            if beat_in_bar == 0 {
                downbeats.push(at);
            }
            beat_in_bar = (beat_in_bar + 1) % meter.0;
            at += interval;
        }
        if at > end && end < duration {
            // Carry the remaining fractional beat into the next tempo segment.
            let next_interval = 60.0 / markers[index + 1].1 * 4.0 / f64::from(meter.1);
            at = end + (at - end) / interval * next_interval;
        }
    }
    Ok((beats, downbeats))
}
