//! Reading and writing what the engine needs of an `.rpp`: tracks and
//! their mixer state, audio items and their sources, tempo, markers and
//! regions.
//!
//! Reading is an import (a REAPER project, or one an Ableton set was
//! converted to); writing is how the engine hands what it plays to the
//! `daw` engine's loader, which reads REAPER text. Anything else in the
//! file (FX, envelopes, routing) is not read here.

use std::fmt::Write as _;

/// A project, as far as the engine reads one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Project {
    pub bpm: f64,
    pub meter: (u32, u32),
    /// `(seconds, bpm, meter)`.
    pub tempo: Vec<(f64, f64, Option<(u32, u32)>)>,
    pub markers: Vec<Mark>,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mark {
    pub id: u32,
    pub at: f64,
    /// Some for a region.
    pub end: Option<f64>,
    pub name: String,
    pub color: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub name: String,
    pub color: Option<u32>,
    /// Linear.
    pub volume: f64,
    pub pan: f64,
    pub mute: bool,
    pub solo: bool,
    /// Folder depth, 0 at the top.
    pub depth: u32,
    pub items: Vec<Clip>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Clip {
    pub position: f64,
    pub length: f64,
    pub name: String,
    pub offset: f64,
    /// Linear.
    pub gain: f64,
    pub mute: bool,
    pub color: Option<u32>,
    pub fade_in: f64,
    pub fade_out: f64,
    /// The source's file, as the project names it.
    pub file: Option<String>,
}

/// A line's words: quoted strings as one word, without their quotes.
fn words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = line.trim().chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let mut word = String::new();
        if matches!(c, '"' | '\'' | '`') {
            chars.next();
            for d in chars.by_ref() {
                if d == c {
                    break;
                }
                word.push(d);
            }
        } else {
            while let Some(&d) = chars.peek() {
                if d.is_whitespace() {
                    break;
                }
                word.push(d);
                chars.next();
            }
        }
        out.push(word);
    }
    out
}

fn num(w: &[String], at: usize) -> Option<f64> {
    w.get(at)?.parse().ok()
}

/// REAPER's colour: a COLORREF (red low) with 0x1000000 set when custom.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a 32-bit colour"
)]
fn color(v: f64) -> Option<u32> {
    let v = v as i64 as u32;
    (v & 0x0100_0000 != 0).then(|| {
        let (r, g, b) = (v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff);
        (r << 16) | (g << 8) | b
    })
}

fn reaper_color(rgb: u32) -> u32 {
    let (r, g, b) = ((rgb >> 16) & 0xff, (rgb >> 8) & 0xff, rgb & 0xff);
    0x0100_0000 | (b << 16) | (g << 8) | r
}

/// Reads `text`.
///
/// # Errors
///
/// Not a REAPER project.
#[expect(clippy::too_many_lines, reason = "one pass over the file's chunks")]
pub fn read(text: &str) -> Result<Project, String> {
    if !text.trim_start().starts_with("<REAPER_PROJECT") {
        return Err("not a REAPER project".to_owned());
    }
    let mut p = Project {
        bpm: 120.0,
        meter: (4, 4),
        ..Project::default()
    };
    // The chunks open around the line being read.
    let mut stack: Vec<String> = Vec::new();
    let mut depth: i64 = 0;
    let mut regions: std::collections::BTreeMap<u32, Mark> = std::collections::BTreeMap::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix('<') {
            let name = rest.split_whitespace().next().unwrap_or("").to_owned();
            let w = words(rest);
            match (stack.last().map(String::as_str), name.as_str()) {
                (Some("REAPER_PROJECT"), "TRACK") => p.tracks.push(Track {
                    volume: 1.0,
                    depth: u32::try_from(depth.max(0)).unwrap_or(0),
                    ..Track::default()
                }),
                (Some("TRACK"), "ITEM") => {
                    if let Some(track) = p.tracks.last_mut() {
                        track.items.push(Clip {
                            gain: 1.0,
                            ..Clip::default()
                        });
                    }
                }
                _ => {}
            }
            let _ = w;
            stack.push(name);
            continue;
        }
        if t == ">" {
            stack.pop();
            continue;
        }
        let w = words(t);
        let Some(key) = w.first().map(String::as_str) else {
            continue;
        };
        match stack.last().map(String::as_str) {
            Some("REAPER_PROJECT") => match key {
                "TEMPO" => {
                    p.bpm = num(&w, 1).unwrap_or(120.0);
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a meter"
                    )]
                    {
                        p.meter = (
                            num(&w, 2).unwrap_or(4.0) as u32,
                            num(&w, 3).unwrap_or(4.0) as u32,
                        );
                    }
                }
                "MARKER" => {
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "an index"
                    )]
                    let id = num(&w, 1).unwrap_or(0.0) as u32;
                    let at = num(&w, 2).unwrap_or(0.0);
                    let name = w.get(3).cloned().unwrap_or_default();
                    #[expect(clippy::cast_possible_truncation, reason = "flags")]
                    let flags = num(&w, 4).unwrap_or(0.0) as i64;
                    let c = num(&w, 5).and_then(color);
                    if flags & 1 == 1 {
                        // A region is two lines: its start, then its end.
                        if let Some(open) = regions.get_mut(&id).filter(|r| r.end.is_none()) {
                            open.end = Some(at);
                        } else {
                            regions.insert(
                                id,
                                Mark {
                                    id,
                                    at,
                                    end: None,
                                    name,
                                    color: c,
                                },
                            );
                        }
                    } else {
                        p.markers.push(Mark {
                            id,
                            at,
                            end: None,
                            name,
                            color: c,
                        });
                    }
                }
                _ => {}
            },
            Some("TEMPOENVEX") if key == "PT" => {
                let (Some(s), Some(bpm)) = (num(&w, 1), num(&w, 2)) else {
                    continue;
                };
                // A meter change rides the point's fourth field: num + 65536 · den.
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a packed meter"
                )]
                let meter = num(&w, 4).filter(|v| *v > 0.0).map(|v| {
                    let v = v as u32;
                    (v & 0xffff, v >> 16)
                });
                p.tempo.push((s, bpm, meter));
            }
            Some("TRACK") => {
                let Some(track) = p.tracks.last_mut() else {
                    continue;
                };
                match key {
                    "NAME" => track.name = w.get(1).cloned().unwrap_or_default(),
                    "PEAKCOL" => track.color = num(&w, 1).and_then(color),
                    "VOLPAN" => {
                        track.volume = num(&w, 1).unwrap_or(1.0);
                        track.pan = num(&w, 2).unwrap_or(0.0);
                    }
                    "MUTESOLO" => {
                        track.mute = num(&w, 1).is_some_and(|v| v != 0.0);
                        track.solo = num(&w, 2).is_some_and(|v| v != 0.0);
                    }
                    // A folder opens below this track, or closes after it.
                    "ISBUS" => {
                        #[expect(clippy::cast_possible_truncation, reason = "a depth change")]
                        let (kind, change) = (
                            num(&w, 1).unwrap_or(0.0) as i64,
                            num(&w, 2).unwrap_or(0.0) as i64,
                        );
                        if kind == 1 || kind == 2 {
                            depth += change;
                        }
                    }
                    _ => {}
                }
            }
            Some("ITEM") => {
                let Some(clip) = p.tracks.last_mut().and_then(|t| t.items.last_mut()) else {
                    continue;
                };
                match key {
                    "POSITION" => clip.position = num(&w, 1).unwrap_or(0.0),
                    "LENGTH" => clip.length = num(&w, 1).unwrap_or(0.0),
                    // The first take's name (a later TAKE's comes after it).
                    "NAME" if clip.name.is_empty() => {
                        clip.name = w.get(1).cloned().unwrap_or_default();
                    }
                    "SOFFS" => clip.offset = num(&w, 1).unwrap_or(0.0),
                    "VOLPAN" => clip.gain = num(&w, 1).unwrap_or(1.0),
                    "MUTE" => clip.mute = num(&w, 1).is_some_and(|v| v != 0.0),
                    "COLOR" => clip.color = num(&w, 1).and_then(color),
                    "FADEIN" => clip.fade_in = num(&w, 2).unwrap_or(0.0),
                    "FADEOUT" => clip.fade_out = num(&w, 2).unwrap_or(0.0),
                    _ => {}
                }
            }
            Some("SOURCE") if key == "FILE" => {
                if let Some(clip) = p.tracks.last_mut().and_then(|t| t.items.last_mut())
                    && clip.file.is_none()
                {
                    clip.file = w.get(1).cloned();
                }
            }
            _ => {}
        }
    }
    p.markers.extend(regions.into_values());
    p.markers.sort_by(|a, b| a.at.total_cmp(&b.at));
    Ok(p)
}

/// `p` as REAPER text the engine's loader reads. Sources are written as
/// given (the caller resolves them to absolute paths first).
#[must_use]
pub fn write(p: &Project) -> String {
    let mut s = String::from("<REAPER_PROJECT 0.1 \"7.0/fts\" 0\n");
    let _ = writeln!(s, "  TEMPO {} {} {} 0", p.bpm, p.meter.0, p.meter.1);
    if !p.tempo.is_empty() {
        s.push_str("  <TEMPOENVEX\n    ACT 1 -1\n");
        for (at, bpm, meter) in &p.tempo {
            match meter {
                Some((n, d)) => {
                    let _ = writeln!(s, "    PT {at} {bpm} 1 {}", n + (d << 16));
                }
                None => {
                    let _ = writeln!(s, "    PT {at} {bpm} 1");
                }
            }
        }
        s.push_str("  >\n");
    }
    for m in &p.markers {
        let c = m.color.map_or(0, reaper_color);
        let flag = u32::from(m.end.is_some());
        let _ = writeln!(s, "  MARKER {} {} {:?} {flag} {c}", m.id, m.at, m.name);
        if let Some(end) = m.end {
            let _ = writeln!(s, "  MARKER {} {end} \"\" 1", m.id);
        }
    }
    let mut depth = 0_i64;
    for (i, t) in p.tracks.iter().enumerate() {
        s.push_str("  <TRACK\n");
        let _ = writeln!(s, "    NAME {:?}", t.name);
        if let Some(c) = t.color {
            let _ = writeln!(s, "    PEAKCOL {}", reaper_color(c));
        }
        let _ = writeln!(s, "    VOLPAN {} {} -1 -1 1", t.volume, t.pan);
        let _ = writeln!(
            s,
            "    MUTESOLO {} {} 0",
            u8::from(t.mute),
            u8::from(t.solo)
        );
        // Folders from the depths: this track's change to the next's.
        let next = p.tracks.get(i + 1).map_or(0, |n| i64::from(n.depth));
        let change = next - i64::from(t.depth);
        if change > 0 {
            let _ = writeln!(s, "    ISBUS 1 {change}");
        } else if change < 0 {
            let _ = writeln!(s, "    ISBUS 2 {change}");
        }
        depth += change;
        s.push_str("    NCHAN 2\n");
        for c in &t.items {
            s.push_str("    <ITEM\n");
            let _ = writeln!(s, "      POSITION {}", c.position);
            let _ = writeln!(s, "      LENGTH {}", c.length);
            let _ = writeln!(s, "      MUTE {} 0", u8::from(c.mute));
            let _ = writeln!(s, "      FADEIN 1 {} 0", c.fade_in);
            let _ = writeln!(s, "      FADEOUT 1 {} 0", c.fade_out);
            let _ = writeln!(s, "      VOLPAN {} 0 1 -1", c.gain);
            let _ = writeln!(s, "      NAME {:?}", c.name);
            let _ = writeln!(s, "      SOFFS {}", c.offset);
            if let Some(file) = &c.file {
                let _ = writeln!(s, "      <SOURCE WAVE\n        FILE {file:?}\n      >");
            }
            s.push_str("    >\n");
        }
        s.push_str("  >\n");
    }
    let _ = depth;
    s.push_str(">\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<REAPER_PROJECT 0.1 "7.0/x64" 0
  TEMPO 160 4 4 0
  <TEMPOENVEX
    PT 0.0 160.0 1
    PT 10.5 120.0 1 196611
  >
  MARKER 4 123 "Drive To LA" 0 0 0 B "" 0
  MARKER 9 1 "Verse" 1 16777471
  MARKER 9 9 "" 1
  <TRACK
    NAME "Drums"
    PEAKCOL 16777471
    VOLPAN 0.5 -0.25 -1 -1 1
    MUTESOLO 1 0 0
    ISBUS 1 1
    <ITEM
      POSITION 2
      LENGTH 4.5
      NAME "Kick take"
      SOFFS 9
      <SOURCE WAVE
        FILE "/Users/jt/Project/Samples/Kick.wav"
      >
    >
  >
  <TRACK
    NAME Kick
    ISBUS 2 -1
  >
>
"#;

    #[test]
    fn a_project_reads_its_tracks_items_tempo_and_marks() {
        let p = read(SAMPLE).unwrap();
        assert_eq!((p.bpm, p.meter), (160.0, (4, 4)));
        assert_eq!(p.tempo[1], (10.5, 120.0, Some((3, 3))));
        assert_eq!(p.tracks.len(), 2);
        let d = &p.tracks[0];
        assert_eq!(
            (d.name.as_str(), d.volume, d.pan, d.mute, d.depth),
            ("Drums", 0.5, -0.25, true, 0)
        );
        assert_eq!(d.color, Some(0xff_00_00));
        assert_eq!(p.tracks[1].depth, 1);
        let c = &d.items[0];
        assert_eq!((c.position, c.length, c.offset), (2.0, 4.5, 9.0));
        assert_eq!(c.name, "Kick take");
        assert_eq!(
            c.file.as_deref(),
            Some("/Users/jt/Project/Samples/Kick.wav")
        );
        let region = p.markers.iter().find(|m| m.end.is_some()).unwrap();
        assert_eq!(
            (region.at, region.end, region.name.as_str()),
            (1.0, Some(9.0), "Verse")
        );
    }

    #[test]
    fn what_it_writes_reads_back_the_same() {
        let p = read(SAMPLE).unwrap();
        let again = read(&write(&p)).unwrap();
        assert_eq!(again, p);
    }
}
