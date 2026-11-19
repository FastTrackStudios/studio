//! A REAPER project's tempo map, read from its file: the reference maps
//! the owner accepted are `TEMPOENVEX` points in `.RPP` files.

use std::path::Path;

use crate::map::{TempoMap, TempoPoint};
use crate::meter::Meter;

/// What a project file says about time.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Project {
    /// The tempo envelope's points, in project seconds.
    pub points: Vec<TempoPoint>,
    /// Where the audio sits in the project: the position most of its
    /// audio items start at, seconds. A map's times less this are times of the recording.
    pub offset: f64,
}

/// Reads the tempo envelope and the audio items' position of the project
/// in `text`.
#[must_use]
pub fn parse(text: &str) -> Project {
    let mut project = Project::default();
    let mut in_tempo = false;
    // Each item's position, and whether it plays a file (the click's item
    // is generated and sits at 0 whatever the audio does).
    let mut items: Vec<(f64, bool)> = Vec::new();
    let mut in_item = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("<TEMPOENVEX") {
            in_tempo = true;
            continue;
        }
        if line.starts_with("<ITEM") {
            in_item = true;
            items.push((0.0, false));
            continue;
        }
        if in_tempo {
            if line == ">" {
                in_tempo = false;
            } else if let Some(rest) = line.strip_prefix("PT ") {
                let mut fields = rest.split_whitespace();
                let time = fields.next().and_then(|f| f.parse::<f64>().ok());
                let bpm = fields.next().and_then(|f| f.parse::<f64>().ok());
                let _shape = fields.next();
                // The meter, when the point sets one: the numerator in the
                // low 16 bits, the denominator above.
                let meter = fields
                    .next()
                    .and_then(|f| f.parse::<u32>().ok())
                    .filter(|v| *v != 0)
                    .map(|v| Meter::new(v & 0xffff, v >> 16))
                    .filter(|m| m.numerator > 0 && m.denominator > 0);
                if let (Some(time), Some(bpm)) = (time, bpm) {
                    project.points.push(TempoPoint { time, bpm, meter });
                }
            }
            continue;
        }
        if in_item {
            if let Some(item) = items.last_mut() {
                if let Some(rest) = line.strip_prefix("POSITION ") {
                    item.0 = rest.trim().parse::<f64>().unwrap_or(0.0);
                } else if line.starts_with("FILE ") {
                    item.1 = true;
                }
            }
        }
    }
    // Where most of the audio items start.
    let mut best: Option<(usize, f64)> = None;
    for (position, _) in items.iter().filter(|(_, file)| *file) {
        let count = items
            .iter()
            .filter(|(p, file)| *file && (p - position).abs() < 1e-6)
            .count();
        if best.is_none_or(|(c, _)| count > c) {
            best = Some((count, *position));
        }
    }
    let offset = best.map(|(_, position)| position);
    project.offset = offset.unwrap_or(0.0);
    project
}

/// The map a project built by the song scripts holds, in the recording's
/// time: its first point is the count-in, each point after it a bar, the
/// last where the song ends.
///
/// # Errors
///
/// A file that cannot be read, or one with fewer than three tempo points.
pub fn reference(path: &Path) -> Result<TempoMap, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let project = parse(&text);
    if project.points.len() < 3 {
        return Err(format!(
            "{}: {} tempo points, not a map",
            path.display(),
            project.points.len()
        ));
    }
    let end = project.points.last().map_or(0.0, |p| p.time);
    Ok(TempoMap::from_points(&project.points, end, 1).shifted(-project.offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RPP: &str = r#"<REAPER_PROJECT 0.1 "7.0"
  TEMPO 120 4 4 0
  <TEMPOENVEX
    ACT 1 -1
    PT 0.000000000000 120.0000000000 1 262148 0 1 0 "" 0 169 0 ABBB
    PT 2.000000000000 120.0000000000 1
    PT 4.000000000000 100.0000000000 1
    PT 6.400000000000 100.0000000000 1 262147
    PT 8.200000000000 100.0000000000 1
  >
  <TRACK
    <ITEM
      POSITION 0
      <SOURCE CLICK
      >
    >
  >
  <TRACK
    <ITEM
      POSITION 0.5
      LENGTH 10
      <SOURCE WAVE
        FILE "mix.wav"
      >
    >
    <ITEM
      POSITION 0.5
      <SOURCE WAVE
        FILE "kick.wav"
      >
    >
    <ITEM
      POSITION 3
      <SOURCE WAVE
        FILE "late.wav"
      >
    >
  >
>"#;

    #[test]
    fn the_envelope_and_the_audio_s_place_are_read() {
        let project = parse(RPP);
        assert_eq!(project.points.len(), 5);
        assert_eq!(project.points[0].meter, Some(Meter::COMMON));
        assert_eq!(project.points[3].meter, Some(Meter::new(3, 4)));
        assert!((project.offset - 0.5).abs() < 1e-12);
    }

    #[test]
    fn a_built_project_s_points_are_its_bars() {
        let project = parse(RPP);
        let map = TempoMap::from_points(&project.points, 8.2, 1).shifted(-project.offset);
        let times: Vec<f64> = map.bars.iter().map(|b| b.time).collect();
        assert_eq!(times.len(), 3);
        for (got, want) in times.iter().zip([1.5, 3.5, 5.9]) {
            assert!((got - want).abs() < 1e-9, "{got} for {want}");
        }
        assert_eq!(map.bars[2].meter, Meter::new(3, 4));
        assert!((map.end - 7.7).abs() < 1e-9);
    }
}
