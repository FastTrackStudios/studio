//! The drum MIDI's map set by hand: which note number is which piece.
//!
//! The map is learned from the recording (`drum_reference::learn_map`),
//! and learning can go wrong: on Unbreakable it took the kick as 23 and
//! 46 and left 24 (the kick) and 26 (the snare) unmapped, so every target
//! the MIDI gave went to the wrong line. `DRUMS_REFERENCE_MAP {set}` sets
//! it by hand — `{"Kick": 24, "Snare": 26, "Tom 1": 48}` — kept with the
//! song on the drum MIDI track; every read of the map ([`map_for`]) puts
//! the hand's over the learned one: a set piece is on its notes only,
//! every other pitch the learning gave it unmapped.

use daw::service::{ProjectContext, TrackRef, Tracks};
use facet::Facet;

use crate::drum_reference::{Mapped, RefNote};

/// Where the hand's map is kept on the drum MIDI track.
const KEY: &str = "fts.drum_map";

/// The hand's map: each piece and its note numbers.
#[derive(Facet, Clone, Debug, Default, PartialEq)]
pub struct HandMap {
    pub pieces: Vec<PieceNotes>,
}

/// One piece and its notes.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct PieceNotes {
    pub piece: String,
    pub pitches: Vec<u8>,
}

/// The hand's map on the song's drum MIDI track; empty when none is set
/// (or there is no such track).
#[must_use]
pub fn read(daw: &daw::standalone::Standalone, project: &ProjectContext) -> HandMap {
    let Some(track) = crate::drum_reference::find_track(daw, project) else {
        return HandMap::default();
    };
    Tracks::ext_state(daw, project.clone(), TrackRef::Guid(track))
        .into_iter()
        .find(|e| e.key == KEY)
        .and_then(|e| facet_json::from_str::<HandMap>(&e.value).ok())
        .unwrap_or_default()
}

/// Keep `map` as the hand's (empty: none, back to the learned map).
///
/// # Errors
/// The song has no drum MIDI track, or it would not keep it.
pub fn write(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    map: &HandMap,
) -> Result<(), String> {
    let track = crate::drum_reference::find_track(daw, project)
        .ok_or("no drum MIDI track (one named for drum MIDI) to keep a map on")?;
    let text = if map.pieces.is_empty() {
        String::new()
    } else {
        facet_json::to_string(map).map_err(|e| e.to_string())?
    };
    Tracks::set_ext_state(daw, project.clone(), TrackRef::Guid(track), KEY, &text)
        .map_err(|e| e.to_string())
}

/// `set` (`{piece: pitch}`) made the hand's map over `old`: each piece
/// given is on that note alone; a pitch of 0 or below takes the piece out
/// of the hand's map. `clear` starts from none.
#[must_use]
pub fn merged(old: &HandMap, set: &std::collections::HashMap<String, i64>, clear: bool) -> HandMap {
    let mut pieces: Vec<PieceNotes> = if clear {
        Vec::new()
    } else {
        old.pieces.clone()
    };
    let mut given: Vec<(&String, &i64)> = set.iter().collect();
    given.sort();
    for (piece, pitch) in given {
        pieces.retain(|p| !p.piece.eq_ignore_ascii_case(piece));
        if let Ok(pitch) = u8::try_from(*pitch)
            && pitch > 0
            && pitch < 128
        {
            // A pitch is one piece's: set to another, it leaves the first.
            for p in &mut pieces {
                p.pitches.retain(|n| *n != pitch);
            }
            pieces.push(PieceNotes {
                piece: piece.trim().to_owned(),
                pitches: vec![pitch],
            });
        }
    }
    pieces.retain(|p| !p.pitches.is_empty());
    pieces.sort_by(|a, b| a.piece.cmp(&b.piece));
    HandMap { pieces }
}

/// `learned` with the hand's map over it: a pitch the hand gives a piece
/// is that piece's (agreement 1); any other pitch the learning gave a
/// piece the hand sets is unmapped; a set pitch with no notes is listed
/// with none.
#[must_use]
pub fn over(learned: Vec<Mapped>, hand: &HandMap) -> Vec<Mapped> {
    if hand.pieces.is_empty() {
        return learned;
    }
    let owner = |pitch: u8| {
        hand.pieces
            .iter()
            .find(|p| p.pitches.contains(&pitch))
            .map(|p| p.piece.clone())
    };
    let set = |piece: &str| {
        hand.pieces
            .iter()
            .any(|p| p.piece.eq_ignore_ascii_case(piece))
    };
    let mut out: Vec<Mapped> = learned
        .into_iter()
        .map(|mut m| {
            if let Some(piece) = owner(m.pitch) {
                m.piece = Some(piece);
                m.agreement = 1.0;
            } else if m.piece.as_deref().is_some_and(set) {
                m.piece = None;
            }
            m
        })
        .collect();
    for p in &hand.pieces {
        for pitch in &p.pitches {
            if !out.iter().any(|m| m.pitch == *pitch) {
                out.push(Mapped {
                    pitch: *pitch,
                    notes: 0,
                    piece: Some(p.piece.clone()),
                    agreement: 1.0,
                });
            }
        }
    }
    out.sort_by_key(|m| m.pitch);
    out
}

/// The map the song's drum MIDI is read through: learned from `notes`
/// against `pieces` (each piece's hit times, by time) within `window`,
/// with the hand's over it.
#[must_use]
pub fn map_for(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    notes: &[RefNote],
    pieces: &[(String, Vec<f64>)],
    window: f64,
) -> Vec<Mapped> {
    over(
        crate::drum_reference::learn_map(notes, pieces, window),
        &read(daw, project),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn learned() -> Vec<Mapped> {
        vec![
            Mapped {
                pitch: 23,
                notes: 40,
                piece: Some("Kick".into()),
                agreement: 0.26,
            },
            Mapped {
                pitch: 24,
                notes: 300,
                piece: None,
                agreement: 0.04,
            },
            Mapped {
                pitch: 38,
                notes: 120,
                piece: Some("Snare".into()),
                agreement: 0.7,
            },
        ]
    }

    #[test]
    fn a_hand_set_map_wins_over_the_learned_one() {
        let set = std::collections::HashMap::from([
            ("Kick".to_owned(), 24_i64),
            ("Snare".to_owned(), 26),
        ]);
        let hand = merged(&HandMap::default(), &set, false);
        let map = over(learned(), &hand);
        let piece = |pitch: u8| {
            map.iter()
                .find(|m| m.pitch == pitch)
                .and_then(|m| m.piece.clone())
        };
        assert_eq!(piece(24).as_deref(), Some("Kick"));
        assert_eq!(piece(23), None, "the learned kick pitch is let go");
        assert_eq!(
            piece(26).as_deref(),
            Some("Snare"),
            "a set pitch with no notes is listed"
        );
        assert_eq!(piece(38), None);
    }

    #[test]
    fn a_pitch_set_to_another_piece_leaves_the_first_and_zero_unsets() {
        let first = merged(
            &HandMap::default(),
            &std::collections::HashMap::from([("Tom 1".to_owned(), 48_i64)]),
            false,
        );
        let moved = merged(
            &first,
            &std::collections::HashMap::from([("Tom 2".to_owned(), 48_i64)]),
            false,
        );
        assert_eq!(moved.pieces.len(), 1);
        assert_eq!(moved.pieces[0].piece, "Tom 2");
        let gone = merged(
            &moved,
            &std::collections::HashMap::from([("Tom 2".to_owned(), 0_i64)]),
            false,
        );
        assert!(gone.pieces.is_empty());
        assert_eq!(over(learned(), &gone), learned());
    }
}
