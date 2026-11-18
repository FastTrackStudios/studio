//! What the arrangement draws: rows of clips, markers, regions and the
//! tempo, built from the session (`fts.session`, every lane and item,
//! whoever owns it) and, when an engine is there, the DAW's own data
//! (`fts.daw`: names, colours, mixer state, peaks).

use std::collections::HashMap;
use std::sync::Arc;

use daw_view::grid::{Change, Tempo};
use daw_view::{ITEM_KIND, TRACK_KIND};

/// A row of the arrangement: a lane.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Row {
    pub lane: String,
    pub name: String,
    pub color: Option<u32>,
    pub depth: u32,
    pub mute: bool,
    pub solo: bool,
    /// The lane's kind (`daw.track`, `ignition.cues`, …).
    pub kind: String,
    pub clips: Vec<ClipView>,
}

/// Something on a row, in seconds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClipView {
    pub id: String,
    pub start: f64,
    pub end: f64,
    pub name: String,
    pub color: Option<u32>,
    pub mute: bool,
    /// Into its source, seconds.
    pub offset: f64,
    /// Its source's peaks, by hash.
    pub peaks: Option<String>,
}

/// A source's waveform.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Wave {
    /// Pairs a second.
    pub per_second: f64,
    pub pairs: Vec<(f32, f32)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mark {
    pub at: f64,
    /// A region's end.
    pub end: Option<f64>,
    pub name: String,
    pub color: Option<u32>,
}

/// Everything the view draws, as of the latest read.
#[derive(Debug, Clone, Default)]
pub struct Model {
    /// Moves on with every new read that changed something.
    pub revision: u64,
    pub session: Option<String>,
    pub name: String,
    pub rows: Vec<Row>,
    pub marks: Vec<Mark>,
    pub tempo: Tempo,
    /// Where the last thing ends, seconds.
    pub duration: f64,
    pub waves: HashMap<String, Arc<Wave>>,
    /// Whether an engine (`fts.daw`) answered.
    pub engine: bool,
    pub problem: Option<String>,
    /// A synthetic model (`ARRANGEMENT_FIXTURE`): no reads replace it.
    pub fixture: bool,
}

impl Model {
    /// How many clips it has.
    #[must_use]
    pub fn clip_count(&self) -> usize {
        self.rows.iter().map(|r| r.clips.len()).sum()
    }

    fn finish(&mut self) {
        self.duration = self
            .rows
            .iter()
            .flat_map(|r| r.clips.iter().map(|c| c.end))
            .chain(self.marks.iter().map(|m| m.end.unwrap_or(m.at)))
            .fold(0.0, f64::max);
    }
}

/// A session's timeline and structure (`fts.session` `ReadTimeline`,
/// `ReadStructure`), drawn generically: a row per lane, a block per item.
#[must_use]
pub fn generic(
    timeline: &session::service::Response,
    structure: &session::service::Response,
) -> Model {
    let mut m = Model::default();
    if let session::service::Response::Structure {
        tempo,
        markers,
        regions,
    } = structure
    {
        m.tempo = Tempo {
            bpm: tempo.default_bpm,
            meter: (tempo.numerator, tempo.denominator),
            changes: tempo
                .points
                .iter()
                .map(|p| Change {
                    seconds: p.seconds,
                    bpm: p.bpm,
                    meter: p.meter.map(|m| (m.numerator, m.denominator)),
                })
                .collect(),
        };
        let seconds = |t: &session::Time| match t {
            session::Time::Seconds(s) => *s,
            // A musical time at the starting tempo: right for a song
            // that holds it, which the ruler's own grid says otherwise.
            session::Time::QuarterNotes(q) => q * 60.0 / tempo.default_bpm.max(1.0),
        };
        for mk in markers.values() {
            m.marks.push(Mark {
                at: seconds(&mk.at),
                end: None,
                name: mk.name.clone(),
                color: mk.color,
            });
        }
        for r in regions.values() {
            m.marks.push(Mark {
                at: seconds(&r.start),
                end: Some(seconds(&r.end)),
                name: r.name.clone(),
                color: r.color,
            });
        }
        m.marks.sort_by(|a, b| a.at.total_cmp(&b.at));
    }
    if let session::service::Response::Timeline { lanes, items } = timeline {
        let bpm = m.tempo.bpm.max(1.0);
        let seconds = |t: &session::Time| match t {
            session::Time::Seconds(s) => *s,
            session::Time::QuarterNotes(q) => q * 60.0 / bpm,
        };
        let mut order: Vec<_> = lanes.iter().collect();
        order.sort_by(|a, b| a.1.order.total_cmp(&b.1.order));
        let mut rows: Vec<Row> = order
            .iter()
            .map(|(id, lane)| Row {
                lane: id.to_string(),
                name: lane.name.clone(),
                kind: lane.kind.clone(),
                ..Row::default()
            })
            .collect();
        let at: HashMap<String, usize> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| (r.lane.clone(), i))
            .collect();
        for (id, item) in items {
            let Some(&i) = at.get(&item.lane.to_string()) else {
                continue;
            };
            let start = seconds(&item.at);
            let end = item.end.as_ref().map_or(start, seconds);
            if let Some(row) = rows.get_mut(i) {
                row.clips.push(ClipView {
                    id: id.to_string(),
                    start,
                    end,
                    ..ClipView::default()
                });
            }
        }
        for r in &mut rows {
            r.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
        }
        m.rows = rows;
    }
    m.finish();
    m
}

/// `m` with the DAW's own data: its tracks in the engine's order (folders
/// nested), named and coloured, its clips with names and peaks. Lanes
/// that are not the DAW's stay, after its tracks.
pub fn with_daw(m: &mut Model, tracks: &[daw_view::Track], clips: &[daw_view::Clip]) {
    let mut by_lane: HashMap<&str, Vec<ClipView>> = HashMap::new();
    for c in clips {
        by_lane.entry(c.lane.as_str()).or_default().push(ClipView {
            id: c.id.clone(),
            start: c.start,
            end: c.end,
            name: c.name.clone(),
            color: c.color,
            mute: c.mute,
            offset: c.offset,
            peaks: c.peaks.clone(),
        });
    }
    let mut rows: Vec<Row> = tracks
        .iter()
        .map(|t| Row {
            lane: t.lane.clone(),
            name: t.name.clone(),
            color: t.color,
            depth: t.depth,
            mute: t.mute,
            solo: t.solo,
            kind: TRACK_KIND.to_owned(),
            clips: by_lane.remove(t.lane.as_str()).unwrap_or_default(),
        })
        .collect();
    let daw: std::collections::HashSet<&str> = tracks.iter().map(|t| t.lane.as_str()).collect();
    rows.extend(
        m.rows
            .iter()
            .filter(|r| !daw.contains(r.lane.as_str()) && r.kind != TRACK_KIND)
            .cloned(),
    );
    let _ = ITEM_KIND;
    m.rows = rows;
    m.engine = true;
    m.finish();
}

/// A synthetic arrangement of `tracks` rows of `clips` each, with
/// waveforms: what a measurement draws when it wants a size no song has.
#[must_use]
pub fn fixture(tracks: u32, clips: u32) -> Model {
    let mut m = Model {
        name: format!("fixture {tracks}×{clips}"),
        fixture: true,
        engine: false,
        tempo: Tempo {
            bpm: 120.0,
            ..Tempo::default()
        },
        ..Model::default()
    };
    // One wave shared by every clip of a track: a source used many times.
    for t in 0..tracks {
        let hash = format!("fixture-{}", t % 16);
        m.waves.entry(hash.clone()).or_insert_with(|| {
            let pairs = (0..20_000_u32)
                .map(|i| {
                    let x = f64::from(i) / 40.0 + f64::from(t);
                    #[expect(clippy::cast_possible_truncation, reason = "a sample")]
                    let a = ((x * 0.7).sin() * (x * 0.13).cos()).abs() as f32;
                    (-a, a * 0.9)
                })
                .collect();
            Arc::new(Wave {
                per_second: 93.75,
                pairs,
            })
        });
        let row = Row {
            lane: format!("fixture-{t}"),
            name: format!("Track {}", t + 1),
            color: Some(0x40_80_c0 ^ (t * 0x10_20_30 & 0xff_ff_ff)),
            depth: u32::from(t % 8 != 0),
            kind: TRACK_KIND.to_owned(),
            clips: (0..clips)
                .map(|c| {
                    let start = f64::from(c) * 4.0 + f64::from(t % 3) * 0.5;
                    ClipView {
                        id: format!("{t}-{c}"),
                        start,
                        end: start + 3.5,
                        name: format!("Take {}", c + 1),
                        color: None,
                        mute: false,
                        offset: f64::from(c % 40) * 4.0,
                        peaks: Some(hash.clone()),
                    }
                })
                .collect(),
            ..Row::default()
        };
        m.rows.push(row);
    }
    for b in 0..(clips / 8) {
        m.marks.push(Mark {
            at: f64::from(b) * 32.0,
            end: Some(f64::from(b) * 32.0 + 16.0),
            name: format!("Section {}", b + 1),
            color: Some(0x60_a0_60),
        });
    }
    m.finish();
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixture_has_the_size_it_was_asked() {
        let m = fixture(300, 67);
        assert_eq!(m.rows.len(), 300);
        assert_eq!(m.clip_count(), 20_100);
        assert!(m.duration > 260.0);
    }
}
