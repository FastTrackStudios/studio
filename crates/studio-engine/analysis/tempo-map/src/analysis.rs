//! A song's analysis: everything found in its stems, kept, and the
//! decisions made on it (bar 1, the meter, the hand fixes).
//!
//! It is plain data: saved as JSON beside nothing in particular, read back
//! and worked on without the audio (the map, the comparison, the hit
//! markers). Only what listens to the stems again (a re-snap, the
//! evaluation, the candidates) needs the [`Features`].

use std::collections::BTreeMap;

use crate::downbeat::{self, Candidate, MeterGuess};
use crate::eval::{self, Evaluation, Tracked};
use crate::features::{Features, KIT};
use crate::map::{MapOptions, TempoMap};
use crate::meter::{Layout, Meter, MeterChange};
use crate::onset::Hit;
use crate::phase::{self, Irregular, Repair};
use crate::track::{self, TrackParams};

/// The format of a saved analysis.
pub const VERSION: u32 = 1;

/// How a song is analysed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Options {
    pub track: TrackParams,
    /// The meter, when known; else what the beats say (three or four).
    pub meter: Option<Meter>,
    pub changes: Vec<MeterChange>,
    /// Leave the beats as tracked: no repair of added or dropped beats.
    pub no_repair: bool,
}

/// A song, analysed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Analysis {
    pub version: u32,
    /// The folder the stems came from.
    pub stems: String,
    /// The mix's length, seconds.
    pub duration: f64,
    pub params: TrackParams,
    /// The stems the beat envelope was summed from.
    pub sources: Vec<String>,
    /// The tempo the tracker held, bpm.
    pub tempo: f64,
    /// The beats, seconds, and what each sits on.
    pub beats: Vec<f64>,
    pub beat_sources: Vec<String>,
    /// The hits of each audible kit piece.
    pub hits: BTreeMap<String, Vec<Hit>>,
    /// The meter of every bar.
    pub layout: Layout,
    /// What the beats said the meter is.
    pub meter_guess: MeterGuess,
    /// The beat that is bar 1's downbeat, once chosen.
    pub bar1: Option<usize>,
    /// The candidates for it, best first.
    pub candidates: Vec<Candidate>,
    /// From this song bar on the downbeats are the beats as given.
    pub raw_from_bar: Option<u32>,
    /// The beats the repair re-spaced, and the changes of phase it left.
    pub repair: Repair,
    /// The stem the backbeat was read from.
    pub backbeat: Option<String>,
    /// What was changed by hand, in order.
    #[serde(default)]
    pub edits: Vec<String>,
}

/// A kit hit with where it falls on the map.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HitMark {
    pub time: f64,
    pub strength: f32,
    /// The song bar (0 and below: before bar 1).
    pub bar: i64,
    /// The beat in the bar, from 1, with its fraction: 1.0 is on the
    /// downbeat, 2.5 the and of two. A hit just ahead of a bar line is
    /// that bar's (0.98, not the bar before's 4.98), so the beat a hit is
    /// on is this rounded.
    pub beat: f64,
}

/// What a report of a map says.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Summary {
    pub beats: usize,
    pub on_hits: usize,
    pub bars: usize,
    pub meter: String,
    /// The slowest, the median and the fastest bar, bpm.
    pub tempo: (f64, f64, f64),
    pub bar1: usize,
    pub bar1_time: f64,
    /// `set` by hand, or the best `candidate`.
    pub bar1_by: String,
    pub phase_line: String,
    pub backbeat: Option<String>,
    pub irregular: Vec<Irregular>,
    pub repaired: usize,
    pub left: usize,
    pub edits: Vec<String>,
}

/// Analyses a song from its measured stems.
#[must_use]
pub fn analyse(features: &Features, options: &Options) -> Analysis {
    let pulse = track::pulse(features, &options.track);
    let hits = features.kit_hits();
    let (beats, sources) = track::snap(&pulse.beats, &hits, &options.track);
    let meter_guess = downbeat::guess_meter(features, &beats);
    let layout = Layout::new(
        options.meter.unwrap_or(meter_guess.meter),
        options.changes.clone(),
    );
    let (beats, sources, repair) = if options.no_repair {
        (beats, sources, Repair::default())
    } else {
        phase::repair(features, &hits, &beats, &sources, &layout, &options.track)
    };
    let candidates = downbeat::candidates(features, &beats, layout.meter);
    let backbeat = phase::backbeat_stem(features, &beats).map(|s| s.name.clone());
    Analysis {
        version: VERSION,
        stems: features.folder.display().to_string(),
        duration: features.duration,
        params: options.track,
        sources: pulse.sources,
        tempo: pulse.bpm,
        beats,
        beat_sources: sources,
        hits: KIT
            .iter()
            .filter_map(|name| features.stem(name))
            .filter(|s| s.audible() && !s.hits.is_empty())
            .map(|s| (s.name.clone(), s.hits.clone()))
            .collect(),
        layout,
        meter_guess,
        bar1: None,
        candidates,
        raw_from_bar: None,
        repair,
        backbeat,
        edits: Vec::new(),
    }
}

impl Analysis {
    /// The beat taken as bar 1's downbeat: the one set, else the best
    /// candidate, else the first.
    #[must_use]
    pub fn bar1_beat(&self) -> usize {
        self.bar1
            .or_else(|| self.candidates.first().map(|c| c.beat))
            .unwrap_or(0)
            .min(self.beats.len().saturating_sub(1))
    }

    /// The beat nearest `time`.
    #[must_use]
    pub fn beat_near(&self, time: f64) -> Option<usize> {
        (0..self.beats.len()).min_by(|a, b| {
            (self.beats[*a] - time)
                .abs()
                .total_cmp(&(self.beats[*b] - time).abs())
        })
    }

    #[must_use]
    pub fn map_options(&self) -> MapOptions {
        MapOptions {
            bar1: self.bar1_beat(),
            layout: self.layout.clone(),
            raw_from_bar: self.raw_from_bar,
        }
    }

    /// The tempo map: one point per bar.
    #[must_use]
    pub fn map(&self) -> TempoMap {
        TempoMap::build(&self.beats, &self.beat_sources, &self.map_options())
    }

    /// The map, measured against the stems.
    #[must_use]
    pub fn evaluate(&self, features: &Features) -> Evaluation {
        eval::evaluate(
            &self.map(),
            features,
            Some(Tracked {
                beats: &self.beats,
                sources: &self.beat_sources,
                bar1: self.bar1_beat(),
            }),
        )
    }

    /// Sets bar 1's downbeat to beat `beat`.
    ///
    /// # Errors
    ///
    /// A beat the song does not have.
    pub fn set_bar1(&mut self, beat: usize) -> Result<(), String> {
        if beat >= self.beats.len() {
            return Err(format!(
                "no beat {beat}: the song has {} (0 to {})",
                self.beats.len(),
                self.beats.len().saturating_sub(1)
            ));
        }
        self.bar1 = Some(beat);
        self.edits
            .push(format!("bar 1 at beat {beat} ({:.3} s)", self.beats[beat]));
        Ok(())
    }

    /// Sets the meter and its changes; the candidates for bar 1 are found
    /// again, and a bar 1 chosen before is forgotten when the meter of bar
    /// 1 changed.
    pub fn set_meter(&mut self, features: &Features, meter: Meter, changes: Vec<MeterChange>) {
        let was = self.layout.meter;
        self.layout = Layout::new(meter, changes);
        self.candidates = downbeat::candidates(features, &self.beats, meter);
        if was != meter {
            self.bar1 = None;
        }
        let list: Vec<String> = self
            .layout
            .changes
            .iter()
            .map(|c| format!("{} from bar {}", c.meter, c.bar))
            .collect();
        self.edits.push(if list.is_empty() {
            format!("meter {meter}")
        } else {
            format!("meter {meter}, {}", list.join(", "))
        });
    }

    /// The hand fix: the beats from `from` to `to` replaced by `intervals`
    /// evenly spaced ones, snapped to hits again when `snap`. Bar 1 keeps
    /// its beat (its index moves with the beats before it).
    ///
    /// # Errors
    ///
    /// A span the song does not have.
    pub fn respace(
        &mut self,
        features: &Features,
        from: usize,
        to: usize,
        intervals: usize,
        snap: bool,
    ) -> Result<(), String> {
        if from >= to || to >= self.beats.len() || intervals == 0 {
            return Err(format!(
                "beats {from} to {to} in {intervals}: not a span of the song's {} beats",
                self.beats.len()
            ));
        }
        let was = to - from;
        let (beats, sources) = phase::respace(&self.beats, &self.beat_sources, from, to, intervals);
        if snap {
            let params = TrackParams {
                from: 0.0,
                ..self.params
            };
            let (snapped, who) = track::snap(&beats, &features.kit_hits(), &params);
            self.beats = snapped;
            self.beat_sources = who;
        } else {
            self.beats = beats;
            self.beat_sources = sources;
        }
        if let Some(bar1) = self.bar1 {
            if bar1 >= to {
                self.bar1 = Some(bar1 + intervals - was);
            } else if bar1 > from {
                self.bar1 = None;
            }
        }
        self.candidates = downbeat::candidates(features, &self.beats, self.layout.meter);
        self.edits.push(format!(
            "beats {from} to {to} ({:.2} s to {:.2} s): {was} intervals made {intervals}",
            self.beats[from],
            self.beats[(from + intervals).min(self.beats.len() - 1)]
        ));
        Ok(())
    }

    /// Places beats by hand: the beats from `from` on are replaced by
    /// `times` (an ending with a ritard, placed from the hits). Give
    /// `raw_from_bar` too, so their downbeats are taken as given.
    ///
    /// # Errors
    ///
    /// A beat the song does not have, or times that do not rise from the
    /// beat before `from`.
    pub fn place(&mut self, from: usize, times: &[f64]) -> Result<(), String> {
        if from > self.beats.len() {
            return Err(format!("no beat {from}: the song has {}", self.beats.len()));
        }
        let mut before = from.checked_sub(1).map(|i| self.beats[i]);
        for t in times {
            if before.is_some_and(|b| *t <= b) {
                return Err(format!("{t} s does not come after the beat before it"));
            }
            before = Some(*t);
        }
        self.beats.truncate(from);
        self.beat_sources.truncate(from);
        self.beats.extend_from_slice(times);
        self.beat_sources
            .extend(std::iter::repeat_n("placed".to_owned(), times.len()));
        if self.bar1.is_some_and(|b| b >= self.beats.len()) {
            self.bar1 = None;
        }
        self.edits.push(format!(
            "{} beats placed by hand from beat {from}",
            times.len()
        ));
        Ok(())
    }

    /// Takes the downbeats as played from song bar `bar` on (a ritard), or
    /// fits them all (`None`).
    pub fn set_raw_from(&mut self, bar: Option<u32>) {
        self.raw_from_bar = bar;
        self.edits.push(bar.map_or_else(
            || "every downbeat fitted".to_owned(),
            |b| format!("as played from bar {b}"),
        ));
    }

    /// Every kit piece's hits with where each falls on the map.
    #[must_use]
    pub fn hit_marks(&self) -> BTreeMap<String, Vec<HitMark>> {
        let map = self.map();
        self.hits
            .iter()
            .map(|(piece, hits)| {
                let marks = hits
                    .iter()
                    .map(|h| {
                        let (bar, beat) = map.hit_position(h.time);
                        HitMark {
                            time: h.time,
                            strength: h.strength,
                            bar,
                            beat,
                        }
                    })
                    .collect();
                (piece.clone(), marks)
            })
            .collect()
    }

    /// What a report says, without the audio: the phase line is read from
    /// the stems, so it is given.
    #[must_use]
    pub fn summary(&self, phase_line: String) -> Summary {
        let map = self.map();
        let bar1 = self.bar1_beat();
        Summary {
            beats: self.beats.len(),
            on_hits: self
                .beat_sources
                .iter()
                .filter(|s| track::on_a_hit(s))
                .count(),
            bars: map.bars.len(),
            meter: self.layout.meter.to_string(),
            tempo: map.tempo_range(),
            bar1,
            bar1_time: self.beats.get(bar1).copied().unwrap_or(0.0),
            bar1_by: if self.bar1.is_some() {
                "set"
            } else {
                "candidate"
            }
            .to_owned(),
            phase_line,
            backbeat: self.backbeat.clone(),
            irregular: phase::irregular(&self.beats, 0.04),
            repaired: self.repair.fixed.len(),
            left: self.repair.left.len(),
            edits: self.edits.clone(),
        }
    }

    /// The phase line from bar 1, read from the stems.
    #[must_use]
    pub fn phase_line(&self, features: &Features) -> String {
        phase::backbeat_stem(features, &self.beats).map_or_else(String::new, |stem| {
            let peaks = phase::beat_peaks(stem, &self.beats);
            phase::line(&phase::phases(&peaks, &self.layout, self.bar1_beat()))
        })
    }
}
