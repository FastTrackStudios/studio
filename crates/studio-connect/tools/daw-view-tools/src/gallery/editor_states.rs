//! What the window gives an editor beyond its take — the song's bar lines,
//! its markers and sections — and the states a shot holds it in (a razor
//! area across the melody), set on the editor directly.

use daw_theme::gallery::View;
use midi_editor::expression::Expression;

use crate::golden::Golden;

/// Ticks to a quarter note, as the golden takes are written.
const PPQ: f64 = 960.0;
/// Seconds to a quarter note where the MIDI sits (120 bpm).
const QUARTER: f64 = 0.5;

/// Give `editor` the song around its take, and the states `view` shows.
pub fn apply(editor: &mut Expression, golden: &Golden, view: View) {
    let doc = &editor.editor.doc;
    editor.editor.playhead = Some((doc.end - doc.start).mul_add(0.42, doc.start));
    match view {
        View::Vocal => return sung(editor),
        View::Stack => return with_guide_lane(editor),
        _ => {}
    }
    let item = match view {
        View::Drums => &golden.drums.item,
        _ => &golden.melody.item,
    };
    let start = golden.item_start(item).unwrap_or(0.0);
    let take = match view {
        View::Drums => &golden.drums,
        _ => &golden.melody,
    };
    select_as_taken(editor, take);
    let doc = &mut editor.editor.doc;
    doc.bars = bars(doc.end);
    doc.markers = markers(golden, start, doc.end);
    doc.regions = regions(golden, start, doc.end);
    if matches!(view, View::Piano) {
        // A razor area over the held chord's top notes, on a triplet grid.
        editor
            .editor
            .razor
            .areas
            .push(timeline::razor::RazorArea::new(
                20.0 * PPQ,
                22.0 * PPQ,
                62,
                67,
            ));
        editor.editor.grid.set_division(1.0 / 8.0);
        editor.editor.grid.set_triplet(true);
        // Rows tall enough for the keys' labels and the line at every C.
        editor.editor.camera.vertical.center = 66.0;
        editor.editor.camera.vertical.px_per_row *= 1.8;
    }
    if matches!(view, View::Controllers) {
        for lane in &mut editor.editor.doc.cc.lanes {
            lane.pinned = true;
        }
    }
}

/// The take's selected notes selected in the editor too: the editor reads
/// a take's notes but not which were selected, so they are found again by
/// where they start and how hard they play.
fn select_as_taken(editor: &mut Expression, take: &crate::golden::Take) {
    let chosen: Vec<(f64, u8)> = take
        .snapshot
        .notes
        .iter()
        .filter(|n| n.selected)
        .map(|n| (n.start_ppq, n.velocity))
        .collect();
    let ed = &mut editor.editor;
    ed.selection.notes = ed
        .doc
        .notes
        .iter()
        .filter(|n| {
            chosen.iter().any(|(at, velocity)| {
                (n.start - at).abs() < 0.5
                    && n.velocity.mul_add(127.0, -f64::from(*velocity)).abs() < 0.6
            })
        })
        .map(|n| n.id)
        .collect();
}

/// A guide mic beside the kit's role lanes: a lane no role claims, drawn
/// in the editor's own waveform colour.
fn with_guide_lane(editor: &mut Expression) {
    let ed = &mut editor.editor;
    let guide = ed
        .tracks
        .doc_of(1)
        .cloned()
        .unwrap_or_else(|| ed.doc.clone());
    ed.add_track_with_guid("guide", "Guide", guide);
    editor.invalidate_stack();
}

/// The sung take's states: its waveform behind the notes, the zoned note
/// selected so its handles and zone lines show.
fn sung(editor: &mut Expression) {
    let ed = &mut editor.editor;
    let span = (ed.doc.end - ed.doc.start).max(1.0);
    ed.doc.peaks = (0..600_u32)
        .map(|k| {
            let f = f64::from(k) / 600.0;
            let breath = (f * span * 0.02).sin().abs();
            daw_view_kit::num::narrow(breath.mul_add(0.6, 0.15))
        })
        .collect();
    let zoned = ed
        .doc
        .notes
        .iter()
        .find(|n| !n.splits.is_empty())
        .or_else(|| ed.doc.notes.first())
        .map(|n| n.id);
    ed.selection.notes = zoned.into_iter().collect();
}

/// Seconds into the song as ticks into the take starting at `start`.
fn ticks(at: f64, start: f64) -> f64 {
    (at - start) / QUARTER * PPQ
}

/// A bar line every four quarter notes up to `end` ticks.
fn bars(end: f64) -> Vec<f64> {
    // A take is minutes, not years: the bar count fits a u32 many times.
    let count = daw_view_kit::num::index(end / (4.0 * PPQ)).saturating_add(1);
    (0..count)
        .map(|bar| daw_view_kit::num::coord(bar) * 4.0 * PPQ)
        .collect()
}

/// The song's markers that fall inside the take.
fn markers(golden: &Golden, start: f64, end: f64) -> Vec<editor::doc::Marker> {
    golden
        .project
        .markers
        .iter()
        .map(|m| (ticks(m.at, start), m))
        .filter(|(t, _)| (0.0..=end).contains(t))
        .map(|(t, m)| editor::doc::Marker {
            t,
            label: Some(m.name.clone()),
            color: m.color.clone(),
            lane: None,
        })
        .collect()
}

/// The song's sections over the take, cut to it.
fn regions(golden: &Golden, start: f64, end: f64) -> Vec<editor::doc::Region> {
    golden
        .project
        .sections
        .iter()
        .filter(|s| s.lane == 0)
        .map(|s| {
            (
                ticks(s.start, start).max(0.0),
                ticks(s.end, start).min(end),
                s,
            )
        })
        .filter(|(a, b, _)| b > a)
        .map(|(a, b, s)| editor::doc::Region {
            start: a,
            end: b,
            label: s.name.clone(),
            color: s.color.clone(),
            lane: None,
        })
        .collect()
}
