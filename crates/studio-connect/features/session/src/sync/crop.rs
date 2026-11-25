//! A song cut out of a show: the model between two moments, moved to start
//! at zero.
//!
//! A recital recorded as one long session becomes a setlist of songs, each
//! its own session (`ableton::setlist` finds where they start and end).
//! [`crop`] makes one: every item that sounds inside `[start, end)` — the
//! ones that cross an edge trimmed there, their takes' offsets moved with
//! the trim so they still play the same audio — and the automation,
//! markers, regions and tempo between, all moved earlier by `start`. Media
//! is untouched: a cropped song names the same files the show does, so the
//! songs of a show share its recordings.

use std::collections::BTreeMap;

use super::model::{
    EnvelopePointState, EnvelopeState, ItemState, SessionModel, TakeState, TempoPoint,
};

/// The part of `model` in `[start, end)` seconds, moved to start at 0.
#[must_use]
pub fn crop(model: &SessionModel, start: f64, end: f64) -> SessionModel {
    let length = (end - start).max(0.0);
    let mut out = model.clone();
    out.items = model
        .items
        .iter()
        .filter_map(|(guid, item)| crop_item(item, start, end).map(|i| (guid.clone(), i)))
        .collect();
    out.markers = model
        .markers
        .iter()
        .filter(|(_, m)| m.at >= start && m.at < end)
        .map(|(g, m)| {
            let mut m = m.clone();
            m.at -= start;
            (g.clone(), m)
        })
        .collect();
    out.regions = model
        .regions
        .iter()
        .filter(|(_, r)| r.end > start && r.start < end)
        .map(|(g, r)| {
            let mut r = r.clone();
            r.start = r.start.max(start) - start;
            r.end = r.end.min(end) - start;
            (g.clone(), r)
        })
        .collect();
    out.tempo = crop_tempo(&model.tempo, start, end);
    for track in &mut out.tracks {
        track.envelopes = crop_envelopes(&track.envelopes, start, length);
    }
    out.project.master.envelopes = crop_envelopes(&model.project.master.envelopes, start, length);
    // A show's chart is the show's; a song starts with none.
    out.chart = String::new();
    out
}

/// `item` cut to `[start, end)` and moved earlier by `start`; `None` when it
/// does not sound there.
fn crop_item(item: &ItemState, start: f64, end: f64) -> Option<ItemState> {
    let from = item.position;
    let to = item.position + item.length;
    if to <= start || from >= end {
        return None;
    }
    let mut out = item.clone();
    let head = (start - from).max(0.0);
    let tail = (to - end).max(0.0);
    if head > 0.0 {
        out.fade_in = 0.0;
        trim_take_head(&mut out.take, head);
        for take in &mut out.other_takes {
            trim_take_head(take, head);
        }
    }
    if tail > 0.0 {
        out.fade_out = 0.0;
    }
    out.length = item.length - head - tail;
    out.position = from.max(start) - start;
    Some(out)
}

/// A take whose item lost its first `head` seconds: it starts that much
/// later in its source, and what it keeps in item time moves earlier.
fn trim_take_head(take: &mut TakeState, head: f64) {
    let rate = if take.playrate > 0.0 {
        take.playrate
    } else {
        1.0
    };
    take.start_offset += head * rate;
    take.volume.retain(|p| p.at >= head);
    for p in &mut take.volume {
        p.at -= head;
    }
    take.stretch.retain(|p| p.at >= head);
    for p in &mut take.stretch {
        p.at -= head;
    }
    take.notes.retain(|n| n.start + n.length > head);
    for n in &mut take.notes {
        let cut = (head - n.start).max(0.0);
        n.start = (n.start - head).max(0.0);
        n.length -= cut;
    }
    for env in take.envelopes.values_mut() {
        *env = crop_envelope(env, head, f64::INFINITY);
    }
}

/// The tempo in force at `start` at 0, then the changes before `end`.
///
/// A change a hair past `start` (a song cut where its tempo changes, the
/// change read back from a file a few ulps later) is the change at it: the
/// song starts at its own tempo, not the one before for a picosecond.
fn crop_tempo(tempo: &[TempoPoint], start: f64, end: f64) -> Vec<TempoPoint> {
    const SAME: f64 = 1e-9;
    let mut sorted: Vec<&TempoPoint> = tempo.iter().collect();
    sorted.sort_by(|a, b| a.at.total_cmp(&b.at));
    let mut out = Vec::new();
    if let Some(at_start) = sorted.iter().rev().find(|t| t.at <= start + SAME) {
        let mut first = (*at_start).clone();
        first.at = 0.0;
        out.push(first);
    }
    for t in sorted
        .into_iter()
        .filter(|t| t.at > start + SAME && t.at < end)
    {
        let mut t = t.clone();
        t.at -= start;
        out.push(t);
    }
    out
}

fn crop_envelopes(
    envelopes: &BTreeMap<String, EnvelopeState>,
    start: f64,
    length: f64,
) -> BTreeMap<String, EnvelopeState> {
    envelopes
        .iter()
        .map(|(k, e)| (k.clone(), crop_envelope(e, start, length)))
        .collect()
}

/// An envelope's points in `[start, start + length]`, moved earlier by
/// `start`, with its value at each cut edge kept as a point there (read off
/// the line between the points around it), so the song starts and ends at
/// the level the show had. Automation items outside are dropped.
fn crop_envelope(env: &EnvelopeState, start: f64, length: f64) -> EnvelopeState {
    let end = start + length;
    let mut out = env.clone();
    let mut points: Vec<EnvelopePointState> = env.points.clone();
    points.sort_by(|a, b| a.at.total_cmp(&b.at));
    let mut kept: Vec<EnvelopePointState> = Vec::new();
    if points.iter().any(|p| p.at < start)
        && let Some(edge) = point_at(&points, start)
    {
        kept.push(edge);
    }
    kept.extend(
        points
            .iter()
            .filter(|p| p.at >= start && p.at <= end)
            .cloned(),
    );
    if end.is_finite()
        && points.iter().any(|p| p.at > end)
        && let Some(edge) = point_at(&points, end)
    {
        kept.push(edge);
    }
    for p in &mut kept {
        p.at -= start;
    }
    out.points = kept;
    out.items = env
        .items
        .iter()
        .filter(|i| i.position + i.length > start && i.position < end)
        .map(|i| {
            let mut i = i.clone();
            i.position = i.position.max(start) - start;
            i
        })
        .collect();
    out
}

/// A point at `at` with the envelope's value there: linear between the
/// points around it, held past the last.
fn point_at(points: &[EnvelopePointState], at: f64) -> Option<EnvelopePointState> {
    let after = points.iter().position(|p| p.at >= at);
    let (value, like) = match after {
        Some(0) => (points[0].value, &points[0]),
        Some(i) => {
            let (a, b) = (&points[i - 1], &points[i]);
            let span = b.at - a.at;
            let t = if span > 0.0 { (at - a.at) / span } else { 1.0 };
            (a.value + (b.value - a.value) * t, a)
        }
        None => {
            let last = points.last()?;
            (last.value, last)
        }
    };
    let mut p = like.clone();
    p.at = at;
    p.value = value;
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::model::{GainPoint, MarkerState, RegionState};

    fn item(position: f64, length: f64, offset: f64) -> ItemState {
        ItemState {
            track: "drums".into(),
            position,
            length,
            take: TakeState {
                source: Some("Samples/Recital Drums.wav".into()),
                start_offset: offset,
                playrate: 1.0,
                ..TakeState::default()
            },
            ..ItemState::default()
        }
    }

    fn env(points: &[(f64, f64)]) -> EnvelopeState {
        EnvelopeState {
            points: points
                .iter()
                .map(|&(at, value)| EnvelopePointState {
                    at,
                    value,
                    ..EnvelopePointState::default()
                })
                .collect(),
            ..EnvelopeState::default()
        }
    }

    #[test]
    fn items_inside_move_earlier_and_items_across_an_edge_are_trimmed_there() {
        let mut show = SessionModel::default();
        show.items.insert("before".into(), item(0.0, 50.0, 0.0));
        show.items
            .insert("across-start".into(), item(90.0, 20.0, 90.0));
        show.items.insert("inside".into(), item(120.0, 10.0, 120.0));
        show.items
            .insert("across-end".into(), item(190.0, 30.0, 190.0));
        show.items.insert("after".into(), item(300.0, 10.0, 300.0));

        let song = crop(&show, 100.0, 200.0);

        let got: Vec<(&str, f64, f64, f64)> = song
            .items
            .iter()
            .map(|(g, i)| (g.as_str(), i.position, i.length, i.take.start_offset))
            .collect();
        assert_eq!(
            got,
            [
                // 10 s cut off its head: it now starts 10 s later in the file.
                ("across-end", 90.0, 10.0, 190.0),
                ("across-start", 0.0, 10.0, 100.0),
                ("inside", 20.0, 10.0, 120.0),
            ],
            "every item still plays the same audio at the same moment of the song"
        );
        assert_eq!(
            song.items["inside"].take.source.as_deref(),
            Some("Samples/Recital Drums.wav"),
            "the song names the show's recordings: nothing copied"
        );
    }

    #[test]
    fn a_trimmed_head_moves_the_takes_points_and_drops_the_fade() {
        let mut it = item(90.0, 20.0, 90.0);
        it.fade_in = 0.5;
        it.take.volume = vec![
            GainPoint {
                at: 2.0,
                value: 0.5,
            },
            GainPoint {
                at: 15.0,
                value: 1.0,
            },
        ];
        let cut = crop_item(&it, 100.0, 200.0).expect("it sounds there");
        assert_eq!(cut.fade_in, 0.0);
        assert_eq!(
            cut.take.volume,
            vec![GainPoint {
                at: 5.0,
                value: 1.0
            }]
        );
    }

    #[test]
    fn markers_regions_and_tempo_follow_the_song() {
        let mut show = SessionModel::default();
        for (g, at) in [("verse", 110.0), ("outside", 250.0)] {
            show.markers.insert(
                g.into(),
                MarkerState {
                    at,
                    name: g.into(),
                    ..MarkerState::default()
                },
            );
        }
        show.regions.insert(
            "chorus".into(),
            RegionState {
                start: 180.0,
                end: 240.0,
                name: "Chorus".into(),
                ..RegionState::default()
            },
        );
        show.tempo = vec![
            TempoPoint {
                at: 0.0,
                bpm: 126.0,
                beats_per_bar: 4,
                beat_unit: 4,
                ..TempoPoint::default()
            },
            TempoPoint {
                at: 150.0,
                bpm: 92.0,
                beats_per_bar: 4,
                beat_unit: 4,
                ..TempoPoint::default()
            },
        ];
        let song = crop(&show, 100.0, 200.0);
        assert_eq!(song.markers.keys().collect::<Vec<_>>(), ["verse"]);
        assert_eq!(song.markers["verse"].at, 10.0);
        assert_eq!(
            (song.regions["chorus"].start, song.regions["chorus"].end),
            (80.0, 100.0)
        );
        let tempo: Vec<(f64, f64)> = song.tempo.iter().map(|t| (t.at, t.bpm)).collect();
        assert_eq!(
            tempo,
            [(0.0, 126.0), (50.0, 92.0)],
            "the tempo in force at the cut, then the change"
        );
    }

    #[test]
    fn a_song_cut_at_its_tempo_change_starts_at_its_own_tempo() {
        // `Heart isn't Ready`: 160 before it, 107 from where it starts —
        // the change read back from the show's file 2e-13 s past the cut.
        let point = |at, bpm| TempoPoint {
            at,
            bpm,
            beats_per_bar: 4,
            beat_unit: 4,
        };
        let start = 1529.515_400_425_068_7;
        let show = SessionModel {
            tempo: vec![point(1527.0, 160.0), point(start + 2.3e-13, 107.0)],
            ..SessionModel::default()
        };
        let song = crop(&show, start, start + 200.0);
        let tempo: Vec<(f64, f64)> = song.tempo.iter().map(|t| (t.at, t.bpm)).collect();
        assert_eq!(tempo, [(0.0, 107.0)], "not 160 with a 107 a picosecond in");
    }

    #[test]
    fn automation_keeps_its_level_at_both_edges() {
        let e = env(&[(0.0, 0.0), (200.0, 1.0), (400.0, 1.0)]);
        let cut = crop_envelope(&e, 100.0, 200.0);
        let points: Vec<(f64, f64)> = cut.points.iter().map(|p| (p.at, p.value)).collect();
        assert_eq!(points, [(0.0, 0.5), (100.0, 1.0), (200.0, 1.0)]);
    }
}
